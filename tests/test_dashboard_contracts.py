"""Contracts introduced by the React dashboard, exercised at the Python boundary."""

import asyncio
from datetime import datetime, timezone
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest
from fastapi import HTTPException

from src.config.settings import default_settings
from src.web import app as web
from src.web.managers.campaigns import CampaignProgressManager
from src.web.managers.channels import ChannelListManager
from src.web.managers.inventory import InventoryManager
from src.web.managers.settings import SettingsManager


@pytest.mark.asyncio
async def test_estimates_never_change_confirmed_progress_payload():
    broadcaster = SimpleNamespace(emit=AsyncMock())
    manager = CampaignProgressManager(broadcaster)
    drop = SimpleNamespace(
        id="drop",
        name="Reward",
        campaign=SimpleNamespace(id="campaign", name="Campaign", game=SimpleNamespace(name="Game")),
        current_minutes=48,
        real_current_minutes=42,
        confirmed_at=datetime(2026, 9, 26, tzinfo=timezone.utc),
        required_minutes=60,
        progress=0.8,
    )
    manager.update(drop, 30)
    await asyncio.sleep(0)
    payload = manager.get_current_drop()
    assert payload["confirmed_minutes"] == 42
    assert payload["current_minutes"] == 48
    assert payload["confirmed_at"] == "2026-09-26T00:00:00+00:00"
    broadcaster.emit.assert_awaited_once_with("drop_progress", payload)


@pytest.mark.asyncio
async def test_catalog_availability_survives_snapshot_and_broadcast():
    broadcaster = SimpleNamespace(emit=AsyncMock())
    manager = InventoryManager(broadcaster, MagicMock())
    manager.set_availability(False)
    await asyncio.sleep(0)
    assert manager.availability["available"] is False
    assert manager.availability["checked_at"]
    broadcaster.emit.assert_awaited_once_with("inventory_status", manager.availability)


@pytest.mark.asyncio
async def test_concurrent_settings_writes_require_current_revision(monkeypatch):
    settings = SimpleNamespace(**default_settings, save=MagicMock())
    manager = SettingsManager(SimpleNamespace(emit=AsyncMock()), settings, MagicMock())
    monkeypatch.setattr(web, "gui_manager", SimpleNamespace(settings=manager))
    revision = manager.revision
    results = await asyncio.gather(
        web.update_settings(
            web.SettingsUpdate(revision=revision, minimum_refresh_interval_minutes=45)
        ),
        web.update_settings(
            web.SettingsUpdate(revision=revision, minimum_refresh_interval_minutes=90)
        ),
        return_exceptions=True,
    )
    assert settings.minimum_refresh_interval_minutes == 45
    assert manager.revision != revision
    assert results[0]["success"] is True
    assert isinstance(results[1], HTTPException) and results[1].status_code == 409
    settings.save.assert_called_once()


@pytest.mark.asyncio
async def test_channel_snapshot_tracks_watching_changes():
    manager = ChannelListManager(SimpleNamespace(emit=AsyncMock()))
    manager._channels = {1: {"id": 1, "watching": False}, 2: {"id": 2, "watching": True}}
    manager.set_watching(SimpleNamespace(id=1))
    assert [item["watching"] for item in manager.get_channels()] == [True, False]
    manager.clear_watching()
    assert all(not item["watching"] for item in manager.get_channels())
    await asyncio.sleep(0)
