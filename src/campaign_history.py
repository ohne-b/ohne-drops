"""Durable display snapshots of confirmed completed campaigns, separate from caches."""

from __future__ import annotations

import copy
import json
import logging
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from src.utils import json_save


logger = logging.getLogger("TwitchDrops")


class CampaignHistory:
    def __init__(self, path: Path) -> None:
        self._path = path
        self._campaigns: dict[str, dict[str, Any]] = {}
        self._writable = True
        self._dirty = False
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
            if data["version"] != 1 or not isinstance(data["campaigns"], list):
                raise ValueError("Unsupported campaign history")
            for campaign in data["campaigns"]:
                self._validate(campaign)
                self._campaigns[campaign["id"]] = campaign
        except FileNotFoundError:
            pass
        except (OSError, ValueError, KeyError, TypeError):
            # Keep the original file for recovery instead of overwriting it.
            self._writable = False
            logger.error("Cannot load completed campaign history; preserving the original file")

    @staticmethod
    def _validate(campaign: dict[str, Any]) -> None:
        for key in ("id", "name", "game_name", "game_box_art_url", "campaign_url", "link_url"):
            if not isinstance(campaign[key], str):
                raise ValueError("Invalid campaign history")
        for key in ("starts_at", "ends_at"):
            if datetime.fromisoformat(campaign[key]).tzinfo is None:
                raise ValueError("Missing campaign timezone")
        if not campaign["drops"] or campaign["finished"] is not True:
            raise ValueError("Incomplete campaign")
        for drop in campaign["drops"]:
            if drop["is_claimed"] is not True or not isinstance(drop["name"], str):
                raise ValueError("Unclaimed reward")
            if not isinstance(drop["id"], str) or not isinstance(drop["required_minutes"], int) or drop["required_minutes"] <= 0:
                raise ValueError("Invalid watch reward")
            for benefit in drop["benefits"]:
                if not all(isinstance(benefit[key], str) for key in ("name", "type", "image_url")):
                    raise ValueError("Invalid reward")

    def record(self, campaign: dict[str, Any]) -> None:
        if not campaign["finished"] or not campaign["drops"]:
            return
        snapshot = copy.deepcopy(campaign)
        if self._campaigns.get(campaign["id"]) == snapshot and not self._dirty:
            return
        self._campaigns[campaign["id"]] = snapshot
        self._dirty = True
        if self._writable:
            try:
                self._path.parent.mkdir(parents=True, exist_ok=True)
                json_save(self._path, {"version": 1, "campaigns": list(self._campaigns.values())})
                self._dirty = False
            except OSError:
                logger.exception("Could not persist completed campaign history")

    def get_campaigns(self) -> dict[str, dict[str, Any]]:
        campaigns = copy.deepcopy(self._campaigns)
        now = datetime.now(timezone.utc)
        for campaign in campaigns.values():
            start = datetime.fromisoformat(campaign["starts_at"])
            end = datetime.fromisoformat(campaign["ends_at"])
            campaign.update(active=start <= now < end, upcoming=now < start, expired=end <= now)
        return campaigns
