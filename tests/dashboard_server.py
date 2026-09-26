"""Offline browser-test server. Never starts the miner or contacts Twitch/Telegram."""

import asyncio
import copy
import json
import tempfile
from pathlib import Path
from types import SimpleNamespace

from src.drop_history import DropHistory
from src.i18n import _
from src.web import app as web
from src.web.managers.settings import SettingsManager


fixture = json.loads(Path("frontend/tests/fixture.json").read_text(encoding="utf-8"))
state = copy.deepcopy(fixture)
temporary = tempfile.TemporaryDirectory(prefix="tdm-ui-")
web.web_auth.path = Path(temporary.name) / "web_auth.json"
web.web_auth.password_hash = ""
web.web_auth.sessions = {}


class Broadcast:
    async def emit(self, name, data):
        await web.sio.emit(name, data)


class FakeSettings(SimpleNamespace):
    def save(self):
        pass


class FakeGui:
    def __init__(self):
        self.status = SimpleNamespace(get=lambda: state["status"])
        self.channels = SimpleNamespace(get_channels=lambda: state["channels"])
        self.inv = SimpleNamespace(
            get_campaigns=lambda: state["campaigns"], availability=state["inventory_status"]
        )
        self.output = SimpleNamespace(
            get_history=lambda: state["console"], print=lambda message: None
        )
        self.progress = SimpleNamespace(get_current_drop=lambda: state["current_drop"])
        self.login = SimpleNamespace(
            get_status=lambda: state["login"], _login_event=asyncio.Event()
        )
        values = {
            key: value
            for key, value in state["settings"].items()
            if key not in {"revision", "games_available", "telegram_configured"}
        }
        self.settings = SettingsManager(Broadcast(), FakeSettings(**values), self.output)
        self.settings._available_games = state["settings"]["games_available"]

    def set_socketio(self, sio):
        pass

    def get_wanted_game_tree(self):
        return state["wanted_items"]

    def select_channel(self, channel_id):
        for channel in state["channels"]:
            channel["watching"] = channel["id"] == channel_id
        state["manual_mode"] = {"active": True, "game_name": "Rust", "channel_name": "harbor"}
        asyncio.create_task(web.sio.emit("channel_watching", {"id": channel_id}))
        asyncio.create_task(web.sio.emit("manual_mode_update", state["manual_mode"]))


class FakeTwitch:
    def __init__(self):
        self.channels = {
            channel["id"]: SimpleNamespace(name=channel["name"], game=True)
            for channel in state["channels"]
        }
        self.inventory = []
        self.drop_history = DropHistory(Path(temporary.name))
        self.drop_history._entries = [
            {
                "id": "past-drop",
                "claimed_at": "2026-09-25T18:00:00+00:00",
                "game": "Rust",
                "campaign": "Autumn expedition",
                "drop_name": "Canvas pack",
                "benefits": ["Canvas pack"],
                "required_minutes": 30,
                "campaign_id": "campaign-1",
            }
        ]

    def get_manual_mode_info(self):
        return state["manual_mode"]

    def request_inventory_refresh(self, clear_cache=False):
        return True

    def change_state(self, value):
        pass

    def is_manual_mode(self):
        return state["manual_mode"]["active"]

    def exit_manual_mode(self, reason):
        state["manual_mode"] = {"active": False}
        asyncio.create_task(web.sio.emit("manual_mode_update", state["manual_mode"]))

    def close(self):
        pass


web.app.router.routes = [
    route
    for route in web.app.router.routes
    if getattr(route, "path", "")
    not in {"/api/version", "/api/settings/test-telegram", "/api/settings/verify-proxy"}
]


@web.app.get("/api/version")
async def version():
    return {
        "current_version": "1.3.2",
        "latest_version": None,
        "update_available": False,
        "download_url": "",
    }


@web.app.post("/api/settings/test-telegram")
@web.app.post("/api/settings/verify-proxy")
async def test_connection():
    return {"success": True}


web.AuthMiddleware.PUBLIC.add("/__test/reset")
web.AuthMiddleware.PUBLIC.add("/__test/event")
web.AuthMiddleware.PUBLIC.add("/__test/health")


@web.app.get("/__test/health")
async def health():
    return {"fixture": True}


@web.app.post("/__test/reconnect")
async def reconnect():
    state["channels"] = []
    for sid in list(web.sio.tokens):
        await web.sio.disconnect(sid)
    return {"ok": True}


@web.app.post("/__test/reset")
async def reset():
    global state
    _.set_language("English")
    state = copy.deepcopy(fixture)
    web.web_auth.save("", {})
    web.web_auth.attempts.clear()
    for sid in list(web.sio.tokens):
        await web.sio.disconnect(sid)
    web.set_managers(FakeGui(), FakeTwitch())
    return {"ok": True}


@web.app.post("/__test/event")
async def event(body: dict):
    await web.sio.emit(body["event"], body["data"])
    return {"ok": True}


web.set_managers(FakeGui(), FakeTwitch())
app = web.socket_app
