"""Recover campaign metadata from participating Twitch channels."""

from __future__ import annotations

import asyncio
import logging
from typing import TYPE_CHECKING

from src.config import GQL_OPERATIONS, GQLQuery
from src.exceptions import MinerException
from src.models.game import Game
from src.utils import chunk


if TYPE_CHECKING:
    from collections.abc import Iterable

    from src.config import JsonType
    from src.core.client import Twitch


logger = logging.getLogger("TwitchDrops")


class CampaignDiscovery:
    """Read Twitch metadata only; Inventory still owns account progress/claims."""

    GAMES_QUERY = """
        query DropsDiscoveryGames($after: Cursor) {
          games(first: 100, after: $after) {
            edges { cursor node { streams(first: 3, options: {systemFilters: [DROPS_ENABLED]}) {
              edges { node { broadcaster { id login displayName } } }
            } } }
            pageInfo { hasNextPage }
          }
        }
    """
    CAMPAIGNS_QUERY = """
        query ChannelDropsRecovery($channelID: ID!) {
          channel(id: $channelID) { id viewerDropCampaigns {
            id name status startAt endAt accountLinkURL
            game { id name displayName slug boxArtURL }
            allow { isEnabled channels { id name displayName } }
            timeBasedDrops {
              id name startAt endAt requiredMinutesWatched preconditionDrops { id }
              benefitEdges { benefit { id name distributionType imageAssetURL } }
            }
          } }
        }
    """

    def __init__(self, twitch: Twitch):
        self._twitch = twitch

    @staticmethod
    def _channels(game: JsonType | None) -> list[JsonType]:
        streams = (game or {}).get("streams") or {}
        return [
            channel
            for edge in streams.get("edges") or []
            if (channel := (edge.get("node") or {}).get("broadcaster"))
        ]

    async def fetch(self, games: Iterable[Game]) -> dict[str, JsonType]:
        campaigns: dict[str, JsonType] = {}
        channels: dict[str, JsonType] = {}
        try:
            async with asyncio.timeout(60):
                # Include known/saved games even when they are outside the directory scan.
                slugs = list(dict.fromkeys(game.slug for game in games))[:100]
                for slugs_chunk in chunk(slugs, 20):
                    responses = await self._twitch.gql_request([
                        GQL_OPERATIONS["GameDirectory"].with_variables({
                            "slug": slug, "limit": 3,
                            "options": {"systemFilters": ["DROPS_ENABLED"]},
                        })
                        for slug in slugs_chunk
                    ])
                    for response in responses if isinstance(responses, list) else [responses]:
                        for channel in self._channels((response.get("data") or {}).get("game")):
                            channels[channel["id"]] = channel

                cursor = None
                # ponytail: sample 500 categories/3 streams, expand only if coverage warrants it.
                for _page in range(5):
                    response = await self._twitch.gql_request(GQLQuery(
                        "DropsDiscoveryGames", self.GAMES_QUERY, variables={"after": cursor},
                    ))
                    if not isinstance(response, dict):
                        break
                    directory = (response.get("data") or {}).get("games") or {}
                    edges = directory.get("edges") or []
                    for edge in edges:
                        for channel in self._channels(edge.get("node")):
                            channels[channel["id"]] = channel
                    next_cursor = edges[-1].get("cursor") if edges else None
                    if (
                        not next_cursor or next_cursor == cursor
                        or not (directory.get("pageInfo") or {}).get("hasNextPage")
                    ):
                        break
                    cursor = next_cursor

                for channel_ids in chunk(channels, 20):
                    responses = await self._twitch.gql_request([
                        GQLQuery("ChannelDropsRecovery", self.CAMPAIGNS_QUERY,
                                 variables={"channelID": channel_id})
                        for channel_id in channel_ids
                    ])
                    for response in responses if isinstance(responses, list) else [responses]:
                        source = (response.get("data") or {}).get("channel") or {}
                        channel = channels.get(source.get("id", ""))
                        if channel is None:
                            continue
                        for data in source.get("viewerDropCampaigns") or []:
                            if (
                                not data or not data.get("game")
                                or data.get("status") not in ("ACTIVE", "UPCOMING")
                                or not data.get("timeBasedDrops")
                            ):
                                continue
                            cid = data["id"]
                            if cid not in campaigns:
                                # Never request self edges here: they can gate the entire list.
                                campaigns[cid] = {
                                    **data, "self": {"isAccountConnected": None},
                                    "discovery_channels": [],
                                }
                            campaigns[cid]["discovery_channels"].append({
                                "id": channel["id"], "name": channel["login"],
                                "displayName": channel.get("displayName"),
                            })
        except (MinerException, TimeoutError, KeyError, TypeError, ValueError) as exc:
            logger.warning("Channel campaign discovery incomplete (%s)", type(exc).__name__)
        logger.info("Discovered %d campaign records on %d Twitch channels", len(campaigns), len(channels))
        return campaigns
