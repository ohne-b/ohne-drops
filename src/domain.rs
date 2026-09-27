use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use crate::{config::Settings, dto::*, policy::DropPolicy};

pub const MAX_ESTIMATED_MINUTES: u32 = 15;

#[derive(Debug, thiserror::Error)]
#[error("invalid Twitch campaign data")]
pub struct InvalidData;

pub fn text(value: &Value, key: &str) -> Result<String, InvalidData> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or(InvalidData)
}

pub fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn timestamp(value: &Value, key: &str) -> Result<DateTime<Utc>, InvalidData> {
    let at: DateTime<Utc> = value[key]
        .as_str()
        .ok_or(InvalidData)?
        .parse()
        .map_err(|_| InvalidData)?;
    // Every campaign/drop date must support scheduling lead time and claim grace.
    at.checked_sub_signed(Duration::hours(1))
        .ok_or(InvalidData)?;
    at.checked_add_signed(Duration::hours(24))
        .ok_or(InvalidData)?;
    Ok(at)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub image_url: String,
}

impl Game {
    pub fn parse(value: &Value) -> Result<Self, InvalidData> {
        let name = text(value, "displayName").or_else(|_| text(value, "name"))?;
        Ok(Self {
            id: number(&value["id"]).ok_or(InvalidData)?,
            slug: text(value, "slug").unwrap_or_else(|_| Self::slug(&name)),
            name,
            image_url: value["boxArtURL"].as_str().unwrap_or_default().to_owned(),
        })
    }

    pub fn slug(name: &str) -> String {
        let mut result = String::new();
        for c in name.to_lowercase().chars().filter(|c| *c != '\'') {
            if c.is_alphanumeric() || c == '_' {
                result.push(c);
            } else if !result.is_empty() && !result.ends_with('-') {
                result.push('-');
            }
        }
        result.trim_end_matches('-').to_owned()
    }

    pub fn special(&self) -> bool {
        matches!(self.id, 509663 | 509672)
    }
}

#[derive(Clone, Debug)]
pub struct Benefit {
    pub id: String,
    pub view: BenefitView,
}

impl Benefit {
    fn parse(value: &Value) -> Result<Self, InvalidData> {
        let kind = value["distributionType"]
            .as_str()
            .filter(|kind| matches!(*kind, "BADGE" | "EMOTE" | "DIRECT_ENTITLEMENT"))
            .unwrap_or("UNKNOWN");
        Ok(Self {
            id: text(value, "id")?,
            view: BenefitView {
                name: text(value, "name")?,
                kind: kind.to_owned(),
                image_url: value["imageAssetURL"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            },
        })
    }

    pub fn wanted(&self, settings: &Settings) -> bool {
        settings
            .mining_benefits
            .get(&self.view.kind)
            .copied()
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug)]
pub struct Drop {
    pub id: String,
    pub name: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub required_minutes: u32,
    pub confirmed_minutes: u32,
    pub estimated_minutes: u32,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub claimed: bool,
    pub claim_id: Option<String>,
    pub prerequisites: Vec<String>,
    pub benefits: Vec<Benefit>,
}

impl Drop {
    fn parse(
        value: &Value,
        awards: &HashMap<String, DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Self, InvalidData> {
        let starts_at = timestamp(value, "startAt")?;
        let ends_at = timestamp(value, "endAt")?;
        let benefits = value["benefitEdges"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|edge| Benefit::parse(&edge["benefit"]))
            .collect::<Result<Vec<_>, _>>()?;
        let account = value.get("self").filter(|v| !v.is_null());
        if let Some(account) = account {
            let claimed = account["isClaimed"].as_bool().ok_or(InvalidData)?;
            if !claimed && account["currentMinutesWatched"].as_u64().is_none() {
                return Err(InvalidData);
            }
        }
        let claimed = account
            .map(|v| v["isClaimed"].as_bool().unwrap_or(false))
            .unwrap_or_else(|| {
                !benefits.is_empty()
                    && benefits.iter().all(|benefit| {
                        awards
                            .get(&benefit.id)
                            .is_some_and(|at| starts_at <= *at && *at < ends_at)
                    })
            });
        let required_minutes = value["requiredMinutesWatched"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or(InvalidData)?;
        let confirmed_minutes = if claimed {
            required_minutes
        } else {
            account
                .and_then(|v| v["currentMinutesWatched"].as_u64())
                .unwrap_or(0)
                .min(u64::from(required_minutes)) as u32
        };
        Ok(Self {
            id: text(value, "id")?,
            name: text(value, "name")?,
            starts_at,
            ends_at,
            required_minutes,
            confirmed_minutes,
            estimated_minutes: 0,
            confirmed_at: account.map(|_| now),
            claimed,
            claim_id: account
                .and_then(|v| v["dropInstanceID"].as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            prerequisites: value["preconditionDrops"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| text(v, "id"))
                .collect::<Result<_, _>>()?,
            benefits,
        })
    }

    pub fn watch_reward(&self) -> bool {
        self.required_minutes > 0
    }
    pub fn current_minutes(&self) -> u32 {
        self.confirmed_minutes
            .saturating_add(self.estimated_minutes)
    }
    pub fn remaining_minutes(&self) -> u32 {
        self.required_minutes.saturating_sub(self.current_minutes())
    }
    pub fn progress(&self) -> f64 {
        if self.required_minutes == 0 {
            0.0
        } else {
            (f64::from(self.current_minutes()) / f64::from(self.required_minutes)).min(1.0)
        }
    }

    pub fn confirm(&mut self, minutes: u32, now: DateTime<Utc>) {
        self.confirmed_minutes = if self.claimed {
            self.required_minutes
        } else {
            minutes.min(self.required_minutes)
        };
        self.estimated_minutes = 0;
        self.confirmed_at = Some(now);
    }

    pub fn mark_claimed(&mut self, now: DateTime<Utc>) {
        self.claimed = true;
        self.confirm(self.required_minutes, now);
    }

    pub fn can_claim(&self, campaign_end: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        self.claim_id.is_some() && !self.claimed && now < campaign_end + Duration::hours(24)
    }

    pub fn image_url(&self) -> String {
        self.benefits
            .iter()
            .find(|b| !b.view.image_url.is_empty())
            .map(|b| b.view.image_url.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelIdentity {
    pub id: u64,
    pub login: String,
    pub name: String,
}

impl ChannelIdentity {
    pub fn parse(value: &Value) -> Result<Self, InvalidData> {
        let login = text(value, "login").or_else(|_| text(value, "name"))?;
        Ok(Self {
            id: number(&value["id"]).ok_or(InvalidData)?,
            name: text(value, "displayName").unwrap_or_else(|_| login.clone()),
            login,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub identity: ChannelIdentity,
    pub game: Option<Game>,
    pub broadcast_id: Option<String>,
    pub viewers: Option<u64>,
    pub drops_enabled: bool,
    pub acl_based: bool,
    pub beacon_url: Option<url::Url>,
}

impl Channel {
    pub fn offline(identity: ChannelIdentity, acl_based: bool) -> Self {
        Self {
            identity,
            game: None,
            broadcast_id: None,
            viewers: None,
            drops_enabled: false,
            acl_based,
            beacon_url: None,
        }
    }

    pub fn online(&self) -> bool {
        self.broadcast_id.is_some()
    }

    pub fn view(&self, watching: Option<u64>) -> ChannelView {
        ChannelView {
            id: self.identity.id,
            login: self.identity.login.clone(),
            name: self.identity.name.clone(),
            game: self.game.as_ref().map(|g| g.name.clone()),
            game_id: self.game.as_ref().map(|g| g.id),
            game_icon: self.game.as_ref().map(|g| g.image_url.clone()),
            viewers: self.viewers,
            online: self.online(),
            drops_enabled: self.drops_enabled,
            acl_based: self.acl_based,
            watching: watching == Some(self.identity.id),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Campaign {
    pub id: String,
    pub name: String,
    pub game: Game,
    pub linked: Option<bool>,
    pub link_url: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub valid: bool,
    pub allowed_channels: Vec<ChannelIdentity>,
    pub drops: Vec<Drop>,
}

impl Campaign {
    pub fn parse(
        value: &Value,
        awards: &HashMap<String, DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Self, InvalidData> {
        let channel_list = |v: &Value| {
            v.as_array()
                .into_iter()
                .flatten()
                .filter(|v| !v.is_null())
                .map(ChannelIdentity::parse)
                .collect::<Result<Vec<_>, _>>()
        };
        let raw_drops = value["timeBasedDrops"].as_array().ok_or(InvalidData)?;
        if raw_drops.len() > 5000 {
            return Err(InvalidData);
        }
        let drops = raw_drops
            .iter()
            .map(|d| Drop::parse(d, awards, now))
            .collect::<Result<Vec<_>, _>>()?;
        let mut seen = HashSet::new();
        if drops.iter().any(|d| !seen.insert(&d.id)) {
            return Err(InvalidData);
        }
        Ok(Self {
            id: text(value, "id")?,
            name: text(value, "name")?,
            game: Game::parse(&value["game"])?,
            linked: value["self"]["isAccountConnected"].as_bool(),
            link_url: value["accountLinkURL"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            starts_at: timestamp(value, "startAt")?,
            ends_at: timestamp(value, "endAt")?,
            valid: value["status"].as_str() != Some("EXPIRED"),
            allowed_channels: if value["allow"]
                .get("isEnabled")
                .is_some_and(|enabled| enabled != true)
            {
                vec![]
            } else {
                channel_list(&value["allow"]["channels"])?
            },
            drops,
        })
    }

    pub fn url(&self) -> String {
        let mut url =
            url::Url::parse("https://www.twitch.tv/drops/campaigns").expect("constant URL");
        url.query_pairs_mut().append_pair("dropID", &self.id);
        url.into()
    }

    pub fn active(&self, now: DateTime<Utc>) -> bool {
        self.valid && self.starts_at <= now && now < self.ends_at
    }
    pub fn upcoming(&self, now: DateTime<Utc>) -> bool {
        self.valid && now < self.starts_at
    }
    pub fn expired(&self, now: DateTime<Utc>) -> bool {
        !self.valid || self.ends_at <= now
    }
    pub fn policy(&self, settings: &Settings) -> DropPolicy {
        DropPolicy::evaluate(&self.drops, &settings.drop_name_blacklist)
    }

    pub fn prerequisites_met(&self, drop: &Drop) -> bool {
        drop.prerequisites
            .iter()
            .all(|id| self.drops.iter().any(|d| &d.id == id && d.claimed))
    }

    pub fn drop_eligible(
        &self,
        drop: &Drop,
        policy: &DropPolicy,
        now: DateTime<Utc>,
        before: DateTime<Utc>,
    ) -> bool {
        policy.mineable.contains(&drop.id)
            && self.prerequisites_met(drop)
            && drop.estimated_minutes < MAX_ESTIMATED_MINUTES
            && now < drop.ends_at
            && drop.starts_at < before
    }

    pub fn can_earn_within(
        &self,
        settings: &Settings,
        now: DateTime<Utc>,
        before: DateTime<Utc>,
    ) -> bool {
        let policy = self.policy(settings);
        self.valid
            && now < self.ends_at
            && self.starts_at < before
            && self
                .drops
                .iter()
                .any(|d| self.drop_eligible(d, &policy, now, before))
    }

    pub fn channel_eligible(&self, channel: &Channel, ignore_channel_status: bool) -> bool {
        (self.allowed_channels.is_empty()
            || self
                .allowed_channels
                .iter()
                .any(|c| c.id == channel.identity.id))
            && (ignore_channel_status
                || channel.game.as_ref().is_some_and(|g| g.id == self.game.id)
                || channel.online() && self.game.special() && !self.allowed_channels.is_empty())
    }

    pub fn can_watch(&self, channel: &Channel, settings: &Settings, now: DateTime<Utc>) -> bool {
        self.can_mine(settings, now) && self.matches_channel(channel)
    }

    pub fn matches_channel(&self, channel: &Channel) -> bool {
        channel.online()
            && (self.game.special() || channel.drops_enabled)
            && self.channel_eligible(channel, false)
    }

    pub fn can_mine(&self, settings: &Settings, now: DateTime<Utc>) -> bool {
        let policy = self.policy(settings);
        settings.selected(&self.game.name)
            && self.active(now)
            && self.valid
            && self
                .drops
                .iter()
                .any(|d| self.drop_eligible(d, &policy, now, now + Duration::nanoseconds(1)))
            && self.drops.iter().any(|d| {
                policy.mineable.contains(&d.id) && d.benefits.iter().any(|b| b.wanted(settings))
            })
    }

    pub fn first_drop(&self, settings: &Settings, now: DateTime<Utc>) -> Option<&Drop> {
        let policy = self.policy(settings);
        self.drops
            .iter()
            .filter(|d| self.drop_eligible(d, &policy, now, now + Duration::nanoseconds(1)))
            .min_by_key(|d| d.remaining_minutes())
    }

    pub fn bump_estimates(&mut self, settings: &Settings, now: DateTime<Utc>) -> bool {
        let policy = self.policy(settings);
        let eligible: HashSet<_> = self
            .drops
            .iter()
            .filter(|d| self.drop_eligible(d, &policy, now, now + Duration::nanoseconds(1)))
            .map(|d| d.id.clone())
            .collect();
        let mut stalled = false;
        for drop in &mut self.drops {
            if eligible.contains(&drop.id) {
                drop.estimated_minutes += 1;
                stalled |= drop.estimated_minutes >= MAX_ESTIMATED_MINUTES;
            }
        }
        stalled
    }

    pub fn view(&self, settings: &Settings, now: DateTime<Utc>) -> CampaignView {
        let policy = self.policy(settings);
        let drops: Vec<_> = self
            .drops
            .iter()
            .filter(|d| d.watch_reward())
            .map(|drop| {
                let reason = policy.reasons.get(&drop.id);
                let mineable = policy.mineable.contains(&drop.id);
                DropView {
                    id: drop.id.clone(),
                    name: drop.name.clone(),
                    current_minutes: drop.current_minutes(),
                    confirmed_minutes: drop.confirmed_minutes,
                    confirmed_at: drop.confirmed_at,
                    required_minutes: drop.required_minutes,
                    progress: drop.progress(),
                    is_claimed: drop.claimed,
                    can_claim: drop.can_claim(self.ends_at, now),
                    is_ignored: reason.is_some(),
                    is_mineable: mineable,
                    is_skipped: !drop.claimed && reason.is_none() && !mineable,
                    ignored_reason: reason.map(|r| r.kind().to_owned()),
                    ignored_keyword: reason.and_then(|r| r.keyword()).map(str::to_owned),
                    ignored_precondition: reason.and_then(|r| r.precondition()).map(str::to_owned),
                    benefits: drop.benefits.iter().map(|b| b.view.clone()).collect(),
                    starts_at: drop.starts_at,
                    ends_at: drop.ends_at,
                }
            })
            .collect();
        CampaignView {
            id: self.id.clone(),
            name: self.name.clone(),
            game_name: self.game.name.clone(),
            game_box_art_url: self.game.image_url.clone(),
            campaign_url: self.url(),
            link_url: self.link_url.clone(),
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            linked: self.linked,
            active: self.active(now),
            upcoming: self.upcoming(now),
            expired: self.expired(now),
            finished: !drops.is_empty() && drops.iter().all(|d| d.is_claimed),
            mining_finished: policy.mineable.is_empty(),
            claimed_drops: drops.iter().filter(|d| d.is_claimed).count(),
            total_drops: drops.len(),
            ignored_drops: drops.iter().filter(|d| d.is_ignored).count(),
            skipped_drops: drops.iter().filter(|d| d.is_skipped).count(),
            drops,
        }
    }

    pub fn progress(&self, drop: &Drop) -> Progress {
        Progress {
            drop_id: drop.id.clone(),
            drop_name: drop.name.clone(),
            campaign_id: self.id.clone(),
            campaign_name: self.name.clone(),
            game_name: self.game.name.clone(),
            current_minutes: drop.current_minutes(),
            confirmed_minutes: drop.confirmed_minutes,
            confirmed_at: drop.confirmed_at,
            required_minutes: drop.required_minutes,
            progress: drop.progress(),
            remaining_seconds: drop.remaining_minutes().saturating_mul(60),
        }
    }

    pub fn history_entry(&self, drop: &Drop, now: DateTime<Utc>) -> HistoryEntry {
        HistoryEntry {
            id: drop.id.clone(),
            claimed_at: now,
            game: self.game.name.clone(),
            campaign: self.name.clone(),
            drop_name: drop.name.clone(),
            benefits: drop.benefits.iter().map(|b| b.view.name.clone()).collect(),
            required_minutes: drop.required_minutes,
            campaign_id: self.id.clone(),
            image_url: drop.image_url(),
        }
    }
}

pub fn wanted_items(
    campaigns: &[Campaign],
    settings: &Settings,
    now: DateTime<Utc>,
) -> Vec<WantedGame> {
    let mut result: Vec<WantedGame> = vec![];
    for name in &settings.games_to_watch {
        for campaign in campaigns.iter().filter(|c| {
            crate::config::fold(&c.game.name) == crate::config::fold(name)
                && c.can_earn_within(settings, now, now + Duration::hours(1))
        }) {
            let policy = campaign.policy(settings);
            let drops: Vec<_> = campaign
                .drops
                .iter()
                .filter(|d| d.watch_reward() && now < d.ends_at && policy.mineable.contains(&d.id))
                .filter_map(|d| {
                    let benefits: Vec<_> =
                        d.benefits.iter().filter(|b| b.wanted(settings)).collect();
                    (!benefits.is_empty()).then(|| WantedDrop {
                        name: d.name.clone(),
                        benefits: benefits.iter().map(|b| b.view.name.clone()).collect(),
                        image_url: benefits
                            .iter()
                            .find(|b| !b.view.image_url.is_empty())
                            .map(|b| b.view.image_url.clone())
                            .unwrap_or_default(),
                    })
                })
                .collect();
            if drops.is_empty() {
                continue;
            }
            let entry = WantedCampaign {
                id: campaign.id.clone(),
                name: campaign.name.clone(),
                url: campaign.url(),
                drops,
            };
            if let Some(game) = result
                .iter_mut()
                .find(|g| g.game_id == Some(campaign.game.id))
            {
                game.campaigns.push(entry);
            } else {
                result.push(WantedGame {
                    game_name: campaign.game.name.clone(),
                    game_icon: Some(campaign.game.image_url.clone()),
                    game_id: Some(campaign.game.id),
                    campaigns: vec![entry],
                });
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::CampaignArchive;
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        "2026-09-26T12:00:00Z".parse().unwrap()
    }
    fn raw_drop(id: &str, prerequisites: &[&str]) -> Value {
        json!({"id":id,"name":id,"startAt":"2026-09-25T12:00:00Z","endAt":"2026-09-28T12:00:00Z",
            "requiredMinutesWatched":60,"preconditionDrops":prerequisites.iter().map(|id| json!({"id":id})).collect::<Vec<_>>(),
            "benefitEdges":[{"benefit":{"id":format!("b-{id}"),"name":id,"distributionType":"DIRECT_ENTITLEMENT","imageAssetURL":"https://static-cdn.jtvnw.net/reward.png"}}]})
    }
    fn raw_campaign(drops: Vec<Value>) -> Value {
        json!({"id":"c","name":"Campaign","game":{"id":"1","name":"Rust"},"startAt":"2026-09-25T12:00:00Z",
            "endAt":"2026-09-28T12:00:00Z","status":"ACTIVE","allow":{"channels":[],"isEnabled":true},"timeBasedDrops":drops})
    }
    fn campaign(drops: Vec<Value>) -> Campaign {
        Campaign::parse(&raw_campaign(drops), &HashMap::new(), now()).unwrap()
    }
    fn selected() -> Settings {
        Settings::default()
            .patched(&json!({"games_to_watch":["Rust"]}))
            .unwrap()
    }
    fn channel() -> Channel {
        Channel {
            identity: ChannelIdentity {
                id: 10,
                login: "streamer".into(),
                name: "Streamer".into(),
            },
            game: Some(Game {
                id: 1,
                name: "Rust".into(),
                slug: "rust".into(),
                image_url: String::new(),
            }),
            broadcast_id: Some("b".into()),
            viewers: None,
            drops_enabled: true,
            acl_based: false,
            beacon_url: None,
        }
    }

    #[test]
    fn discovery_cannot_select_games_or_invent_account_progress() {
        let c = campaign(vec![raw_drop("coat", &[])]);
        assert!(!c.can_watch(&channel(), &Settings::default(), now()));
        assert!(wanted_items(std::slice::from_ref(&c), &Settings::default(), now()).is_empty());
        assert!(c.can_watch(&channel(), &selected(), now()));
        let view = c.view(&selected(), now());
        assert_eq!(view.linked, None);
        assert_eq!(view.drops[0].confirmed_at, None);
        assert_eq!(view.drops[0].confirmed_minutes, 0);
        assert!(!view.finished);
    }

    #[test]
    fn timing_prerequisites_and_estimate_ceiling_control_eligibility() {
        let mut c = campaign(vec![raw_drop("first", &[]), raw_drop("second", &["first"])]);
        assert!(c.can_watch(&channel(), &selected(), c.starts_at));
        assert!(!c.can_watch(
            &channel(),
            &selected(),
            c.starts_at - Duration::nanoseconds(1)
        ));
        assert!(!c.can_watch(&channel(), &selected(), c.ends_at));
        assert_eq!(c.first_drop(&selected(), now()).unwrap().id, "first");
        for _ in 0..MAX_ESTIMATED_MINUTES - 1 {
            assert!(!c.bump_estimates(&selected(), now()));
        }
        assert!(c.bump_estimates(&selected(), now()));
        assert!(!c.can_watch(&channel(), &selected(), now()));
        assert_eq!(c.drops[0].confirmed_minutes, 0);
        assert_eq!(c.drops[1].estimated_minutes, 0);
        c.drops[0].confirm(0, now());
        assert!(c.can_watch(&channel(), &selected(), now()));
        c.drops[0].mark_claimed(now());
        assert_eq!(c.first_drop(&selected(), now()).unwrap().id, "second");
        assert_eq!(c.policy(&selected()).remaining_minutes(), 60);
    }

    #[test]
    fn campaign_and_drop_dates_leave_room_for_scheduling_and_claim_deadlines() {
        for path in [
            "/startAt",
            "/endAt",
            "/timeBasedDrops/0/startAt",
            "/timeBasedDrops/0/endAt",
        ] {
            for at in [DateTime::<Utc>::MIN_UTC, DateTime::<Utc>::MAX_UTC] {
                let mut raw = raw_campaign(vec![raw_drop("coat", &[])]);
                *raw.pointer_mut(path).unwrap() = json!(at.to_rfc3339());
                assert!(
                    Campaign::parse(&raw, &HashMap::new(), now()).is_err(),
                    "{path}: {at}"
                );
            }
        }
        let mut raw = raw_campaign(vec![raw_drop("coat", &[])]);
        raw["startAt"] = json!((DateTime::<Utc>::MIN_UTC + Duration::hours(1)).to_rfc3339());
        raw["endAt"] = json!((DateTime::<Utc>::MAX_UTC - Duration::hours(24)).to_rfc3339());
        let mut campaign = Campaign::parse(&raw, &HashMap::new(), now()).unwrap();
        campaign.drops[0].claim_id = Some("instance".into());
        assert!(campaign.drops[0].can_claim(campaign.ends_at, now()));
    }

    #[test]
    fn claims_are_independent_of_selection_ignore_and_expiry_until_strict_grace_end() {
        let mut c = campaign(vec![raw_drop("coat", &[])]);
        c.drops[0].claim_id = Some("instance".into());
        let settings = Settings::default()
            .patched(&json!({"drop_name_blacklist":["coat"]}))
            .unwrap();
        assert!(c.drops[0].can_claim(c.ends_at, c.ends_at + Duration::hours(23)));
        assert!(!c.drops[0].can_claim(c.ends_at, c.ends_at + Duration::hours(24)));
        assert!(c.view(&settings, now()).drops[0].can_claim);
        c.drops[0].mark_claimed(now());
        assert!(!c.drops[0].can_claim(c.ends_at, now()));
        assert!(c.view(&settings, now()).finished);
    }

    #[test]
    fn every_benefit_needs_account_evidence_within_the_reward_window() {
        let mut drop = raw_drop("coat", &[]);
        drop["benefitEdges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"benefit":{"id":"second","name":"Extra"}}));
        let raw = raw_campaign(vec![drop]);
        let mut awards = HashMap::from([("b-coat".into(), now())]);
        assert!(!Campaign::parse(&raw, &awards, now()).unwrap().drops[0].claimed);
        awards.insert("second".into(), "2026-09-28T12:00:00Z".parse().unwrap());
        assert!(!Campaign::parse(&raw, &awards, now()).unwrap().drops[0].claimed);
        awards.insert("second".into(), now());
        let claimed = Campaign::parse(&raw, &awards, now()).unwrap();
        assert!(claimed.drops[0].claimed);
        assert_eq!(claimed.drops[0].confirmed_minutes, 60);
        let mut explicit = raw;
        explicit["timeBasedDrops"][0]["self"] =
            json!({"isClaimed":false,"currentMinutesWatched":7});
        assert!(!Campaign::parse(&explicit, &awards, now()).unwrap().drops[0].claimed);
    }

    #[test]
    fn incomplete_metadata_cannot_create_or_erase_confirmed_completion() {
        let drop = raw_drop("coat", &[]);
        let awards = HashMap::from([("b-coat".into(), now())]);
        let raw = raw_campaign(vec![drop]);
        let mut missing_benefit = raw.clone();
        missing_benefit["timeBasedDrops"][0]["benefitEdges"]
            .as_array_mut()
            .unwrap()
            .push(Value::Null);
        assert!(Campaign::parse(&missing_benefit, &awards, now()).is_err());
        let mut missing_drop = raw.clone();
        missing_drop["timeBasedDrops"]
            .as_array_mut()
            .unwrap()
            .push(Value::Null);
        assert!(Campaign::parse(&missing_drop, &awards, now()).is_err());
        for incomplete in [
            json!({}),
            json!({"isClaimed":false}),
            json!({"currentMinutesWatched":4}),
        ] {
            let mut unknown = raw.clone();
            unknown["timeBasedDrops"][0]["self"] = incomplete;
            assert!(Campaign::parse(&unknown, &awards, now()).is_err());
        }
        let c = Campaign::parse(&raw, &awards, now()).unwrap();
        assert!(c.view(&selected(), now()).finished);
    }

    #[test]
    fn ignored_dependencies_prune_only_unused_branches() {
        let mut starter = raw_drop("starter", &[]);
        starter["benefitEdges"] = json!([]);
        let mut c = campaign(vec![
            starter,
            raw_drop("coat", &["starter"]),
            raw_drop("boots", &["starter"]),
            raw_drop("dependent", &["coat"]),
        ]);
        let settings = selected()
            .patched(&json!({"drop_name_blacklist":["coat"]}))
            .unwrap();
        let policy = c.policy(&settings);
        assert_eq!(
            policy.mineable,
            HashSet::from(["starter".into(), "boots".into()])
        );
        assert_eq!(policy.reasons["dependent"].precondition(), Some("coat"));
        let view = c.view(&settings, now());
        assert_eq!(
            (view.claimed_drops, view.ignored_drops, view.skipped_drops),
            (0, 2, 0)
        );
        c.drops[2].mark_claimed(now());
        assert!(!c.policy(&settings).mineable.contains("starter"));
        assert_eq!(c.view(&settings, now()).skipped_drops, 1);
        c.drops[1].mark_claimed(now());
        assert!(c.policy(&settings).mineable.contains("dependent"));
        assert!(c.prerequisites_met(&c.drops[3]));
    }

    #[test]
    fn missing_prerequisites_and_cycles_are_blocked_but_claimed_cycles_are_safe() {
        let mut c = campaign(vec![
            raw_drop("missing", &["absent"]),
            raw_drop("a", &["b"]),
            raw_drop("b", &["a"]),
        ]);
        assert!(c.policy(&selected()).mineable.is_empty());
        assert!(!c.can_watch(&channel(), &selected(), now()));
        c.drops[1].mark_claimed(now());
        let policy = c.policy(&selected());
        assert_eq!(policy.mineable, HashSet::from(["b".into()]));
        assert_eq!(policy.remaining_minutes(), 60);
    }

    #[test]
    fn special_categories_require_real_enabled_acl_for_cross_category_streams() {
        let mut raw = raw_campaign(vec![raw_drop("reward", &[])]);
        raw["game"]["id"] = "509663".into();
        raw["game"]["name"] = "Special Events".into();
        raw["allow"]["channels"] = json!([{"id":"10","name":"streamer"}]);
        let mut c = Campaign::parse(&raw, &HashMap::new(), now()).unwrap();
        let settings = selected()
            .patched(&json!({"games_to_watch":["Special Events"]}))
            .unwrap();
        let mut stream = channel();
        stream.drops_enabled = false;
        stream.game = None;
        assert!(c.can_watch(&stream, &settings, now()));
        assert!(!c.can_watch(&stream, &selected(), now()));
        stream.broadcast_id = None;
        assert!(!c.can_watch(&stream, &settings, now()));
        assert!(c.channel_eligible(&stream, true));
        stream = channel();
        c.allowed_channels.clear();
        assert!(!c.can_watch(&stream, &settings, now()));
        raw["allow"]["isEnabled"] = false.into();
        assert!(
            !Campaign::parse(&raw, &HashMap::new(), now())
                .unwrap()
                .can_watch(&stream, &settings, now())
        );
        raw["allow"]["isEnabled"] = Value::Null;
        assert!(
            !Campaign::parse(&raw, &HashMap::new(), now())
                .unwrap()
                .can_watch(&stream, &settings, now())
        );
        c.game.id = 1;
        c.game.name = "Rust".into();
        assert!(c.can_watch(&stream, &selected(), now()));
        stream.drops_enabled = false;
        assert!(!c.can_watch(&stream, &selected(), now()));
    }

    #[test]
    fn queue_omits_subscription_expired_and_ignored_rewards_but_keeps_sequences_and_art() {
        let mut subscription = raw_drop("subscription", &[]);
        subscription["requiredMinutesWatched"] = 0.into();
        let mut expired = raw_drop("expired", &[]);
        expired["endAt"] = "2026-09-26T11:00:00Z".into();
        let mut upcoming = raw_drop("upcoming", &[]);
        upcoming["startAt"] = "2026-09-26T12:30:00Z".into();
        let c = campaign(vec![
            subscription,
            expired,
            upcoming,
            raw_drop("coat", &[]),
            raw_drop("boots", &["coat"]),
            raw_drop("ignored", &[]),
        ]);
        let settings = selected()
            .patched(&json!({"drop_name_blacklist":["ignored"]}))
            .unwrap();
        let queue = wanted_items(&[c], &settings, now());
        let drops = &queue[0].campaigns[0].drops;
        assert_eq!(
            drops.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            ["upcoming", "coat", "boots"]
        );
        assert!(drops.iter().all(|d| d.image_url.ends_with("reward.png")));
        let empty = campaign(vec![]);
        assert!(!empty.view(&settings, now()).finished);
    }

    #[test]
    fn completed_archive_survives_restart_refresh_and_metadata_only_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let mut archive = CampaignArchive::load(dir.path());
        let mut c = campaign(vec![raw_drop("coat", &[])]);
        c.drops[0].mark_claimed(now());
        archive.update(&[c.view(&selected(), now())]).unwrap();
        let mut archive = CampaignArchive::load(dir.path());
        let historical = archive.merge(vec![], now() + Duration::days(90));
        assert!(historical[0].finished && historical[0].expired);
        assert!(!historical[0].active);
        c.drops[0].claimed = false;
        c.drops[0].confirmed_at = None;
        archive.update(&[c.view(&selected(), now())]).unwrap();
        assert!(archive.merge(vec![c.view(&selected(), now())], now())[0].finished);
        c.drops[0].confirm(3, now());
        archive.update(&[c.view(&selected(), now())]).unwrap();
        assert!(!archive.merge(vec![c.view(&selected(), now())], now())[0].finished);
        assert!(
            CampaignArchive::load(dir.path())
                .merge(vec![], now())
                .is_empty()
        );
        c.drops[0].mark_claimed(now());
        archive.update(&[c.view(&selected(), now())]).unwrap();
        c.drops
            .push(campaign(vec![raw_drop("new", &[])]).drops.remove(0));
        archive.update(&[c.view(&selected(), now())]).unwrap();
        assert!(
            CampaignArchive::load(dir.path())
                .merge(vec![], now())
                .is_empty()
        );
    }
}
