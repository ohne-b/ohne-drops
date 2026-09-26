use std::{
    collections::{BTreeMap, HashMap, HashSet},
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::{
    TwitchClient, TwitchError,
    operations::{Operation, directory},
};
use crate::{
    config::Settings,
    domain::{Campaign, ChannelIdentity, Game, number},
    dto::InventoryStatus,
};

const GAMES_QUERY: &str = r#"query DropsDiscoveryGames($after: Cursor) {
  games(first: 100, after: $after) {
    edges { cursor node { streams(first: 3, options: {systemFilters: [DROPS_ENABLED]}) {
      edges { node { broadcaster { id login displayName } } }
    } } }
    pageInfo { hasNextPage }
  }
}"#;
const RECOVERY_QUERY: &str = r#"query ChannelDropsRecovery($channelID: ID!) {
  channel(id: $channelID) { id viewerDropCampaigns {
    id name status startAt endAt accountLinkURL
    game { id name displayName slug boxArtURL }
    allow { isEnabled channels { id name displayName } }
    timeBasedDrops {
      id name startAt endAt requiredMinutesWatched preconditionDrops { id }
      benefitEdges { benefit { id name distributionType imageAssetURL } }
    }
  } }
}"#;

pub struct Inventory {
    pub campaigns: Vec<Campaign>,
    pub status: InventoryStatus,
}

impl TwitchClient {
    pub async fn inventory(&self, settings: &Settings) -> Result<Inventory, TwitchError> {
        let response = self.gql(Operation::Inventory.request(json!({}))).await?;
        let inventory = response
            .pointer("/data/currentUser/inventory")
            .filter(|v| v.is_object())
            .ok_or(TwitchError::InvalidResponse)?;
        let awards: HashMap<String, DateTime<Utc>> = values(&inventory["gameEventDrops"])
            .filter_map(|v| {
                Some((
                    v["id"].as_str()?.to_owned(),
                    v["lastAwardedAt"].as_str()?.parse().ok()?,
                ))
            })
            .collect();
        let mut account = records(&inventory["dropCampaignsInProgress"]);
        let response = self.gql(Operation::Campaigns.request(json!({}))).await?;
        let catalog = response
            .pointer("/data/currentUser/dropCampaigns")
            .and_then(Value::as_array);
        let summaries: BTreeMap<String, Value> = catalog
            .into_iter()
            .flatten()
            .filter(|v| matches!(v["status"].as_str(), Some("ACTIVE" | "UPCOMING")))
            .filter_map(|v| Some((v["id"].as_str()?.to_owned(), v.clone())))
            .collect();
        let ids: Vec<_> = summaries.keys().collect();
        let mut fetched = HashSet::new();
        for ids in ids.chunks(20) {
            let queries: Vec<_> = ids
                .iter()
                .map(|id| {
                    Operation::CampaignDetails
                        .request(json!({"channelLogin":self.user_id.to_string(),"dropID":id}))
                })
                .collect();
            for response in self.batch(queries).await? {
                let Some(mut detail) = response
                    .pointer("/data/user/dropCampaign")
                    .filter(|v| v.is_object())
                    .cloned()
                else {
                    continue;
                };
                let Some(id) = detail["id"]
                    .as_str()
                    .filter(|id| ids.iter().any(|requested| requested.as_str() == *id))
                    .map(str::to_owned)
                else {
                    continue;
                };
                fill_missing(&mut detail, &summaries[&id]);
                if let Some(ongoing) = account.get_mut(&id) {
                    fill_missing(ongoing, &detail);
                } else {
                    account.insert(id.clone(), detail);
                }
                fetched.insert(id);
            }
        }
        let now = Utc::now();
        let mut campaigns: BTreeMap<_, _> = account
            .iter()
            .filter_map(|(id, v)| {
                Campaign::parse(v, &awards, now)
                    .ok()
                    .map(|c| (id.clone(), c))
            })
            .collect();
        let available = catalog.is_some()
            && summaries
                .keys()
                .all(|id| fetched.contains(id) && campaigns.contains_key(id));
        let mut recovered = 0;
        // An authoritative empty catalog does not need speculative channel discovery.
        if !available && catalog.is_none_or(|v| !v.is_empty()) {
            let mut slugs = Vec::new();
            let mut seen = HashSet::new();
            for slug in account
                .values()
                .chain(summaries.values())
                .filter_map(|v| Game::parse(&v["game"]).ok())
                .map(|g| g.slug)
                .chain(settings.games_to_watch.iter().map(|s| Game::slug(s)))
            {
                if !slug.is_empty() && seen.insert(slug.clone()) {
                    slugs.push(slug);
                }
            }
            for (id, mut raw) in self.recover_campaigns(&slugs).await? {
                // Never blend metadata-only recovery into an account record, including
                // an incomplete one. That would manufacture certainty about its rewards.
                if account.contains_key(&id) || catalog.is_some() && !summaries.contains_key(&id) {
                    continue;
                }
                if let Some(account) = summaries.get(&id).and_then(|v| v.get("self")) {
                    raw["self"] = account.clone();
                }
                if let Ok(campaign) = Campaign::parse(&raw, &awards, now)
                    && (campaign.active(now) || campaign.upcoming(now))
                    && campaign.drops.iter().any(|d| d.watch_reward())
                {
                    campaigns.insert(id, campaign);
                    recovered += 1;
                }
            }
        }
        Ok(Inventory {
            campaigns: campaigns.into_values().collect(),
            status: InventoryStatus {
                available,
                recovered,
                checked_at: Some(now),
            },
        })
    }

    pub(crate) async fn batch(&self, queries: Vec<Value>) -> Result<Vec<Value>, TwitchError> {
        if queries.is_empty() {
            return Ok(vec![]);
        }
        let expected = queries.len();
        let response = self.gql(Value::Array(queries)).await?;
        match response {
            Value::Array(values) if values.len() == expected => Ok(values),
            Value::Object(_) if expected == 1 => Ok(vec![response]),
            _ => Err(TwitchError::InvalidResponse),
        }
    }

    async fn recover_campaigns(
        &self,
        slugs: &[String],
    ) -> Result<BTreeMap<String, Value>, TwitchError> {
        let mut discovered = BTreeMap::new();
        let scan = self.scan_campaigns(slugs, &mut discovered);
        match tokio::time::timeout(Duration::from_secs(60), scan).await {
            Ok(Err(error @ (TwitchError::Unauthorized | TwitchError::Cancelled))) => {
                return Err(error);
            }
            Ok(Ok(())) => {}
            _ => tracing::warn!("Live-channel campaign discovery is incomplete"),
        }
        Ok(discovered)
    }

    async fn scan_campaigns(
        &self,
        slugs: &[String],
        discovered: &mut BTreeMap<String, Value>,
    ) -> Result<(), TwitchError> {
        let mut channels = BTreeMap::new();
        for slugs in slugs[..slugs.len().min(100)].chunks(20) {
            for response in self
                .batch(slugs.iter().map(|slug| directory(slug, 3)).collect())
                .await?
            {
                collect_channels(&response["data"]["game"], &mut channels);
            }
        }
        let mut cursor = Value::Null;
        for _ in 0..5 {
            let response = self.gql(json!({"operationName":"DropsDiscoveryGames","query":GAMES_QUERY,"variables":{"after":cursor}})).await?;
            let games = &response["data"]["games"];
            for edge in values(&games["edges"]).take(100) {
                collect_channels(&edge["node"], &mut channels);
            }
            let next = values(&games["edges"])
                .filter_map(|v| v["cursor"].as_str())
                .next_back();
            if games["pageInfo"]["hasNextPage"] != true
                || next.is_none_or(|s| s.is_empty() || cursor == s)
            {
                break;
            }
            cursor = next.unwrap().into();
        }
        let ids: Vec<_> = channels.keys().copied().collect();
        for ids in ids.chunks(20) {
            let queries = ids.iter().map(|id|json!({"operationName":"ChannelDropsRecovery","query":RECOVERY_QUERY,"variables":{"channelID":id.to_string()}})).collect();
            for response in self.batch(queries).await? {
                let source = &response["data"]["channel"];
                let Some(channel) = number(&source["id"])
                    .filter(|id| ids.contains(id))
                    .and_then(|id| channels.get(&id))
                else {
                    continue;
                };
                for raw in values(&source["viewerDropCampaigns"]) {
                    let Some(id) = raw["id"].as_str().filter(|id| !id.is_empty()) else {
                        continue;
                    };
                    if !matches!(raw["status"].as_str(), Some("ACTIVE" | "UPCOMING"))
                        || !raw["game"].is_object()
                    {
                        continue;
                    }
                    let value = discovered.entry(id.to_owned()).or_insert_with(|| {
                        let mut raw = raw.clone();
                        raw["self"] = json!({"isAccountConnected":null});
                        raw["discovery_channels"] = json!([]);
                        raw
                    });
                    value["discovery_channels"].as_array_mut().unwrap().push(json!({"id":channel.id.to_string(),"login":channel.login,"displayName":channel.name}));
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn values(value: &Value) -> impl DoubleEndedIterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}
fn records(value: &Value) -> BTreeMap<String, Value> {
    values(value)
        .filter_map(|v| Some((v["id"].as_str()?.to_owned(), v.clone())))
        .collect()
}
fn collect_channels(game: &Value, channels: &mut BTreeMap<u64, ChannelIdentity>) {
    for raw in values(&game["streams"]["edges"]).take(3) {
        if let Ok(channel) = ChannelIdentity::parse(&raw["node"]["broadcaster"]) {
            channels.insert(channel.id, channel);
        }
    }
}
fn fill_missing(primary: &mut Value, secondary: &Value) {
    if let (Some(primary), Some(secondary)) = (primary.as_object_mut(), secondary.as_object()) {
        for (key, value) in secondary {
            if let Some(existing) = primary.get_mut(key) {
                fill_missing(existing, value);
            } else {
                primary.insert(key.clone(), value.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::twitch::tests::{campaign_json, gql_mock, http, session};
    use std::sync::Arc;
    use wiremock::MockServer;

    fn streams() -> Value {
        json!({"streams":{"edges":[null,{"node":null},{"node":{"broadcaster":{"id":"10","login":"streamer","displayName":"Streamer"}}}]}})
    }
    fn metadata(id: &str) -> Value {
        let mut value = campaign_json(id);
        value.as_object_mut().unwrap().remove("self");
        value["timeBasedDrops"][0]
            .as_object_mut()
            .unwrap()
            .remove("self");
        value
    }

    #[tokio::test]
    async fn live_recovery_preserves_account_progress_and_discovery_never_selects_games() {
        let server = MockServer::start().await;
        gql_mock(&server,|query|match query["operationName"].as_str().unwrap() {
            "Inventory"=>json!({"data":{"currentUser":{"inventory":{"dropCampaignsInProgress":[campaign_json("owned")],"gameEventDrops":[]}}}}),
            "ViewerDropsDashboard"=>json!({"data":{"currentUser":{"dropCampaigns":null}}}),
            "DirectoryPage_Game"=>json!({"data":{"game":streams()}}),
            "DropsDiscoveryGames"=>json!({"data":{"games":{"edges":[null,{"cursor":"end","node":streams()}],"pageInfo":{"hasNextPage":false}}}}),
            "ChannelDropsRecovery"=> {
                assert!(!query["query"].as_str().unwrap().contains("self"));
                let records:Vec<_>=(0..48).map(|i|metadata(&format!("recovered{i}"))).chain([metadata("owned")]).collect();
                json!({"data":{"channel":{"id":"10","viewerDropCampaigns":records}}})
            },_=>panic!("unexpected query"),
        }).await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        let settings = Settings::default();
        let inventory = client.inventory(&settings).await.unwrap();
        assert!(!inventory.status.available);
        assert_eq!(inventory.status.recovered, 48);
        assert_eq!(inventory.campaigns.len(), 49);
        let owned = inventory
            .campaigns
            .iter()
            .find(|c| c.id == "owned")
            .unwrap();
        assert_eq!(owned.drops[0].confirmed_minutes, 12);
        assert!(owned.drops[0].confirmed_at.is_some());
        let recovered = inventory
            .campaigns
            .iter()
            .find(|c| c.id == "recovered0")
            .unwrap();
        assert_eq!(recovered.linked, None);
        assert!(recovered.drops[0].confirmed_at.is_none());
        assert_eq!(recovered.discovery_channels.as_ref().unwrap().len(), 1);
        assert!(settings.games_to_watch.is_empty());
        assert!(
            crate::domain::wanted_items(&inventory.campaigns, &settings, Utc::now()).is_empty()
        );
    }

    #[tokio::test]
    async fn null_details_recover_only_catalog_ids_but_empty_catalog_never_scans() {
        for empty in [false, true] {
            let server = MockServer::start().await;
            gql_mock(&server, move |query|match query["operationName"].as_str().unwrap() {
                "Inventory"=>json!({"data":{"currentUser":{"inventory":{"dropCampaignsInProgress":[campaign_json("owned")],"gameEventDrops":[]}}}}),
                "ViewerDropsDashboard"=>json!({"data":{"currentUser":{"dropCampaigns":if empty {vec![]} else {vec![campaign_json("listed")]}}}}),
                "DropCampaignDetails"=>json!({"data":{"user":{"dropCampaign":null}}}),
                "DirectoryPage_Game"=>json!({"data":{"game":streams()}}),
                "DropsDiscoveryGames"=>json!({"data":{"games":null}}),
                "ChannelDropsRecovery"=>json!({"data":{"channel":{"id":"10","viewerDropCampaigns":[metadata("listed"),metadata("stray")]}}}),
                _=>panic!("unexpected query"),
            }).await;
            let client = TwitchClient::new(Arc::new(http(&server)), &session());
            let inventory = client.inventory(&Settings::default()).await.unwrap();
            assert_eq!(inventory.status.available, empty);
            assert!(inventory.campaigns.iter().any(|c| c.id == "owned"));
            assert!(!inventory.campaigns.iter().any(|c| c.id == "stray"));
            if empty {
                assert_eq!(server.received_requests().await.unwrap().len(), 2);
            } else {
                assert_eq!(
                    inventory
                        .campaigns
                        .iter()
                        .find(|c| c.id == "listed")
                        .unwrap()
                        .linked,
                    Some(true)
                );
            }
        }
    }

    #[tokio::test]
    async fn account_details_fill_only_missing_fields_and_subscription_recovery_is_omitted() {
        let server = MockServer::start().await;
        gql_mock(&server,|query|match query["operationName"].as_str().unwrap() {
            "Inventory"=> {let mut campaign=campaign_json("owned");campaign.as_object_mut().unwrap().remove("accountLinkURL");
                json!({"data":{"currentUser":{"inventory":{"dropCampaignsInProgress":[campaign],"gameEventDrops":[]}}}})},
            "ViewerDropsDashboard"=>json!({"data":{"currentUser":{"dropCampaigns":[campaign_json("owned"),campaign_json("subscription")]}}}),
            "DropCampaignDetails"=>if query["variables"]["dropID"]=="owned" {
                let mut campaign=campaign_json("owned");campaign["timeBasedDrops"][0]["self"]["currentMinutesWatched"]=59.into();campaign["accountLinkURL"]="https://example.com".into();
                json!({"data":{"user":{"dropCampaign":campaign}}})
            }else {json!({"data":{"user":null}})},
            "DirectoryPage_Game"=>json!({"data":{"game":streams()}}),
            "DropsDiscoveryGames"=>json!({"data":{"games":null}}),
            "ChannelDropsRecovery"=> {let mut campaign=metadata("subscription");campaign["timeBasedDrops"][0]["requiredMinutesWatched"]=0.into();
                json!({"data":{"channel":{"id":"10","viewerDropCampaigns":[campaign,null]}}})},
            _=>panic!("unexpected query"),
        }).await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        let inventory = client.inventory(&Settings::default()).await.unwrap();
        assert_eq!(inventory.campaigns.len(), 1);
        assert_eq!(inventory.campaigns[0].drops[0].confirmed_minutes, 12);
        assert_eq!(inventory.campaigns[0].link_url, "https://example.com");
        assert!(!inventory.status.available);
    }

    #[tokio::test]
    async fn recovery_preserves_earlier_batches_but_propagates_authentication_and_cancellation() {
        for auth in [false, true] {
            let server = MockServer::start().await;
            gql_mock(&server,move|query|match query["operationName"].as_str().unwrap() {
                "DropsDiscoveryGames"=> {
                    let edges:Vec<_>=(1..=21).map(|i|json!({"cursor":i.to_string(),"node":{"streams":{"edges":[{"node":{"broadcaster":{"id":i.to_string(),"login":format!("stream{i}")}}}]}}})).collect();
                    json!({"data":{"games":{"edges":edges,"pageInfo":{"hasNextPage":false}}}})
                },
                "ChannelDropsRecovery"=>if query["variables"]["channelID"]=="21" {
                    json!({"errors":[{"message":if auth {"Unauthorized"} else {"unknown error"}}]})
                }else {json!({"data":{"channel":{"id":query["variables"]["channelID"],"viewerDropCampaigns":[metadata("found")]}}})},
                _=>panic!("unexpected query"),
            }).await;
            let client = TwitchClient::new(Arc::new(http(&server)), &session());
            let result = client.recover_campaigns(&[]).await;
            if auth {
                assert!(matches!(result, Err(TwitchError::Unauthorized)));
            } else {
                assert_eq!(
                    result.unwrap()["found"]["discovery_channels"]
                        .as_array()
                        .unwrap()
                        .len(),
                    20
                );
            }
            client.http.cancel.cancel();
            assert!(matches!(
                client.recover_campaigns(&[]).await,
                Err(TwitchError::Cancelled)
            ));
        }
    }
}
