use std::{
    collections::{BTreeMap, HashMap, HashSet},
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{TwitchError, TwitchHttp};
use crate::domain::Campaign;

const MAX_CAMPAIGNS: usize = 2000;

pub(super) struct Catalog {
    pub campaigns: BTreeMap<String, Campaign>,
    pub updated_at: DateTime<Utc>,
    pub complete: bool,
}

impl TwitchHttp {
    pub(super) async fn catalog(&self) -> Result<Value, TwitchError> {
        // One inventory job owns this request. Bound retries and body reads together.
        tokio::time::timeout(Duration::from_secs(30), async {
            let response = self
                .execute_with(
                    &self.catalog_client,
                    self.catalog_client
                        .get(self.endpoints.catalog.clone())
                        .header("Accept", "application/json"),
                    true,
                )
                .await?;
            // A public-feed 401/403 is not a Twitch logout.
            if !response.status().is_success() {
                return Err(TwitchError::InvalidResponse);
            }
            serde_json::from_slice(response.body()).map_err(|_| TwitchError::InvalidResponse)
        })
        .await
        .map_err(|_| TwitchError::Network)?
    }
}

impl Catalog {
    pub fn parse(
        payload: Value,
        awards: &HashMap<String, DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Self, TwitchError> {
        let updated_at: DateTime<Utc> = payload["lastUpdatedAt"]
            .as_str()
            .and_then(|v| v.parse().ok())
            .ok_or(TwitchError::InvalidResponse)?;
        if updated_at < now - chrono::Duration::minutes(30)
            || updated_at > now + chrono::Duration::minutes(5)
        {
            return Err(TwitchError::InvalidResponse);
        }
        let groups = payload["data"]
            .as_array()
            .ok_or(TwitchError::InvalidResponse)?;
        if groups.len() > MAX_CAMPAIGNS {
            return Err(TwitchError::InvalidResponse);
        }
        let mut catalog = Self {
            campaigns: BTreeMap::new(),
            updated_at,
            complete: true,
        };
        let mut count = 0;
        let mut seen = HashSet::new();
        for group in groups {
            let Some(records) = group["rewards"].as_array() else {
                catalog.complete = false;
                continue;
            };
            count += records.len();
            if count > MAX_CAMPAIGNS {
                return Err(TwitchError::InvalidResponse);
            }
            for record in records {
                let Some(campaign) = public_campaign(record.clone(), group, awards, now) else {
                    catalog.complete = false;
                    continue;
                };
                if !seen.insert(campaign.id.clone()) {
                    catalog.complete = false;
                    catalog.campaigns.remove(&campaign.id);
                    continue;
                }
                if campaign.active(now) || campaign.upcoming(now) {
                    catalog.campaigns.insert(campaign.id.clone(), campaign);
                }
            }
        }
        Ok(catalog)
    }
}

fn public_campaign(
    mut record: Value,
    group: &Value,
    awards: &HashMap<String, DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<Campaign> {
    record.as_object_mut()?.remove("self");
    record["game"].as_object()?;
    if !matches!(
        record["status"].as_str()?,
        "ACTIVE" | "UPCOMING" | "EXPIRED"
    ) {
        return None;
    }
    let restricted = record["allow"]["isEnabled"].as_bool()?;
    if restricted
        && record["allow"]["channels"]
            .as_array()
            .is_none_or(|channels| channels.is_empty() || channels.iter().any(Value::is_null))
    {
        return None;
    }
    for drop in record["timeBasedDrops"].as_array_mut()? {
        drop.as_object_mut()?.remove("self");
        // Missing dependency or benefit data cannot be interpreted as unrestricted.
        if !(drop.get("preconditionDrops")?.is_null() || drop["preconditionDrops"].is_array())
            || drop["benefitEdges"].as_array()?.is_empty()
        {
            return None;
        }
    }
    if record["game"]["boxArtURL"].as_str().is_none() && record["game"]["id"] == group["gameId"] {
        record["game"]["boxArtURL"] = group["gameBoxArtURL"].clone();
    }
    let campaign = Campaign::parse(&record, awards, now).ok()?;
    if campaign.game.id == 0
        || campaign.allowed_channels.iter().any(|channel| {
            channel.id == 0
                || super::channels::channel_login(&channel.login)
                    .is_none_or(|login| !login.eq_ignore_ascii_case(&channel.login))
        })
        || campaign.starts_at >= campaign.ends_at
        || campaign
            .drops
            .iter()
            .any(|drop| drop.starts_at >= drop.ends_at)
        || restricted && campaign.allowed_channels.is_empty()
    {
        return None;
    }
    Some(campaign)
}
