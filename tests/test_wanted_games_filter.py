import unittest
from datetime import datetime, timezone
from unittest.mock import MagicMock

from src.models.campaign import DropsCampaign
from src.models.game import Game
from src.services.stream_selector import StreamSelector


class TestWantedGamesFilter(unittest.TestCase):
    def setUp(self):
        # Mock Settings
        self.settings = MagicMock()
        self.settings.games_to_watch = ["Game1", "Game2"]
        self.settings.mining_benefits = {
            "BADGE": True,
            "DIRECT_ENTITLEMENT": True,
        }  # both allowed by default

    def test_filter_wanted_campaigns(self):
        # Setup Campaigns

        # Campaign 1: Game1, Can Earn, Has Wanted Benefits -> Should be selected
        c1 = MagicMock(spec=DropsCampaign)
        c1.game = Game({"id": 1, "name": "Game1"})
        c1.can_earn_within.return_value = True
        c1.id = "123"
        c1.name = "Test Campaign"
        c1.campaign_url = "http://test.url"
        d1 = MagicMock()
        d1.name = "Test Drop"
        d1.is_claimed = False
        d1.ends_at = datetime.max.replace(tzinfo=timezone.utc)
        d1.get_wanted_unclaimed_benefits.return_value = ["Benefit1"]
        c1.drops = [d1]
        c1.has_wanted_unclaimed_benefits.side_effect = (
            DropsCampaign.has_wanted_unclaimed_benefits.__get__(c1, DropsCampaign)
        )

        # Campaign 2: Game2, Can Earn, NO Wanted Benefits -> Should NOT be selected
        c2 = MagicMock(spec=DropsCampaign)
        c2.game = Game({"id": 2, "name": "Game2"})
        c2.can_earn_within.return_value = True
        d2 = MagicMock()
        d2.is_claimed = False
        d2.ends_at = datetime.max.replace(tzinfo=timezone.utc)
        d2.get_wanted_unclaimed_benefits.return_value = []
        c2.drops = [d2]
        c2.has_wanted_unclaimed_benefits.side_effect = (
            DropsCampaign.has_wanted_unclaimed_benefits.__get__(c2, DropsCampaign)
        )

        # Campaign 3: Game3 (newly discovered) -> not explicitly selected
        c3 = MagicMock(spec=DropsCampaign)
        c3.game = Game({"id": 3, "name": "Game3"})
        c3.id = "campaign3"
        c3.name = "New campaign"
        c3.campaign_url = "https://example.test/campaign3"
        c3.can_earn_within.return_value = True
        d3 = MagicMock()
        d3.is_claimed = False
        d3.ends_at = datetime.max.replace(tzinfo=timezone.utc)
        d3.get_wanted_unclaimed_benefits.return_value = ["Benefit3"]
        c3.drops = [d3]
        c3.has_wanted_unclaimed_benefits.side_effect = (
            DropsCampaign.has_wanted_unclaimed_benefits.__get__(c3, DropsCampaign)
        )

        # Campaign 4: Game1, Can Earn, Has Claimed Wanted Benefits -> Should NOT be selected
        c4 = MagicMock(spec=DropsCampaign)
        c4.game = Game({"id": 1, "name": "Game1"})
        c4.can_earn_within.return_value = True
        c4.id = "123"
        c4.name = "Test Campaign"
        c4.campaign_url = "http://test.url"
        d4 = MagicMock()
        d4.name = "Test Drop"
        d4.is_claimed = True
        d4.ends_at = datetime.max.replace(tzinfo=timezone.utc)
        d4.get_wanted_unclaimed_benefits.return_value = ["Benefit4"]
        c4.drops = [d4]
        c4.has_wanted_unclaimed_benefits.side_effect = (
            DropsCampaign.has_wanted_unclaimed_benefits.__get__(c4, DropsCampaign)
        )

        # Campaign 5: Game1, Can Not Earn, Has Wanted Benefits -> Should NOT be selected
        c5 = MagicMock(spec=DropsCampaign)
        c5.game = Game({"id": 1, "name": "Game1"})
        c5.can_earn_within.return_value = False
        c5.id = "123"
        c5.name = "Test Campaign"
        c5.campaign_url = "http://test.url"
        d5 = MagicMock()
        d5.name = "Test Drop"
        d5.is_claimed = False
        d5.ends_at = datetime.max.replace(tzinfo=timezone.utc)
        d5.get_wanted_unclaimed_benefits.return_value = ["Benefit5"]
        c5.drops = [d5]
        c5.has_wanted_unclaimed_benefits.side_effect = (
            DropsCampaign.has_wanted_unclaimed_benefits.__get__(c5, DropsCampaign)
        )

        inventory = [c1, c2, c3, c4, c5]
        stream_selector = StreamSelector()
        wanted_games = stream_selector.get_wanted_games(self.settings, inventory)

        self.assertEqual([game.name for game in wanted_games], ["Game1"])
        self.assertEqual(wanted_games[0].name, "Game1")


if __name__ == "__main__":
    unittest.main()


def test_discovery_does_not_select_games_and_explicit_choices_are_deduplicated():
    from types import SimpleNamespace
    from tests.test_watch_drop_filtering import _campaign, _drop
    a = _campaign("a", [_drop("a", "Reward", 10)])
    b = _campaign("b", [_drop("b", "Reward", 10)])
    a.game = Game({"id": "1", "name": "Alpha"})
    b.game = Game({"id": "2", "name": "Beta"})
    selector = StreamSelector()
    settings = SimpleNamespace(games_to_watch=[], mining_benefits={"DIRECT_ENTITLEMENT": True})
    assert selector.get_wanted_games(settings, [b, a]) == []
    assert selector.get_wanted_game_tree(settings, [b, a]) == []
    settings.games_to_watch = ["beta", "BETA"]
    assert [g.name for g in selector.get_wanted_games(settings, [a, b])] == ["Beta"]
    assert settings.games_to_watch == ["beta", "BETA"]


def test_queue_uses_reward_art_and_allows_missing_art():
    from types import SimpleNamespace
    from tests.test_watch_drop_filtering import _campaign, _drop
    campaign = _campaign("art", [_drop("reward", "Reward", 10)])
    settings = SimpleNamespace(games_to_watch=[campaign.game.name], mining_benefits={"DIRECT_ENTITLEMENT": True})
    selector = StreamSelector()
    benefit = next(iter(campaign.drops)).benefits[0]
    benefit.image_url = "https://example.test/reward.png"
    assert selector.get_wanted_game_tree(settings, [campaign])[0]["campaigns"][0]["drops"][0]["image_url"] == benefit.image_url
    benefit.image_url = None
    assert selector.get_wanted_game_tree(settings, [campaign])[0]["campaigns"][0]["drops"][0]["image_url"] == ""
