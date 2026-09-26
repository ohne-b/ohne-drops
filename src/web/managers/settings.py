"""Settings manager for application configuration."""

from __future__ import annotations

import asyncio
import copy
import logging
import secrets
from collections.abc import Callable
from typing import TYPE_CHECKING, Any

from src.config.settings import default_settings
from src.models.game import Game
from src.services.telegram_service import TELEGRAM_TOKEN_MASK, TelegramNotifier
from src.utils import DropIgnorePolicy, merge_json


logger = logging.getLogger("TwitchDrops")

if TYPE_CHECKING:
    from src.config.settings import Settings
    from src.web.managers.broadcaster import WebSocketBroadcaster
    from src.web.managers.console import ConsoleOutputManager


class SettingsManager:
    """Manages application settings in the web interface.

    Provides access to and modification of user preferences including
    game priorities, proxy configuration, and UI preferences.
    """

    def __init__(
        self,
        broadcaster: WebSocketBroadcaster,
        settings: Settings,
        console: ConsoleOutputManager,
        on_change: Callable[[], None] | None = None,
    ):
        self._broadcaster = broadcaster
        self._settings = settings
        self._console = console
        self._on_change = on_change
        self._available_games: list[str] = []
        self.revision = secrets.token_hex(16)

    def get_settings(self, legacy_show_not_linked: bool | None = None) -> dict[str, Any]:
        """Get current settings for display.

        Args:
            legacy_show_not_linked: Request-scoped value echoed only in the
                immediate settings POST response for a legacy frontend. It is
                never persisted, mapped to ``show_only_not_linked``, or returned
                by a later GET/page reload.

        Returns:
            Dictionary containing all user-configurable settings
        """
        settings = vars(self._settings).copy()
        settings["games_available"] = self._available_games
        settings["revision"] = self.revision
        settings["telegram_configured"] = bool(settings.get("telegram_bot_token"))
        settings["telegram_bot_token"] = TELEGRAM_TOKEN_MASK if settings["telegram_configured"] else ""
        # TODO(remove in 1.3.x): Retain this POST-only echo long enough for stale
        # pre-versioned frontends to age out; it never survives a page reload.
        if legacy_show_not_linked is not None:
            inventory_filters = copy.deepcopy(dict(self._settings.inventory_filters))
            inventory_filters["show_not_linked"] = legacy_show_not_linked
            settings["inventory_filters"] = inventory_filters
        return settings

    def _log_change(self, message: str):
        """Log setting change to both console and system logger."""
        self._console.print(message)

    def update_settings(self, settings_data: dict[str, Any]) -> dict[str, Any]:
        """Update settings from user input.

        Args:
            settings_data: Dictionary of settings to update
        """
        previous = {key: copy.deepcopy(getattr(self._settings, key))
                    for key in default_settings if hasattr(self._settings, key)}
        should_trigger_update = False
        should_trigger_update |= self.check_and_update_setting(
            "games_to_watch", settings_data.get("games_to_watch"), True
        )
        drop_name_blacklist = settings_data.get("drop_name_blacklist")
        if drop_name_blacklist is not None:
            drop_name_blacklist = DropIgnorePolicy.normalize_keywords(
                drop_name_blacklist
            )
        should_trigger_update |= self.check_and_update_setting(
            "drop_name_blacklist", drop_name_blacklist, True
        )
        should_trigger_update |= self.check_and_update_setting(
            "dark_mode", settings_data.get("dark_mode")
        )
        should_trigger_update |= self.check_and_update_setting(
            "connection_quality", settings_data.get("connection_quality")
        )
        if "proxy" in settings_data:
            proxy_value = settings_data["proxy"]
            should_trigger_update |= self.check_and_update_setting(
                "proxy",
                str(proxy_value).strip() if proxy_value else "",
                True,
                lambda proxy: self._log_change("Proxy cleared") if proxy == "" else None,
            )
        should_trigger_update |= self.check_and_update_setting(
            "minimum_refresh_interval_minutes",
            settings_data.get("minimum_refresh_interval_minutes"),
        )
        token = (settings_data.get("telegram_bot_token") or "").strip()
        if token and token != TELEGRAM_TOKEN_MASK:
            self.check_and_update_setting("telegram_bot_token", token)
        if settings_data.get("telegram_chat_id") is not None:
            self.check_and_update_setting("telegram_chat_id", settings_data["telegram_chat_id"].strip())
        inventory_filters = settings_data.get("inventory_filters")
        legacy_show_not_linked = None
        if inventory_filters is not None:
            legacy_value = inventory_filters.get("show_not_linked")
            if isinstance(legacy_value, bool):
                legacy_show_not_linked = legacy_value
            inventory_filters = self._normalize_inventory_filters(inventory_filters)
        should_trigger_update |= self.check_and_update_setting("inventory_filters", inventory_filters)
        should_trigger_update |= self.check_and_update_setting(
            "inventory_list_view", settings_data.get("inventory_list_view")
        )
        should_trigger_update |= self.check_and_update_setting(
            "mining_benefits", settings_data.get("mining_benefits"), True
        )

        try:
            self._settings.save()
        except Exception:
            for key, value in previous.items():
                setattr(self._settings, key, value)
            raise
        self.revision = secrets.token_hex(16)
        response_settings = self.get_settings(legacy_show_not_linked)
        asyncio.create_task(self._broadcaster.emit("settings_updated", response_settings))

        if should_trigger_update and self._on_change:
            self._on_change()

        return response_settings

    async def test_telegram(self, bot_token: str, chat_id: str) -> bool:
        """Test draft credentials without persisting them; blank token reuses the saved one."""
        token = bot_token.strip()
        if not token or token == TELEGRAM_TOKEN_MASK:
            token = self._settings.telegram_bot_token
        return await TelegramNotifier(token, chat_id.strip()).test_connection()

    def _normalize_inventory_filters(self, updates: dict[str, Any]) -> dict[str, Any]:
        """Merge partial filter updates and discard legacy or unknown keys."""
        current: dict[str, Any] = copy.deepcopy(dict(self._settings.inventory_filters))
        current.pop("show_not_linked", None)
        current.update(updates)
        current.pop("show_not_linked", None)
        template = default_settings["inventory_filters"]
        assert isinstance(template, dict)
        merge_json(current, template)
        return current

    def check_and_update_setting(
        self,
        key: str,
        new_value: Any,
        should_trigger_update: bool = False,
        action: Callable[[Any], None] = lambda x: None,
    ):
        if new_value is None or getattr(self._settings, key, None) == new_value:
            return False
        setattr(self._settings, key, new_value)
        log_value = (
            TELEGRAM_TOKEN_MASK
            if key in {"proxy", "telegram_bot_token", "telegram_chat_id"}
            else new_value
        )
        self._log_change(f"Setting changed: {key} = {log_value}")
        action(new_value)
        return should_trigger_update

    def set_games(self, games: set[Game]):
        """Update the list of available games for settings panel.

        Args:
            games: Set of Game objects discovered from campaigns
        """
        # Store and broadcast available games for settings panel
        game_names = sorted([g.name for g in games])
        self._available_games = game_names
        asyncio.create_task(self._broadcaster.emit("games_available", {"games": game_names}))
