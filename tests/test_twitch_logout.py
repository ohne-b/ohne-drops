"""Account logout drains work before forgetting credentials; no live requests."""

import asyncio
from copy import deepcopy
from types import SimpleNamespace
from unittest.mock import MagicMock

import aiohttp
import pytest

from src.config import ClientType
from src.config.settings import default_settings
from src.core.client import Twitch
from src.models.channel import Channel
from src.services.channel_service import ChannelService
from src.web.gui_manager import WebGUIManager
from src.websocket.pool import WebsocketPool
from src.websocket.websocket import Websocket


@pytest.mark.asyncio
@pytest.mark.parametrize("pending_oauth", [False, True])
async def test_logout_drains_tasks_forgets_cookie_and_keeps_dashboard(tmp_path, monkeypatch, pending_oauth):
    cookies = tmp_path / "cookies.jar"
    monkeypatch.setattr("src.core.client.DATA_DIR", tmp_path)
    monkeypatch.setattr("src.core.client.COOKIES_PATH", cookies)
    monkeypatch.setattr("src.api.http_client.COOKIES_PATH", cookies)
    client = Twitch(SimpleNamespace(**deepcopy(default_settings)))
    client.gui = WebGUIManager(client)
    client._ensure_api_clients()
    session = await client.get_session()
    session.cookie_jar.update_cookies({"auth-token": "old-account-token"}, ClientType.SMARTBOX.CLIENT_URL)
    session.cookie_jar.save(cookies)
    client._auth_state.access_token = "old-account-token"
    client._auth_state.user_id = 123
    client._auth_state._logged_in.set()
    settings = client.settings
    history = client.drop_history
    started, restarted = asyncio.Event(), asyncio.Event()
    cancelled = []

    async def background(name):
        try:
            await asyncio.Event().wait()
        finally:
            await asyncio.sleep(0)
            cancelled.append(name)

    async def account_session():
        if started.is_set():
            restarted.set()
            await asyncio.Event().wait()
        client._watching_task = asyncio.create_task(background("watch"))
        client._mnt_task = asyncio.create_task(background("maintenance"))
        channel_task = asyncio.create_task(background("channel"))
        client._channel_tasks.add(channel_task)
        channel_task.add_done_callback(client._channel_tasks.discard)
        # CHANNELS_FETCH may have already cleared its displayed channel dictionary.
        assert not client.channels
        started.set()
        if pending_oauth:
            await client.gui.login.ask_enter_code("https://www.twitch.tv/activate", "OLD-CODE")
        else:
            client.gui.login.update("Logged in", 123)
            await asyncio.Event().wait()

    client._run = account_session
    runner = asyncio.create_task(client.run())
    try:
        await asyncio.wait_for(started.wait(), 1)
        await asyncio.sleep(0)
        await asyncio.wait_for(asyncio.gather(client.logout(), client.logout()), 3)
        await asyncio.wait_for(restarted.wait(), 1)
        assert not runner.done()
        assert sorted(cancelled) == ["channel", "maintenance", "watch"]
        assert session.closed and not cookies.exists()
        assert not hasattr(client._auth_state, "access_token")
        assert client.gui.login.get_status() == {"status": "Logged out", "user_id": None}
        assert not client.gui.login._login_event.is_set()
        assert not client.channels and not client.inventory
        assert client.settings is settings and client.drop_history is history
        restored = await client.get_session()
        assert not restored.cookie_jar.filter_cookies(ClientType.SMARTBOX.CLIENT_URL)
        client.close()
        await asyncio.wait_for(runner, 1)
        await client.shutdown()
        jar = aiohttp.CookieJar()
        jar.load(cookies)
        assert not jar.filter_cookies(ClientType.SMARTBOX.CLIENT_URL)
    finally:
        runner.cancel()
        await asyncio.gather(runner, return_exceptions=True)
        if client._http_client._session is not None:
            await client._http_client._session.close()


@pytest.mark.asyncio
async def test_websocket_stop_drains_inflight_message_handlers():
    twitch = SimpleNamespace(gui=MagicMock())
    socket = Websocket(SimpleNamespace(_twitch=twitch), 0)
    started, stopped = asyncio.Event(), asyncio.Event()

    async def callback(data):
        started.set()
        try:
            await asyncio.Event().wait()
        finally:
            await asyncio.sleep(0)
            stopped.set()

    socket.topics["test"] = callback
    socket._handle_message({"data": {"topic": "test", "message": "{}"}})
    await started.wait()
    await socket.stop(remove=True)
    assert stopped.is_set() and not socket._message_tasks and not socket.topics


@pytest.mark.asyncio
async def test_retiring_sockets_and_replaced_channel_delays_are_drained():
    twitch = SimpleNamespace(gui=MagicMock(), _channel_tasks=set())
    channel = Channel(twitch, id=1, login="test")
    channel.check_online()
    first = channel._pending_stream_up
    await asyncio.sleep(0)
    channel.set_offline()
    channel.check_online()
    second = channel._pending_stream_up
    await asyncio.gather(first, return_exceptions=True)
    assert channel._pending_stream_up is second
    second.cancel()
    await asyncio.gather(second, return_exceptions=True)
    assert not twitch._channel_tasks

    pool = WebsocketPool(twitch)
    socket = Websocket(pool, 0)
    pool.websockets.append(socket)
    done = asyncio.Event()
    async def stop(*, remove):
        await asyncio.sleep(0)
        done.set()
    socket.stop = stop
    pool.remove_topics(["unused"])
    assert not pool.websockets
    await pool.stop(clear_topics=True)
    assert done.is_set() and not pool._retiring


@pytest.mark.asyncio
async def test_close_during_logout_waits_for_batch_cancellation(tmp_path, monkeypatch):
    monkeypatch.setattr("src.core.client.DATA_DIR", tmp_path)
    monkeypatch.setattr("src.core.client.COOKIES_PATH", tmp_path / "cookies.jar")
    client = Twitch(SimpleNamespace(**deepcopy(default_settings)))
    client.gui = WebGUIManager(client)
    requested, cleaning, release = asyncio.Event(), asyncio.Event(), asyncio.Event()

    async def request(operations):
        requested.set()
        try:
            await asyncio.Event().wait()
        finally:
            cleaning.set()
            await release.wait()

    client.gql_request = request
    channel = SimpleNamespace(stream_gql={})
    client._run = lambda: ChannelService(client).bulk_check_online([channel])
    runner = asyncio.create_task(client.run())
    await requested.wait()
    logout = asyncio.create_task(client.logout())
    await cleaning.wait()
    client.close()
    await asyncio.sleep(0)
    assert not logout.done() and not runner.done()
    release.set()
    await asyncio.wait_for(asyncio.gather(logout, runner), 3)
