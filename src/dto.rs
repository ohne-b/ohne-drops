use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Settings;

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SettingsView {
    #[serde(flatten)]
    pub values: Settings,
    pub revision: String,
    pub games_available: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BenefitView {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub image_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DropView {
    pub id: String,
    pub name: String,
    pub current_minutes: u32,
    #[serde(default)]
    pub confirmed_minutes: u32,
    #[serde(default)]
    pub confirmed_at: Option<DateTime<Utc>>,
    pub required_minutes: u32,
    pub progress: f64,
    pub is_claimed: bool,
    pub can_claim: bool,
    pub is_ignored: bool,
    pub is_mineable: bool,
    pub is_skipped: bool,
    pub ignored_reason: Option<String>,
    pub ignored_keyword: Option<String>,
    pub ignored_precondition: Option<String>,
    pub benefits: Vec<BenefitView>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CampaignView {
    pub id: String,
    pub name: String,
    pub game_name: String,
    pub game_box_art_url: String,
    pub campaign_url: String,
    pub link_url: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub linked: Option<bool>,
    pub active: bool,
    pub upcoming: bool,
    pub expired: bool,
    pub finished: bool,
    #[serde(default)]
    pub mining_finished: bool,
    pub claimed_drops: usize,
    pub total_drops: usize,
    pub ignored_drops: usize,
    pub skipped_drops: usize,
    pub drops: Vec<DropView>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChannelView {
    pub id: u64,
    #[serde(default)]
    pub login: String,
    pub name: String,
    pub game: Option<String>,
    pub game_id: Option<u64>,
    pub game_icon: Option<String>,
    pub viewers: Option<u64>,
    pub online: bool,
    pub drops_enabled: bool,
    pub acl_based: bool,
    pub watching: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Login {
    pub status: String,
    pub user_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_pending: Option<OAuthCode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthCode {
    pub url: String,
    pub code: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ManualMode {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InventoryStatus {
    pub available: bool,
    pub checked_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefreshState {
    #[default]
    Idle,
    Refreshing,
    Refreshed,
    Failed,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InventoryRefresh {
    pub sequence: u64,
    pub state: RefreshState,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Progress {
    pub drop_id: String,
    pub drop_name: String,
    pub campaign_id: String,
    pub campaign_name: String,
    pub game_name: String,
    pub current_minutes: u32,
    pub confirmed_minutes: u32,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub required_minutes: u32,
    pub progress: f64,
    pub remaining_seconds: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WantedDrop {
    pub name: String,
    pub benefits: Vec<String>,
    #[serde(default)]
    pub image_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WantedCampaign {
    pub id: String,
    pub name: String,
    pub url: String,
    pub drops: Vec<WantedDrop>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WantedGame {
    pub game_name: String,
    pub game_icon: Option<String>,
    pub game_id: Option<u64>,
    pub campaigns: Vec<WantedCampaign>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub status: String,
    pub channels: Vec<ChannelView>,
    pub campaigns: Vec<CampaignView>,
    pub console: Vec<String>,
    pub settings: SettingsView,
    pub login: Login,
    pub manual_mode: ManualMode,
    pub current_drop: Option<Progress>,
    pub wanted_items: Vec<WantedGame>,
    pub inventory_status: InventoryStatus,
    #[serde(default)]
    pub inventory_refresh: InventoryRefresh,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub id: String,
    pub claimed_at: DateTime<Utc>,
    pub game: String,
    pub campaign: String,
    pub drop_name: String,
    pub benefits: Vec<String>,
    pub required_minutes: u32,
    pub campaign_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image_url: String,
}
