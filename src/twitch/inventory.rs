use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::{TwitchClient, TwitchError, operations::Operation};
use crate::{domain::Campaign, dto::InventoryStatus};

pub struct Inventory {
    pub campaigns: Vec<Campaign>,
    pub status: InventoryStatus,
    pub awards: HashMap<String, DateTime<Utc>>,
}

impl TwitchClient {
    pub async fn inventory(&self) -> Result<Inventory, TwitchError> {
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
        let account_complete = inventory["dropCampaignsInProgress"]
            .as_array()
            .is_some_and(|records| records.len() == account.len());
        let response = match self.gql(Operation::Campaigns.request(json!({}))).await {
            Ok(response) => response,
            Err(error @ (TwitchError::Unauthorized | TwitchError::Cancelled)) => return Err(error),
            Err(_) => Value::Null,
        };
        let catalog = response
            .pointer("/data/currentUser/dropCampaigns")
            .and_then(Value::as_array);
        let catalog_complete = catalog.is_some_and(|entries| {
            entries.iter().all(|entry| {
                entry["id"].as_str().is_some_and(|id| !id.is_empty())
                    && entry["status"].as_str().is_some_and(|s| !s.is_empty())
            })
        });
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
            let responses = match self.batch(queries).await {
                Ok(responses) => responses,
                Err(error @ (TwitchError::Unauthorized | TwitchError::Cancelled)) => {
                    return Err(error);
                }
                Err(_) => continue,
            };
            for response in responses {
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
        let campaigns: BTreeMap<_, _> = account
            .iter()
            .filter_map(|(id, v)| {
                Campaign::parse(v, &awards, now)
                    .ok()
                    .map(|c| (id.clone(), c))
            })
            .collect();
        let available = account_complete
            && account.len() == campaigns.len()
            && catalog_complete
            && summaries
                .keys()
                .all(|id| fetched.contains(id) && campaigns.contains_key(id));
        Ok(Inventory {
            campaigns: campaigns.into_values().collect(),
            awards,
            status: InventoryStatus {
                available,
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
}

pub(crate) fn values(value: &Value) -> impl DoubleEndedIterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}
fn records(value: &Value) -> BTreeMap<String, Value> {
    values(value)
        .filter_map(|v| Some((v["id"].as_str()?.to_owned(), v.clone())))
        .collect()
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
    use crate::{
        config::Settings,
        twitch::tests::{campaign_json, gql_mock, http, session},
    };
    use std::sync::Arc;
    use wiremock::MockServer;

    #[tokio::test]
    async fn catalog_fetches_all_details_without_scanning_or_selecting_games() {
        let server = MockServer::start().await;
        gql_mock(&server, |q| match q["operationName"].as_str().unwrap() {
            "Inventory" => json!({"data":{"currentUser":{"inventory":{"dropCampaignsInProgress":[campaign_json("owned")],"gameEventDrops":[]}}}}),
            "ViewerDropsDashboard" => json!({"data":{"currentUser":{"dropCampaigns":(0..48).map(|i|json!({"id":format!("listed{i}"),"status":"ACTIVE"})).collect::<Vec<_>>()}}}),
            "DropCampaignDetails" => json!({"data":{"user":{"dropCampaign":campaign_json(q["variables"]["dropID"].as_str().unwrap())}}}),
            other => panic!("unexpected discovery operation {other}"),
        }).await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        let inventory = client.inventory().await.unwrap();
        assert!(inventory.status.available);
        assert_eq!(inventory.campaigns.len(), 49);
        assert_eq!(server.received_requests().await.unwrap().len(), 5);
        assert!(
            crate::domain::wanted_items(&inventory.campaigns, &Settings::default(), Utc::now())
                .is_empty()
        );
    }

    #[tokio::test]
    async fn empty_null_and_incomplete_catalogs_are_distinct_and_keep_account_progress() {
        for mode in [
            "empty",
            "null",
            "details_null",
            "catalog_error",
            "details_error",
            "invalid_account_record",
            "missing_account_id",
            "null_inventory",
            "null_catalog_entry",
            "malformed_catalog_entry",
        ] {
            let server = MockServer::start().await;
            gql_mock(&server, move |q| match q["operationName"].as_str().unwrap() {
                "Inventory" => {
                    let mut invalid = campaign_json("invalid");
                    invalid["game"] = Value::Null;
                    let account = match mode {
                        "invalid_account_record" => json!([campaign_json("owned"), invalid]),
                        "missing_account_id" => json!([campaign_json("owned"), {"id":null}]),
                        "null_inventory" => Value::Null,
                        _ => json!([campaign_json("owned")]),
                    };
                    json!({"data":{"currentUser":{"inventory":{"dropCampaignsInProgress":account,"gameEventDrops":[]}}}})
                },
                "ViewerDropsDashboard" if mode == "catalog_error" => json!({"errors":[{"message":"unknown error"}]}),
                "ViewerDropsDashboard" => json!({"data":{"currentUser":{"dropCampaigns":match mode {
                    "empty" | "invalid_account_record" | "missing_account_id" => json!([]),
                    "null_inventory" => json!([campaign_json("owned")]),
                    "null_catalog_entry" => json!([null]),
                    "malformed_catalog_entry" => json!([{"status":"ACTIVE","id":null}]),
                    "null" => Value::Null,
                    _ => json!([campaign_json("owned"),campaign_json("new")])
                }}}}),
                "DropCampaignDetails" if mode == "null_inventory" => json!({"data":{"user":{"dropCampaign":campaign_json("owned")}}}),
                "DropCampaignDetails" if mode == "details_error" => json!({"errors":[{"message":"unknown error"}]}),
                "DropCampaignDetails" => json!({"data":{"user":{"dropCampaign":null}}}),
                other => panic!("unexpected discovery operation {other}"),
            }).await;
            let client = TwitchClient::new(Arc::new(http(&server)), &session());
            let inventory = client.inventory().await.unwrap();
            assert_eq!(inventory.status.available, mode == "empty");
            assert_eq!(inventory.campaigns.len(), 1);
            assert_eq!(inventory.campaigns[0].drops[0].confirmed_minutes, 12);
            assert!(inventory.campaigns[0].drops[0].confirmed_at.is_some());
            assert!(server.received_requests().await.unwrap().len() <= 3);
        }
    }

    #[tokio::test]
    async fn nullable_details_do_not_discard_neighbors_or_overwrite_account_evidence() {
        let server = MockServer::start().await;
        gql_mock(&server, |q| match q["operationName"].as_str().unwrap() {
            "Inventory" => {
                let mut owned = campaign_json("owned");
                owned.as_object_mut().unwrap().remove("accountLinkURL");
                json!({"data":{"currentUser":{"inventory":{"dropCampaignsInProgress":[owned],"gameEventDrops":[]}}}})
            },
            "ViewerDropsDashboard" => json!({"data":{"currentUser":{"dropCampaigns":[campaign_json("owned"),campaign_json("missing"),campaign_json("new")]}}}),
            "DropCampaignDetails" if q["variables"]["dropID"] == "missing" => json!({"data":{"user":null},"errors":[{"message":"server error","path":["user","dropCampaign"]}]}),
            "DropCampaignDetails" => {
                let mut detail = campaign_json(q["variables"]["dropID"].as_str().unwrap());
                detail["timeBasedDrops"][0]["self"]["currentMinutesWatched"] = 59.into();
                detail["accountLinkURL"] = "https://example.com/link".into();
                json!({"data":{"user":{"dropCampaign":detail}}})
            },
            other => panic!("unexpected discovery operation {other}"),
        }).await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        let inventory = client.inventory().await.unwrap();
        assert!(!inventory.status.available);
        assert_eq!(inventory.campaigns.len(), 2);
        let owned = inventory
            .campaigns
            .iter()
            .find(|c| c.id == "owned")
            .unwrap();
        assert_eq!(owned.drops[0].confirmed_minutes, 12);
        assert_eq!(owned.link_url, "https://example.com/link");
        assert!(inventory.campaigns.iter().any(|c| c.id == "new"));
    }

    #[tokio::test]
    async fn catalog_authentication_and_cancellation_failures_propagate() {
        for details in [false, true] {
            let server = MockServer::start().await;
            gql_mock(&server, move |q| match q["operationName"].as_str().unwrap() {
                "Inventory" => json!({"data":{"currentUser":{"inventory":{"dropCampaignsInProgress":[],"gameEventDrops":[]}}}}),
                "ViewerDropsDashboard" if details => json!({"data":{"currentUser":{"dropCampaigns":[campaign_json("one")]}}}),
                "ViewerDropsDashboard" | "DropCampaignDetails" => json!({"errors":[{"message":"Unauthorized"}]}),
                other => panic!("unexpected discovery operation {other}"),
            }).await;
            let client = TwitchClient::new(Arc::new(http(&server)), &session());
            assert!(matches!(
                client.inventory().await,
                Err(TwitchError::Unauthorized)
            ));
            client.http.cancel.cancel();
            assert!(matches!(
                client.inventory().await,
                Err(TwitchError::Cancelled)
            ));
        }
    }
}
