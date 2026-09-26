import asyncio
import json
from copy import deepcopy
from datetime import datetime, timedelta, timezone
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.campaign_history import CampaignHistory
from src.models import DropsCampaign
from src.web import app as web
from src.web.managers.inventory import InventoryManager
from tests.test_catalog_availability import campaign


def manager(path):
    broadcaster = MagicMock(emit=AsyncMock())
    return InventoryManager(broadcaster, MagicMock(), CampaignHistory(path))


def model(cid, *, claimed=False, expired=False):
    data = campaign(cid)
    data["timeBasedDrops"][0]["self"]["isClaimed"] = claimed
    if expired:
        data["endAt"] = (datetime.now(timezone.utc) - timedelta(hours=1)).isoformat()
        data["status"] = "EXPIRED"
    twitch = MagicMock()
    twitch.settings.drop_name_blacklist = []
    return DropsCampaign(twitch, data, {})


@pytest.mark.asyncio
async def test_completed_campaign_survives_refresh_clear_restart_and_api(tmp_path, monkeypatch):
    path = tmp_path / "completed_campaigns.json"
    inv = manager(path)
    completed = model("completed", claimed=True, expired=True)
    await inv.add_campaign(completed)
    await inv.add_campaign(model("expired", expired=True))
    assert {c["id"]: c["finished"] for c in inv.get_campaigns()} == {"completed": True, "expired": False}
    inv.refresh_campaigns([])
    inv.clear()
    await asyncio.sleep(0)
    assert [c["id"] for c in inv.get_campaigns()] == ["completed"]
    assert inv._broadcaster.emit.call_args.args[1]["campaigns"][0]["finished"]
    restarted = manager(path)
    restarted.start_batch()
    await restarted.finalize_batch()
    monkeypatch.setattr(web, "gui_manager", MagicMock(inv=restarted))
    result = await web.get_campaigns()
    assert len(result["campaigns"]) == 1
    assert result["campaigns"][0]["finished"] and result["campaigns"][0]["expired"]


@pytest.mark.asyncio
async def test_last_claim_archives_immediately_but_ignored_and_expired_do_not(tmp_path):
    path = tmp_path / "completed_campaigns.json"
    inv = manager(path)
    incomplete = model("incomplete")
    incomplete._twitch.settings.drop_name_blacklist = ["Reward"]
    await inv.add_campaign(incomplete)
    await inv.add_campaign(model("expired", expired=True))
    assert not path.exists()
    drop = next(iter(incomplete.drops))
    drop.is_claimed = True
    drop.real_current_minutes = drop.required_minutes
    inv.update_drop(drop)
    assert manager(path).get_campaigns()[0]["id"] == "incomplete"
    await asyncio.sleep(0)


@pytest.mark.asyncio
async def test_failed_atomic_write_preserves_previous_file_and_retries(tmp_path, monkeypatch):
    path = tmp_path / "completed_campaigns.json"
    inv = manager(path)
    await inv.add_campaign(model("first", claimed=True))
    original = path.read_bytes()
    with monkeypatch.context() as patch:
        patch.setattr("src.utils.json_utils.os.replace", MagicMock(side_effect=OSError("disk full")))
        await inv.add_campaign(model("second", claimed=True))
    assert path.read_bytes() == original
    data = inv._campaigns["second"]
    inv._history.record(data)
    assert set(CampaignHistory(path).get_campaigns()) == {"first", "second"}


@pytest.mark.parametrize("contents", ['{broken', '{"version": 99, "campaigns": []}', '{"version": 1, "campaigns": [{}]}'])
@pytest.mark.asyncio
async def test_unreadable_archive_is_never_overwritten(tmp_path, contents):
    path = tmp_path / "completed_campaigns.json"
    path.write_text(contents)
    inv = manager(path)
    await inv.add_campaign(model("completed", claimed=True))
    assert path.read_text() == contents


@pytest.mark.asyncio
async def test_archive_does_not_override_new_authoritative_campaign_state(tmp_path):
    path = tmp_path / "completed_campaigns.json"
    inv = manager(path)
    await inv.add_campaign(model("same", claimed=True))
    await inv.add_campaign(model("same"))
    assert not inv.get_campaigns()[0]["finished"]
    inv.clear()
    assert inv.get_campaigns() == []
    assert manager(path).get_campaigns() == []
    await asyncio.sleep(0)
    await inv.add_campaign(model("same", claimed=True))
    contents = json.loads(path.read_text())
    invalid = deepcopy(contents)
    invalid["campaigns"][0]["drops"][0]["is_claimed"] = False
    path.write_text(json.dumps(invalid))
    assert CampaignHistory(path).get_campaigns() == {}


@pytest.mark.asyncio
async def test_metadata_only_refresh_preserves_proven_completion(tmp_path):
    path = tmp_path / "completed_campaigns.json"
    inv = manager(path)
    await inv.add_campaign(model("same", claimed=True))
    unknown = model("same")
    for drop in unknown.drops:
        drop.confirmed_at = None
    await inv.add_campaign(unknown)
    assert inv.get_campaigns()[0]["finished"]
    assert inv._broadcaster.emit.call_args.args[1]["finished"]
    assert not unknown.finished  # display history never manufactures miner progress
    inv.clear()
    assert manager(path).get_campaigns()[0]["finished"]
    await asyncio.sleep(0)


@pytest.mark.asyncio
@pytest.mark.parametrize("invalid", ["benefits", "totals", "duplicate"])
async def test_malformed_ui_snapshot_stays_readonly(tmp_path, invalid):
    path = tmp_path / "completed_campaigns.json"
    inv = manager(path)
    await inv.add_campaign(model("complete", claimed=True))
    contents = json.loads(path.read_text())
    data = contents["campaigns"][0]
    if invalid == "benefits":
        data["drops"][0]["benefits"] = {}
    elif invalid == "totals":
        data["total_drops"] = 9
    else:
        data["drops"].append(deepcopy(data["drops"][0]))
        data["total_drops"] = data["claimed_drops"] = 2
    path.write_text(json.dumps(contents))
    original = path.read_bytes()
    restarted = manager(path)
    assert restarted.get_campaigns() == []
    await restarted.add_campaign(model("new", claimed=True))
    assert path.read_bytes() == original
