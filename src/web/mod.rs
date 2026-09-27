mod releases;
pub mod socket;
#[cfg(test)]
mod tests;

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use anyhow::Result;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{ConnectInfo, Path, Query, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::{NaiveDate, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use socketioxide::SocketIo;
use tokio::sync::{Mutex, RwLock, Semaphore, mpsc, oneshot};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{
    auth::{
        AuthAction, AuthError, AuthSettingsRequest, LoginRequest, WebAuth, random_hex,
        session_cookie, token_from_headers, unix_now,
    },
    dto::{SettingsView, Snapshot},
    origin::DashboardOrigin,
    store::{CampaignArchive, DataDirectory, History, HistoryFilter},
};
use socket::SocketHub;

static ENGLISH: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../lang/English.json")).expect("valid English catalog")
});

pub fn message(path: &str, replacements: &[(&str, &str)]) -> String {
    let mut value = &*ENGLISH;
    for key in path.split('.') {
        value = &value[key];
    }
    let mut result = value.as_str().unwrap_or(path).to_owned();
    for (key, value) in replacements {
        result = result.replace(&format!("{{{key}}}"), value);
    }
    result
}

#[derive(Clone, Debug)]
pub enum Command {
    Refresh { clear_cache: bool },
    SettingsChanged,
    SelectChannel(u64),
    MineChannel(String),
    ExitManual,
    ConfirmOAuth,
    Logout,
    Shutdown,
}

pub struct CommandRequest {
    pub command: Command,
    pub complete: oneshot::Sender<Result<(), String>>,
}

pub struct App {
    pub data: Arc<DataDirectory>,
    pub snapshot: RwLock<Snapshot>,
    pub history: Mutex<History>,
    pub archive: Mutex<CampaignArchive>,
    pub auth: Arc<WebAuth>,
    pub origin: DashboardOrigin,
    pub sockets: SocketHub,
    pub shutdown: CancellationToken,
    writes: TaskTracker,
    pub(crate) settings_slot: Arc<Semaphore>,
    releases: releases::Releases,
    commands: mpsc::Sender<CommandRequest>,
    #[cfg(feature = "dashboard-fixture")]
    pub fixture: bool,
}

impl App {
    pub fn open(
        directory: PathBuf,
        public_base_url: &str,
    ) -> Result<(Arc<Self>, mpsc::Receiver<CommandRequest>)> {
        let data = Arc::new(DataDirectory::open(directory)?);
        let settings = data.settings()?;
        let auth = Arc::new(WebAuth::open(&data.path)?);
        let origin = DashboardOrigin::new(public_base_url)?;
        let (commands, receiver) = mpsc::channel(64);
        let history = History::load(&data.path);
        let archive = CampaignArchive::load(&data.path);
        let snapshot = Snapshot {
            settings: SettingsView {
                values: settings,
                revision: random_hex::<16>()?,
                games_available: vec![],
            },
            campaigns: archive.merge(vec![], Utc::now()),
            ..Snapshot::default()
        };
        Ok((
            Arc::new(Self {
                data,
                snapshot: RwLock::new(snapshot),
                history: Mutex::new(history),
                archive: Mutex::new(archive),
                sockets: SocketHub::new(auth.clone()),
                auth,
                origin,
                shutdown: CancellationToken::new(),
                writes: TaskTracker::new(),
                settings_slot: Arc::new(Semaphore::new(1)),
                releases: releases::Releases::new()?,
                commands,
                #[cfg(feature = "dashboard-fixture")]
                fixture: false,
            }),
            receiver,
        ))
    }

    pub async fn command(&self, command: Command) -> Result<(), ApiError> {
        if self.shutdown.is_cancelled() {
            return Err(ApiError(StatusCode::CONFLICT, "shutting_down"));
        }
        let (complete, result) = oneshot::channel();
        self.commands
            .send(CommandRequest { command, complete })
            .await
            .map_err(|_| ApiError::unavailable())?;
        result
            .await
            .map_err(|_| ApiError::unavailable())?
            .map_err(|_| ApiError::unavailable())
    }

    pub async fn console(&self, text: String) {
        let line = format!("[{}] | {text}", Utc::now().format("%Y-%m-%d %H:%M:%S"));
        {
            let mut snapshot = self.snapshot.write().await;
            if snapshot
                .console
                .last()
                .is_some_and(|last| last.split_once(" | ").is_some_and(|(_, old)| old == text))
            {
                return;
            }
            snapshot.console.push(line.clone());
            if snapshot.console.len() > 1000 {
                snapshot.console.remove(0);
            }
        }
        tracing::info!("{text}");
        self.sockets
            .emit("console_output", &json!({"message":line}))
            .await;
    }

    pub async fn status(&self, text: String) {
        self.snapshot.write().await.status = text.clone();
        self.sockets
            .emit("status_update", &json!({"status":text}))
            .await;
    }

    pub async fn drain_writes(&self) {
        self.writes.close();
        self.writes.wait().await;
    }

    async fn change_settings(
        &self,
        update: impl FnOnce(&SettingsView) -> Result<crate::config::Settings, ApiError>,
    ) -> Result<SettingsView, ApiError> {
        let settings = {
            let mut state = self.snapshot.write().await;
            let next = update(&state.settings)?;
            let data = self.data.clone();
            let saved = next.clone();
            let revision = random_hex::<16>().map_err(|_| ApiError::unavailable())?;
            tokio::task::spawn_blocking(move || data.save_settings(&saved))
                .await
                .map_err(|_| ApiError::unavailable())?
                .map_err(|_| ApiError::unavailable())?;
            state.settings.values = next;
            state.settings.revision = revision;
            state.settings.clone()
        };
        self.sockets.emit("settings_updated", &settings).await;
        Ok(settings)
    }

    // Called by the owned mining task after an explicit Mine channel request.
    // Append under the same transaction as autosaves, preserving other browsers' edits.
    pub async fn select_game(
        &self,
        game: &str,
        eligible: impl FnOnce(&crate::config::Settings) -> bool,
    ) -> Result<SettingsView, ApiError> {
        let _permit = self
            .settings_slot
            .acquire()
            .await
            .map_err(|_| ApiError::unavailable())?;
        self.change_settings(|current| {
            if !eligible(&current.values) {
                return Err(ApiError(StatusCode::CONFLICT, "channel_selection_changed"));
            }
            let mut next = current.values.clone();
            if !next.selected(game) {
                next.games_to_watch.push(game.to_owned());
            }
            next.patched(&json!({"games_to_watch":next.games_to_watch}))
                .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_settings"))
        })
        .await
    }
}

#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub &'static str);
impl ApiError {
    fn invalid() -> Self {
        Self(StatusCode::BAD_REQUEST, "invalid_request")
    }
    fn unavailable() -> Self {
        Self(StatusCode::SERVICE_UNAVAILABLE, "request_failed")
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"detail":self.1}))).into_response()
    }
}
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let mut response =
            (self.status(), Json(json!({"detail":self.to_string()}))).into_response();
        if self == Self::RateLimited {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, "60".parse().unwrap());
        }
        response
    }
}

pub fn router(app: Arc<App>) -> Router {
    let (layer, io) = SocketIo::builder().max_payload(1024 * 1024).build_layer();
    SocketHub::attach(&app, io);
    let mut routes = Router::new()
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/", get(index))
        .route("/campaigns", get(index))
        .route("/history", get(index))
        .route("/activity", get(index))
        .route("/settings", get(index))
        .route("/login", get(login_page))
        .route("/assets/{*path}", get(asset))
        .route("/api/auth/status", get(auth_status))
        .route("/api/auth/login", post(auth_login))
        .route("/api/auth/logout", post(auth_logout))
        .route("/api/auth/settings", post(auth_settings))
        .route("/api/status", get(status))
        .route("/api/channels", get(channels))
        .route("/api/channels/select", post(select_channel))
        .route("/api/campaigns", get(campaigns))
        .route("/api/console", get(console))
        .route("/api/settings", get(settings).post(update_settings))
        .route("/api/settings/verify-proxy", post(verify_proxy))
        .route("/api/version", get(version))
        .route("/api/history", get(history).delete(clear_history))
        .route("/api/history/stats", get(history_stats))
        .route("/api/history/export.csv", get(history_csv))
        .route("/api/twitch/logout", post(logout_twitch))
        .route("/api/oauth/confirm", post(confirm_oauth))
        .route("/api/reload", post(reload))
        .route("/api/cache/clear", post(clear_cache))
        .route("/api/mode/exit-manual", post(exit_manual))
        .route("/api/close", post(close));
    #[cfg(feature = "dashboard-fixture")]
    if app.fixture {
        routes = super::fixture::routes(routes);
    }
    routes = routes
        .layer(layer)
        .layer(middleware::from_fn_with_state(app.clone(), guard));
    routes.with_state(app)
}

async fn guard(State(app): State<Arc<App>>, mut request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let secure_transport = request.uri().scheme_str() == Some("https");
    if !app
        .origin
        .permits(request.method(), &path, request.headers(), secure_transport)
    {
        return private(ApiError(StatusCode::FORBIDDEN, "forbidden").into_response());
    }
    let asset =
        path.starts_with("/assets/") && matches!(*request.method(), Method::GET | Method::HEAD);
    let public = asset
        || matches!(
            path.as_str(),
            "/login" | "/healthz" | "/api/auth/status" | "/api/auth/login"
        );
    #[cfg(feature = "dashboard-fixture")]
    let public = public
        || app.fixture
            && matches!(
                path.as_str(),
                "/__test/health" | "/__test/reset" | "/__test/event"
            );
    if !public
        && !app
            .auth
            .state
            .lock()
            .await
            .allowed(&token_from_headers(request.headers()), unix_now())
    {
        let response = if matches!(
            path.as_str(),
            "/" | "/campaigns" | "/history" | "/activity" | "/settings"
        ) {
            Redirect::to("/login").into_response()
        } else {
            ApiError(StatusCode::UNAUTHORIZED, "authentication_required").into_response()
        };
        return private(response);
    }
    // Bound every submitted document and suppress extractor errors that could echo secrets.
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && !path.starts_with("/socket.io")
    {
        let limit = if path.starts_with("/api/auth/") {
            16384
        } else {
            1024 * 1024
        };
        let (parts, body) = request.into_parts();
        let Ok(body) = to_bytes(body, limit).await else {
            return private(
                ApiError(StatusCode::PAYLOAD_TOO_LARGE, "invalid_request").into_response(),
            );
        };
        request = Request::from_parts(parts, Body::from(body));
    }
    let response = next.run(request).await;
    if asset && response.status().is_success() {
        response
    } else {
        private(response)
    }
}

fn private(mut response: Response) -> Response {
    if !response.headers().contains_key(header::CACHE_CONTROL) {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    }
    response
        .headers_mut()
        .insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    response
        .headers_mut()
        .insert(header::REFERRER_POLICY, "same-origin".parse().unwrap());
    response
}

#[derive(rust_embed::RustEmbed)]
#[folder = "web/"]
struct Assets;

async fn index() -> Response {
    serve_asset("index.html", false)
}
async fn login_page(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if app
        .auth
        .state
        .lock()
        .await
        .allowed(&token_from_headers(&headers), unix_now())
    {
        Redirect::to("/").into_response()
    } else {
        index().await
    }
}
async fn asset(Path(path): Path<String>) -> Response {
    serve_asset(&format!("assets/{path}"), true)
}
fn serve_asset(path: &str, immutable: bool) -> Response {
    if path
        .split('/')
        .any(|part| matches!(part, "." | "..") || part.contains('\\'))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(asset) = Assets::get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    (
        [
            (
                header::CONTENT_TYPE,
                mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string(),
            ),
            (
                header::CACHE_CONTROL,
                if immutable {
                    "public, max-age=31536000, immutable"
                } else {
                    "no-cache"
                }
                .to_owned(),
            ),
        ],
        asset.data.into_owned(),
    )
        .into_response()
}

async fn document<T: serde::de::DeserializeOwned>(request: Request) -> Result<T, ApiError> {
    let bytes = to_bytes(request.into_body(), 1024 * 1024)
        .await
        .map_err(|_| ApiError::invalid())?;
    serde_json::from_slice(&bytes).map_err(|_| ApiError::invalid())
}

async fn auth_status(State(app): State<Arc<App>>, headers: HeaderMap) -> Json<Value> {
    let state = app.auth.state.lock().await;
    Json(
        json!({"enabled":state.enabled(),"authenticated":state.allowed(&token_from_headers(&headers),unix_now()),"translations":ENGLISH["gui"]["auth"]}),
    )
}

fn peer(request: &Request) -> Option<std::net::IpAddr> {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|v| v.0.ip())
}
fn cookie_secure(app: &App, request: &Request) -> bool {
    app.origin
        .secure_cookie(request.uri().scheme_str() == Some("https"))
}
async fn auth_response(
    app: &App,
    token: &str,
    remember: bool,
    secure: bool,
    disconnect_all: bool,
) -> Response {
    app.sockets.prune(disconnect_all).await;
    let enabled = app.auth.state.lock().await.enabled();
    (
        [(header::SET_COOKIE, session_cookie(token, remember, secure))],
        Json(json!({"success":true,"enabled":enabled})),
    )
        .into_response()
}
async fn auth_login(State(app): State<Arc<App>>, request: Request) -> Response {
    let previous = token_from_headers(request.headers());
    let peer = peer(&request);
    let secure = cookie_secure(&app, &request);
    let data = match document::<LoginRequest>(request).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let remember = data.remember;
    match app.auth.login(data, &previous, peer).await {
        Ok(token) => auth_response(&app, &token, remember, secure, false).await,
        Err(e) => e.into_response(),
    }
}
async fn auth_logout(State(app): State<Arc<App>>, request: Request) -> Response {
    let token = token_from_headers(request.headers());
    let secure = cookie_secure(&app, &request);
    match app.auth.logout(&token).await {
        Ok(()) => auth_response(&app, "", false, secure, false).await,
        Err(e) => e.into_response(),
    }
}
async fn auth_settings(State(app): State<Arc<App>>, request: Request) -> Response {
    let token = token_from_headers(request.headers());
    let peer = peer(&request);
    let secure = cookie_secure(&app, &request);
    let data = match document::<AuthSettingsRequest>(request).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let disable = data.action == AuthAction::Disable;
    match app.auth.configure(data, &token, peer).await {
        Ok(token) => auth_response(&app, &token, false, secure, disable).await,
        Err(e) => e.into_response(),
    }
}

async fn status(State(app): State<Arc<App>>) -> Json<Value> {
    let state = app.snapshot.read().await;
    Json(json!({"status":state.status,"login":state.login,"manual_mode":state.manual_mode}))
}
async fn channels(State(app): State<Arc<App>>) -> Json<Value> {
    Json(json!({"channels":app.snapshot.read().await.channels}))
}
async fn campaigns(State(app): State<Arc<App>>) -> Json<Value> {
    Json(json!({"campaigns":app.snapshot.read().await.campaigns}))
}
async fn console(State(app): State<Arc<App>>) -> Json<Value> {
    Json(json!({"lines":app.snapshot.read().await.console}))
}
async fn settings(State(app): State<Arc<App>>) -> Json<SettingsView> {
    Json(app.snapshot.read().await.settings.clone())
}

async fn update_settings(
    State(app): State<Arc<App>>,
    request: Request,
) -> Result<Json<Value>, ApiError> {
    let patch: Value = document(request).await?;
    let permit = app
        .settings_slot
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ApiError::unavailable())?;
    let owned = app.clone();
    // Once a write starts, an HTTP disconnect cannot leave disk and live state
    // disagreeing. Shutdown drains the transaction before releasing the data lock.
    app.writes
        .spawn(async move {
            let result = save_settings(owned.clone(), patch).await;
            // The miner can append a manually selected game during reconfiguration.
            // Never hold the settings transaction while waiting for it to drain.
            drop(permit);
            if result.is_ok() {
                owned.command(Command::SettingsChanged).await?;
            }
            result
        })
        .await
        .map_err(|_| ApiError::unavailable())?
}

async fn save_settings(app: Arc<App>, patch: Value) -> Result<Json<Value>, ApiError> {
    let settings = app
        .change_settings(|current| {
            if patch
                .get("revision")
                .is_some_and(|v| !v.is_null() && v.as_str() != Some(current.revision.as_str()))
            {
                return Err(ApiError(StatusCode::CONFLICT, "settings_conflict"));
            }
            current
                .values
                .patched(&patch)
                .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_settings"))
        })
        .await?;
    Ok(Json(json!({"success":true,"settings":settings})))
}

async fn select_channel(
    State(app): State<Arc<App>>,
    request: Request,
) -> Result<Json<Value>, ApiError> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Selection {
        Id { channel_id: u64 },
        Login { channel: String },
    }
    let selection: Selection = document(request).await?;
    if app.snapshot.read().await.login.user_id.is_none() {
        return Err(ApiError(StatusCode::CONFLICT, "twitch_login_required"));
    }
    if let Selection::Login { channel } = selection {
        let login = crate::twitch::channels::channel_login(&channel)
            .ok_or(ApiError(StatusCode::BAD_REQUEST, "invalid_channel"))?;
        app.command(Command::MineChannel(login)).await?;
        return Ok(Json(json!({"success":true})));
    }
    let Selection::Id { channel_id } = selection else {
        unreachable!()
    };
    if !app
        .snapshot
        .read()
        .await
        .channels
        .iter()
        .any(|c| c.id == channel_id)
    {
        return Err(ApiError(StatusCode::NOT_FOUND, "channel_not_found"));
    }
    app.command(Command::SelectChannel(channel_id)).await?;
    Ok(Json(json!({"success":true})))
}

#[derive(Deserialize, Default)]
struct HistoryQuery {
    game: Option<String>,
    campaign_id: Option<String>,
    since: Option<String>,
    limit: Option<usize>,
}
impl HistoryQuery {
    fn filter(self) -> HistoryFilter {
        let since = self.since.and_then(|s| {
            s.parse()
                .ok()
                .or_else(|| {
                    NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                        .ok()
                        .and_then(|d| d.and_hms_opt(0, 0, 0))
                        .map(|d| d.and_utc())
                })
                .or_else(|| {
                    NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S")
                        .ok()
                        .map(|d| d.and_utc())
                })
        });
        HistoryFilter {
            game: self.game.filter(|s| !s.is_empty()),
            campaign_id: self.campaign_id,
            since,
            limit: self.limit.filter(|n| *n > 0).map(|n| n.min(5000)),
        }
    }
}
async fn history(State(app): State<Arc<App>>, Query(query): Query<HistoryQuery>) -> Json<Value> {
    let history = app.history.lock().await;
    Json(json!({"total":history.total(),"entries":history.entries(&query.filter())}))
}
async fn history_stats(State(app): State<Arc<App>>) -> Json<Value> {
    Json(app.history.lock().await.stats())
}
async fn history_csv(
    State(app): State<Arc<App>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Response, ApiError> {
    let name = query
        .game
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|game| format!("drop_history_{game}.csv"))
        .unwrap_or_else(|| "drop_history.csv".into());
    let encoded = url::form_urlencoded::byte_serialize(name.as_bytes())
        .collect::<String>()
        .replace('+', "%20");
    let content = app
        .history
        .lock()
        .await
        .csv(&query.filter())
        .map_err(|_| ApiError::unavailable())?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"drop_history.csv\"; filename*=UTF-8''{encoded}"),
            ),
        ],
        content,
    )
        .into_response())
}
async fn clear_history(State(app): State<Arc<App>>) -> Result<Json<Value>, ApiError> {
    app.history
        .lock()
        .await
        .clear()
        .map_err(|_| ApiError::unavailable())?;
    Ok(Json(json!({"success":true})))
}

async fn run_command(app: &App, command: Command) -> Result<Json<Value>, ApiError> {
    app.command(command).await?;
    Ok(Json(json!({"success":true})))
}
async fn reload(State(app): State<Arc<App>>) -> Result<Json<Value>, ApiError> {
    run_command(&app, Command::Refresh { clear_cache: false }).await
}
async fn clear_cache(State(app): State<Arc<App>>) -> Result<Json<Value>, ApiError> {
    run_command(&app, Command::Refresh { clear_cache: true }).await
}
async fn logout_twitch(State(app): State<Arc<App>>) -> Result<Json<Value>, ApiError> {
    run_command(&app, Command::Logout).await
}
async fn confirm_oauth(State(app): State<Arc<App>>) -> Result<Json<Value>, ApiError> {
    run_command(&app, Command::ConfirmOAuth).await
}
async fn exit_manual(State(app): State<Arc<App>>) -> Result<Json<Value>, ApiError> {
    run_command(&app, Command::ExitManual).await
}
async fn close(State(app): State<Arc<App>>) -> Result<Json<Value>, ApiError> {
    run_command(&app, Command::Shutdown).await
}

async fn verify_proxy(
    State(app): State<Arc<App>>,
    request: Request,
) -> Result<Json<Value>, ApiError> {
    #[derive(Deserialize)]
    struct Proxy {
        proxy: String,
    }
    let Proxy { proxy } = document(request).await?;
    crate::config::validate_proxy(&proxy).map_err(|_| ApiError::invalid())?;
    if proxy.is_empty() {
        return Ok(Json(
            json!({"success":false,"message":message("gui.backend.proxy_empty",&[])}),
        ));
    }
    #[cfg(feature = "dashboard-fixture")]
    if app.fixture {
        return Ok(Json(json!({"success":true})));
    }
    let _ = app;
    let started = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .no_proxy()
        .proxy(reqwest::Proxy::all(&proxy).map_err(|_| ApiError::invalid())?)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| ApiError::unavailable())?;
    let response = client.get("https://www.twitch.tv").send().await;
    Ok(Json(match response {
        Ok(response) if response.status().as_u16() < 500 => {
            json!({"success":true,"latency":started.elapsed().as_millis(),"message":message("gui.backend.proxy_connected",&[])})
        }
        _ => json!({"success":false,"message":message("gui.backend.proxy_failed",&[])}),
    }))
}

async fn version(State(app): State<Arc<App>>) -> Json<releases::ReleaseInfo> {
    #[cfg(feature = "dashboard-fixture")]
    if app.fixture {
        return Json(releases::ReleaseInfo::default());
    }
    Json(app.releases.check(&app.shutdown).await)
}
