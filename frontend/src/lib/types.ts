export interface Benefit {
  name: string;
  type: string;
  image_url: string;
}
export interface Drop {
  id: string;
  name: string;
  current_minutes: number;
  confirmed_minutes?: number;
  required_minutes: number;
  progress: number;
  is_claimed: boolean;
  can_claim: boolean;
  is_ignored: boolean;
  is_mineable: boolean;
  is_skipped: boolean;
  ignored_reason?: string | null;
  ignored_keyword?: string | null;
  ignored_precondition?: string | null;
  benefits: Benefit[];
  starts_at: string;
  ends_at: string;
}
export interface Campaign {
  id: string;
  name: string;
  game_name: string;
  game_box_art_url: string;
  campaign_url: string;
  link_url: string;
  starts_at: string;
  ends_at: string;
  linked: boolean | null;
  active: boolean;
  upcoming: boolean;
  expired: boolean;
  finished: boolean;
  mining_finished?: boolean;
  claimed_drops: number;
  total_drops: number;
  ignored_drops: number;
  skipped_drops: number;
  drops: Drop[];
}
export interface Channel {
  id: number;
  login?: string;
  name: string;
  game: string | null;
  game_id: number | null;
  game_icon: string | null;
  viewers: number | null;
  online: boolean;
  drops_enabled: boolean;
  acl_based: boolean;
  watching: boolean;
}
export interface Filters {
  game_name_search: string[];
  show_active: boolean;
  show_upcoming: boolean;
  show_expired: boolean;
  show_finished: boolean;
  show_only_not_linked: boolean;
  show_benefit_badge: boolean;
  show_benefit_emote: boolean;
  show_benefit_item: boolean;
  show_benefit_other: boolean;
}
export interface Settings {
  revision?: string;
  games_to_watch: string[];
  games_available: string[];
  drop_name_blacklist: string[];
  proxy: string;
  connection_quality: number;
  minimum_refresh_interval_minutes: number;
  mining_benefits: Record<string, boolean>;
  inventory_filters: Filters;
  inventory_list_view: boolean;
}
export interface OAuth {
  url: string;
  code: string;
}
export interface TwitchLogin {
  status: string;
  user_id: number | null;
  oauth_pending?: OAuth;
}
export interface Progress {
  drop_id: string;
  drop_name: string;
  campaign_id: string;
  campaign_name: string;
  game_name: string;
  current_minutes: number;
  confirmed_minutes?: number;
  confirmed_at?: string | null;
  required_minutes: number;
  progress: number;
  remaining_seconds: number;
}
export interface WantedGame {
  game_name: string;
  game_icon?: string;
  game_id?: number;
  campaigns: {
    id: string;
    name: string;
    url: string;
    drops: { name: string; benefits: string[]; image_url?: string }[];
  }[];
}
export interface ManualMode {
  active: boolean;
  game_name?: string;
  channel_name?: string;
  expires_at?: string;
  pending_channel?: string;
  error?: string;
}
export interface InventoryStatus {
  available: boolean;
  checked_at: string | null;
}
export interface InventoryRefresh {
  sequence: number;
  state: 'idle' | 'refreshing' | 'refreshed' | 'failed';
  error: string | null;
}
export interface Snapshot {
  status: string;
  channels: Channel[];
  campaigns: Campaign[];
  console: string[];
  settings: Settings;
  login: TwitchLogin;
  manual_mode: ManualMode;
  current_drop: Progress | null;
  wanted_items: WantedGame[];
  inventory_status?: InventoryStatus;
  inventory_refresh?: InventoryRefresh;
}
export interface HistoryEntry {
  image_url?: string;
  id: string;
  claimed_at: string;
  game: string;
  campaign: string;
  drop_name: string;
  benefits: string[];
  required_minutes: number;
  campaign_id: string;
}
export interface AuthStatus {
  enabled: boolean;
  authenticated: boolean;
  translations?: Record<string, string>;
}
export interface Result {
  success: boolean;
  message?: string;
}
export interface ServerEvents {
  initial_state: (data: Snapshot) => void;
  status_update: (data: { status: string }) => void;
  console_output: (data: { message: string }) => void;
  channel_add: (data: Channel) => void;
  channel_update: (data: Channel) => void;
  channel_remove: (data: { id: number }) => void;
  channels_clear: () => void;
  channels_batch_update: (data: { channels: Channel[] }) => void;
  channel_watching: (data: { id: number }) => void;
  channel_watching_clear: () => void;
  drop_progress: (data: Progress) => void;
  drop_progress_stop: () => void;
  campaign_add: (data: Campaign) => void;
  inventory_clear: () => void;
  inventory_batch_update: (data: { campaigns: Campaign[] }) => void;
  inventory_status: (data: InventoryStatus) => void;
  inventory_refresh: (data: InventoryRefresh) => void;
  drop_update: (data: {
    campaign_id: string;
    campaign: Partial<Campaign>;
    drop: Drop;
    drops?: Drop[];
  }) => void;
  login_required: () => void;
  oauth_code_required: (data: OAuth) => void;
  login_status: (data: TwitchLogin) => void;
  login_clear: (data: { login: boolean; password: boolean; token: boolean }) => void;
  settings_updated: (data: Settings) => void;
  games_available: (data: { games: string[] }) => void;
  manual_mode_update: (data: ManualMode) => void;
  wanted_items_update: (data: WantedGame[]) => void;
  notification: (data: { title: string; message: string }) => void;
  attention_required: (data: { sound: boolean }) => void;
}
export interface ReleaseInfo {
  current_version: string;
  latest_version: string | null;
  update_available: boolean;
  check_succeeded: boolean;
  download_url: string;
}
