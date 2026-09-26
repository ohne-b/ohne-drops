import asyncio
import copy
import json
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

from src.config.settings import Settings, default_settings
from src.utils import merge_json
from src.web.managers.inventory import InventoryManager


PROJECT_ROOT = Path(__file__).resolve().parents[1]
INDEX_HTML = PROJECT_ROOT / "web" / "index.html"


def test_inventory_filter_defaults_hide_finished_without_restricting_link_state():
    filters = default_settings["inventory_filters"]

    assert filters["show_finished"] is False
    assert filters["show_active"] is True
    assert filters["show_upcoming"] is True
    assert filters["show_only_not_linked"] is False
    assert "show_not_linked" not in filters



def test_legacy_not_linked_setting_migrates_to_neutral_restriction():
    legacy_filters = {
        "show_not_linked": True,
    }

    merge_json(legacy_filters, default_settings["inventory_filters"])

    assert "show_not_linked" not in legacy_filters
    assert legacy_filters["show_only_not_linked"] is False


def test_only_legacy_default_filters_migrate_once(tmp_path, monkeypatch):
    path = tmp_path / "settings.json"
    monkeypatch.setattr("src.config.settings.SETTINGS_PATH", path)
    old = copy.deepcopy(default_settings)
    del old["inventory_filters_version"]
    old["inventory_filters"]["show_active"] = False
    old["games_to_watch"] = ["Rust"]
    path.write_text(json.dumps(old))
    settings = Settings()
    assert settings.inventory_filters["show_active"]
    assert settings.games_to_watch == ["Rust"]
    settings.inventory_filters["show_active"] = False
    settings.save()
    assert not Settings().inventory_filters["show_active"]
    old["inventory_filters"]["game_name_search"] = ["Custom"]
    path.write_text(json.dumps(old))
    assert Settings().inventory_filters == old["inventory_filters"]


def test_benefits_without_artwork_keep_their_filterable_type():
    from tests.test_watch_drop_filtering import _campaign, _drop
    campaign = _campaign("artless", [_drop("drop", "Reward", 10)])
    drop = next(iter(campaign.drops))
    drop.benefits[0].image_url = None
    data = InventoryManager._serialize_drop(drop)
    assert data["benefits"] == [{"name": drop.benefits[0].name,
                                "type": "DIRECT_ENTITLEMENT", "image_url": ""}]


class TestInventoryDropUpdates(unittest.IsolatedAsyncioTestCase):
    async def test_final_claim_refreshes_campaign_counts_in_event_and_cache(self):
        broadcaster = MagicMock()
        broadcaster.emit = AsyncMock()
        manager = InventoryManager(broadcaster, MagicMock())
        starts_at = datetime(2026, 1, 1, tzinfo=timezone.utc)
        ends_at = starts_at + timedelta(days=1)
        campaign = SimpleNamespace(
            id="campaign-1",
            name="Campaign 1",
            game=SimpleNamespace(name="Game 1", box_art_url="https://example.test/game.jpg"),
            campaign_url="https://example.test/campaign",
            link_url="https://example.test/link",
            starts_at=starts_at,
            ends_at=ends_at,
            linked=True,
            active=True,
            upcoming=False,
            expired=False,
        )
        drop = SimpleNamespace(
            id="drop-1",
            name="Drop 1",
            campaign=campaign,
            current_minutes=29,
            real_current_minutes=29,
            required_minutes=30,
            progress=0.97,
            is_claimed=False,
            can_claim=False,
            ignore_reason=None,
            is_mineable=True,
            is_watch_drop=True,
            benefits=[],
            starts_at=starts_at,
            ends_at=ends_at,
        )
        campaign.drops = [drop]

        await manager.add_campaign(campaign)
        broadcaster.emit.reset_mock()

        drop.current_minutes = 30
        drop.real_current_minutes = 30
        drop.progress = 1.0
        drop.is_claimed = True
        drop.is_mineable = False

        manager.update_drop(drop)
        await asyncio.sleep(0)

        campaign_data = manager._campaigns["campaign-1"]
        assert campaign_data["claimed_drops"] == 1
        assert campaign_data["total_drops"] == 1
        broadcaster.emit.assert_awaited_once_with(
            "drop_update",
            {
                "campaign_id": "campaign-1",
                "campaign": {
                    "claimed_drops": 1,
                    "total_drops": 1,
                    "ignored_drops": 0,
                    "skipped_drops": 0,
                    "finished": True,
                    "mining_finished": True,
                },
                "drop": campaign_data["drops"][0],
                "drops": campaign_data["drops"],
            },
        )
