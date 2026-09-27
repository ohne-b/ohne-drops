use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use serde::Serialize;
use socketioxide::{SocketIo, extract::SocketRef};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::auth::{SESSION_SECONDS, WebAuth, token_from_headers, unix_now};

use super::{App, Command};

#[derive(Clone)]
struct SocketSession {
    token: String,
    cancelled: CancellationToken,
}

pub struct SocketHub {
    pub io: OnceLock<SocketIo>,
    auth: Arc<WebAuth>,
    tasks: TaskTracker,
}

impl SocketHub {
    pub fn new(auth: Arc<WebAuth>) -> Self {
        Self {
            io: OnceLock::new(),
            auth,
            tasks: TaskTracker::new(),
        }
    }

    pub fn attach(app: &Arc<App>, io: SocketIo) {
        let weak = Arc::downgrade(app);
        io.ns("/", async move |socket: SocketRef| {
            if let Some(app) = weak.upgrade() {
                app.sockets.connected(socket, &app).await;
            } else {
                let _ = socket.disconnect();
            }
        });
        assert!(
            app.sockets.io.set(io).is_ok(),
            "socket server already attached"
        );
    }

    async fn connected(&self, socket: SocketRef, app: &Arc<App>) {
        let token = token_from_headers(&socket.req_parts().headers);
        let state = self.auth.state.lock().await;
        let now = unix_now();
        if !state.allowed(&token, now) {
            let _ = socket.disconnect();
            return;
        }
        let cancelled = app.shutdown.child_token();
        socket.extensions.insert(SocketSession {
            token: token.clone(),
            cancelled: cancelled.clone(),
        });
        socket.on_disconnect(async |socket: SocketRef| {
            if let Some(session) = socket.extensions.remove::<SocketSession>() {
                session.cancelled.cancel();
            }
        });
        if state.enabled() {
            let remaining = expiry_delay(state.expires_at(&token).unwrap_or(now), now);
            let expiring = socket.clone();
            self.tasks
                .spawn(expire_after(remaining, cancelled, move || {
                    let _ = expiring.disconnect();
                }));
        }
        // Hold the auth policy through emission so enabling protection cannot race
        // an anonymous initial snapshot. No state writer holds a lock while emitting.
        let snapshot = app.snapshot.read().await;
        if socket.emit("initial_state", &*snapshot).is_err() {
            let _ = socket.clone().disconnect();
        }
        drop(snapshot);
        drop(state);
        for (event, command) in [
            ("request_reload", Command::Refresh { clear_cache: false }),
            ("request_login", Command::ConfirmOAuth),
        ] {
            let weak = Arc::downgrade(app);
            socket.on(event, async move |socket: SocketRef| {
                if let Some(app) = weak.upgrade()
                    && app.sockets.authorized(&socket).await
                {
                    if matches!(command, Command::Refresh { clear_cache: false }) {
                        let _ = app.refresh_inventory().await;
                    } else {
                        let _ = app.command(command.clone()).await;
                    }
                }
            });
        }
        let weak = Arc::downgrade(app);
        socket.on("get_wanted_items", async move |socket: SocketRef| {
            if let Some(app) = weak.upgrade() {
                let auth = app.auth.state.lock().await;
                if Self::allowed(&socket, &auth) {
                    let snapshot = app.snapshot.read().await;
                    let _ = socket.emit("wanted_items_update", &snapshot.wanted_items);
                } else {
                    let _ = socket.disconnect();
                }
            }
        });
    }

    fn allowed(socket: &SocketRef, state: &crate::auth::AuthState) -> bool {
        socket
            .extensions
            .get::<SocketSession>()
            .is_some_and(|session| state.allowed(&session.token, unix_now()))
    }

    async fn authorized(&self, socket: &SocketRef) -> bool {
        let state = self.auth.state.lock().await;
        if Self::allowed(socket, &state) {
            true
        } else {
            let _ = socket.clone().disconnect();
            false
        }
    }

    pub async fn emit<T: Serialize + Sync>(&self, event: &str, data: &T) {
        let state = self.auth.state.lock().await;
        if let Some(io) = self.io.get() {
            for socket in io.sockets() {
                if Self::allowed(&socket, &state) {
                    if socket.emit(event, data).is_err() {
                        let _ = socket.disconnect();
                    }
                } else {
                    let _ = socket.disconnect();
                }
            }
        }
    }

    pub async fn prune(&self, all: bool) {
        let state = self.auth.state.lock().await;
        if let Some(io) = self.io.get() {
            for socket in io.sockets() {
                if all || !Self::allowed(&socket, &state) {
                    let _ = socket.disconnect();
                }
            }
        }
    }

    pub async fn close(&self) {
        self.prune(true).await;
        self.tasks.close();
        self.tasks.wait().await;
        if let Some(io) = self.io.get() {
            io.close().await;
        }
    }
}

fn expiry_delay(expires_at: f64, now: f64) -> Duration {
    // A backward wall-clock adjustment must not create an unbounded live socket.
    Duration::from_secs_f64((expires_at - now).clamp(0.0, f64::from(SESSION_SECONDS)))
}

async fn expire_after(
    remaining: Duration,
    cancelled: CancellationToken,
    disconnect: impl FnOnce(),
) {
    tokio::select! {
        biased;
        _ = cancelled.cancelled()=>{},
        _ = tokio::time::sleep(remaining)=>disconnect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test(start_paused = true)]
    async fn idle_expiry_uses_remaining_lifetime_and_cleans_cancelled_timers() {
        let remaining = expiry_delay(10_000.0, 6_400.0);
        assert_eq!(remaining, Duration::from_secs(3600));
        let tracker = TaskTracker::new();
        let closed = Arc::new(AtomicBool::new(false));
        let flag = closed.clone();
        let task = tracker.spawn(expire_after(
            remaining,
            CancellationToken::new(),
            move || {
                flag.store(true, Ordering::SeqCst);
            },
        ));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(3599)).await;
        assert!(!closed.load(Ordering::SeqCst));
        tokio::time::advance(Duration::from_secs(1)).await;
        task.await.unwrap();
        assert!(closed.load(Ordering::SeqCst));
        assert!(tracker.is_empty());
        let cancellation = CancellationToken::new();
        let task = tracker.spawn(expire_after(remaining, cancellation.clone(), || {
            panic!("cancelled session must not disconnect again")
        }));
        tokio::task::yield_now().await;
        cancellation.cancel();
        task.await.unwrap();
        assert!(tracker.is_empty());
        assert_eq!(expiry_delay(99.0, 100.0), Duration::ZERO);
        assert_eq!(
            expiry_delay(f64::MAX, 100.0),
            Duration::from_secs(u64::from(SESSION_SECONDS))
        );
    }
}
