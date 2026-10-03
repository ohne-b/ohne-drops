use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Clone, Debug)]
pub enum Command {
    Refresh { clear_cache: bool },
    SettingsChanged,
    SelectChannel(u64, Option<Duration>),
    MineChannel(String, Option<Duration>),
    ExitManual,
    ConfirmOAuth,
    Logout,
    Shutdown,
}

pub struct CommandRequest {
    pub command: Command,
    pub complete: oneshot::Sender<Result<(), String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("shutting_down")]
    ShuttingDown,
    #[error("request_failed")]
    Unavailable,
    #[error("twitch_login_required")]
    LoginRequired,
    #[error("settings_conflict")]
    SettingsConflict,
    #[error("invalid_settings")]
    InvalidSettings,
}
