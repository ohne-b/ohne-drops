"""Missing catalogs/details must not erase separately returned Twitch inventory."""

import asyncio
from collections import deque
from copy import deepcopy
from datetime import datetime, timedelta, timezone
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.config import State
from src.config.settings import default_settings
from src.core.client import Twitch
from src.i18n import _
from src.services.inventory_service import InventoryService
from src.web.gui_manager import WebGUIManager


@pytest.fixture(autouse=True)
def unavailable_channel_discovery(monkeypatch):
    monkeypatch.setattr(
        "src.services.campaign_discovery.CampaignDiscovery.fetch", AsyncMock(return_value={}),
    )


def campaign(cid):
    now = datetime.now(timezone.utc)
    start, end = (now - timedelta(days=1)).isoformat(), (now + timedelta(days=1)).isoformat()
    return {
        "id": cid, "name": cid, "game": {"id": "1", "name": "Game"},
        "self": {"isAccountConnected": False}, "accountLinkURL": "https://example.test/link",
        "startAt": start, "endAt": end, "status": "ACTIVE",
        "allow": {"isEnabled": False, "channels": []},
        "timeBasedDrops": [{
            "id": cid + "-drop", "name": "Reward", "startAt": start, "endAt": end,
            "preconditionDrops": [], "requiredMinutesWatched": 30,
            "self": {"currentMinutesWatched": 12, "isClaimed": False, "dropInstanceID": None},
            "benefitEdges": [{"benefit": {"id": "item", "name": "Item",
                "distributionType": "DIRECT_ENTITLEMENT", "imageAssetURL": "https://example.test/item.png"}}],
        }],
    }


@pytest.mark.asyncio
@pytest.mark.parametrize("ongoing", [False, True])
async def test_missing_catalog_keeps_inventory_then_recovers(ongoing):
    twitch = MagicMock()
    twitch.settings.drop_name_blacklist = []
    twitch._drops, twitch._campaigns, twitch.inventory = {}, {}, []
    twitch._mnt_triggers, twitch._mnt_task, twitch._state = deque(), None, State.IDLE
    twitch.gui.inv.add_campaign = AsyncMock()
    twitch._maintenance_service.run_maintenance_task = AsyncMock()
    service = InventoryService(twitch)
    for catalog in (None, [], None, []):
        twitch.gql_request = AsyncMock(side_effect=[
            {"data": {"currentUser": {"inventory": {
                "dropCampaignsInProgress": [campaign("ongoing")] if ongoing else [],
                "gameEventDrops": [],
            }}}},
            {"data": {"currentUser": {"dropCampaigns": catalog}}},
        ])
        await service.fetch_inventory()
        twitch.gui.inv.set_availability.assert_called_with(catalog is not None, recovered=0)
        assert len(twitch.inventory) == int(ongoing)
        if ongoing:
            assert twitch._drops["ongoing-drop"].current_minutes == 12
        await twitch._mnt_task


@pytest.mark.asyncio
async def test_inaccessible_details_skip_summary_and_preserve_ongoing_inventory():
    twitch = MagicMock()
    twitch.settings.drop_name_blacklist = []
    twitch._drops, twitch._campaigns, twitch.inventory = {}, {}, []
    twitch._mnt_triggers, twitch._mnt_task, twitch._state = deque(), None, State.IDLE
    twitch.gui.inv.add_campaign = AsyncMock()
    twitch.get_auth = AsyncMock(return_value=SimpleNamespace(user_id=123))
    twitch._maintenance_service.run_maintenance_task = AsyncMock()
    twitch.gql_request = AsyncMock(side_effect=[
        {"data": {"currentUser": {"inventory": {
            "dropCampaignsInProgress": [campaign("ongoing")], "gameEventDrops": [],
        }}}},
        {"data": {"currentUser": {"dropCampaigns": [
            {"id": cid, "status": "ACTIVE"} for cid in ["new", "ongoing", "inaccessible"]
        ]}}},
        [{"data": {"user": {"dropCampaign": campaign("new")}}},
         {"data": {"user": {"dropCampaign": None}}}, {"data": {"user": None}}],
    ])
    await InventoryService(twitch).fetch_inventory()
    twitch.gui.inv.set_availability.assert_called_once_with(False, recovered=0)
    assert set(twitch._campaigns) == {"new", "ongoing"}
    assert twitch._drops["ongoing-drop"].current_minutes == 12
    await twitch._mnt_task


@pytest.mark.asyncio
async def test_active_summary_without_details_is_unavailable_until_recovery():
    twitch = MagicMock()
    twitch.settings.drop_name_blacklist = []
    twitch._drops, twitch._campaigns, twitch.inventory = {}, {}, []
    twitch._mnt_triggers, twitch._mnt_task, twitch._state = deque(), None, State.IDLE
    twitch.gui.inv.add_campaign = AsyncMock()
    twitch.get_auth = AsyncMock(return_value=SimpleNamespace(user_id=123))
    twitch._maintenance_service.run_maintenance_task = AsyncMock()
    for details in (None, campaign("new")):
        twitch.gql_request = AsyncMock(side_effect=[
            {"data": {"currentUser": {"inventory": {
                "dropCampaignsInProgress": [], "gameEventDrops": [],
            }}}},
            {"data": {"currentUser": {"dropCampaigns": [{"id": "new", "status": "ACTIVE"}]}}},
            [{"data": {"user": {"dropCampaign": details}}}],
        ])
        await InventoryService(twitch).fetch_inventory()
        twitch.gui.inv.set_availability.assert_called_with(details is not None, recovered=0)
        assert len(twitch.inventory) == int(details is not None)
        await twitch._mnt_task


@pytest.mark.asyncio
async def test_cancelled_inventory_waits_for_detail_tasks():
    twitch = MagicMock()
    twitch.gql_request = AsyncMock(side_effect=[
        {"data": {"currentUser": {"inventory": {
            "dropCampaignsInProgress": [], "gameEventDrops": [],
        }}}},
        {"data": {"currentUser": {"dropCampaigns": [{"id": "pending", "status": "ACTIVE"}]}}},
    ])
    service = InventoryService(twitch)
    started, stopped = asyncio.Event(), asyncio.Event()
    async def details(chunk):
        started.set()
        try:
            await asyncio.Event().wait()
        finally:
            await asyncio.sleep(0)
            stopped.set()
    service.fetch_campaigns = details
    task = asyncio.create_task(service.fetch_inventory())
    await started.wait()
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert stopped.is_set()


@pytest.mark.asyncio
@pytest.mark.parametrize("available", [True, False])
@pytest.mark.parametrize("selected", [[], ["Game"]])
async def test_empty_inventory_reports_catalog_status_truthfully(tmp_path, monkeypatch, available, selected):
    monkeypatch.setattr("src.core.client.DATA_DIR", tmp_path)
    client = Twitch(SimpleNamespace(**deepcopy(default_settings)))
    client.settings.games_to_watch = selected
    client.gui = WebGUIManager(client)
    client.gui.inv.availability["available"] = available
    client.get_auth = AsyncMock(return_value=SimpleNamespace(user_id=123))
    client.websocket = MagicMock(start=AsyncMock(), stop=AsyncMock())
    client.fetch_inventory = AsyncMock()
    client._watch_service.watch_loop = AsyncMock()
    expected = _.t["status"]["no_selection" if not selected else "no_campaign" if available else "catalog_unavailable"]
    messages = []
    def output(message, **kwargs):
        messages.append(message)
        if message == expected:
            client.close()
    client.gui.print = output
    await asyncio.wait_for(client.run(), 1)
    await client.shutdown()
    assert expected in messages
