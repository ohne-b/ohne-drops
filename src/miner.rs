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
    domain::{Campaign, Channel, wanted_items},
    dto::{HistoryEntry, InventoryStatus, Login, ManualMode},
    store::ClaimJournal,
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
const CHANNEL_DELAY: Duration = Duration::from_secs(120);

#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
struct Intent {
    refresh: u64,
    clear: u64,
    settings: u64,
    manual_revision: u64,
    selected: Option<u64>,
}

struct Generation {
    cancel: CancellationToken,
    confirmed: Arc<Notify>,
    intent: watch::Sender<Intent>,
    task: JoinHandle<Result<(), TwitchError>>,
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

    fn start(&self, settings: Settings) -> Generation {
        let cancel = CancellationToken::new();
        let confirmed = Arc::new(Notify::new());
        let (intent, receiver) = watch::channel(Intent::default());
        let app = self.app.clone();
        let endpoints = self.endpoints.clone();
        let stopped = cancel.clone();
        let confirmation = confirmed.clone();
        let task = tokio::spawn(async move {
            run_generation(app, settings, endpoints, stopped, confirmation, receiver).await
        });
        Generation {
            cancel,
            confirmed,
            intent,
            task,
        }
    }

    pub async fn run(mut self) -> Result<(), TwitchError> {
        while !self.app.shutdown.is_cancelled() {
            let settings = self.app.snapshot.read().await.settings.values.clone();
            let mut generation = self.start(settings.clone());
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
                            Command::Logout=>{self.logout(generation,request.complete).await;break;},
                            Command::Shutdown=>{
                                self.app.shutdown.cancel();generation.cancel.cancel();let _=generation.task.await;
                                let _=request.complete.send(Ok(()));return Ok(());
                            },
                            Command::ConfirmOAuth=>{generation.confirmed.notify_one();let _=request.complete.send(Ok(()));},
                            Command::SettingsChanged=>{
                                let current=self.app.snapshot.read().await.settings.values.clone();
                                if current.proxy!=settings.proxy || current.connection_quality!=settings.connection_quality {
                                    generation.cancel.cancel();let _=generation.task.await;
                                    let _=request.complete.send(Ok(()));break;
                                }
                                generation.intent.send_modify(|intent|intent.settings=intent.settings.wrapping_add(1));
                                let _=request.complete.send(Ok(()));
                            },
                            command=>{
                                generation.intent.send_modify(|intent|match command {
                                    Command::Refresh{clear_cache}=>{intent.refresh=intent.refresh.wrapping_add(1);if clear_cache{intent.clear=intent.clear.wrapping_add(1);}},
                                    Command::SelectChannel(id)=>{intent.selected=Some(id);intent.manual_revision=intent.manual_revision.wrapping_add(1);},
                                    Command::ExitManual=>{intent.selected=None;intent.manual_revision=intent.manual_revision.wrapping_add(1);},
                                    _=>{},
                                });
                                let _=request.complete.send(Ok(()));
                            },
                        }
                    },
                    result=&mut generation.task=>{
                        match result {
                            Ok(Err(TwitchError::Unauthorized))=>{
                                if remove_session(&self.app).await.is_err(){self.app.console(message("gui.backend.session_storage",&[])).await;}
                                reset_session(&self.app).await;
                            },
                            Ok(Err(TwitchError::Cancelled))|Ok(Ok(()))=>{},
                            Ok(Err(error))=>{
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
            Command::SelectChannel(_) => {
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
    let result = mining.run(&mut pool).await;
    cancel.cancel();
    // Owned jobs include durable claim writes. Cancellation stops network work,
    // while any confirmed claim finishes its disk transaction before logout.
    while mining.jobs.join_next().await.is_some() {}
    pool.close().await;
    result
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum JobKind {
    Inventory,
    Channels,
    Watch,
    Poll,
    Claim,
    Update,
    Notification,
}
enum Job {
    Inventory {
        result: Result<Inventory, TwitchError>,
        requested_at: chrono::DateTime<Utc>,
    },
    Channels(Result<Vec<Channel>, TwitchError>),
    Watch {
        channel: Box<Channel>,
        result: Result<bool, TwitchError>,
        at: Instant,
    },
    Poll {
        channel: u64,
        result: Result<Option<(String, u32)>, TwitchError>,
    },
    Claim {
        id: String,
        result: Result<bool, TwitchError>,
    },
    Update(Result<Vec<Channel>, TwitchError>),
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
    status: InventoryStatus,
    watching: Option<u64>,
    manual: Option<(u64, u64)>,
    jobs: JoinSet<CompletedJob>,
    busy: HashSet<JobKind>,
    watch_abort: Option<tokio::task::AbortHandle>,
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
    notifications: HashSet<String>,
    claim_retry: HashMap<String, Instant>,
    claim_wait: Option<(String, Instant, u8)>,
    last_inventory: Instant,
    next_transition: Option<chrono::DateTime<Utc>>,
}
impl Mining {
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
            status: InventoryStatus::default(),
            watching: None,
            manual: None,
            jobs: JoinSet::new(),
            busy: HashSet::new(),
            watch_abort: None,
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
            notifications: HashSet::new(),
            claim_retry: HashMap::new(),
            claim_wait: None,
            last_inventory: now,
            next_transition: None,
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
            self.watching = None;
            self.manual = None;
            self.poll_at = None;
            self.claim_wait = None;
            self.refresh_channels.clear();
            self.status = InventoryStatus::default();
            pool.set_channels(&[]);
            self.publish = true;
        }
        if intent.refresh != self.seen.refresh {
            self.refresh = true;
            self.next_retry = Instant::now();
        }
        if intent.settings != self.seen.settings {
            self.cancel_watch();
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
        if intent.manual_revision != self.seen.manual_revision {
            let settings = self.app.snapshot.read().await.settings.values.clone();
            self.manual = intent.selected.and_then(|id| {
                let channel = self.channels.iter().find(|c| c.identity.id == id)?;
                // The target is an earnable campaign, which can be a special
                // category different from the channel's streamed game.
                let game = self
                    .campaigns
                    .iter()
                    .filter(|c| c.can_watch(channel, &settings, Utc::now()))
                    .min_by_key(|c| {
                        c.first_drop(&settings, Utc::now())
                            .map(|d| d.remaining_minutes())
                            .unwrap_or(u32::MAX)
                    })?
                    .game
                    .id;
                Some((id, game))
            });
            self.publish = true;
        }
        self.seen = intent;
    }

    async fn event(&mut self, event: Event) -> Result<(), TwitchError> {
        match event {
            Event::Unauthorized => return Err(TwitchError::Unauthorized),
            Event::Progress { id, minutes } => {
                self.confirm(&id, minutes);
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
                if self.channels.iter().any(|c| c.identity.id == id) {
                    self.refresh_channels
                        .entry(id)
                        .or_insert(Instant::now() + CHANNEL_DELAY);
                }
            }
            Event::Viewers { id, count } => {
                if let Some(channel) = self.channels.iter_mut().find(|c| c.identity.id == id) {
                    if channel.online() {
                        channel.viewers = Some(count);
                        self.publish = true;
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
    fn confirm(&mut self, id: &str, minutes: u32) -> bool {
        if let Some(drop) = self
            .campaigns
            .iter_mut()
            .flat_map(|c| &mut c.drops)
            .find(|d| d.id == id)
        {
            drop.confirm(minutes, Utc::now());
            self.last_progress = Some((id.to_owned(), Instant::now()));
            self.publish = true;
            true
        } else {
            false
        }
    }

    async fn reselect(&mut self, settings: &Settings) {
        let now = Utc::now();
        if let Some((_, game)) = self.manual
            && !self.channels.iter().any(|channel| {
                self.campaigns
                    .iter()
                    .any(|c| c.game.id == game && c.can_watch(channel, settings, now))
            })
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
            self.manual,
        );
        if next != self.watching {
            self.cancel_watch();
            self.watching = next;
            self.next_watch = Instant::now();
            self.poll_at = None;
            self.last_progress = None;
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
                let instance = drop.claim_id.clone().unwrap();
                let entry = campaign.history_entry(drop, wall);
                let client = self.client.clone();
                let app = self.app.clone();
                let journal = self.journal.clone();
                self.spawn(JobKind::Claim, async move {
                    let result = claim(&app, &client, &journal, entry, &instance).await;
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
                                || !self
                                    .channels
                                    .iter()
                                    .find(|c| c.identity.id == channel)
                                    .is_some_and(|channel| {
                                        self.campaigns.iter().any(|c| {
                                            c.can_watch(channel, settings, wall)
                                                && c.drops.iter().any(|d| &d.id == id)
                                        })
                                    })
                        })
                    {
                        let client = self.client.clone();
                        self.spawn(JobKind::Poll, async move {
                            Job::Poll {
                                channel,
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
                                && self
                                    .campaigns
                                    .iter()
                                    .any(|campaign| campaign.can_watch(c, settings, wall))
                        })
                        .cloned()
                {
                    self.next_watch = now + WATCH_INTERVAL;
                    let client = self.client.clone();
                    self.spawn(JobKind::Watch, async move {
                        let result = client.send_watch(&mut channel, Utc::now()).await;
                        Job::Watch {
                            channel: Box::new(channel),
                            result,
                            at: Instant::now(),
                        }
                    });
                    return;
                }
            }
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
            let client = self.client.clone();
            let settings = settings.clone();
            self.app
                .status(message("gui.status.fetching_inventory", &[]))
                .await;
            self.spawn(JobKind::Inventory, async move {
                let requested_at = Utc::now();
                Job::Inventory {
                    result: client.inventory(&settings).await,
                    requested_at,
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
                .find(|c| Some(c.identity.id) == self.watching)
                .cloned();
            self.app.status(message("gui.status.gathering", &[])).await;
            self.spawn(JobKind::Channels, async move {
                Job::Channels(
                    client
                        .channels(&campaigns, &settings, current.as_ref())
                        .await,
                )
            });
            return;
        }
        let updates: Vec<_> = self
            .channels
            .iter()
            .filter(|c| {
                self.refresh_channels
                    .get(&c.identity.id)
                    .is_some_and(|at| now >= *at)
            })
            .cloned()
            .collect();
        if !updates.is_empty() {
            for channel in &updates {
                self.refresh_channels.remove(&channel.identity.id);
            }
            let client = self.client.clone();
            self.spawn(JobKind::Update, async move {
                let mut updates = updates;
                let result = client.update_channels(&mut updates).await.map(|()| updates);
                Job::Update(result)
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
        let error = match job {
            Job::Inventory {
                result: Ok(mut inventory),
                requested_at,
            } => {
                for campaign in &mut inventory.campaigns {
                    for drop in &mut campaign.drops {
                        if let Some(previous) = self
                            .campaigns
                            .iter()
                            .find(|c| c.id == campaign.id)
                            .and_then(|c| c.drops.iter().find(|d| d.id == drop.id))
                            && previous.required_minutes == drop.required_minutes
                            && previous
                                .confirmed_at
                                .is_some_and(|at| at > requested_at || drop.confirmed_at.is_none())
                        {
                            drop.confirmed_minutes = previous.confirmed_minutes;
                            drop.confirmed_at = previous.confirmed_at;
                            drop.estimated_minutes = previous.estimated_minutes;
                            drop.claimed = previous.claimed;
                            drop.claim_id = previous.claim_id.clone();
                        }
                    }
                }
                self.campaigns = inventory.campaigns;
                self.status = inventory.status;
                self.recover_claims().await?;
                self.last_inventory = now;
                self.set_transition();
                self.next_refresh = now
                    + Duration::from_secs(
                        u64::from(settings.minimum_refresh_interval_minutes) * 60,
                    );
                self.channels_dirty = true;
                self.publish = true;
                None
            }
            Job::Channels(Ok(channels)) => {
                self.channels = channels;
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
                result: Ok(true),
                at,
            } => {
                if self.watching == Some(channel.identity.id) {
                    if let Some(current) = self.channels.iter_mut().find(|c| {
                        c.identity.id == channel.identity.id
                            && c.broadcast_id == channel.broadcast_id
                    }) {
                        current.beacon_url = channel.beacon_url;
                    }
                    self.next_watch = at + WATCH_INTERVAL;
                    self.poll_at = Some(at + PROGRESS_DELAY);
                }
                None
            }
            Job::Watch {
                channel, result, ..
            } => {
                self.refresh_channels.insert(channel.identity.id, now);
                match result {
                    Err(e) => Some(e),
                    Ok(false) => Some(TwitchError::Network),
                    _ => None,
                }
            }
            Job::Poll { channel, result } => {
                if self.watching == Some(channel) {
                    let current = result.as_ref().ok().and_then(|v| v.as_ref());
                    if let Some((id, minutes)) = current {
                        self.confirm(id, *minutes);
                    }
                    if let Some((claimed, _, attempts)) = self.claim_wait.take() {
                        if current.is_some_and(|(id, _)| id == &claimed) && attempts < 7 {
                            self.claim_wait =
                                Some((claimed, now + Duration::from_secs(2), attempts + 1));
                        } else {
                            self.next_watch = now;
                        }
                    } else if current.is_none() {
                        let watching = self.channels.iter().find(|c| c.identity.id == channel);
                        for campaign in &mut self.campaigns {
                            if watching
                                .is_some_and(|c| campaign.can_watch(c, &settings, Utc::now()))
                            {
                                campaign.bump_estimates(&settings, Utc::now());
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
                self.claim_wait = Some((id, now + Duration::from_secs(4), 0));
                self.publish = true;
                None
            }
            Job::Claim { id, result } => {
                self.claim_retry.insert(id, now + Duration::from_secs(60));
                result.err()
            }
            Job::Update(Ok(updated)) => {
                for channel in updated {
                    if let Some(current) = self
                        .channels
                        .iter_mut()
                        .find(|c| c.identity.id == channel.identity.id)
                    {
                        *current = channel;
                    }
                }
                self.publish = true;
                None
            }
            Job::Notification(result) => result.err(),
            Job::Inventory {
                result: Err(error), ..
            } => {
                self.refresh = true;
                Some(error)
            }
            Job::Channels(Err(error)) => {
                self.channels_dirty = true;
                Some(error)
            }
            Job::Update(Err(error)) => Some(error),
        };
        if let Some(error) = error {
            if matches!(error, TwitchError::Unauthorized | TwitchError::Cancelled) {
                return Err(error);
            }
            self.next_retry = now + Duration::from_secs(10);
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
        let key = if settings.games_to_watch.is_empty() {
            "status.no_selection"
        } else if !self.campaigns.iter().any(|c| {
            settings.selected(&c.game.name)
                && c.can_earn_within(
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
        let channels: Vec<_> = self
            .channels
            .iter()
            .map(|c| c.view(self.watching))
            .collect();
        let wanted = wanted_items(&self.campaigns, settings, now);
        let active = self
            .channels
            .iter()
            .find(|c| Some(c.identity.id) == self.watching)
            .and_then(|channel| {
                self.campaigns
                    .iter()
                    .filter(|c| c.can_watch(channel, settings, now))
                    .filter_map(|c| c.first_drop(settings, now).map(|d| (c, d)))
                    .min_by_key(|(_, d)| d.remaining_minutes())
            });
        let progress = active.map(|(c, d)| c.progress(d));
        let manual = self
            .manual
            .map(|(_, game)| ManualMode {
                active: true,
                game_name: self
                    .campaigns
                    .iter()
                    .find(|c| c.game.id == game)
                    .map(|c| c.game.name.clone()),
                channel_name: self
                    .channels
                    .iter()
                    .find(|c| Some(c.identity.id) == self.watching)
                    .map(|c| c.identity.name.clone()),
            })
            .unwrap_or_default();
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

    async fn recover_claims(&self) -> Result<(), TwitchError> {
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
        tokio::task::spawn_blocking(move || {
            let mut journal = journal.blocking_lock();
            for entry in journal
                .pending(user_id)
                .into_iter()
                .filter(|e| confirmed.contains(&e.id))
            {
                app.history.blocking_lock().record(entry.clone())?;
                journal.finish(user_id, &entry.id)?;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .map_err(|_| TwitchError::Storage)?
        .map_err(|_| TwitchError::Storage)
    }
}

async fn claim(
    app: &Arc<App>,
    client: &TwitchClient,
    journal: &Arc<Mutex<ClaimJournal>>,
    entry: HistoryEntry,
    instance: &str,
) -> Result<bool, TwitchError> {
    let pending = journal.clone();
    let user_id = client.user_id;
    let entry =
        tokio::task::spawn_blocking(move || pending.blocking_lock().prepare(user_id, entry))
            .await
            .map_err(|_| TwitchError::Storage)?
            .map_err(|_| TwitchError::Storage)?;
    let claimed = client.claim(instance).await?;
    let app = app.clone();
    let journal = journal.clone();
    tokio::task::spawn_blocking(move || {
        if claimed {
            app.history.blocking_lock().record(entry.clone())?;
        }
        journal.blocking_lock().finish(user_id, &entry.id)?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|_| TwitchError::Storage)?
    .map_err(|_| TwitchError::Storage)?;
    Ok(claimed)
}
