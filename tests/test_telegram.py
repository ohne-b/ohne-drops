"""Telegram restoration: no real accounts, HTTP requests or messages."""

import asyncio
import copy
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.config.settings import default_settings
from src.models.drop import BaseDrop
from src.services.message_handlers import MessageHandlerService
from src.services.telegram_service import TELEGRAM_TOKEN_MASK, TelegramNotifier
from src.web.managers.settings import SettingsManager
from tests.test_watch_drop_filtering import _campaign, _drop
# Reuse the production HTTP/auth fixture, which isolates every credential store.
from tests.test_web_auth import client, hashed, login, protected, web  # noqa: F401


TOKEN = "123456:synthetic_secret"


@pytest.fixture
def manager(client, monkeypatch):
    settings = SimpleNamespace(**copy.deepcopy(default_settings))
    settings.telegram_bot_token = TOKEN
    settings.telegram_chat_id = "42"
    settings.save = MagicMock()
    manager = SettingsManager(AsyncMock(), settings, MagicMock())
    monkeypatch.setattr(web, "gui_manager", SimpleNamespace(settings=manager))
    return manager


@pytest.mark.parametrize("token", ["", TELEGRAM_TOKEN_MASK, "123:new_secret"])
def test_settings_preserve_or_replace_and_mask_credentials(client, manager, token):
    response = client.post("/api/settings", json={
        "telegram_bot_token": token, "telegram_chat_id": " -7 ",
        "revision": manager.revision,
    })
    assert response.status_code == 200
    expected = token if token and token != TELEGRAM_TOKEN_MASK else TOKEN
    assert manager._settings.telegram_bot_token == expected
    assert manager._settings.telegram_chat_id == "-7"
    assert response.json()["settings"]["telegram_bot_token"] == TELEGRAM_TOKEN_MASK
    assert response.json()["settings"]["telegram_configured"] is True
    assert expected not in response.text + client.get("/api/settings").text
    assert expected not in str(manager._broadcaster.emit.call_args_list)
    assert expected not in str(manager._console.print.call_args_list)


def test_clear_chat_disables_without_erasing_token(client, manager):
    response = client.post("/api/settings", json={"telegram_chat_id": ""})
    assert response.status_code == 200
    assert manager._settings.telegram_chat_id == ""
    assert manager._settings.telegram_bot_token == TOKEN


@pytest.mark.parametrize("token", ["", TELEGRAM_TOKEN_MASK, "123:replacement"])
def test_test_endpoint_reuses_or_replaces_without_saving(client, manager, monkeypatch, token):
    sent = []

    async def send(self, text):
        sent.append((self.bot_token, self.chat_id))
        return True

    monkeypatch.setattr(TelegramNotifier, "_send_message", send)
    response = client.post("/api/settings/test-telegram", json={
        "telegram_bot_token": token, "telegram_chat_id": " 7 ",
    })
    assert response.json() == {"success": True}
    assert sent == [(token if token and token != TELEGRAM_TOKEN_MASK else TOKEN, "7")]
    manager._settings.save.assert_not_called()
    assert manager._settings.telegram_chat_id == "42"
    assert TOKEN not in response.text


def test_telegram_requires_auth_and_same_origin_write_header(protected, manager, monkeypatch):
    send = AsyncMock(return_value=True)
    monkeypatch.setattr(TelegramNotifier, "test_connection", send)
    path = "/api/settings/test-telegram"
    assert protected.post(path, json={}).status_code == 401
    assert login(protected).status_code == 200
    assert protected.post(path, json={}, headers={"X-TDM-Request": ""}).status_code == 403
    assert protected.post(path, json={}, headers={"Origin": "https://foreign.test"}).status_code == 403
    send.assert_not_awaited()


@pytest.mark.parametrize("path", ["/api/settings", "/api/settings/test-telegram"])
def test_credential_validation_errors_never_echo_input(client, manager, path):
    response = client.post(path, json={"telegram_bot_token": TOKEN * 100})
    assert response.status_code == 422
    assert TOKEN not in response.text
    assert response.json() == {"detail": "invalid_request"}


@pytest.mark.asyncio
async def test_failed_settings_save_restores_credentials_and_does_not_broadcast(manager):
    revision = manager.revision
    manager._settings.save.side_effect = OSError("disk full")
    with pytest.raises(OSError):
        manager.update_settings({"telegram_bot_token": "123:new", "telegram_chat_id": "7"})
    assert manager._settings.telegram_bot_token == TOKEN
    assert manager._settings.telegram_chat_id == "42"
    assert manager.revision == revision
    manager._broadcaster.emit.assert_not_called()


@pytest.fixture
def transport(monkeypatch):
    session_factory = MagicMock()
    session = session_factory.return_value
    session.__aenter__.return_value = session
    response = session.post.return_value.__aenter__.return_value
    response.status = 200
    response.json = AsyncMock(return_value={"ok": True})
    monkeypatch.setattr("src.services.telegram_service.aiohttp.ClientSession", session_factory)
    return session_factory, session, response


@pytest.mark.asyncio
async def test_transport_success_has_timeout_fixed_destination_and_no_redirects(transport):
    factory, session, _ = transport
    assert await TelegramNotifier(TOKEN, "42").test_connection()
    args, kwargs = session.post.call_args
    assert args == (f"https://api.telegram.org/bot{TOKEN}/sendMessage",)
    assert kwargs["timeout"].total == 10
    assert kwargs["allow_redirects"] is False
    assert kwargs["json"]["chat_id"] == "42"
    assert kwargs["json"]["parse_mode"] == "HTML"
    assert "test successful" in kwargs["json"]["text"]
    session.__aexit__.assert_awaited_once()


@pytest.mark.asyncio
@pytest.mark.parametrize("failure", ["http", "rejected", "timeout", "exception", "invalid_json"])
async def test_delivery_failures_are_safe_and_credential_free(transport, caplog, failure):
    _, session, response = transport
    if failure == "http":
        response.status = 403
    elif failure == "rejected":
        response.json.return_value = {"ok": False, "description": TOKEN}
    elif failure == "invalid_json":
        response.json.side_effect = ValueError(TOKEN)
    else:
        session.post.return_value.__aenter__.side_effect = (
            TimeoutError(TOKEN) if failure == "timeout" else OSError(TOKEN)
        )
    assert not await TelegramNotifier(TOKEN, "42").test_connection()
    assert TOKEN not in caplog.text
    response.text.assert_not_awaited()
    session.__aexit__.assert_awaited_once()


@pytest.mark.asyncio
@pytest.mark.parametrize("token,chat", [("", "42"), (TOKEN, ""), ("invalid/token", "42")])
async def test_missing_or_invalid_credentials_do_not_contact_telegram(transport, token, chat):
    assert not await TelegramNotifier(token, chat).test_connection()
    transport[0].assert_not_called()


@pytest.fixture
def claimed_drop(monkeypatch):
    campaign = _campaign("telegram", [_drop("watch", "Watch", 30)])
    twitch = campaign._twitch
    twitch.settings.telegram_bot_token = TOKEN
    twitch.settings.telegram_chat_id = "42"
    twitch.gui.broadcast_wanted_items_now = AsyncMock()
    twitch.gql_request = AsyncMock(return_value={"data": {"claimDropRewards": {"status": "ELIGIBLE_FOR_ALL"}}})
    drop = campaign.timed_drops["watch"]
    drop.update_claim("instance")
    twitch._drops = {drop.id: drop}
    twitch.watching_channel.get_with_default.return_value = None
    send = AsyncMock(return_value=True)
    monkeypatch.setattr(TelegramNotifier, "_send_message", send)
    monkeypatch.setattr("src.services.message_handlers.asyncio.sleep", AsyncMock())
    return drop, twitch, send


@pytest.mark.asyncio
async def test_direct_then_repeated_websocket_claims_notify_once(claimed_drop):
    drop, twitch, send = claimed_drop
    assert await drop.claim()
    message = {"type": "drop-claim", "data": {"drop_id": drop.id, "drop_instance_id": "instance"}}
    await MessageHandlerService(twitch).process_drops(1, message)
    await MessageHandlerService(twitch).process_drops(1, message)
    send.assert_awaited_once()
    twitch.drop_history.record.assert_called_once_with(drop, drop.campaign)
    twitch.gui.broadcast_wanted_items_now.assert_awaited_once()


@pytest.mark.asyncio
async def test_websocket_and_base_drop_claims_notify(claimed_drop):
    drop, twitch, send = claimed_drop
    await MessageHandlerService(twitch).process_drops(1, {
        "type": "drop-claim", "data": {"drop_id": drop.id, "drop_instance_id": "instance"},
    })
    send.assert_awaited_once()
    base = BaseDrop(drop.campaign, _drop("base", "Base", 0), {})
    base.update_claim("other-instance")
    assert await base.claim()
    assert send.await_count == 2


@pytest.mark.asyncio
async def test_failed_or_disabled_claim_does_not_notify(claimed_drop):
    drop, twitch, send = claimed_drop
    twitch.gql_request.return_value = {"data": {"claimDropRewards": None}}
    assert not await drop.claim()
    send.assert_not_awaited()
    twitch.drop_history.record.assert_not_called()
    twitch.gql_request.return_value = {"data": {"claimDropRewards": {"status": "ELIGIBLE_FOR_ALL"}}}
    twitch.settings.telegram_chat_id = ""
    assert await drop.claim()
    send.assert_not_awaited()


@pytest.mark.asyncio
@pytest.mark.parametrize("cancel", [False, True])
async def test_notification_failure_or_cancellation_cannot_erase_a_claim(claimed_drop, cancel, caplog):
    drop, twitch, send = claimed_drop

    async def fail(*_):
        twitch.drop_history.record.assert_called_once_with(drop, drop.campaign)
        twitch.gui.inv.update_drop.assert_called_once_with(drop)
        assert drop.current_minutes == drop.required_minutes
        raise asyncio.CancelledError() if cancel else RuntimeError(TOKEN)

    send.side_effect = fail
    if cancel:
        with pytest.raises(asyncio.CancelledError):
            await drop.claim()
    else:
        assert await drop.claim()
    assert drop.is_claimed
    assert TOKEN not in caplog.text


@pytest.mark.asyncio
async def test_dynamic_notification_fields_are_html_escaped(claimed_drop):
    drop, _, send = claimed_drop
    drop.name = "Drop <script> & name"
    drop.campaign.name = "Campaign <b>"
    assert await drop.claim()
    message = send.await_args.args[0]
    assert "Drop &lt;script&gt; &amp; name" in message
    assert "Campaign &lt;b&gt;" in message
    assert "<b>Drop Claimed!</b>" in message
