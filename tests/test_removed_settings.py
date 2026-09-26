"""Migrations must discard removed preferences without touching active settings."""

import json

from src.config.settings import Settings


def test_saved_language_is_ignored_and_removed_on_save(tmp_path, monkeypatch):
    path = tmp_path / "settings.json"
    path.write_text(json.dumps({"language": "Deutsch", "games_to_watch": ["Rust"]}))
    monkeypatch.setattr("src.config.settings.SETTINGS_PATH", path)
    settings = Settings()
    assert not hasattr(settings, "language")
    assert settings.games_to_watch == ["Rust"]
    settings.save()
    assert "language" not in json.loads(path.read_text())


def test_removed_notification_credentials_are_discarded(tmp_path, monkeypatch):
    path = tmp_path / "settings.json"
    path.write_text(json.dumps({"telegram_bot_token": "old-secret", "telegram_chat_id": "123"}))
    monkeypatch.setattr("src.config.settings.SETTINGS_PATH", path)
    settings = Settings()
    assert not any(key.startswith("telegram_") for key in vars(settings))
    settings.save()
    assert "old-secret" not in path.read_text()
