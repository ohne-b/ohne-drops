use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashSet},
    sync::LazyLock,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Duration, Utc};
use http::{Method, StatusCode};
use regex::Regex;
use serde_json::{Value, json};
use url::Url;

use super::{
    TwitchClient, TwitchError,
    inventory::values,
    operations::{Operation, directory},
    success,
};
use crate::{
    config::{Settings, fold},
    domain::{Campaign, Channel, ChannelIdentity, Game, number},
};

pub const MAX_CHANNELS: usize = 199;
static BEACON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)"beacon_?url"\s*:\s*"([^"\s]+)""#).unwrap());
static SETTINGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)src="(https?://[^"\s]+/config/settings\.[0-9a-f]{32}\.js)""#).unwrap()
});

/// Accept only a Twitch login or a channel root URL, never an arbitrary fetch target.
pub fn channel_login(input: &str) -> Option<String> {
    if input.len() > 256 {
        return None;
    }
    let input = input.trim();
    let login = if input.contains('/') || input.contains(':') {
        let url = Url::parse(input).ok()?;
        if !matches!(url.scheme(), "https" | "http")
            || !matches!(
                url.host_str(),
                Some("twitch.tv" | "www.twitch.tv" | "m.twitch.tv")
            )
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        url.path().trim_matches('/').to_owned()
    } else {
        input.trim_start_matches('@').to_owned()
    };
    (1..=25).contains(&login.len()).then_some(())?;
    login
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        .then(|| login.to_ascii_lowercase())
}

#[derive(Clone)]
pub struct ResolvedChannel {
    pub channel: Channel,
    pub campaigns: HashSet<String>,
}

impl TwitchClient {
    pub async fn resolve_channel(
        &self,
        login: &str,
    ) -> Result<Option<ResolvedChannel>, TwitchError> {
        let login = channel_login(login).ok_or(TwitchError::InvalidResponse)?;
        let response = self
            .gql(Operation::StreamInfo.request(json!({"channel":login})))
            .await?;
        let user = &response["data"]["user"];
        if user.is_null() {
            return Ok(None);
        }
        let identity = ChannelIdentity {
            id: number(&user["id"]).ok_or(TwitchError::InvalidResponse)?,
            name: user["displayName"]
                .as_str()
                .filter(|name| !name.is_empty())
                .unwrap_or(&login)
                .to_owned(),
            login,
        };
        let mut channel = Channel::offline(identity, false);
        update_stream(&mut channel, user);
        let campaigns = if channel.online() {
            // Reward metadata is optional for an explicit watch request. Keep it
            // off the critical path after this short attempt; auth still fails closed.
            match tokio::time::timeout(
                std::time::Duration::from_secs(2),
                self.gql(
                    Operation::AvailableDrops
                        .request(json!({"channelID":channel.identity.id.to_string()})),
                ),
            )
            .await
            {
                Ok(Ok(response)) => values(&response["data"]["channel"]["viewerDropCampaigns"])
                    .filter_map(|v| v["id"].as_str().map(str::to_owned))
                    .collect(),
                Ok(Err(error @ (TwitchError::Unauthorized | TwitchError::Cancelled))) => {
                    return Err(error);
                }
                _ if self.http.cancel.is_cancelled() => return Err(TwitchError::Cancelled),
                _ => HashSet::new(),
            }
        } else {
            HashSet::new()
        };
        channel.drops_enabled = !campaigns.is_empty();
        Ok(Some(ResolvedChannel { channel, campaigns }))
    }

    pub async fn update_manual_channel(&self, channel: &mut Channel) -> Result<(), TwitchError> {
        let resolved = self.resolve_channel(&channel.identity.login).await?;
        let mut fresh = resolved
            .filter(|r| r.channel.identity.id == channel.identity.id)
            .map(|r| r.channel)
            .unwrap_or_else(|| Channel::offline(channel.identity.clone(), channel.acl_based));
        fresh.acl_based = channel.acl_based;
        if fresh.broadcast_id == channel.broadcast_id {
            fresh.beacon_url = channel.beacon_url.clone();
        }
        *channel = fresh;
        Ok(())
    }

    pub async fn channels(
        &self,
        campaigns: &[Campaign],
        settings: &Settings,
        current: Option<&Channel>,
    ) -> Result<Vec<Channel>, TwitchError> {
        let now = Utc::now();
        let mut campaigns: Vec<_> = campaigns
            .iter()
            .filter(|c| {
                settings.selected(&c.game.name)
                    && c.can_earn_within(settings, now, now + Duration::hours(1))
            })
            .collect();
        campaigns.sort_by_key(|c| game_priority(settings, Some(&c.game)));
        let mut channels = BTreeMap::new();
        let mut directories = BTreeMap::new();
        for campaign in &campaigns {
            if let Some(discovered) = &campaign.discovery_channels {
                for identity in discovered {
                    channels
                        .entry(identity.id)
                        .or_insert_with(|| Channel::offline(identity.clone(), true));
                }
            } else if !campaign.allowed_channels.is_empty() {
                for identity in &campaign.allowed_channels {
                    channels
                        .entry(identity.id)
                        .or_insert_with(|| Channel::offline(identity.clone(), true));
                }
            } else {
                directories.insert(
                    game_priority(settings, Some(&campaign.game)),
                    &campaign.game,
                );
            }
        }
        if let Some(current) = current {
            channels
                .entry(current.identity.id)
                .or_insert_with(|| Channel::offline(current.identity.clone(), current.acl_based));
        }
        let mut restricted: Vec<_> = channels.into_values().collect();
        // Check the participating lists before trimming: an offline popular game
        // must not hide a live participant farther down its campaign ACL.
        self.update_channels(&mut restricted).await?;
        let mut channels: BTreeMap<_, _> =
            restricted.into_iter().map(|c| (c.identity.id, c)).collect();
        let games: Vec<_> = directories.into_values().collect();
        for games in games.chunks(20) {
            let responses = self
                .batch(games.iter().map(|g| directory(&g.slug, 20)).collect())
                .await?;
            for (game, response) in games.iter().zip(responses) {
                for edge in values(&response["data"]["game"]["streams"]["edges"]).take(20) {
                    if let Some(channel) = directory_channel(&edge["node"], game) {
                        channels
                            .entry(channel.identity.id)
                            .and_modify(|old| {
                                let acl = old.acl_based;
                                *old = channel.clone();
                                old.acl_based = acl;
                            })
                            .or_insert(channel);
                    }
                }
            }
        }
        if let Some(current) = current
            && let Some(updated) = channels.get_mut(&current.identity.id)
            && updated.broadcast_id == current.broadcast_id
        {
            updated.beacon_url = current.beacon_url.clone();
        }
        let mut channels: Vec<_> = channels.into_values().collect();
        let mineable: Vec<_> = campaigns
            .iter()
            .filter(|c| c.can_mine(settings, now))
            .copied()
            .collect();
        channels.sort_by_key(|c| {
            (
                channel_priority(c, &mineable, settings),
                Reverse(c.acl_based),
                Reverse(c.viewers),
                c.identity.id,
            )
        });
        // Preserve the watched row through a settings rebuild. Eligibility is still
        // checked against the new settings before another beacon can be sent.
        if let Some(current) = current
            && let Some(index) = channels
                .iter()
                .position(|c| c.identity.id == current.identity.id)
        {
            let current = channels.remove(index);
            channels.insert(0, current);
        }
        channels.truncate(MAX_CHANNELS);
        Ok(channels)
    }

    pub async fn update_channels(&self, channels: &mut [Channel]) -> Result<(), TwitchError> {
        for channels in channels.chunks_mut(10) {
            let queries = channels
                .iter()
                .flat_map(|c| {
                    [
                        Operation::StreamInfo.request(json!({"channel":c.identity.login})),
                        Operation::AvailableDrops
                            .request(json!({"channelID":c.identity.id.to_string()})),
                    ]
                })
                .collect();
            let responses = self.batch(queries).await?;
            for (channel, responses) in channels.iter_mut().zip(responses.chunks_exact(2)) {
                let user = &responses[0]["data"]["user"];
                update_stream(channel, user);
                channel.drops_enabled = channel.online()
                    && values(&responses[1]["data"]["channel"]["viewerDropCampaigns"])
                        .any(|v| v["id"].is_string());
            }
        }
        Ok(())
    }

    async fn page(&self, url: Url) -> Result<String, TwitchError> {
        let response = self
            .http
            .execute(self.http.request(Method::GET, url), true)
            .await?;
        success(response.status())?;
        String::from_utf8(response.into_body()).map_err(|_| TwitchError::InvalidResponse)
    }

    fn trusted_url(&self, raw: &str) -> Result<Url, TwitchError> {
        let url = Url::parse(raw).map_err(|_| TwitchError::InvalidResponse)?;
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(TwitchError::InvalidResponse);
        }
        #[cfg(test)]
        if url.origin() == self.http.endpoints.web.origin() {
            return Ok(url);
        }
        let trusted = url.scheme() == "https"
            && url.port().is_none()
            && url.host_str().is_some_and(|host| {
                ["twitch.tv", "jtvnw.net", "ttvnw.net"]
                    .iter()
                    .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
            });
        if trusted {
            Ok(url)
        } else {
            Err(TwitchError::InvalidResponse)
        }
    }

    pub async fn beacon(&self, channel: &Channel) -> Result<Url, TwitchError> {
        if channel.identity.login.is_empty()
            || !channel
                .identity
                .login
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(TwitchError::InvalidResponse);
        }
        let url = self
            .http
            .endpoints
            .web
            .join(&channel.identity.login)
            .map_err(|_| TwitchError::InvalidResponse)?;
        let page = self.page(url).await?;
        if let Some(beacon) = BEACON.captures(&page) {
            return self.trusted_url(&beacon[1]);
        }
        let settings = SETTINGS
            .captures(&page)
            .ok_or(TwitchError::InvalidResponse)?;
        let script = self.page(self.trusted_url(&settings[1])?).await?;
        let beacon = BEACON
            .captures(&script)
            .ok_or(TwitchError::InvalidResponse)?;
        self.trusted_url(&beacon[1])
    }

    pub async fn send_watch(
        &self,
        channel: &mut Channel,
        now: DateTime<Utc>,
    ) -> Result<bool, TwitchError> {
        let Some(broadcast) = &channel.broadcast_id else {
            return Ok(false);
        };
        let payload = json!([{"event":"minute-watched","properties":{
            "broadcast_id":broadcast,"channel_id":channel.identity.id.to_string(),"channel":channel.identity.login,
            "client_time":now.to_rfc3339_opts(chrono::SecondsFormat::Micros,true),
            "game":channel.game.as_ref().map_or("",|g|g.name.as_str()),"game_id":channel.game.as_ref().map(|g|g.id.to_string()).unwrap_or_default(),
            "hidden":false,"is_live":true,"live":true,"location":"channel","logged_in":true,"minutes_logged":1,
            "muted":false,"player":"site","user_id":self.user_id,
        }}]);
        let url = match &channel.beacon_url {
            Some(url) => url.clone(),
            None => self.beacon(channel).await?,
        };
        channel.beacon_url = Some(url.clone());
        let response = self
            .http
            .execute(
                self.http
                    .request(Method::POST, url)
                    .form(&[("data", STANDARD.encode(payload.to_string()))]),
                false,
            )
            .await?;
        if !response.status().is_success() {
            channel.beacon_url = None;
        }
        Ok(response.status() == StatusCode::NO_CONTENT)
    }

    pub async fn current_drop(
        &self,
        channel_id: u64,
    ) -> Result<Option<(String, u32)>, TwitchError> {
        let response = self
            .gql(Operation::CurrentDrop.request(json!({"channelID":channel_id.to_string()})))
            .await?;
        let Some(drop) = response
            .pointer("/data/currentUser/dropCurrentSession")
            .filter(|v| !v.is_null())
        else {
            return Ok(None);
        };
        let id = drop["dropID"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or(TwitchError::InvalidResponse)?;
        let minutes = number(&drop["currentMinutesWatched"])
            .and_then(|v| u32::try_from(v).ok())
            .ok_or(TwitchError::InvalidResponse)?;
        Ok(Some((id.to_owned(), minutes)))
    }

    pub async fn claim(&self, claim_id: &str) -> Result<bool, TwitchError> {
        let response = self
            .gql(Operation::ClaimDrop.request(json!({"input":{"dropInstanceID":claim_id}})))
            .await?;
        Ok(matches!(
            response["data"]["claimDropRewards"]["status"].as_str(),
            Some("ELIGIBLE_FOR_ALL" | "DROP_INSTANCE_ALREADY_CLAIMED")
        ))
    }

    pub async fn delete_notification(&self, id: &str) -> Result<(), TwitchError> {
        self.gql(Operation::DeleteNotification.request(json!({"input":{"id":id}})))
            .await?;
        Ok(())
    }
}

fn update_stream(channel: &mut Channel, user: &Value) {
    let stream = &user["stream"];
    let broadcast_id = stream["id"]
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    if channel.broadcast_id != broadcast_id {
        channel.beacon_url = None;
    }
    channel.broadcast_id = broadcast_id;
    channel.game = channel
        .online()
        .then(|| Game::parse(&user["broadcastSettings"]["game"]).ok())
        .flatten();
    channel.viewers = channel
        .online()
        .then(|| number(&stream["viewersCount"]))
        .flatten();
    if let Some(name) = user["displayName"].as_str().filter(|n| !n.is_empty()) {
        channel.identity.name = name.to_owned();
    }
}

fn directory_channel(raw: &Value, game: &Game) -> Option<Channel> {
    Some(Channel {
        identity: ChannelIdentity::parse(&raw["broadcaster"]).ok()?,
        game: Some(Game::parse(&raw["game"]).unwrap_or_else(|_| game.clone())),
        broadcast_id: Some(raw["id"].as_str().filter(|s| !s.is_empty())?.to_owned()),
        viewers: number(&raw["viewersCount"]),
        drops_enabled: true,
        acl_based: false,
        beacon_url: None,
    })
}
pub fn game_priority(settings: &Settings, game: Option<&Game>) -> usize {
    game.and_then(|game| {
        settings
            .games_to_watch
            .iter()
            .position(|name| fold(name) == fold(&game.name))
    })
    .unwrap_or(usize::MAX)
}

fn channel_priority(channel: &Channel, campaigns: &[&Campaign], settings: &Settings) -> usize {
    campaigns
        .iter()
        .filter(|c| c.matches_channel(channel))
        .map(|c| game_priority(settings, Some(&c.game)))
        .min()
        .unwrap_or(usize::MAX)
}

pub fn select_channel(
    channels: &[Channel],
    campaigns: &[Campaign],
    settings: &Settings,
    now: DateTime<Utc>,
    current: Option<u64>,
    manual: Option<u64>,
) -> Option<u64> {
    if let Some(id) = manual {
        return channels
            .iter()
            .find(|c| c.identity.id == id && c.online())
            .map(|c| c.identity.id);
    }
    let campaigns: Vec<_> = campaigns
        .iter()
        .filter(|c| c.can_mine(settings, now))
        .collect();
    let eligible = |channel: &&Channel| campaigns.iter().any(|c| c.matches_channel(channel));
    let best = channels.iter().filter(eligible).min_by_key(|c| {
        (
            channel_priority(c, &campaigns, settings),
            Reverse(c.acl_based),
            Reverse(c.viewers),
            c.identity.id,
        )
    })?;
    if let Some(current) = channels
        .iter()
        .filter(eligible)
        .find(|c| Some(c.identity.id) == current)
        && (
            channel_priority(current, &campaigns, settings),
            Reverse(current.acl_based),
        ) <= (
            channel_priority(best, &campaigns, settings),
            Reverse(best.acl_based),
        )
    {
        return Some(current.identity.id);
    }
    Some(best.identity.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::twitch::tests::{campaign_json, gql_mock, http, session};
    use std::{collections::HashMap, sync::Arc};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[test]
    fn manual_input_is_a_twitch_login_not_a_network_target() {
        for input in [
            "  Extra_Streamer  ",
            "@Extra_Streamer",
            "https://www.twitch.tv/Extra_Streamer/",
        ] {
            assert_eq!(channel_login(input).as_deref(), Some("extra_streamer"));
        }
        for input in [
            "",
            "https://evil.test/a",
            "https://twitch.tv.evil.test/a",
            "https://user:secret@twitch.tv/a",
            "https://twitch.tv:8443/a",
            "https://twitch.tv/a/videos",
            "https://twitch.tv/a?token=secret",
            "https://twitch.tv/a#secret",
            "http://127.0.0.1/a",
            "a b",
            "a\\b",
            "a".repeat(26).as_str(),
        ] {
            assert!(
                channel_login(input).is_none(),
                "accepted invalid channel input"
            );
        }
    }

    #[test]
    fn cross_category_acl_streams_use_the_selected_campaign_priority() {
        let now = Utc::now();
        let rust = Campaign::parse(&campaign_json("rust"), &HashMap::new(), now).unwrap();
        let mut event = rust.clone();
        event.id = "event".into();
        event.game.id = 509663;
        event.game.name = "Special Events".into();
        let regular = channel(10);
        let mut host = channel(11);
        host.game.as_mut().unwrap().id = 509658;
        host.game.as_mut().unwrap().name = "Just Chatting".into();
        event.allowed_channels = vec![host.identity.clone()];
        let settings = Settings {
            games_to_watch: vec!["Special Events".into(), "Rust".into()],
            ..Settings::default()
        };
        assert_eq!(
            select_channel(
                &[regular, host],
                &[rust, event],
                &settings,
                now,
                Some(10),
                None
            ),
            Some(11)
        );
    }

    #[tokio::test]
    async fn eligible_streams_survive_the_limit_ahead_of_unrelated_recovered_channels() {
        let server = MockServer::start().await;
        gql_mock(&server, |q| match q["operationName"].as_str().unwrap() {
            "VideoPlayerStreamInfoOverlayChannel" => {
                let live = q["variables"]["channel"] == "wanted";
                json!({"data":{"user":{"stream":{"id":"stream","viewersCount":if live {1} else {10000}},"broadcastSettings":{"game":{"id":if live {"1"} else {"2"},"name":if live {"Rust"} else {"Other"}}}}}})
            },
            "DropsHighlightService_AvailableDrops" => json!({"data":{"channel":{"viewerDropCampaigns":[{"id":"one"}]}}}),
            other => panic!("unexpected operation {other}"),
        }).await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        let mut campaign =
            Campaign::parse(&campaign_json("one"), &HashMap::new(), Utc::now()).unwrap();
        let mut identities: Vec<_> = (1..=MAX_CHANNELS as u64)
            .map(|id| ChannelIdentity {
                id,
                login: format!("unrelated{id}"),
                name: format!("Unrelated {id}"),
            })
            .collect();
        identities.push(ChannelIdentity {
            id: 999,
            login: "wanted".into(),
            name: "Wanted".into(),
        });
        campaign.discovery_channels = Some(identities);
        let settings = Settings {
            games_to_watch: vec!["Rust".into()],
            ..Settings::default()
        };
        let channels = client.channels(&[campaign], &settings, None).await.unwrap();
        assert_eq!(channels.len(), MAX_CHANNELS);
        assert_eq!(channels[0].identity.id, 999);
    }

    #[tokio::test]
    async fn manual_lookup_resolves_identity_and_available_campaigns_without_scanning_directories()
    {
        let server = MockServer::start().await;
        gql_mock(&server, |q| match q["operationName"].as_str().unwrap() {
            "VideoPlayerStreamInfoOverlayChannel" => {
                assert_eq!(q["variables"]["channel"], "extra_streamer");
                json!({"data":{"user":{"id":"999","displayName":"Extra Streamer","stream":{"id":"live","viewersCount":null},"broadcastSettings":{"game":{"id":"1","name":"Rust","slug":"rust"}}}}})
            },
            "DropsHighlightService_AvailableDrops" => {
                assert_eq!(q["variables"]["channelID"], "999");
                json!({"data":{"channel":{"viewerDropCampaigns":[null,{"id":"one"}]}}})
            },
            other => panic!("unexpected operation {other}"),
        }).await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        let resolved = client
            .resolve_channel("extra_streamer")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.channel.identity.id, 999);
        assert_eq!(resolved.channel.viewers, None);
        assert_eq!(resolved.channel.game.unwrap().name, "Rust");
        assert!(resolved.channel.drops_enabled);
        assert_eq!(resolved.campaigns, HashSet::from(["one".into()]));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
        server.reset().await;
        gql_mock(&server, |_| json!({"data":{"user":null}})).await;
        assert!(client.resolve_channel("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn manual_lookup_and_refresh_tolerate_failed_or_slow_metadata_but_propagate_auth() {
        for error in ["catalog unavailable", "service unavailable", "Unauthorized"] {
            let server = MockServer::start().await;
            gql_mock(&server, move |q| match q["operationName"].as_str().unwrap() {
                "VideoPlayerStreamInfoOverlayChannel" => json!({"data":{"user":{"id":"10","displayName":"Streamer","stream":{"id":"live"}}}}),
                "DropsHighlightService_AvailableDrops" => json!({"errors":[{"message":error}]}),
                other => panic!("unexpected operation {other}"),
            }).await;
            let client = TwitchClient::new(Arc::new(http(&server)), &session());
            let lookup = tokio::time::timeout(
                std::time::Duration::from_secs(4),
                client.resolve_channel("streamer"),
            )
            .await
            .unwrap();
            let mut channel = Channel::offline(channel(10).identity, false);
            let refresh = tokio::time::timeout(
                std::time::Duration::from_secs(4),
                client.update_manual_channel(&mut channel),
            )
            .await
            .unwrap();
            if error == "Unauthorized" {
                assert!(matches!(lookup, Err(TwitchError::Unauthorized)));
                assert_eq!(refresh, Err(TwitchError::Unauthorized));
                assert!(!channel.online());
            } else {
                let resolved = lookup.unwrap().unwrap();
                assert!(resolved.channel.online());
                assert!(resolved.campaigns.is_empty());
                refresh.unwrap();
                assert!(channel.online());
                assert!(!channel.drops_enabled);
                if error == "catalog unavailable" {
                    assert!(
                        client.update_channels(&mut [channel]).await.is_err(),
                        "automatic discovery must still require reward evidence"
                    );
                }
            }
        }
    }

    fn channel(id: u64) -> Channel {
        Channel {
            identity: ChannelIdentity {
                id,
                login: "streamer".into(),
                name: "Streamer".into(),
            },
            game: Some(Game {
                id: 1,
                name: "Rust".into(),
                slug: "rust".into(),
                image_url: String::new(),
            }),
            broadcast_id: Some("123".into()),
            viewers: Some(50),
            drops_enabled: true,
            acl_based: false,
            beacon_url: None,
        }
    }

    #[tokio::test]
    async fn both_beacon_formats_send_exact_watch_events_without_playlists_or_credentials() {
        for script in [false, true] {
            let server = MockServer::start().await;
            let url = format!("{}/track", server.uri());
            let body = format!(r#"{{"beacon_url":"{url}"}}"#);
            let script_path = "/config/settings.0123456789abcdef0123456789abcdef.js";
            let html = if script {
                format!(r#"<script src="{}{script_path}"></script>"#, server.uri())
            } else {
                body.clone()
            };
            Mock::given(method("GET"))
                .and(path("/streamer"))
                .respond_with(ResponseTemplate::new(200).set_body_string(html))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(script_path))
                .respond_with(ResponseTemplate::new(200).set_body_string(body))
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/track"))
                .respond_with(ResponseTemplate::new(204))
                .mount(&server)
                .await;
            let client = TwitchClient::new(Arc::new(http(&server)), &session());
            let mut channel = channel(10);
            let now = Utc::now();
            assert!(client.send_watch(&mut channel, now).await.unwrap());
            assert!(client.send_watch(&mut channel, now).await.unwrap());
            let requests = server.received_requests().await.unwrap();
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r.url.path() == "/streamer")
                    .count(),
                1
            );
            assert_eq!(requests.len(), if script { 4 } else { 3 });
            let watch = requests.iter().find(|r| r.method == "POST").unwrap();
            assert!(!watch.headers.contains_key("Authorization"));
            let form: HashMap<_, _> = url::form_urlencoded::parse(&watch.body).collect();
            let payload: Value =
                serde_json::from_slice(&STANDARD.decode(form["data"].as_bytes()).unwrap()).unwrap();
            assert_eq!(payload[0]["event"], "minute-watched");
            let fields = &payload[0]["properties"];
            assert_eq!(fields["channel_id"], "10");
            assert_eq!(fields["broadcast_id"], "123");
            assert_eq!(fields["game_id"], "1");
            assert_eq!(fields["user_id"], 42);
            assert_eq!(fields["minutes_logged"], 1);
            assert_eq!(
                fields["client_time"]
                    .as_str()
                    .unwrap()
                    .parse::<DateTime<Utc>>()
                    .unwrap()
                    .timestamp(),
                now.timestamp()
            );
            assert_eq!(fields["hidden"], false);
            assert_eq!(fields["is_live"], true);
            assert_eq!(fields["player"], "site");
            for hostile in [
                "https://twitch.tv.evil.example/track",
                "https://evil.example/track",
                "http://spade.twitch.tv/track",
                "https://user:pass@spade.twitch.tv/track",
                "https://spade.twitch.tv:8443/track",
            ] {
                assert!(client.trusted_url(hostile).is_err());
            }
            assert!(client.trusted_url("https://spade.twitch.tv/track").is_ok());
            assert!(
                client
                    .trusted_url("https://assets.twitch.tv/config/file.js")
                    .is_ok()
            );
        }
    }

    #[tokio::test]
    async fn stream_updates_preserve_nullable_viewers_and_clear_stale_stream_artifacts() {
        let server = MockServer::start().await;
        gql_mock(&server,|query|match query["operationName"].as_str().unwrap() {
            "VideoPlayerStreamInfoOverlayChannel"=>json!({"data":{"user":{"id":"10","displayName":"New name","stream":{"id":"456","viewersCount":null},"broadcastSettings":{"game":{"id":"1","name":"Rust"}}}}}),
            "DropsHighlightService_AvailableDrops"=>json!({"data":{"channel":{"viewerDropCampaigns":[null,{"id":"campaign"}]}}}),
            _=>panic!("unexpected query"),
        }).await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        let mut channels = vec![channel(10)];
        channels[0].beacon_url = Some(Url::parse("https://spade.twitch.tv/old").unwrap());
        client.update_channels(&mut channels).await.unwrap();
        assert_eq!(channels[0].broadcast_id.as_deref(), Some("456"));
        assert!(channels[0].beacon_url.is_none());
        assert!(channels[0].viewers.is_none());
        assert!(channels[0].drops_enabled);
        server.reset().await;
        gql_mock(&server, |_| json!({"data":{"user":null,"channel":null}})).await;
        client.update_channels(&mut channels).await.unwrap();
        assert!(!channels[0].online());
        assert!(channels[0].game.is_none());
        assert!(!channels[0].drops_enabled);
    }

    #[test]
    fn selection_preserves_healthy_streams_and_special_category_failover_without_bypassing_opt_in()
    {
        let now = Utc::now();
        let mut campaign =
            Campaign::parse(&campaign_json("campaign"), &HashMap::new(), now).unwrap();
        let mut settings = Settings::default();
        let mut channels = vec![channel(10), channel(11)];
        channels[1].viewers = Some(100);
        assert_eq!(
            select_channel(&channels, &[campaign.clone()], &settings, now, None, None),
            None
        );
        settings.games_to_watch = vec!["Rust".into()];
        assert_eq!(
            select_channel(&channels, &[campaign.clone()], &settings, now, None, None),
            Some(11)
        );
        assert_eq!(
            select_channel(
                &channels,
                &[campaign.clone()],
                &settings,
                now,
                Some(10),
                None
            ),
            Some(10)
        );
        channels[1].acl_based = true;
        assert_eq!(
            select_channel(
                &channels,
                &[campaign.clone()],
                &settings,
                now,
                Some(10),
                None
            ),
            Some(11)
        );
        assert_eq!(
            select_channel(
                &channels,
                &[campaign.clone()],
                &settings,
                now,
                None,
                Some(10)
            ),
            Some(10)
        );
        campaign.game.id = 509663;
        campaign.game.name = "Special Events".into();
        campaign.allowed_channels = channels.iter().map(|c| c.identity.clone()).collect();
        settings.games_to_watch = vec!["Special Events".into()];
        channels[0].broadcast_id = None;
        channels[1].drops_enabled = false;
        assert_eq!(
            select_channel(
                &channels,
                &[campaign.clone()],
                &settings,
                now,
                Some(10),
                None
            ),
            Some(11)
        );
        campaign.allowed_channels.clear();
        assert_eq!(
            select_channel(&channels, &[campaign], &settings, now, None, None),
            None
        );
    }

    #[tokio::test]
    async fn claims_use_account_ids_and_only_accept_twitch_success_states() {
        let server = MockServer::start().await;
        gql_mock(&server, |query| {
            assert_eq!(
                query["variables"]["input"]["dropInstanceID"],
                "earned-instance"
            );
            json!({"data":{"claimDropRewards":{"status":"DROP_INSTANCE_ALREADY_CLAIMED"}}})
        })
        .await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        assert!(client.claim("earned-instance").await.unwrap());
        server.reset().await;
        gql_mock(
            &server,
            |_| json!({"data":{"claimDropRewards":{"status":"NOT_ELIGIBLE"}}}),
        )
        .await;
        assert!(!client.claim("earned-instance").await.unwrap());
    }
}
