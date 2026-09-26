from __future__ import annotations

import json
from typing import TypedDict

from src.config import LANG_PATH


class StatusMessages(TypedDict):
    terminated: str
    watching: str
    goes_online: str
    goes_offline: str
    claimed_drop: str
    no_channel: str
    no_campaign: str
    no_selection: str
    catalog_unavailable: str


class LoginStatus(TypedDict):
    logged_in: str
    logged_out: str
    logging_in: str
    required: str
    waiting_auth: str


class LoginMessages(TypedDict):
    error_code: str
    unexpected_content: str
    email_code_required: str
    twofa_code_required: str
    incorrect_login_pass: str
    incorrect_email_code: str
    incorrect_twofa_code: str
    status: LoginStatus


class ErrorMessages(TypedDict):
    captcha: str
    no_connection: str
    site_down: str


class GUIStatus(TypedDict):
    name: str
    idle: str
    ready: str
    exiting: str
    terminated: str
    cleanup: str
    gathering: str
    switching: str
    fetching_inventory: str
    fetching_campaigns: str
    adding_campaigns: str


class GUITabs(TypedDict):
    history: str
    main: str
    inventory: str
    settings: str
    help: str


class GUILoginForm(TypedDict):
    name: str
    labels: str
    request: str
    username: str
    password: str
    twofa_code: str
    button: str
    oauth_prompt: str
    oauth_activate: str
    oauth_confirm: str


class GUIWebsocket(TypedDict):
    name: str
    websocket: str
    initializing: str
    connected: str
    disconnected: str
    connecting: str
    disconnecting: str
    reconnecting: str


class GUIProgress(TypedDict):
    name: str
    drop: str
    game: str
    campaign: str
    remaining: str
    drop_progress: str
    campaign_progress: str
    no_drop: str
    return_to_auto: str
    manual_mode_info: str


class GUIChannels(TypedDict):
    name: str
    online: str
    pending: str
    offline: str
    no_channels: str
    no_channels_for_games: str
    channel_count: str
    channel_count_plural: str
    viewers: str


class GUIFooter(TypedDict):
    version: str
    loading: str
    update_available: str


class GUIBadgeItem(TypedDict):
    title: str


class GUIBadges(TypedDict):
    manual: GUIBadgeItem
    auto: GUIBadgeItem
    proxy: GUIBadgeItem


class GUIWanted(TypedDict):
    name: str
    none: str


class GUIInvFilters(TypedDict):
    active: str
    not_linked: str
    upcoming: str
    expired: str
    finished: str
    item: str
    badge: str
    emote: str
    other: str
    clear: str
    search_placeholder: str


class GUIInvStatus(TypedDict):
    active: str
    expired: str
    upcoming: str
    claimed: str
    ignored: str
    skipped: str


class GUIInventory(TypedDict):
    no_campaigns: str
    status: GUIInvStatus
    starts: str
    ends: str
    claimed_drops: str
    ignored_drops: str
    skipped_drops: str
    ignored_keyword_reason: str
    ignored_precondition_reason: str
    skipped_branch_reason: str
    filters: GUIInvFilters


class GUISettingsGeneral(TypedDict):
    name: str
    dark_mode: str


class GUITelegramSettings(TypedDict):
    name: str
    description: str
    bot_token: str
    chat_id: str
    save_settings: str
    test_connection: str
    credentials_help: str
    configured: str
    not_configured: str
    how_to_setup: str
    setup_bot: str
    setup_start: str
    setup_chat: str
    success: str
    saved: str
    error: str
    claim_message: str
    test_message: str
    delivery_failed: str


class GUISettings(TypedDict):
    telegram: GUITelegramSettings
    general: GUISettingsGeneral
    mining_benefits: str
    mining_benefits_help: str
    reload: str
    reload_campaigns: str
    drop_name_blacklist: str
    drop_name_blacklist_help: str
    drop_name_blacklist_placeholder: str
    clear_all_cache: str
    clear_all_cache_help: str
    games_to_watch: str
    games_help: str
    search_games: str
    add_game: str
    add_game_hint: str
    confirm_btn: str
    cancel_btn: str
    remove_game: str
    available_games: str
    no_games_match: str
    multiple_games_found: str
    manual_game_warning: str
    actions: str
    connection_quality: str
    minimum_refresh: str


class GUIHelp(TypedDict):
    about: str
    about_text: str
    how_to_use: str
    how_to_use_items: list[str]
    features: str
    features_items: list[str]
    important_notes: str
    important_notes_items: list[str]
    github_repo: str


class GUIHeader(TypedDict):
    title: str
    initializing: str
    auto_mode: str
    manual_mode: str
    connected: str
    disconnected: str


class GUIAuth(TypedDict):
    title: str
    help: str
    login_title: str
    password: str
    remember: str
    login: str
    logout: str
    current_password: str
    new_password: str
    confirm_password: str
    enable: str
    change: str
    disable: str
    enabled: str
    disabled: str
    invalid_password: str
    password_length: str
    password_mismatch: str
    rate_limited: str
    auth_changed: str
    authentication_required: str
    forbidden: str
    invalid_request: str
    request_failed: str


class GUIHistory(TypedDict):
    title: str
    filter_game: str
    since: str
    apply: str
    export: str
    stats: str
    clear: str
    total: str
    claimed_at: str
    game: str
    campaign: str
    drop: str
    rewards: str
    minutes: str
    loading: str
    empty: str
    count: str
    filtered_count: str
    load_error: str
    previous: str
    next: str
    by_game: str
    by_month: str
    clear_confirm: str
    cleared: str
    clear_error: str
    stats_error: str


class GUIMessages(TypedDict):
    redesign: dict[str, str]
    auth: GUIAuth
    history: GUIHistory
    output: str
    status: GUIStatus
    tabs: GUITabs
    login: GUILoginForm
    websocket: GUIWebsocket
    progress: GUIProgress
    channels: GUIChannels
    inventory: GUIInventory
    settings: GUISettings
    help: GUIHelp
    header: GUIHeader
    footer: GUIFooter
    badges: GUIBadges
    wanted: GUIWanted


class Translation(TypedDict):
    status: StatusMessages
    login: LoginMessages
    error: ErrorMessages
    gui: GUIMessages


class Translator:
    """The application has one English message catalog."""

    def __init__(self) -> None:
        self.t: Translation = json.loads((LANG_PATH / "English.json").read_text(encoding="utf-8"))


_ = Translator()
