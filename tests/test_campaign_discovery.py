"""Campaign recovery stays on Twitch and preserves account truth/eligibility."""

import asyncio
from collections import deque
from datetime import datetime, timedelta, timezone
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.config import State
from src.exceptions import ExitRequest, GQLException, LoginException
from src.models import DropsCampaign, Game
from src.models.channel import Channel, Stream
from src.services.campaign_discovery import CampaignDiscovery
from src.services.inventory_service import InventoryService
from src.services.watch_service import WatchService
from tests.test_catalog_availability import campaign


def metadata(cid="recovered"):
    data = campaign(cid)
    del data["self"]
    for drop in data["timeBasedDrops"]:
        del drop["self"]
    return data


def game_channels(*ids):
    return {"streams": {"edges": [
        {"node": {"broadcaster": {"id": cid, "login": "channel" + cid, "displayName": "Channel " + cid}}}
        for cid in ids
    ]}}


def directory(*ids, cursor="next", more=False):
    return {"data": {"games": {
        "edges": [{"cursor": cursor, "node": game_channels(*ids)}],
        "pageInfo": {"hasNextPage": more},
    }}}


def available(channel_id, *campaigns):
    return {"data": {"channel": {"id": channel_id, "viewerDropCampaigns": list(campaigns)}}}


def client():
    twitch = MagicMock()
    twitch.settings.drop_name_blacklist = []
    twitch.settings.games_to_watch = []
    twitch._drops, twitch._campaigns, twitch.inventory = {}, {}, []
    twitch._mnt_triggers, twitch._mnt_task, twitch._state = deque(), None, State.IDLE
    twitch.gui.inv.add_campaign = AsyncMock()
    twitch.get_auth = AsyncMock(return_value=SimpleNamespace(user_id=123))
    twitch._maintenance_service.run_maintenance_task = AsyncMock()
    return twitch


@pytest.mark.asyncio
async def test_discovery_paginates_deduplicates_and_keeps_real_twitch_metadata():
    twitch = client()
    twitch.gql_request = AsyncMock(side_effect=[
        [{"data": {"game": game_channels("10")}}],
        directory("10", "20", more=True), directory("30", cursor="last"),
        [available("10", metadata()), available("20", metadata()), {"data": {"channel": None}}],
    ])
    data = await CampaignDiscovery(twitch).fetch([Game({"id": "1", "name": "Game"})])
    recovered = data["recovered"]
    assert recovered["self"] == {"isAccountConnected": None}
    assert "self" not in recovered["timeBasedDrops"][0]
    assert [channel["id"] for channel in recovered["discovery_channels"]] == ["10", "20"]
    assert recovered["timeBasedDrops"] == metadata()["timeBasedDrops"]
    requests = twitch.gql_request.call_args_list
    assert requests[2].args[0]["variables"] == {"after": "next"}
    assert [op["variables"]["channelID"] for op in requests[3].args[0]] == ["10", "20", "30"]
    assert "self" not in requests[3].args[0][0]["query"]


@pytest.mark.asyncio
@pytest.mark.parametrize("failure", [GQLException("unavailable"), TimeoutError()])
async def test_discovery_keeps_completed_batches_when_later_request_fails(failure):
    twitch = client()
    twitch.gql_request = AsyncMock(side_effect=[
        directory(*(str(i) for i in range(21))),
        [available("0", metadata())], failure,
    ])
    assert set(await CampaignDiscovery(twitch).fetch([])) == {"recovered"}


@pytest.mark.asyncio
async def test_discovery_is_bounded_and_cancellation_propagates():
    twitch = client()
    twitch.gql_request = AsyncMock(side_effect=[directory(cursor=str(i), more=True) for i in range(5)])
    assert await CampaignDiscovery(twitch).fetch([]) == {}
    assert twitch.gql_request.await_count == 5
    started, stopped = asyncio.Event(), asyncio.Event()
    async def pending(_op):
        started.set()
        try:
            await asyncio.Event().wait()
        finally:
            stopped.set()
    twitch.gql_request = pending
    task = asyncio.create_task(CampaignDiscovery(twitch).fetch([]))
    await started.wait()
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert stopped.is_set()


@pytest.mark.asyncio
@pytest.mark.parametrize("error", [ExitRequest(), LoginException("Login required")])
async def test_discovery_never_swallows_shutdown_or_authentication_errors(error):
    twitch = client()
    twitch.gql_request = AsyncMock(side_effect=error)
    with pytest.raises(type(error)):
        await CampaignDiscovery(twitch).fetch([])


@pytest.mark.asyncio
async def test_null_edges_and_responses_do_not_discard_valid_neighbors():
    twitch = client()
    page = directory("10")
    edges = page["data"]["games"]["edges"]
    edges[0]["node"]["streams"]["edges"].extend([
        None, {"node": None}, {"node": {"broadcaster": None}}, "invalid",
    ])
    edges.extend([None, {"node": None}, "invalid"])
    twitch.gql_request = AsyncMock(side_effect=[
        page, [None, {"data": None}, available("10", None, "invalid", {}, metadata())],
    ])
    assert set(await CampaignDiscovery(twitch).fetch([])) == {"recovered"}


@pytest.mark.parametrize("evidence", ["partial", "complete", "outside_window", "empty_benefits"])
def test_claimed_benefit_inference_requires_complete_account_evidence(evidence):
    data = metadata()
    data["self"] = {"isAccountConnected": None}
    data["discovery_channels"] = [{"id": "10", "name": "source"}]
    benefits = data["timeBasedDrops"][0]["benefitEdges"]
    benefits.append({"benefit": {**benefits[0]["benefit"], "id": "second"}})
    now = datetime.now(timezone.utc)
    awarded = {"item": now}
    if evidence in ("complete", "outside_window"):
        awarded["second"] = now if evidence == "complete" else now - timedelta(days=2)
    if evidence == "empty_benefits":
        benefits.clear()
    recovered = DropsCampaign(client(), data, awarded)
    drop = next(iter(recovered.drops))
    assert drop.is_claimed is (evidence == "complete")
    assert drop.real_current_minutes == (30 if evidence == "complete" else 0)
    assert drop.is_mineable is (evidence not in ("complete", "empty_benefits"))
    assert drop.claim_id is None and drop.confirmed_at is None
    authoritative = next(iter(DropsCampaign(client(), campaign("account"), awarded).drops))
    assert not authoritative.is_claimed and authoritative.real_current_minutes == 12


@pytest.mark.asyncio
async def test_null_catalog_recovers_through_actual_service_and_preserves_inventory():
    twitch = client()
    ongoing = campaign("ongoing")
    ongoing["timeBasedDrops"][0]["self"]["isClaimed"] = True
    twitch.gql_request = AsyncMock(side_effect=[
        {"data": {"currentUser": {"inventory": {
            "dropCampaignsInProgress": [ongoing], "gameEventDrops": [],
        }}}},
        {"data": {"currentUser": {"dropCampaigns": None}}},
        [{"data": {"game": game_channels("10")}}], directory("10"),
        [available("10", metadata(), metadata("ongoing"))],
    ])
    await InventoryService(twitch).fetch_inventory()
    await twitch._mnt_task
    assert set(twitch._campaigns) == {"ongoing", "recovered"}
    assert twitch._drops["ongoing-drop"].is_claimed
    assert twitch._campaigns["ongoing"].discovery_channels is None
    recovered = twitch._campaigns["recovered"]
    assert recovered.linked is None
    drop = twitch._drops["recovered-drop"]
    assert (drop.real_current_minutes, drop.claim_id, drop.confirmed_at) == (0, None, None)
    assert not drop.can_claim
    twitch.gui.inv.set_availability.assert_called_once_with(False, recovered=1)

    # Starting estimates cannot manufacture confirmed minutes or a claim.
    channel = recovered.discovery_channels[0]
    channel._stream = Stream(channel, id="999", game={"id": "1", "name": "Game"}, viewers=10, title="Live")
    twitch.wanted_games = [recovered.game]
    assert WatchService(twitch).can_watch(channel)
    drop._bump_minutes(channel)
    assert (drop.real_current_minutes, drop.extra_current_minutes) == (0, 1)
    assert not await drop.claim()
    drop.update_minutes(5)  # a later Twitch progress update
    assert drop.real_current_minutes == 5 and drop.extra_current_minutes == 0
    assert drop.confirmed_at is not None


@pytest.mark.asyncio
async def test_missing_details_recover_only_listed_campaigns_and_keep_known_link_state():
    twitch = client()
    twitch.gql_request = AsyncMock(side_effect=[
        {"data": {"currentUser": {"inventory": {"dropCampaignsInProgress": [], "gameEventDrops": []}}}},
        {"data": {"currentUser": {"dropCampaigns": [
            {"id": "listed", "status": "ACTIVE", "self": {"isAccountConnected": False}},
            {"id": "expired", "status": "EXPIRED"},
        ]}}},
        [{"data": {"user": {"dropCampaign": None}}}], directory("10"),
        [available("10", metadata("listed"), metadata("expired"), metadata("unlisted"))],
    ])
    await InventoryService(twitch).fetch_inventory()
    await twitch._mnt_task
    assert set(twitch._campaigns) == {"listed"}
    assert twitch._campaigns["listed"].linked is False


@pytest.mark.asyncio
async def test_empty_catalog_does_not_start_discovery():
    twitch = client()
    twitch.gql_request = AsyncMock(side_effect=[
        {"data": {"currentUser": {"inventory": {"dropCampaignsInProgress": [], "gameEventDrops": []}}}},
        {"data": {"currentUser": {"dropCampaigns": []}}},
    ])
    await InventoryService(twitch).fetch_inventory()
    await twitch._mnt_task
    assert twitch.gql_request.await_count == 2
    twitch.gui.inv.set_availability.assert_called_once_with(True, recovered=0)


def test_recovered_campaign_keeps_channel_acl_timing_prerequisites_and_ignore_rules():
    twitch = client()
    data = metadata()
    data["self"] = {"isAccountConnected": None}
    data["discovery_channels"] = [{"id": "10", "name": "source"}]
    recovered = DropsCampaign(twitch, data, {})
    source = recovered.discovery_channels[0]
    source._stream = Stream(source, id="999", game={"id": "1", "name": "Game"}, viewers=10, title="Live")
    other = Channel(twitch, id=20, login="other")
    other._stream = Stream(other, id="1000", game={"id": "1", "name": "Game"}, viewers=10, title="Live")
    assert recovered.can_earn(source)
    assert not recovered.can_earn(other)
    assert not recovered.can_earn(other, ignore_channel_status=True)
    recovered.allowed_channels = [other]
    assert not recovered.can_earn(source)
    recovered.allowed_channels = []
    drop = next(iter(recovered.drops))
    drop.precondition_drops = ["missing"]
    assert not recovered.can_earn(source)
    drop.precondition_drops = []
    twitch.settings.drop_name_blacklist = ["Reward"]
    assert not recovered.can_earn(source)
    twitch.settings.drop_name_blacklist = []
    recovered.ends_at = datetime.now(timezone.utc) - timedelta(seconds=1)
    assert not recovered.can_earn(source)


@pytest.mark.asyncio
async def test_malformed_subscription_and_expired_recovery_cannot_break_inventory():
    twitch = client()
    subscription = metadata("subscription")
    subscription["timeBasedDrops"][0]["requiredMinutesWatched"] = 0
    malformed = metadata("broken")
    malformed["startAt"] = "invalid"
    expired = metadata("expired")
    expired["endAt"] = (datetime.now(timezone.utc) - timedelta(days=2)).isoformat()
    twitch.gql_request = AsyncMock(side_effect=[
        {"data": {"currentUser": {"inventory": {"dropCampaignsInProgress": [], "gameEventDrops": []}}}},
        {"data": {"currentUser": {"dropCampaigns": None}}}, directory("10"),
        [available("10", malformed, subscription, expired, metadata())],
    ])
    await InventoryService(twitch).fetch_inventory()
    await twitch._mnt_task
    assert set(twitch._campaigns) == {"recovered"}
    twitch.gui.inv.set_availability.assert_called_once_with(False, recovered=1)
