"""Browser imports cannot race saved-login validation or survive logout."""

import asyncio
from contextlib import asynccontextmanager
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import aiohttp
import pytest

from src.auth.browser_session import BrowserIdentity
from src.auth.session_bundle import SessionError
from src.config import ClientType
from src.web.managers.login import LoginFormManager
from tests.test_imported_session import bundle_data, transport
from tests.test_session_import_auth import client


@pytest.mark.asyncio
@pytest.mark.parametrize("import_account", [42, 99])
async def test_import_waits_for_legacy_validation_and_preserves_account(monkeypatch, tmp_path, import_account):
    miner = client(monkeypatch, tmp_path)
    miner.gui.login = LoginFormManager(MagicMock(emit=AsyncMock()), MagicMock())
    monkeypatch.setattr("src.auth.auth_state.COOKIES_PATH", tmp_path / "cookies.jar")
    jar = aiohttp.CookieJar()
    jar.update_cookies({"auth-token": "legacy-token", "unique_id": "legacy-device"}, ClientType.SMARTBOX.CLIENT_URL)
    miner.get_session = AsyncMock(return_value=SimpleNamespace(cookie_jar=jar))
    miner._auth_state.device_id = "legacy-device"
    entered, release = asyncio.Event(), asyncio.Event()

    @asynccontextmanager
    async def request(method, url, **kwargs):
        assert str(url).endswith("/validate")
        entered.set()
        await release.wait()
        yield SimpleNamespace(status=200, json=AsyncMock(return_value={
            "client_id": ClientType.SMARTBOX.CLIENT_ID, "user_id": "42",
        }))

    miner.request = request
    importer = miner._browser
    importer._clock = lambda: 1000
    importer._transport = transport(str(import_account))
    validation = asyncio.create_task(miner.get_auth())
    await asyncio.wait_for(entered.wait(), 1)
    try:
        with pytest.raises(SessionError, match="LOGIN_PENDING"):
            await importer.install(bundle_data())
        assert not importer.path.exists()
    finally:
        release.set()
        await validation
    assert miner._auth_state.user_id == 42
    if import_account == 99:
        with pytest.raises(SessionError, match="ACCOUNT_MISMATCH"):
            await importer.install(bundle_data())
        assert not importer.path.exists()
        assert not miner._auth_state.browser_active
    else:
        await importer.install(bundle_data())
        assert miner._auth_state.browser_active
        assert miner._inventory_refresh_pending
        assert miner._auth_state.user_id == 42


@pytest.mark.asyncio
async def test_saved_import_restores_through_real_auth_state(monkeypatch, tmp_path):
    first = client(monkeypatch, tmp_path)
    first.gui.login = LoginFormManager(MagicMock(emit=AsyncMock()), MagicMock())
    await first.gui.login.import_pending(True)
    first._browser._clock = lambda: 1000
    first._browser._transport = transport("42")
    await first._browser.install(bundle_data())
    await first._browser.close()
    restored = client(monkeypatch, tmp_path)
    restored.gui.login = LoginFormManager(MagicMock(emit=AsyncMock()), MagicMock())
    restored._browser._clock = lambda: 1000
    restored._browser._transport = transport("42")
    auth = await asyncio.wait_for(restored.get_auth(), 1)
    assert auth.browser_active and auth.user_id == 42
    assert restored._browser.status()["state"] == "ready"
    assert restored.gui.login.get_status()["user_id"] == 42


@pytest.mark.asyncio
async def test_logout_invalidates_inflight_import_and_forgets_pairing(monkeypatch, tmp_path):
    miner = client(monkeypatch, tmp_path)
    miner.gui.login = LoginFormManager(MagicMock(emit=AsyncMock()), MagicMock())
    monkeypatch.setattr("src.core.client.COOKIES_PATH", tmp_path / "cookies.jar")
    miner._auth_state.user_id = 42
    miner._auth_state._logged_in.set()
    importer = miner._browser
    importer._clock = lambda: 1000
    importer._transport = transport("42")
    await importer.install(bundle_data())
    credential = await importer.pair()
    entered, release, started = asyncio.Event(), asyncio.Event(), asyncio.Event()

    async def validate(bundle, expected):
        entered.set()
        await release.wait()
        return BrowserIdentity(42, bundle.token, "device", bundle.user_agent)

    importer._transport.validate = validate
    importer._clock = lambda: 1100
    renewal = asyncio.create_task(importer.install(bundle_data(1100, "replacement"), renewal_token=credential))
    await asyncio.wait_for(entered.wait(), 1)

    async def account_session():
        started.set()
        await asyncio.Event().wait()

    miner._run = account_session
    runner = asyncio.create_task(miner.run())
    await asyncio.wait_for(started.wait(), 1)
    try:
        await asyncio.wait_for(miner.logout(), 3)
        release.set()
        with pytest.raises(SessionError, match="STOPPED"):
            await renewal
        assert not importer.path.exists()
        assert miner._browser is not importer
        assert miner._browser.status()["generation"] == 0
        assert not miner._browser.status()["paired"]
        with pytest.raises(SessionError, match="PAIRING"):
            miner._browser._check_renewal(credential)
        assert not hasattr(miner._auth_state, "access_token")
    finally:
        release.set()
        miner.close()
        await asyncio.wait_for(runner, 1)
        await miner.shutdown()
