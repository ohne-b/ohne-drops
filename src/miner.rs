use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use chrono::Utc;
use serde_json::json;
use tokio::{
    sync::{Mutex, Notify, mpsc, oneshot, watch},
    task::{JoinHandle, JoinSet},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::Settings,
    domain::{Campaign, Channel, Drop, MAX_ESTIMATED_MINUTES, wanted_items},
    dto::{InventoryStatus, Login, ManualMode, RefreshState},
    store::{ClaimJournal, PendingClaim},
    twitch::{
        Endpoints, TwitchClient, TwitchError, TwitchHttp,
        channels::select_channel,
        inventory::Inventory,
        oauth::{DeviceLogin, Session},
        pubsub::{Event, PubSub},
    },
    web::{App, Command, CommandRequest, message},
};

const WATCH_INTERVAL: Duration = Duration::from_secs(59);
const PROGRESS_DELAY: Duration = Duration::from_secs(20);
const CHANNEL_DELAY: Duration = Duration::from_secs(2);

#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
struct Intent {
    refresh: u64,
    clear: u64,
    settings: u64,
    manual_revision: u64,
    selected: Option<u64>,
    channel_login: Option<String>,
    manual_duration: Option<Duration>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ManualSelection {
    channel: u64,
    expires_at: Option<Instant>,
}
impl ManualSelection {
    fn new(channel: u64, duration: Option<Duration>) -> Self {
        Self {
            channel,
            expires_at: duration.map(|duration| Instant::now() + duration),
        }
    }
}

struct Generation {
    cancel: CancellationToken,
    confirmed: Arc<Notify>,
    task: JoinHandle<Result<(), TwitchError>>,
}

#[derive(Default)]
struct Resume {
    manual: Option<ManualSelection>,
    channel: Option<Channel>,
    lookup: Option<(String, u64)>,
    seen: Intent,
}

pub struct Miner {
    app: Arc<App>,
    commands: mpsc::Receiver<CommandRequest>,
    endpoints: Endpoints,
}

impl Miner {
    pub fn new(app: Arc<App>, commands: mpsc::Receiver<CommandRequest>) -> Self {
        Self {
            app,
            commands,
            endpoints: Endpoints::default(),
        }
    }

    fn start(
        &self,
        settings: Settings,
        resume: Arc<Mutex<Resume>>,
        receiver: watch::Receiver<Intent>,
    ) -> Generation {
        let cancel = CancellationToken::new();
        let confirmed = Arc::new(Notify::new());
        let app = self.app.clone();
        let endpoints = self.endpoints.clone();
        let stopped = cancel.clone();
        let confirmation = confirmed.clone();
        let task = tokio::spawn(async move {
            run_generation(
                app,
                settings,
                endpoints,
                stopped,
                confirmation,
                receiver,
                resume,
            )
            .await
        });
        Generation {
            cancel,
            confirmed,
            task,
        }
    }

    pub async fn run(mut self) -> Result<(), TwitchError> {
        let resume = Arc::new(Mutex::new(Resume::default()));
        // Commands outlive a network generation, including commands accepted
        // after its last select cycle but before the supervisor observes its exit.
        let (intent, _) = watch::channel(Intent::default());
        while !self.app.shutdown.is_cancelled() {
            let settings = self.app.snapshot.read().await.settings.values.clone();
            let mut generation = self.start(settings.clone(), resume.clone(), intent.subscribe());
            loop {
                tokio::select! {biased;
                    _=self.app.shutdown.cancelled()=>{
                        while let Ok(request)=self.commands.try_recv(){
                            if matches!(request.command,Command::Logout){self.logout(generation,request.complete).await;return Ok(());}
                            let _=request.complete.send(Err("Miner is shutting down".into()));
                        }
                        generation.cancel.cancel();let _=generation.task.await;return Ok(());
                    },
                    request=self.commands.recv()=>{
                        let Some(request)=request else {generation.cancel.cancel();let _=generation.task.await;return Ok(())};
                        match request.command {
                            Command::Logout=>{self.logout(generation,request.complete).await;*resume.lock().await=Resume::default();intent.send_replace(Intent::default());break;},
                            Command::Shutdown=>{
                                self.app.shutdown.cancel();
                                let _=request.complete.send(Ok(()));
                            },
                            Command::ConfirmOAuth=>{generation.confirmed.notify_one();let _=request.complete.send(Ok(()));},
                            Command::SettingsChanged=>{
                                let current=self.app.snapshot.read().await.settings.values.clone();
                                if current.proxy!=settings.proxy || current.connection_quality!=settings.connection_quality {
                                    generation.cancel.cancel();let _=generation.task.await;
                                    let _=request.complete.send(Ok(()));break;
                                }
                                intent.send_modify(|intent|intent.settings=intent.settings.wrapping_add(1));
                                let _=request.complete.send(Ok(()));
                            },
                            command=>{
                                intent.send_modify(|intent|match command {
                                    Command::Refresh{clear_cache}=>{intent.refresh=intent.refresh.wrapping_add(1);if clear_cache{intent.clear=intent.clear.wrapping_add(1);}},
                                    Command::SelectChannel(id,duration)=>{intent.channel_login=None;intent.selected=Some(id);intent.manual_duration=duration;intent.manual_revision=intent.manual_revision.wrapping_add(1);},
                                    Command::MineChannel(login,duration)=>{intent.channel_login=Some(login);intent.selected=None;intent.manual_duration=duration;intent.manual_revision=intent.manual_revision.wrapping_add(1);},
                                    Command::ExitManual=>{intent.channel_login=None;intent.selected=None;intent.manual_revision=intent.manual_revision.wrapping_add(1);},
                                    _=>{},
                                });
                                let _=request.complete.send(Ok(()));
                            },
                        }
                    },
                    result=&mut generation.task=>{
                        match result {
                            Ok(Err(TwitchError::Unauthorized))=>{
                                *resume.lock().await=Resume::default();
                                intent.send_replace(Intent::default());
                                if remove_session(&self.app).await.is_err(){self.app.console(message("gui.backend.session_storage",&[])).await;}
                                reset_session(&self.app).await;
                            },
                            Ok(Err(TwitchError::Cancelled))|Ok(Ok(()))=>{},
                            Ok(Err(error))=>{
                                let sequence=self.app.snapshot.read().await.inventory_refresh.sequence;
                                self.app.finish_inventory_refresh(sequence,Some(message("gui.redesign.refresh_failed_detail",&[]))).await;
                                self.app.console(message("gui.backend.twitch_error",&[("error",&error.to_string())])).await;
                                tokio::select!{_=self.app.shutdown.cancelled()=>{},_=tokio::time::sleep(Duration::from_secs(5))=>{}}
                            },
                            Err(_)=>{
                                self.app.console(message("gui.backend.worker_failed",&[])).await;
                                self.app.shutdown.cancel();return Err(TwitchError::InvalidResponse);
                            },
                        }
                        break;
                    },
                }
            }
        }
        Ok(())
    }

    async fn logout(
        &mut self,
        mut generation: Generation,
        first: oneshot::Sender<Result<(), String>>,
    ) {
        generation.cancel.cancel();
        let mut requests = vec![first];
        // The owner keeps draining even if the requesting browser disconnects or
        // process shutdown starts. No new login can overlap credential removal.
        loop {
            tokio::select! {
                _=&mut generation.task=>break,
                request=self.commands.recv()=>{
                    let Some(request)=request else {let _=generation.task.await;break};
                    self.during_logout(request,&mut requests);
                },
            }
        }
        while let Ok(request) = self.commands.try_recv() {
            self.during_logout(request, &mut requests);
        }
        let result = remove_session(&self.app)
            .await
            .map_err(|error| error.to_string());
        reset_session(&self.app).await;
        for request in requests {
            let _ = request.send(result.clone());
        }
    }
    fn during_logout(
        &self,
        request: CommandRequest,
        requests: &mut Vec<oneshot::Sender<Result<(), String>>>,
    ) {
        match request.command {
            Command::Logout => requests.push(request.complete),
            Command::Shutdown => {
                self.app.shutdown.cancel();
                requests.push(request.complete);
            }
            Command::SelectChannel(..) | Command::MineChannel(..) => {
                let _ = request
                    .complete
                    .send(Err("Twitch login is required".into()));
            }
            _ => {
                let _ = request.complete.send(Ok(()));
            }
        }
    }
}

async fn remove_session(app: &Arc<App>) -> Result<(), TwitchError> {
    let directory = app.data.path.clone();
    tokio::task::spawn_blocking(move || Session::remove(&directory))
        .await
        .map_err(|_| TwitchError::Storage)?
}
async fn publish_login(app: &App, login: Login) {
    app.snapshot.write().await.login = login.clone();
    app.sockets.emit("login_status", &login).await;
}
async fn reset_session(app: &App) {
    let archived = app.archive.lock().await.merge(vec![], Utc::now());
    {
        let mut state = app.snapshot.write().await;
        state.channels.clear();
        state.current_drop = None;
        state.wanted_items.clear();
        state.manual_mode = ManualMode::default();
        state.login = Login {
            status: message("login.status.logged_out", &[]),
            ..Login::default()
        };
        state.campaigns = archived;
        state.inventory_status = InventoryStatus::default();
        state.inventory_refresh.sequence += 1;
        state.inventory_refresh.state = RefreshState::Idle;
        state.inventory_refresh.error = None;
        state.settings.games_available.clear();
    }
    let state = app.snapshot.read().await.clone();
    app.sockets.emit("initial_state", &state).await;
}

async fn authenticate(
    app: &Arc<App>,
    settings: &Settings,
    endpoints: &Endpoints,
    cancel: &CancellationToken,
    confirmed: &Notify,
) -> Result<(TwitchClient, Session), TwitchError> {
    loop {
        let attempt = async {
            let saved = Session::load(&app.data.path)?;
            let mut http = TwitchHttp::build(
                settings,
                saved.as_ref().map(|s| s.device_id.as_str()),
                cancel.clone(),
                endpoints.clone(),
            )?;
            let session = if let Some(saved) = saved {
                saved.restore(&http).await?
            } else {
                http.discover_device().await?;
                let http = Arc::new(http);
                let login = DeviceLogin::start(http.clone()).await?;
                publish_login(
                    app,
                    Login {
                        status: message("login.status.waiting_auth", &[]),
                        user_id: None,
                        oauth_pending: Some(login.code.clone()),
                    },
                )
                .await;
                app.status(message("login.status.required", &[])).await;
                let session = login.finish(confirmed).await?;
                save_session(app, &session).await?;
                return Ok((TwitchClient::new(http, &session), session));
            };
            save_session(app, &session).await?;
            Ok((TwitchClient::new(Arc::new(http), &session), session))
        }
        .await;
        match attempt {
            Ok(result) => return Ok(result),
            Err(TwitchError::Cancelled) => return Err(TwitchError::Cancelled),
            Err(TwitchError::Unauthorized) => {
                remove_session(app).await?;
                reset_session(app).await;
            }
            Err(error) => {
                publish_login(
                    app,
                    Login {
                        status: message("login.status.required", &[]),
                        ..Login::default()
                    },
                )
                .await;
                app.console(message(
                    "gui.backend.twitch_error",
                    &[("error", &error.to_string())],
                ))
                .await;
            }
        }
        tokio::select! {biased;_=cancel.cancelled()=>return Err(TwitchError::Cancelled),_=tokio::time::sleep(Duration::from_secs(5))=>{}}
    }
}
async fn save_session(app: &Arc<App>, session: &Session) -> Result<(), TwitchError> {
    let directory = app.data.path.clone();
    let saved = session.clone();
    tokio::task::spawn_blocking(move || saved.save(&directory))
        .await
        .map_err(|_| TwitchError::Storage)?
}

async fn run_generation(
    app: Arc<App>,
    settings: Settings,
    endpoints: Endpoints,
    cancel: CancellationToken,
    confirmed: Arc<Notify>,
    intent: watch::Receiver<Intent>,
    resume: Arc<Mutex<Resume>>,
) -> Result<(), TwitchError> {
    let (client, session) = authenticate(&app, &settings, &endpoints, &cancel, &confirmed).await?;
    if cancel.is_cancelled() {
        return Err(TwitchError::Cancelled);
    }
    publish_login(
        &app,
        Login {
            status: message("login.status.logged_in", &[]),
            user_id: Some(session.user_id),
            oauth_pending: None,
        },
    )
    .await;
    let journal = Arc::new(Mutex::new(
        ClaimJournal::load(&app.data.path).map_err(|_| TwitchError::Storage)?,
    ));
    let (events, receiver) = mpsc::channel(256);
    let mut pool = PubSub::start(client.clone(), events);
    let mut mining = Mining::new(app, client, journal, intent, receiver);
    {
        let saved = resume.lock().await;
        mining.restore(&saved);
    }
    let result = mining.run(&mut pool).await;
    *resume.lock().await = mining.resume();
    cancel.cancel();
    // Owned jobs include durable claim writes. Cancellation stops network work,
    // while any confirmed claim finishes its disk transaction before logout.
    while mining.jobs.join_next().await.is_some() {}
    pool.close().await;
    result
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum JobKind {
    Manual,
    Inventory,
    Channels,
    Watch,
    Poll,
    Claim,
    Update,
    Notification,
}
enum Job {
    Manual {
        revision: u64,
        requested_at: Instant,
        result: Box<Result<Option<Channel>, TwitchError>>,
    },
    Inventory {
        result: Result<Inventory, TwitchError>,
        requested_at: chrono::DateTime<Utc>,
        refresh_sequence: u64,
    },
    Channels {
        result: Result<Vec<Channel>, TwitchError>,
        requested_at: Instant,
    },
    Watch {
        channel: Box<Channel>,
        result: Result<bool, TwitchError>,
        requested_at: Instant,
        at: Instant,
    },
    Poll {
        channel: u64,
        requested_at: Instant,
        result: Result<Option<(String, u32)>, TwitchError>,
    },
    Claim {
        id: String,
        result: Result<bool, TwitchError>,
    },
    Update {
        result: Result<Vec<Channel>, TwitchError>,
        requested_at: Instant,
    },
    Notification(Result<(), TwitchError>),
}
struct CompletedJob {
    epoch: u64,
    kind: JobKind,
    job: Job,
}

struct Mining {
    app: Arc<App>,
    client: TwitchClient,
    journal: Arc<Mutex<ClaimJournal>>,
    intent: watch::Receiver<Intent>,
    seen: Intent,
    events: mpsc::Receiver<Event>,
    campaigns: Vec<Campaign>,
    channels: Vec<Channel>,
    channels_loaded: bool,
    status: InventoryStatus,
    watching: Option<u64>,
    watch_started: Instant,
    manual: Option<ManualSelection>,
    lookup: Option<(String, u64)>,
    manual_pending: Option<String>,
    manual_error: Option<String>,
    jobs: JoinSet<CompletedJob>,
    busy: HashSet<JobKind>,
    watch_abort: Option<tokio::task::AbortHandle>,
    watch_failures: u8,
    epoch: u64,
    refresh: bool,
    channels_dirty: bool,
    publish: bool,
    next_refresh: Instant,
    next_watch: Instant,
    poll_at: Option<Instant>,
    last_progress: Option<(String, Instant)>,
    next_retry: Instant,
    refresh_channels: HashMap<u64, Instant>,
    channel_events: HashMap<u64, Instant>,
    beacon_events: HashMap<u64, Instant>,
    viewer_events: HashMap<u64, Instant>,
    notifications: HashSet<String>,
    claim_retry: HashMap<String, Instant>,
    claim_wait: Option<(String, Instant, u8)>,
    last_inventory: Instant,
    next_progress_refresh: Instant,
    next_transition: Option<chrono::DateTime<Utc>>,
    pending_claims: Vec<PendingClaim>,
}
impl Mining {
    fn restore(&mut self, saved: &Resume) {
        self.manual = saved.manual;
        self.seen = saved.seen.clone();
        self.lookup = saved.lookup.clone();
        self.manual_pending = saved.lookup.as_ref().map(|(login, _)| login.clone());
        if let Some(extra) = &saved.channel {
            // Keep identity, but require fresh stream
            // eligibility in this network generation before watching again.
            self.channels
                .push(Channel::offline(extra.identity.clone(), extra.acl_based));
            self.refresh_channels
                .insert(extra.identity.id, Instant::now());
        }
    }

    fn resume(&self) -> Resume {
        let manual = self.manual;
        Resume {
            manual,
            channel: manual.and_then(|manual| {
                self.channels
                    .iter()
                    .find(|c| c.identity.id == manual.channel)
                    .cloned()
            }),
            lookup: self
                .manual_pending
                .as_ref()
                .map(|login| (login.clone(), self.seen.manual_revision)),
            seen: self.seen.clone(),
        }
    }
    fn new(
        app: Arc<App>,
        client: TwitchClient,
        journal: Arc<Mutex<ClaimJournal>>,
        intent: watch::Receiver<Intent>,
        events: mpsc::Receiver<Event>,
    ) -> Self {
        let now = Instant::now();
        Self {
            app,
            client,
            journal,
            intent,
            seen: Intent::default(),
            events,
            campaigns: vec![],
            channels: vec![],
            channels_loaded: false,
            status: InventoryStatus::default(),
            watching: None,
            watch_started: now,
            manual: None,
            lookup: None,
            manual_pending: None,
            manual_error: None,
            jobs: JoinSet::new(),
            busy: HashSet::new(),
            watch_abort: None,
            watch_failures: 0,
            epoch: 0,
            refresh: true,
            channels_dirty: false,
            publish: false,
            next_refresh: now,
            next_watch: now,
            poll_at: None,
            last_progress: None,
            next_retry: now,
            refresh_channels: HashMap::new(),
            channel_events: HashMap::new(),
            beacon_events: HashMap::new(),
            viewer_events: HashMap::new(),
            notifications: HashSet::new(),
            claim_retry: HashMap::new(),
            claim_wait: None,
            last_inventory: now,
            next_progress_refresh: now,
            next_transition: None,
            pending_claims: vec![],
        }
    }
    fn spawn(&mut self, kind: JobKind, future: impl Future<Output = Job> + Send + 'static) {
        let epoch = self.epoch;
        self.busy.insert(kind);
        let abort = self.jobs.spawn(async move {
            CompletedJob {
                epoch,
                kind,
                job: future.await,
            }
        });
        if kind == JobKind::Watch {
            self.watch_abort = Some(abort);
        }
    }

    async fn run(&mut self, pool: &mut PubSub) -> Result<(), TwitchError> {
        self.apply_intent(pool).await;
        pool.set_channels(
            &self
                .channels
                .iter()
                .map(|c| c.identity.id)
                .collect::<Vec<_>>(),
        );
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let validate_at = Instant::now() + Duration::from_secs(3600);
        loop {
            tokio::select! {biased;
                _=self.client.http.cancel.cancelled()=>return Err(TwitchError::Cancelled),
                changed=self.intent.changed()=>{if changed.is_err(){return Ok(());}self.apply_intent(pool).await;},
                event=self.events.recv()=>{if let Some(event)=event{self.event(event).await?;}},
                result=self.jobs.join_next(),if !self.jobs.is_empty()=>{
                    match result {
                        Some(Ok(completed))=>{
                            self.busy.remove(&completed.kind);
                            if completed.kind==JobKind::Watch{self.watch_abort=None;}
                            if completed.epoch==self.epoch || matches!(completed.job,Job::Claim{..}){self.complete(completed.job,pool).await?;}
                        },
                        Some(Err(error))=>{
                            if !error.is_cancelled(){return Err(TwitchError::InvalidResponse);}
                            self.busy.remove(&JobKind::Watch);self.watch_abort=None;
                        },
                        _=>{},
                    }
                },
                _=tick.tick()=>{},
            }
            if Instant::now() >= validate_at {
                return Ok(());
            }
            if self.next_transition.is_some_and(|at| Utc::now() >= at) {
                self.channels_dirty = true;
                self.publish = true;
                self.set_transition();
            }
            let settings = self.app.snapshot.read().await.settings.values.clone();
            self.reselect(&settings).await;
            if self.publish {
                self.publish(&settings).await?;
                self.publish = false;
            }
            if Instant::now() >= self.next_retry {
                self.schedule(&settings).await;
            }
        }
    }

    async fn apply_intent(&mut self, pool: &PubSub) {
        let intent = self.intent.borrow_and_update().clone();
        if intent.clear != self.seen.clear {
            self.cancel_watch();
            self.epoch = self.epoch.wrapping_add(1);
            self.campaigns.clear();
            self.channels.clear();
            self.channels_loaded = false;
            self.watching = None;
            self.watch_started = Instant::now();
            self.last_progress = None;
            self.watch_failures = 0;
            self.manual = None;
            self.lookup = None;
            self.manual_pending = None;
            self.manual_error = None;
            self.poll_at = None;
            self.claim_wait = None;
            self.refresh_channels.clear();
            self.channel_events.clear();
            self.beacon_events.clear();
            self.viewer_events.clear();
            self.status = InventoryStatus::default();
            pool.set_channels(&[]);
            self.publish = true;
        }
        if intent.refresh != self.seen.refresh {
            self.refresh |=
                intent.clear != self.seen.clear || !self.busy.contains(&JobKind::Inventory);
            self.next_retry = Instant::now();
        }
        if intent.settings != self.seen.settings {
            self.channels_dirty = true;
            self.publish = true;
            let minutes = self
                .app
                .snapshot
                .read()
                .await
                .settings
                .values
                .minimum_refresh_interval_minutes;
            self.next_refresh = self.last_inventory + Duration::from_secs(u64::from(minutes) * 60);
        }
        self.apply_manual(&intent);
        let manual_revision = self.seen.manual_revision;
        self.seen = intent;
        self.seen.manual_revision = manual_revision;
    }

    fn apply_manual(&mut self, intent: &Intent) {
        if intent.manual_revision != self.seen.manual_revision
            && let Some(login) = &intent.channel_login
        {
            self.lookup = Some((login.clone(), intent.manual_revision));
            self.manual_pending = Some(login.clone());
            self.manual_error = None;
            self.seen.manual_revision = intent.manual_revision;
            self.next_retry = Instant::now();
            self.publish = true;
            return;
        }
        if intent.manual_revision != self.seen.manual_revision
            && (intent.selected.is_none() || self.channels_loaded)
        {
            self.lookup = None;
            self.manual_pending = None;
            self.manual_error = None;
            self.manual = intent.selected.and_then(|id| {
                self.channels
                    .iter()
                    .find(|c| c.identity.id == id && c.online())?;
                Some(ManualSelection::new(id, intent.manual_duration))
            });
            self.seen.manual_revision = intent.manual_revision;
            if intent.selected.is_some() && self.manual.is_none() {
                self.manual_error = Some(message("gui.channels.offline", &[]));
            }
            self.publish = true;
        }
    }

    async fn event(&mut self, event: Event) -> Result<(), TwitchError> {
        match event {
            Event::Unauthorized => return Err(TwitchError::Unauthorized),
            Event::Progress { id, minutes } => {
                let settings = self.app.snapshot.read().await.settings.values.clone();
                self.confirm(&id, minutes, &settings);
            }
            Event::Claim { id, instance } => {
                if let Some(drop) = self
                    .campaigns
                    .iter_mut()
                    .flat_map(|c| &mut c.drops)
                    .find(|d| d.id == id)
                {
                    drop.claim_id = Some(instance);
                    self.claim_retry.remove(&id);
                } else {
                    self.refresh = true;
                }
            }
            Event::Notification(id) => {
                self.refresh = true;
                if self.notifications.len() < 256 {
                    self.notifications.insert(id);
                }
            }
            Event::Offline(id) => {
                self.channel_events.insert(id, Instant::now());
                self.beacon_events.insert(id, Instant::now());
                if let Some(channel) = self.channels.iter_mut().find(|c| c.identity.id == id) {
                    channel.broadcast_id = None;
                    channel.game = None;
                    channel.viewers = None;
                    channel.drops_enabled = false;
                    channel.beacon_url = None;
                    self.publish = true;
                }
                self.refresh_channels.remove(&id);
            }
            Event::Changed(id) => {
                self.channel_events.insert(id, Instant::now());
                self.beacon_events.insert(id, Instant::now());
                if let Some(channel) = self.channels.iter_mut().find(|c| c.identity.id == id) {
                    // The old category is no longer evidence of eligibility.
                    channel.broadcast_id = None;
                    channel.game = None;
                    channel.drops_enabled = false;
                    channel.beacon_url = None;
                    self.publish = true;
                    self.refresh_channels
                        .insert(id, Instant::now() + CHANNEL_DELAY);
                }
            }
            Event::Viewers { id, count } => {
                self.viewer_events.insert(id, Instant::now());
                if let Some(channel) = self.channels.iter_mut().find(|c| c.identity.id == id) {
                    if channel.online() {
                        channel.viewers = Some(count);
                        let view = {
                            let mut state = self.app.snapshot.write().await;
                            state.channels.iter_mut().find(|c| c.id == id).map(|c| {
                                c.viewers = Some(count);
                                c.clone()
                            })
                        };
                        if let Some(view) = view {
                            self.app.sockets.emit("channel_update", &view).await;
                        }
                    } else {
                        self.refresh_channels
                            .entry(id)
                            .or_insert(Instant::now() + CHANNEL_DELAY);
                    }
                }
            }
        }
        Ok(())
    }
    fn confirm(&mut self, id: &str, minutes: u32, settings: &Settings) -> bool {
        let eligible = self
            .watching
            .is_some_and(|channel| self.progress_eligible(id, channel, settings));
        if let Some(drop) = self
            .campaigns
            .iter_mut()
            .flat_map(|c| &mut c.drops)
            .find(|d| d.id == id)
        {
            // Delayed progress cannot undo confirmed completion or restore an old card.
            if minutes < drop.confirmed_minutes {
                return false;
            }
            let advanced = minutes > drop.confirmed_minutes;
            drop.confirm(minutes, Utc::now());
            let completed = !drop.claimed
                && drop.watch_reward()
                && drop.confirmed_minutes >= drop.required_minutes;
            let reported = self
                .watching
                .and_then(|channel| self.reported_drop(id, channel, settings));
            let blocked = reported.is_some_and(|(c, d)| !c.prerequisites_met(d));
            if reported.is_some() && (advanced || self.last_progress.is_none())
                || completed
                    && eligible
                    && self
                        .last_progress
                        .as_ref()
                        .is_none_or(|(previous, _)| previous == id)
            {
                self.last_progress = Some((id.to_owned(), Instant::now()));
            }
            if completed || blocked {
                self.request_progress_refresh();
            }
            self.publish = true;
            true
        } else {
            self.request_progress_refresh();
            false
        }
    }
    fn request_progress_refresh(&mut self) {
        // Unknown rewards and watched completion both need account inventory evidence.
        if Instant::now() >= self.next_progress_refresh {
            self.next_progress_refresh = Instant::now() + Duration::from_secs(60);
            self.refresh = true;
        }
    }
    fn progress_eligible(&self, id: &str, channel: u64, settings: &Settings) -> bool {
        let now = Utc::now();
        self.reported_drop(id, channel, settings)
            .is_some_and(|(c, d)| {
                self.manual.is_some()
                    || c.drop_eligible(
                        d,
                        &c.mining_policy(settings, now),
                        now,
                        now + chrono::Duration::nanoseconds(1),
                    )
            })
    }
    fn reported_drop(
        &self,
        id: &str,
        channel: u64,
        settings: &Settings,
    ) -> Option<(&Campaign, &Drop)> {
        let now = Utc::now();
        let channel = self.channels.iter().find(|c| c.identity.id == channel)?;
        self.campaigns
            .iter()
            .filter(|c| c.active(now) && c.matches_channel(channel))
            .find_map(|c| {
                c.drops
                    .iter()
                    .find(|d| {
                        d.id == id
                            && !d.claimed
                            && d.confirmed_minutes < d.required_minutes
                            && d.starts_at <= now
                            && now < d.ends_at
                            && if self.manual.is_some() {
                                c.prerequisites_met(d)
                            } else {
                                c.mining_policy(settings, now).mineable.contains(id)
                            // Actual successor progress can precede local prerequisite claim evidence.
                            && (c.prerequisites_met(d) || d.confirmed_minutes > 0)
                            }
                    })
                    .map(|d| (c, d))
            })
    }

    async fn reselect(&mut self, settings: &Settings) {
        let now = Utc::now();
        if self
            .manual
            .is_some_and(|manual| manual.expires_at.is_some_and(|at| Instant::now() >= at))
        {
            self.manual = None;
            self.publish = true;
        }
        let next = select_channel(
            &self.channels,
            &self.campaigns,
            settings,
            now,
            self.watching,
            self.manual.map(|manual| manual.channel),
        );
        if next != self.watching {
            self.cancel_watch();
            self.watching = next;
            self.watch_started = Instant::now();
            self.watch_failures = 0;
            self.next_watch = Instant::now();
            self.poll_at = None;
            self.last_progress = None;
            self.claim_wait = None;
            self.publish = true;
            if let Some(channel) = self.channels.iter().find(|c| Some(c.identity.id) == next) {
                let status = message("status.watching", &[("channel", &channel.identity.name)]);
                self.app.status(status.clone()).await;
                self.app.console(status).await;
            }
        }
    }

    async fn schedule(&mut self, settings: &Settings) {
        let now = Instant::now();
        let wall = Utc::now();
        // Retry delayed/missing claim evidence even if completion left no watchable channel.
        if now >= self.next_progress_refresh
            && self.campaigns.iter().any(|c| c.needs_claim_refresh(wall))
        {
            self.request_progress_refresh();
        }
        if !self.busy.contains(&JobKind::Manual)
            && let Some((login, revision)) = self.lookup.take()
        {
            let client = self.client.clone();
            self.spawn(JobKind::Manual, async move {
                let requested_at = Instant::now();
                let result =
                    tokio::time::timeout(Duration::from_secs(30), client.resolve_channel(&login))
                        .await
                        .unwrap_or(Err(TwitchError::Network));
                Job::Manual {
                    revision,
                    requested_at,
                    result: Box::new(result),
                }
            });
        }
        if ![JobKind::Claim, JobKind::Watch, JobKind::Poll]
            .iter()
            .any(|kind| self.busy.contains(kind))
        {
            if let Some((campaign, drop)) = self
                .campaigns
                .iter()
                .filter(|c| !c.upcoming(wall))
                .flat_map(|c| c.drops.iter().map(move |d| (c, d)))
                .find(|(c, d)| {
                    d.can_claim(c.ends_at, wall)
                        && self.claim_retry.get(&d.id).is_none_or(|at| now >= *at)
                })
            {
                let id = drop.id.clone();
                let pending = PendingClaim::new(self.client.user_id, campaign, drop, settings);
                let client = self.client.clone();
                let app = self.app.clone();
                let journal = self.journal.clone();
                self.spawn(JobKind::Claim, async move {
                    let result = claim(&app, &client, &journal, pending).await;
                    Job::Claim { id, result }
                });
                return;
            }
            if let Some(pending) = self
                .pending_claims
                .iter()
                .find(|p| {
                    wall < p.retry_until
                        && self
                            .claim_retry
                            .get(&p.entry.id)
                            .is_none_or(|at| now >= *at)
                })
                .cloned()
            {
                let client = self.client.clone();
                let app = self.app.clone();
                let journal = self.journal.clone();
                let id = pending.entry.id.clone();
                self.spawn(JobKind::Claim, async move {
                    let result = claim(&app, &client, &journal, pending).await;
                    Job::Claim { id, result }
                });
                return;
            }
            if let Some(channel) = self.watching {
                if self
                    .claim_wait
                    .as_ref()
                    .is_some_and(|(_, due, _)| now >= *due)
                    || self.poll_at.is_some_and(|at| now >= at)
                {
                    self.poll_at = None;
                    if self.claim_wait.is_some()
                        || self.last_progress.as_ref().is_none_or(|(id, at)| {
                            now.duration_since(*at) >= WATCH_INTERVAL
                                || !self.progress_eligible(id, channel, settings)
                        })
                    {
                        let client = self.client.clone();
                        self.spawn(JobKind::Poll, async move {
                            Job::Poll {
                                channel,
                                requested_at: Instant::now(),
                                result: client.current_drop(channel).await,
                            }
                        });
                        return;
                    }
                }
                if self.claim_wait.is_none()
                    && now >= self.next_watch
                    && let Some(mut channel) = self
                        .channels
                        .iter()
                        .find(|c| {
                            c.identity.id == channel
                                && c.online()
                                && (self.manual.is_some_and(|manual| {
                                    manual.channel == channel
                                        && manual.expires_at.is_none_or(|at| now < at)
                                }) || self
                                    .campaigns
                                    .iter()
                                    .any(|campaign| campaign.can_watch(c, settings, wall)))
                        })
                        .cloned()
                {
                    self.next_watch = now + WATCH_INTERVAL;
                    let client = self.client.clone();
                    let requested_at = now;
                    self.spawn(JobKind::Watch, async move {
                        let result = client.send_watch(&mut channel, Utc::now()).await;
                        Job::Watch {
                            channel: Box::new(channel),
                            result,
                            requested_at,
                            at: Instant::now(),
                        }
                    });
                    return;
                }
            }
        }
        let manual_update = self
            .manual
            .map(|manual| manual.channel)
            .filter(|id| self.refresh_channels.get(id).is_some_and(|at| now >= *at));
        let updates: Vec<_> = self
            .channels
            .iter()
            .filter(|c| {
                manual_update.is_none_or(|id| c.identity.id == id)
                    && self
                        .refresh_channels
                        .get(&c.identity.id)
                        .is_some_and(|at| now >= *at)
            })
            .cloned()
            .collect();
        if !updates.is_empty() && !self.busy.contains(&JobKind::Update) {
            let client = self.client.clone();
            self.spawn(JobKind::Update, async move {
                let mut updates = updates;
                let requested_at = Instant::now();
                let result = if manual_update.is_some() {
                    client.update_manual_channel(&mut updates[0]).await
                } else {
                    client.update_channels(&mut updates).await
                }
                .map(|()| updates);
                Job::Update {
                    result,
                    requested_at,
                }
            });
            return;
        }
        if [
            JobKind::Inventory,
            JobKind::Channels,
            JobKind::Update,
            JobKind::Notification,
        ]
        .iter()
        .any(|kind| self.busy.contains(kind))
        {
            return;
        }
        if self.refresh || now >= self.next_refresh {
            self.refresh = false;
            self.next_progress_refresh = now + Duration::from_secs(60);
            let (refresh_sequence, _) = self.app.begin_inventory_refresh().await;
            let client = self.client.clone();
            self.app
                .status(message("gui.status.fetching_inventory", &[]))
                .await;
            self.spawn(JobKind::Inventory, async move {
                let requested_at = Utc::now();
                Job::Inventory {
                    result: client.inventory().await,
                    requested_at,
                    refresh_sequence,
                }
            });
            return;
        }
        if self.channels_dirty {
            self.channels_dirty = false;
            let client = self.client.clone();
            let campaigns = self.campaigns.clone();
            let settings = settings.clone();
            let current = self
                .channels
                .iter()
                .find(|c| {
                    Some(c.identity.id)
                        == self.watching.or(self.manual.map(|manual| manual.channel))
                })
                .cloned();
            self.app.status(message("gui.status.gathering", &[])).await;
            self.spawn(JobKind::Channels, async move {
                Job::Channels {
                    requested_at: Instant::now(),
                    result: client
                        .channels(&campaigns, &settings, current.as_ref())
                        .await,
                }
            });
            return;
        }
        if let Some(id) = self.notifications.iter().next().cloned() {
            self.notifications.remove(&id);
            let client = self.client.clone();
            self.spawn(JobKind::Notification, async move {
                Job::Notification(client.delete_notification(&id).await)
            });
        }
    }

    async fn complete(&mut self, job: Job, pool: &PubSub) -> Result<(), TwitchError> {
        let settings = self.app.snapshot.read().await.settings.values.clone();
        let now = Instant::now();
        let inventory_failed = matches!(&job, Job::Inventory { result: Err(_), .. });
        let error = match job {
            Job::Manual {
                revision,
                requested_at,
                result,
            } => {
                let result = *result;
                if matches!(
                    result,
                    Err(TwitchError::Unauthorized | TwitchError::Cancelled)
                ) {
                    return Err(result.err().unwrap());
                }
                if self.client.http.cancel.is_cancelled() {
                    return Err(TwitchError::Cancelled);
                }
                let receiver = self.intent.clone();
                let intent = receiver.borrow();
                if intent.manual_revision == revision {
                    let error = self.finish_manual(result, requested_at, intent.manual_duration);
                    self.lookup = None;
                    self.manual_pending = None;
                    self.manual_error = error.map(|key| message(key, &[]));
                    self.publish = true;
                    pool.set_channels(
                        &self
                            .channels
                            .iter()
                            .map(|c| c.identity.id)
                            .collect::<Vec<_>>(),
                    );
                }
                None
            }
            Job::Inventory {
                result: Ok(mut inventory),
                requested_at,
                refresh_sequence,
            } => {
                if !inventory.status.available {
                    for previous in &self.campaigns {
                        if (previous.active(Utc::now())
                            || previous.upcoming(Utc::now())
                            || previous.needs_claim_refresh(Utc::now()))
                            && !inventory.campaigns.iter().any(|c| c.id == previous.id)
                        {
                            inventory.campaigns.push(previous.clone());
                        }
                    }
                }
                for campaign in &mut inventory.campaigns {
                    for drop in &mut campaign.drops {
                        // An issued claim instance remains valid until claimed. Catalog
                        // refreshes often lag the account's claim-ready event.
                        if !drop.claimed && drop.claim_id.is_none() {
                            drop.claim_id = self
                                .campaigns
                                .iter()
                                .find(|c| c.id == campaign.id)
                                .and_then(|c| c.drops.iter().find(|d| d.id == drop.id))
                                .and_then(|d| d.claim_id.clone());
                        }
                        if let Some(previous) = self
                            .campaigns
                            .iter()
                            .find(|c| c.id == campaign.id)
                            .and_then(|c| c.drops.iter().find(|d| d.id == drop.id))
                            && !drop.claimed
                            && previous.required_minutes == drop.required_minutes
                        {
                            if previous
                                .confirmed_at
                                .is_some_and(|at| at > requested_at || drop.confirmed_at.is_none())
                            {
                                drop.confirmed_minutes = previous.confirmed_minutes;
                                drop.confirmed_at = previous.confirmed_at;
                                drop.estimated_minutes = previous.estimated_minutes;
                                drop.claimed = previous.claimed;
                                drop.claimed_at = previous.claimed_at;
                                if previous.claim_id.is_some() {
                                    drop.claim_id = previous.claim_id.clone();
                                }
                            } else if !previous.claimed
                                && previous.confirmed_minutes > drop.confirmed_minutes
                            {
                                // Inventory can lag CurrentDrop/PubSub even when requested later.
                                // Retain watch evidence, but keep new claim state and instance IDs.
                                drop.confirmed_minutes = previous.confirmed_minutes;
                                drop.confirmed_at = previous.confirmed_at;
                            }
                        }
                        // A completed refresh must release the estimate ceiling, including
                        // retained records, without overwriting newer account evidence.
                        if drop.estimated_minutes >= MAX_ESTIMATED_MINUTES
                            && drop.confirmed_at.is_none_or(|at| at <= requested_at)
                        {
                            drop.estimated_minutes = 0;
                        }
                    }
                }
                self.campaigns = inventory.campaigns;
                self.status = inventory.status;
                self.recover_claims(&inventory.awards).await?;
                let observed_at = Utc::now();
                let entries = self
                    .campaigns
                    .iter()
                    .flat_map(|campaign| {
                        campaign
                            .drops
                            .iter()
                            .filter(|drop| drop.claimed)
                            .map(move |drop| {
                                let mut entry = campaign
                                    .history_entry(drop, drop.claimed_at.unwrap_or(observed_at));
                                entry.claimed_at_is_observed = drop.claimed_at.is_none();
                                entry
                            })
                    })
                    .collect();
                let app = self.app.clone();
                tokio::task::spawn_blocking(move || {
                    app.history.blocking_lock().import_claims(entries)
                })
                .await
                .map_err(|_| TwitchError::Storage)?
                .map_err(|_| TwitchError::Storage)?;
                self.last_inventory = now;
                self.set_transition();
                self.next_refresh = now
                    + Duration::from_secs(
                        u64::from(settings.minimum_refresh_interval_minutes) * 60,
                    );
                self.channels_dirty = true;
                self.publish(&settings).await?;
                self.publish = false;
                self.app
                    .finish_inventory_refresh(
                        refresh_sequence,
                        (!self.status.available)
                            .then(|| message("gui.redesign.campaigns_unavailable", &[])),
                    )
                    .await;
                None
            }
            Job::Channels {
                result: Ok(mut channels),
                requested_at,
            } => {
                self.preserve_channel_events(&mut channels, requested_at);
                if let Some(ManualSelection { channel: id, .. }) = self.manual
                    && !channels.iter().any(|c| c.identity.id == id)
                    && let Some(current) = self.channels.iter().find(|c| c.identity.id == id)
                {
                    channels.insert(0, current.clone());
                    channels.truncate(crate::twitch::channels::MAX_CHANNELS);
                }
                self.channels = channels;
                self.channels_loaded = true;
                let intent = self.intent.borrow().clone();
                self.apply_manual(&intent);
                self.channel_events
                    .retain(|id, _| self.channels.iter().any(|c| c.identity.id == *id));
                self.beacon_events
                    .retain(|id, _| self.channels.iter().any(|c| c.identity.id == *id));
                self.viewer_events
                    .retain(|id, _| self.channels.iter().any(|c| c.identity.id == *id));
                self.refresh_channels
                    .retain(|id, _| self.channels.iter().any(|c| c.identity.id == *id));
                pool.set_channels(
                    &self
                        .channels
                        .iter()
                        .map(|c| c.identity.id)
                        .collect::<Vec<_>>(),
                );
                self.publish = true;
                if self.watching.is_none() {
                    self.idle_status(&settings).await;
                }
                None
            }
            Job::Watch {
                channel,
                result,
                requested_at,
                at,
            } => {
                if matches!(
                    result,
                    Err(TwitchError::Unauthorized | TwitchError::Cancelled)
                ) {
                    return Err(result.err().unwrap());
                }
                let current = self.channels.iter_mut().find(|c| {
                    self.watching == Some(c.identity.id)
                        && c.identity.id == channel.identity.id
                        && c.broadcast_id == channel.broadcast_id
                        && self
                            .beacon_events
                            .get(&c.identity.id)
                            .is_none_or(|at| *at <= requested_at)
                });
                let Some(current) = current else {
                    return Ok(());
                };
                // Persist failure invalidation too, and prevent an older stream
                // refresh from restoring the stale beacon cached in its clone.
                if current.beacon_url != channel.beacon_url {
                    current.beacon_url = channel.beacon_url;
                    self.beacon_events.insert(current.identity.id, at);
                }
                if result == Ok(true) {
                    self.watch_failures = 0;
                    self.next_watch = at + WATCH_INTERVAL;
                    self.poll_at = Some(at + PROGRESS_DELAY);
                    None
                } else {
                    self.refresh_channels.insert(channel.identity.id, now);
                    self.watch_failures += 1;
                    let error = result.err().unwrap_or(TwitchError::Network);
                    if self.watch_failures >= 3 {
                        tracing::warn!(
                            failures = self.watch_failures,
                            "Repeated watch failures; renewing Twitch connections"
                        );
                        return Err(error);
                    }
                    Some(error)
                }
            }
            Job::Poll {
                channel,
                result,
                requested_at,
            } => {
                if self.watching == Some(channel)
                    && requested_at >= self.watch_started
                    && requested_at >= self.last_inventory
                    && self
                        .channel_events
                        .get(&channel)
                        .is_none_or(|at| *at <= requested_at)
                {
                    let current = result.as_ref().ok().and_then(|v| v.as_ref());
                    let newer_progress = self
                        .last_progress
                        .as_ref()
                        .is_some_and(|(_, at)| *at > requested_at);
                    let confirmed = newer_progress
                        || current.is_some_and(|(id, minutes)| {
                            let accepted = self.confirm(id, *minutes, &settings);
                            if accepted && self.reported_drop(id, channel, &settings).is_some() {
                                self.last_progress = Some((id.clone(), now));
                            }
                            accepted && self.progress_eligible(id, channel, &settings)
                        });
                    if let Some((claimed, _, attempts)) = self.claim_wait.take() {
                        if current.is_some_and(|(id, _)| id == &claimed) && attempts < 7 {
                            self.claim_wait =
                                Some((claimed, now + Duration::from_secs(2), attempts + 1));
                        } else {
                            self.next_watch = now;
                        }
                    } else if !confirmed && self.manual.is_none() {
                        let watching = self.channels.iter().find(|c| c.identity.id == channel);
                        for campaign in &mut self.campaigns {
                            if watching
                                .is_some_and(|c| campaign.can_watch(c, &settings, Utc::now()))
                                && campaign.bump_estimates(&settings, Utc::now())
                            {
                                self.refresh = true;
                                self.channels_dirty = true;
                            }
                        }
                        self.publish = true;
                    }
                }
                result.err()
            }
            Job::Claim {
                id,
                result: Ok(true),
            } => {
                if let Some(drop) = self
                    .campaigns
                    .iter_mut()
                    .flat_map(|c| &mut c.drops)
                    .find(|d| d.id == id)
                {
                    drop.mark_claimed(Utc::now());
                    self.app
                        .console(message("status.claimed_drop", &[("drop", &drop.name)]))
                        .await;
                    self.app.sockets.emit("notification",&json!({"title":message("gui.backend.drop_claimed",&[]),"message":drop.name})).await;
                }
                self.claim_retry.remove(&id);
                self.recover_claims(&HashMap::new()).await?;
                self.claim_wait = Some((id, now + Duration::from_secs(4), 0));
                self.publish = true;
                None
            }
            Job::Claim { id, result } => {
                self.claim_retry.insert(id, now + Duration::from_secs(60));
                result.err()
            }
            Job::Update {
                result: Ok(mut updated),
                requested_at,
            } => {
                self.preserve_channel_events(&mut updated, requested_at);
                for channel in updated {
                    if self
                        .refresh_channels
                        .get(&channel.identity.id)
                        .is_some_and(|due| *due <= requested_at)
                    {
                        self.refresh_channels.remove(&channel.identity.id);
                    }
                    if let Some(current) = self
                        .channels
                        .iter_mut()
                        .find(|c| c.identity.id == channel.identity.id)
                    {
                        *current = channel;
                        self.channel_events
                            .entry(current.identity.id)
                            .and_modify(|at| *at = (*at).max(requested_at))
                            .or_insert(requested_at);
                    }
                }
                self.publish = true;
                None
            }
            Job::Notification(result) => result.err(),
            Job::Inventory {
                result: Err(error),
                refresh_sequence,
                ..
            } => {
                self.app
                    .finish_inventory_refresh(
                        refresh_sequence,
                        Some(message("gui.redesign.refresh_failed_detail", &[])),
                    )
                    .await;
                self.refresh = false;
                self.next_refresh = now + Duration::from_secs(60);
                self.next_progress_refresh = self.next_refresh;
                Some(error)
            }
            Job::Channels {
                result: Err(error), ..
            } => {
                self.channels_dirty = true;
                Some(error)
            }
            Job::Update {
                result: Err(error), ..
            } => Some(error),
        };
        if let Some(error) = error {
            if matches!(error, TwitchError::Unauthorized | TwitchError::Cancelled) {
                return Err(error);
            }
            if !inventory_failed {
                self.next_retry = now + Duration::from_secs(10);
            }
            self.app
                .console(message(
                    "gui.backend.twitch_error",
                    &[("error", &error.to_string())],
                ))
                .await;
        }
        Ok(())
    }

    fn cancel_watch(&self) {
        if let Some(watch) = &self.watch_abort {
            watch.abort();
        }
    }

    fn finish_manual(
        &mut self,
        result: Result<Option<Channel>, TwitchError>,
        requested_at: Instant,
        duration: Option<Duration>,
    ) -> Option<&'static str> {
        let mut resolved = match result {
            Ok(Some(resolved)) => resolved,
            Ok(None) => return Some("gui.channels.not_found"),
            Err(_) => return Some("gui.channels.lookup_failed"),
        };
        self.preserve_channel_events(std::slice::from_mut(&mut resolved), requested_at);
        if !resolved.online() {
            return Some("gui.channels.offline");
        }
        let id = resolved.identity.id;
        self.channels.retain(|c| c.identity.id != id);
        self.channels.insert(0, resolved.clone());
        self.channels
            .truncate(crate::twitch::channels::MAX_CHANNELS);
        self.channel_events.insert(id, Instant::now());
        self.beacon_events.insert(id, Instant::now());
        self.manual = Some(ManualSelection::new(id, duration));
        self.channels_dirty = true;
        None
    }
    fn preserve_channel_events(&mut self, channels: &mut [Channel], requested_at: Instant) {
        for channel in channels {
            if channel.online()
                && self
                    .viewer_events
                    .get(&channel.identity.id)
                    .is_some_and(|at| *at > requested_at)
                && let Some(current) = self
                    .channels
                    .iter()
                    .find(|c| c.identity.id == channel.identity.id)
            {
                channel.viewers = current.viewers;
            }
            if self
                .channel_events
                .get(&channel.identity.id)
                .is_some_and(|at| *at > requested_at)
                && let Some(current) = self
                    .channels
                    .iter()
                    .find(|c| c.identity.id == channel.identity.id)
            {
                *channel = current.clone();
            }
            // Stream metadata never discovers beacon addresses. For the same
            // broadcast, the owner has the latest acknowledgement/invalidation.
            if let Some(current) = self.channels.iter().find(|c| {
                c.identity.id == channel.identity.id && c.broadcast_id == channel.broadcast_id
            }) {
                channel.beacon_url = current.beacon_url.clone();
            }
            if self.watching == Some(channel.identity.id)
                && self.channels.iter().any(|c| {
                    c.identity.id == channel.identity.id && c.broadcast_id != channel.broadcast_id
                })
            {
                // A new broadcast on the same channel also starts a new watch context.
                self.cancel_watch();
                self.watch_started = Instant::now();
                self.next_watch = self.watch_started;
                self.watch_failures = 0;
                self.last_progress = None;
                self.poll_at = None;
                self.claim_wait = None;
            }
        }
    }
    fn set_transition(&mut self) {
        let now = Utc::now();
        self.next_transition = self
            .campaigns
            .iter()
            .flat_map(|c| {
                [c.starts_at, c.ends_at]
                    .into_iter()
                    .chain(c.drops.iter().flat_map(|d| [d.starts_at, d.ends_at]))
            })
            .flat_map(|at| [at - chrono::Duration::hours(1), at])
            .filter(|at| *at > now)
            .min();
    }

    async fn idle_status(&self, settings: &Settings) {
        let key = if settings.games_to_watch.is_empty()
            && !settings.auto_mine_badges
            && !settings.auto_mine_emotes
        {
            "status.no_selection"
        } else if !self.campaigns.iter().any(|c| {
            c.can_earn_within(
                settings,
                Utc::now(),
                Utc::now() + chrono::Duration::hours(1),
            )
        }) {
            if self.status.available {
                "status.no_campaign"
            } else {
                "status.catalog_unavailable"
            }
        } else {
            "status.no_channel"
        };
        let text = message(key, &[]);
        self.app.status(text.clone()).await;
        self.app.console(text).await;
    }

    async fn publish(&self, settings: &Settings) -> Result<(), TwitchError> {
        let now = Utc::now();
        let live: Vec<_> = self
            .campaigns
            .iter()
            .filter(|c| c.drops.iter().any(|d| d.watch_reward()))
            .map(|c| c.view(settings, now))
            .collect();
        let app = self.app.clone();
        let campaigns = tokio::task::spawn_blocking(move || {
            let mut archive = app.archive.blocking_lock();
            archive.update(&live)?;
            Ok::<_, anyhow::Error>(archive.merge(live, now))
        })
        .await
        .map_err(|_| TwitchError::Storage)?
        .map_err(|_| TwitchError::Storage)?;
        let mineable: Vec<_> = self
            .campaigns
            .iter()
            .filter(|c| c.can_mine(settings, now))
            .collect();
        let channels: Vec<_> = self
            .channels
            .iter()
            .filter(|channel| {
                self.manual
                    .is_some_and(|manual| manual.channel == channel.identity.id)
                    || mineable.iter().any(|c| c.matches_channel(channel))
            })
            .map(|c| c.view(self.watching))
            .collect();
        let wanted = wanted_items(&self.campaigns, settings, now);
        let active = self
            .channels
            .iter()
            .find(|c| Some(c.identity.id) == self.watching)
            .and_then(|channel| {
                let reported = self
                    .last_progress
                    .as_ref()
                    .and_then(|(id, _)| self.reported_drop(id, channel.identity.id, settings));
                if self.manual.is_some() {
                    return reported;
                }
                reported.or_else(|| {
                    self.campaigns
                        .iter()
                        .filter(|c| c.can_watch(channel, settings, now))
                        .filter_map(|c| c.first_drop(settings, now).map(|d| (c, d)))
                        .min_by_key(|(c, d)| (c.mining_priority(settings), d.remaining_minutes()))
                })
            });
        let progress = active.map(|(c, d)| c.progress(d));
        let mut manual = self
            .manual
            .map(|manual| {
                let channel = self
                    .channels
                    .iter()
                    .find(|c| c.identity.id == manual.channel);
                ManualMode {
                    active: true,
                    game_name: channel
                        .and_then(|c| c.game.as_ref())
                        .map(|g| g.name.clone()),
                    channel_name: channel.map(|c| c.identity.name.clone()),
                    expires_at: manual.expires_at.map(|at| {
                        now + chrono::Duration::from_std(
                            at.saturating_duration_since(Instant::now()),
                        )
                        .unwrap()
                    }),
                    ..ManualMode::default()
                }
            })
            .unwrap_or_default();
        manual.pending_channel = self.manual_pending.clone();
        manual.error = self.manual_error.clone();
        let games: Vec<_> = self
            .campaigns
            .iter()
            .map(|c| (c.game.name.clone(), ()))
            .collect::<BTreeMap<_, _>>()
            .into_keys()
            .collect();
        {
            let mut state = self.app.snapshot.write().await;
            state.campaigns = campaigns.clone();
            state.channels = channels.clone();
            state.current_drop = progress.clone();
            state.wanted_items = wanted.clone();
            state.manual_mode = manual.clone();
            state.inventory_status = self.status.clone();
            state.settings.games_available = games.clone();
        }
        self.app
            .sockets
            .emit("inventory_batch_update", &json!({"campaigns":campaigns}))
            .await;
        self.app
            .sockets
            .emit("channels_batch_update", &json!({"channels":channels}))
            .await;
        self.app.sockets.emit("wanted_items_update", &wanted).await;
        self.app.sockets.emit("manual_mode_update", &manual).await;
        self.app
            .sockets
            .emit("inventory_status", &self.status)
            .await;
        self.app
            .sockets
            .emit("games_available", &json!({"games":games}))
            .await;
        if let Some(progress) = progress {
            self.app.sockets.emit("drop_progress", &progress).await;
        } else {
            self.app
                .sockets
                .emit("drop_progress_stop", &json!({}))
                .await;
        }
        Ok(())
    }

    async fn recover_claims(
        &mut self,
        awards: &HashMap<String, chrono::DateTime<Utc>>,
    ) -> Result<(), TwitchError> {
        let unclaimed: HashSet<_> = self
            .campaigns
            .iter()
            .flat_map(|c| &c.drops)
            .filter(|d| !d.claimed && d.confirmed_at.is_some())
            .map(|d| d.id.clone())
            .collect();
        let confirmed: HashSet<_> = self
            .campaigns
            .iter()
            .flat_map(|c| &c.drops)
            .filter(|d| d.claimed)
            .map(|d| d.id.clone())
            .collect();
        let app = self.app.clone();
        let journal = self.journal.clone();
        let user_id = self.client.user_id;
        let awards = awards.clone();
        let (pending, recovered) = tokio::task::spawn_blocking(move || {
            let mut journal = journal.blocking_lock();
            let mut recovered = HashSet::new();
            for pending in journal.pending(user_id).into_iter().filter(|p| {
                p.confirmed
                    || confirmed.contains(&p.entry.id)
                    || (!unclaimed.contains(&p.entry.id) && p.confirmed_by(&awards))
            }) {
                record_claim(&app, &pending)?;
                journal.finish(user_id, &pending.entry.id)?;
                recovered.insert(pending.entry.id.clone());
            }
            Ok::<_, anyhow::Error>((journal.pending(user_id), recovered))
        })
        .await
        .map_err(|_| TwitchError::Storage)?
        .map_err(|_| TwitchError::Storage)?;
        self.pending_claims = pending;
        for drop in self.campaigns.iter_mut().flat_map(|c| &mut c.drops) {
            if recovered.contains(&drop.id) {
                drop.mark_claimed(Utc::now());
            }
        }
        Ok(())
    }
}

async fn claim(
    app: &Arc<App>,
    client: &TwitchClient,
    journal: &Arc<Mutex<ClaimJournal>>,
    pending_claim: PendingClaim,
) -> Result<bool, TwitchError> {
    if pending_claim.user_id != client.user_id {
        return Err(TwitchError::Unauthorized);
    }
    let pending = journal.clone();
    let user_id = client.user_id;
    let pending_claim =
        tokio::task::spawn_blocking(move || pending.blocking_lock().prepare(pending_claim))
            .await
            .map_err(|_| TwitchError::Storage)?
            .map_err(|_| TwitchError::Storage)?;
    let claimed = pending_claim.confirmed || client.claim(&pending_claim.instance).await?;
    let app = app.clone();
    let journal = journal.clone();
    tokio::task::spawn_blocking(move || {
        if claimed {
            // Keep the receipt until the owner applies the result or reconciles
            // it after restart. A preclaim publication cannot destroy its evidence.
            journal
                .blocking_lock()
                .confirm(user_id, &pending_claim.entry.id)?;
            app.history
                .blocking_lock()
                .record(pending_claim.entry.clone())?;
        } else {
            journal
                .blocking_lock()
                .finish(user_id, &pending_claim.entry.id)?;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|_| TwitchError::Storage)?
    .map_err(|_| TwitchError::Storage)?;
    Ok(claimed)
}

fn record_claim(app: &App, claim: &PendingClaim) -> anyhow::Result<()> {
    app.history.blocking_lock().record(claim.entry.clone())?;
    if let Some(completed) = &claim.completed_campaign {
        app.archive
            .blocking_lock()
            .update(std::slice::from_ref(completed))?;
    }
    Ok(())
}
