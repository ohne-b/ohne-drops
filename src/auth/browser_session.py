"""Shared browser identity and protected Twitch catalog validation."""

from dataclasses import dataclass, field
from typing import Any

from src.exceptions import LoginException
from src.i18n import _


def browser_error(code: str) -> LoginException:
    return LoginException(_.t["login"]["error_code"].format(error_code=f"BROWSER_{code}"))


@dataclass(frozen=True)
class BrowserIdentity:
    user_id: int
    token: str = field(repr=False)
    device_id: str = field(repr=False)
    user_agent: str


class BrowserSession:
    PAGE = "https://www.twitch.tv/drops/campaigns"
    GQL_URL = "https://gql.twitch.tv/gql"
    VALIDATE_URL = "https://id.twitch.tv/oauth2/validate"

    @staticmethod
    def campaign_count(results: Any) -> int:
        """Validate both catalog operations without inferring account eligibility."""
        try:
            if not isinstance(results, list) or len(results) != 2:
                raise ValueError
            inventory = results[0]["data"]["currentUser"]["inventory"]
            campaigns = results[1]["data"]["currentUser"]["dropCampaigns"]
            if (
                any(item.get("errors") for item in results)
                or not isinstance(inventory, dict)
                or not isinstance(inventory.get("gameEventDrops"), list)
                or "dropCampaignsInProgress" not in inventory
                or not isinstance(inventory.get("dropCampaignsInProgress"), (list, type(None)))
                or not isinstance(campaigns, list)
            ):
                raise ValueError
            return len(campaigns)
        except (IndexError, KeyError, TypeError, ValueError, AttributeError):
            raise browser_error("CATALOG") from None
