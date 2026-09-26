import copy
import json
from types import SimpleNamespace
from unittest.mock import AsyncMock, Mock

import pytest

from src.config.settings import default_settings
from src.utils.json_utils import json_save
from src.web.managers.settings import SettingsManager


def test_failed_atomic_write_keeps_last_saved_settings(tmp_path, monkeypatch):
    path = tmp_path / "settings.json"
    path.write_text('{"proxy":""}')
    def fail(*_):
        raise OSError("disk full")
    monkeypatch.setattr("src.utils.json_utils.os.replace", fail)
    with pytest.raises(OSError):
        json_save(path, {"proxy": "new"})
    assert json.loads(path.read_text()) == {"proxy": ""}
    assert list(tmp_path.iterdir()) == [path]


@pytest.mark.asyncio
async def test_failed_save_restores_live_settings_without_broadcast_or_restart():
    settings = SimpleNamespace(**copy.deepcopy(default_settings))
    settings.save = Mock(side_effect=OSError("disk full"))
    broadcaster, restart = AsyncMock(), Mock()
    manager = SettingsManager(broadcaster, settings, Mock(), restart)
    revision = manager.revision
    with pytest.raises(OSError):
        manager.update_settings({"proxy": "http://localhost:1234"})
    assert settings.proxy == ""
    assert manager.revision == revision
    broadcaster.emit.assert_not_called()
    restart.assert_not_called()
