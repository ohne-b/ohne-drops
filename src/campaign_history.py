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
        for key in ("active", "upcoming", "expired", "finished", "mining_finished"):
            if type(campaign[key]) is not bool:
                raise ValueError("Invalid campaign status")
        for key in ("claimed_drops", "total_drops", "ignored_drops", "skipped_drops"):
            if type(campaign[key]) is not int or campaign[key] < 0:
                raise ValueError("Invalid campaign counts")
        if campaign["linked"] is not None and type(campaign["linked"]) is not bool:
            raise ValueError("Invalid link status")
        if not isinstance(campaign["drops"], list) or not campaign["drops"] or campaign["finished"] is not True:
            raise ValueError("Incomplete campaign")
        if campaign["total_drops"] != len(campaign["drops"]) or campaign["claimed_drops"] != len(campaign["drops"]):
            raise ValueError("Inconsistent claim totals")
        if len({drop["id"] for drop in campaign["drops"]}) != len(campaign["drops"]):
            raise ValueError("Duplicate rewards")
        for drop in campaign["drops"]:
            if drop["is_claimed"] is not True or not isinstance(drop["name"], str):
                raise ValueError("Unclaimed reward")
            if not isinstance(drop["id"], str) or type(drop["required_minutes"]) is not int or drop["required_minutes"] <= 0:
                raise ValueError("Invalid watch reward")
            if not isinstance(drop["benefits"], list):
                raise ValueError("Invalid benefits collection")
            for key in ("can_claim", "is_ignored", "is_mineable", "is_skipped"):
                if type(drop[key]) is not bool:
                    raise ValueError("Invalid reward status")
            for key in ("current_minutes", "confirmed_minutes"):
                if type(drop[key]) is not int or drop[key] < 0:
                    raise ValueError("Invalid reward progress")
            for benefit in drop["benefits"]:
                if not all(isinstance(benefit[key], str) for key in ("name", "type", "image_url")):
                    raise ValueError("Invalid reward")

    def record(self, campaign: dict[str, Any]) -> dict[str, Any]:
        if not campaign["finished"] or not campaign["drops"]:
            previous = self._campaigns.get(campaign["id"])
            if previous is not None and (
                any(not drop["is_claimed"] and drop.get("confirmed_at") for drop in campaign["drops"])
                or {drop["id"] for drop in campaign["drops"]} != {drop["id"] for drop in previous["drops"]}
            ):
                # New account evidence or a changed reward set invalidates old completion.
                # Metadata-only recovery cannot disprove previously confirmed claims.
                del self._campaigns[campaign["id"]]
                self._dirty = True
        else:
            snapshot = copy.deepcopy(campaign)
            if self._campaigns.get(campaign["id"]) != snapshot:
                self._campaigns[campaign["id"]] = snapshot
                self._dirty = True
        if self._writable and self._dirty:
            try:
                self._path.parent.mkdir(parents=True, exist_ok=True)
                json_save(self._path, {"version": 1, "campaigns": list(self._campaigns.values())})
                self._dirty = False
            except OSError:
                logger.exception("Could not persist completed campaign history")
        return copy.deepcopy(self._campaigns.get(campaign["id"], campaign))

    def get_campaigns(self) -> dict[str, dict[str, Any]]:
        campaigns = copy.deepcopy(self._campaigns)
        now = datetime.now(timezone.utc)
        for campaign in campaigns.values():
            start = datetime.fromisoformat(campaign["starts_at"])
            end = datetime.fromisoformat(campaign["ends_at"])
            campaign.update(active=start <= now < end, upcoming=now < start, expired=end <= now)
        return campaigns
