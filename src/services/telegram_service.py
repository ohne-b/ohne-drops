"""Best-effort Telegram alerts for successful Twitch claims."""

from __future__ import annotations

import html
import logging
import re
from typing import TYPE_CHECKING

import aiohttp

from src.i18n import _


if TYPE_CHECKING:
    from src.models.drop import BaseDrop


logger = logging.getLogger("TwitchDrops")
TELEGRAM_TOKEN_MASK = "••••••••"
BOT_TOKEN_PATTERN = r"[0-9]+:[A-Za-z0-9_-]+"


class TelegramNotifier:
    """Send only to Telegram's fixed API; never log tokens, URLs or response bodies."""

    def __init__(self, bot_token: str = "", chat_id: str = ""):
        self.bot_token = bot_token
        self.chat_id = chat_id

    async def notify_drop_claimed(self, drop: BaseDrop) -> bool:
        message = _.t["gui"]["settings"]["telegram"]["claim_message"].format(
            campaign=html.escape(drop.campaign.name),
            game=html.escape(drop.campaign.game.name),
            drop=html.escape(drop.name),
            rewards=html.escape(drop.rewards_text()),
        )
        return await self._send_message(message)

    async def test_connection(self) -> bool:
        return await self._send_message(_.t["gui"]["settings"]["telegram"]["test_message"])

    async def _send_message(self, text: str) -> bool:
        if not self.chat_id or not re.fullmatch(BOT_TOKEN_PATTERN, self.bot_token):
            return False
        try:
            async with (
                aiohttp.ClientSession() as session,
                session.post(
                    f"https://api.telegram.org/bot{self.bot_token}/sendMessage",
                    json={"chat_id": self.chat_id, "text": text, "parse_mode": "HTML"},
                    timeout=aiohttp.ClientTimeout(total=10),
                    allow_redirects=False,
                ) as response,
            ):
                if response.status == 200:
                    result = await response.json()
                    if isinstance(result, dict) and result.get("ok") is True:
                        return True
        except Exception:
            # Exceptions may contain the full request URL, including the bot token.
            pass
        logger.warning(_.t["gui"]["settings"]["telegram"]["delivery_failed"])
        return False
