//! Feature-gated, loopback-only browser fixture. It never constructs a Twitch client.
use std::{path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    dto::{HistoryEntry, Login, ManualMode, OAuthCode, Snapshot},
    web::{App, Command, CommandRequest},
};

fn snapshot() -> Snapshot {
    serde_json::from_str(include_str!("../frontend/tests/fixture.json"))
        .expect("valid browser fixture")
}

pub async fn create(directory: PathBuf) -> anyhow::Result<(Arc<App>, JoinHandle<()>)> {
    let (mut app, receiver) = App::open(directory, "")?;
    Arc::get_mut(&mut app).expect("new application").fixture = true;
    reset_state(&app).await?;
    let worker = tokio::spawn(commands(app.clone(), receiver));
    Ok((app, worker))
}

async fn reset_state(app: &App) -> anyhow::Result<()> {
    app.auth.reset().await?;
    app.sockets.prune(true).await;
    let state = snapshot();
    app.data.save_settings(&state.settings.values)?;
    *app.snapshot.write().await = state;
    let mut history = app.history.lock().await;
    history.clear()?;
    history.record(HistoryEntry {
        id: "past-drop".into(),
        claimed_at: "2026-09-25T18:00:00Z".parse()?,
        game: "Rust".into(),
        campaign: "Autumn expedition".into(),
        drop_name: "Canvas pack".into(),
        benefits: vec!["Canvas pack".into()],
        required_minutes: 30,
        campaign_id: "campaign-1".into(),
        image_url: String::new(),
    })?;
    Ok(())
}

pub fn routes(router: Router<Arc<App>>) -> Router<Arc<App>> {
    router
        .route(
            "/__test/health",
            get(|| async { Json(json!({"fixture":true})) }),
        )
        .route("/__test/reset", post(reset))
        .route("/__test/event", post(event))
        .route("/__test/reconnect", post(reconnect))
}
async fn reset(State(app): State<Arc<App>>) -> Result<Json<Value>, crate::web::ApiError> {
    reset_state(&app).await.map_err(|_| {
        crate::web::ApiError(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "fixture_reset_failed",
        )
    })?;
    Ok(Json(json!({"ok":true})))
}
async fn event(State(app): State<Arc<App>>, Json(body): Json<Event>) -> Json<Value> {
    app.sockets.emit(&body.event, &body.data).await;
    Json(json!({"ok":true}))
}
#[derive(Deserialize)]
struct Event {
    event: String,
    data: Value,
}
async fn reconnect(State(app): State<Arc<App>>) -> Json<Value> {
    app.snapshot.write().await.channels.clear();
    app.sockets.prune(true).await;
    Json(json!({"ok":true}))
}

async fn commands(app: Arc<App>, mut receiver: mpsc::Receiver<CommandRequest>) {
    loop {
        let request = tokio::select! {
            _ = app.shutdown.cancelled()=>break,
            request = receiver.recv()=>{ let Some(request)=request else{break};request }
        };
        match request.command {
            Command::SelectChannel(id) => {
                let mode = {
                    let mut state = app.snapshot.write().await;
                    for channel in &mut state.channels {
                        channel.watching = channel.id == id;
                    }
                    state.manual_mode = ManualMode {
                        active: true,
                        game_name: Some("Rust".into()),
                        channel_name: Some("harbor".into()),
                        ..ManualMode::default()
                    };
                    state.manual_mode.clone()
                };
                app.sockets
                    .emit("channel_watching", &json!({"id":id}))
                    .await;
                app.sockets.emit("manual_mode_update", &mode).await;
            }
            Command::ExitManual => {
                let mode = ManualMode::default();
                app.snapshot.write().await.manual_mode = mode.clone();
                app.sockets.emit("manual_mode_update", &mode).await;
            }
            Command::MineChannel(login) => {
                let mode = if login == "missing" {
                    ManualMode {
                        error: Some(crate::web::message("gui.channels.not_found", &[])),
                        ..ManualMode::default()
                    }
                } else {
                    let _ = app.select_game("Rust").await;
                    let channels = {
                        let mut state = app.snapshot.write().await;
                        for c in &mut state.channels {
                            c.watching = false;
                        }
                        state.channels.push(crate::dto::ChannelView {
                            id: 999,
                            login: login.clone(),
                            name: login.clone(),
                            game: Some("Rust".into()),
                            game_id: Some(1),
                            online: true,
                            drops_enabled: true,
                            watching: true,
                            ..Default::default()
                        });
                        state.channels.clone()
                    };
                    app.sockets
                        .emit("channels_batch_update", &json!({"channels":channels}))
                        .await;
                    ManualMode {
                        active: true,
                        game_name: Some("Rust".into()),
                        channel_name: Some(login),
                        ..ManualMode::default()
                    }
                };
                app.snapshot.write().await.manual_mode = mode.clone();
                app.sockets.emit("manual_mode_update", &mode).await;
            }
            Command::Logout => {
                let login = Login {
                    status: "Logged out".into(),
                    user_id: None,
                    oauth_pending: Some(OAuthCode {
                        url: "https://www.twitch.tv/activate".into(),
                        code: "NEWCODE".into(),
                    }),
                };
                app.snapshot.write().await.login = login.clone();
                app.sockets.emit("login_status", &login).await;
            }
            Command::ConfirmOAuth
            | Command::SettingsChanged
            | Command::Refresh { .. }
            | Command::Shutdown => {}
        }
        let _ = request.complete.send(Ok(()));
    }
}
