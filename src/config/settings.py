from __future__ import annotations

from dataclasses import dataclass
from typing import TypedDict

from yarl import URL

from src.config import SETTINGS_PATH
from src.utils import DropIgnorePolicy, json_load, json_save, merge_json


class InventoryFilters(TypedDict):
    game_name_search: list[str]
    show_active: bool
    show_benefit_badge: bool
    show_benefit_emote: bool
    show_benefit_item: bool
    show_benefit_other: bool
    show_expired: bool
    show_finished: bool
    show_only_not_linked: bool
    show_upcoming: bool


default_settings = {
    "connection_quality": 1,
    "dark_mode": False,
    "drop_name_blacklist": [],
    "games_to_watch": [],
    "inventory_filters_version": 2,
    "inventory_filters": {
        "game_name_search": [],
        "show_active": True,
        "show_benefit_badge": True,
        "show_benefit_emote": True,
        "show_benefit_item": True,
        "show_benefit_other": True,
        "show_expired": False,
        "show_finished": False,
        "show_only_not_linked": False,
        "show_upcoming": True,
    },
    "inventory_list_view": False,
    "minimum_refresh_interval_minutes": 30,
    "mining_benefits": {
        "BADGE": True,
        "DIRECT_ENTITLEMENT": True,
        "EMOTE": True,
        "UNKNOWN": True,
    },
    "proxy": "",
}


@dataclass
class Settings:
    connection_quality: int
    dark_mode: bool
    drop_name_blacklist: list[str]
    games_to_watch: list[str]
    inventory_filters: InventoryFilters
    inventory_filters_version: int
    inventory_list_view: bool
    minimum_refresh_interval_minutes: int
    mining_benefits: dict[str, bool]
    proxy: str

    def __init__(self):
        self.load()

    def load(self):
        # TODO: remvoe customized serde in the future
        settings = json_load(SETTINGS_PATH, default_settings, merge=False)
        # Migrate only the old default preset; keep deliberate custom filters.
        template = default_settings["inventory_filters"]
        assert isinstance(template, dict)
        legacy_filters = {**template, "show_active": False}
        if "inventory_filters_version" not in settings and settings.get("inventory_filters") == legacy_filters:
            settings["inventory_filters"] = dict(template)
        merge_json(settings, default_settings)
        for key, value in settings.items():
            if value is URL:
                setattr(self, key, str(value))
            else:
                setattr(self, key, value)
        self.drop_name_blacklist = DropIgnorePolicy.normalize_keywords(
            self.drop_name_blacklist
        )

    def save(self) -> None:
        self.drop_name_blacklist = DropIgnorePolicy.normalize_keywords(
            self.drop_name_blacklist
        )
        json_save(SETTINGS_PATH, vars(self), sort=True)
