use std::{sync::Arc, time::Duration};

use http::Method;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

use super::{
    CLIENT_ID, CLIENT_ORIGIN, Endpoints, TwitchClient, TwitchError, TwitchHttp, gql_errors,
    oauth::Session,
};

mod protocol_regressions {
    use super::super::operations::Operation;
    use super::*;

    #[tokio::test]
    async fn persisted_operation_variables_match_legacy_contract() {
        let server = MockServer::start().await;
        gql_mock(&server, |q| match q["operationName"].as_str().unwrap() {
            "Inventory" => json!({"data":{"currentUser":{"inventory":{"dropCampaignsInProgress":[],"gameEventDrops":[]}}}}),
            "DropCurrentSessionContext" => json!({"data":{"currentUser":{"dropCurrentSession":null}}}),
            _ => unreachable!(),
        }).await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        client.inventory().await.unwrap();
        client.current_drop(10).await.unwrap();
        let bodies: Vec<serde_json::Value> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method == "POST")
            .map(|r| r.body_json().unwrap())
            .collect();
        assert_eq!(
            bodies[0]["variables"]["fetchRewardCampaigns"], false,
            "Inventory request: {}",
            bodies[0]
        );
        assert_eq!(bodies[1]["variables"]["channelLogin"], "");
        assert_eq!(bodies.len(), 2);
    }

    #[tokio::test]
    async fn null_ancestor_keeps_independent_batch_neighbor() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
              {"data":{"user":null},"errors":[{"message":"server error","path":["user","stream"]}]},
              {"data":{"user":{"stream":{"id":"valid-neighbor"}}}}
            ])))
            .mount(&server)
            .await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        let response = client
            .batch(vec![
                Operation::StreamInfo.request(json!({})),
                Operation::StreamInfo.request(json!({})),
            ])
            .await;
        assert!(
            response.is_ok(),
            "valid neighboring detail was discarded: {response:?}"
        );
        assert_eq!(
            response.unwrap()[1]["data"]["user"]["stream"]["id"],
            "valid-neighbor"
        );
    }

    #[tokio::test]
    async fn gql_does_not_exceed_five_inflight_requests() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gql"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(10))
                    .set_body_json(json!({"data":{}})),
            )
            .mount(&server)
            .await;
        let http = Arc::new(http(&server));
        let client = TwitchClient::new(http.clone(), &session());
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..6 {
            let client = client.clone();
            tasks.spawn(async move { client.gql(json!({"query":"slow"})).await });
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while server.received_requests().await.unwrap().len() < 5 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let inflight = server.received_requests().await.unwrap().len();
        http.cancel.cancel();
        while tasks.join_next().await.is_some() {}
        assert!(
            inflight <= 5,
            "{inflight} simultaneous requests reached the server before any completed"
        );
    }
}

use crate::config::Settings;

pub(crate) fn http(server: &MockServer) -> TwitchHttp {
    TwitchHttp::build(
        &Settings::default(),
        Some("testdevice"),
        CancellationToken::new(),
        Endpoints::mock(&server.uri()),
    )
    .unwrap()
}
pub(crate) fn session() -> Session {
    serde_json::from_value(json!({"version":1,"client_id":CLIENT_ID,"user_id":42,"device_id":"testdevice","access_token":"testtoken","refresh_token":"testrefresh"})).unwrap()
}
pub(crate) fn validation() -> serde_json::Value {
    json!({"client_id":CLIENT_ID,"user_id":"42","login":"miner","scopes":[],"expires_in":3600})
}

pub(crate) fn campaign_json(id: &str) -> serde_json::Value {
    let start = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let end = (chrono::Utc::now() + chrono::Duration::hours(4)).to_rfc3339();
    json!({"id":id,"name":format!("Campaign {id}"),"game":{"id":"1","name":"Rust","slug":"rust"},
        "status":"ACTIVE","startAt":start,"endAt":end,"allow":{"isEnabled":false,"channels":[]},"self":{"isAccountConnected":true},
        "timeBasedDrops":[{"id":format!("drop-{id}"),"name":"Reward","startAt":start,"endAt":end,"requiredMinutesWatched":60,
            "preconditionDrops":[],"benefitEdges":[{"benefit":{"id":format!("benefit-{id}"),"name":"Hat","distributionType":"DIRECT_ENTITLEMENT","imageAssetURL":"https://static-cdn.jtvnw.net/hat.png"}}],
            "self":{"isClaimed":false,"currentMinutesWatched":12,"dropInstanceID":null}}]})
}

pub(crate) async fn gql_mock(
    server: &MockServer,
    handler: impl Fn(&serde_json::Value) -> serde_json::Value + Send + Sync + 'static,
) {
    Mock::given(method("GET"))
        .and(path("/catalog"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "lastUpdatedAt": chrono::Utc::now().to_rfc3339(), "data": []
        })))
        .with_priority(255)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/gql"))
        .respond_with(move |request: &wiremock::Request| {
            let request: serde_json::Value = request.body_json().unwrap();
            let response = if let Some(batch) = request.as_array() {
                serde_json::Value::Array(batch.iter().map(&handler).collect())
            } else {
                handler(&request)
            };
            ResponseTemplate::new(200).set_body_json(response)
        })
        .mount(server)
        .await;
}

#[tokio::test]
async fn gql_uses_smartbox_identity_rate_limit_and_preserves_nullable_neighbors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gql"))
        .and(header("Client-Id", CLIENT_ID))
        .and(header("Authorization", "OAuth testtoken"))
        .and(header("Origin", CLIENT_ORIGIN))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"channels":[{"id":"1"},{"id":"2"}]},
            "errors":[{"message":"server error","path":["channels",0]}]})),
        )
        .mount(&server)
        .await;
    let client = TwitchClient::new(Arc::new(http(&server)), &session());
    let response = client.gql(json!({"query":"test"})).await.unwrap();
    assert!(response["data"]["channels"][0].is_null());
    assert_eq!(response["data"]["channels"][1]["id"], "2");
    assert_eq!(client.user_id, 42);
}

#[test]
fn gql_retries_only_recognized_failures_and_never_logs_upstream_secrets() {
    let mut temporary = json!({"errors":[{"message":"PersistedQueryNotFound"}]});
    assert!(gql_errors(&mut temporary, 0).unwrap());
    assert_eq!(gql_errors(&mut temporary, 1), Err(TwitchError::GraphQl));
    let mut unauthorized = json!({"errors":[{"message":"Unauthorized"}]});
    assert_eq!(
        gql_errors(&mut unauthorized, 0),
        Err(TwitchError::Unauthorized)
    );
    let mut hostile =
        json!({"errors":[{"message":"secret testtoken https://user:password@proxy/"}]});
    assert_eq!(
        gql_errors(&mut hostile, 0).unwrap_err().to_string(),
        "Twitch GraphQL request failed"
    );
    let mut invalid_path =
        json!({"data":{},"errors":[{"message":"server error","path":["missing"]}]});
    assert_eq!(gql_errors(&mut invalid_path, 0), Err(TwitchError::GraphQl));
}

#[tokio::test]
async fn public_discovery_rejections_are_http_errors_but_gql_auth_rejections_still_propagate() {
    for status in [401, 403] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tv"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/gql"))
            .and(header("Authorization", "OAuth testtoken"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server)
            .await;
        let mut http = http(&server);
        assert_eq!(
            http.discover_device().await,
            Err(TwitchError::Status(status))
        );
        let client = TwitchClient::new(Arc::new(http), &session());
        assert_eq!(client.gql(json!({})).await, Err(TwitchError::Unauthorized));
    }
}

#[tokio::test(start_paused = true)]
async fn gql_limiter_reserves_five_slots_per_second_and_cancels_waits() {
    let http = Arc::new(
        TwitchHttp::new(
            &Settings::default(),
            Some("device"),
            CancellationToken::new(),
        )
        .unwrap(),
    );
    for _ in 0..5 {
        http.acquire().await.unwrap();
    }
    let next = http.clone();
    let task = tokio::spawn(async move { next.acquire().await });
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    tokio::time::advance(Duration::from_secs(1)).await;
    task.await.unwrap().unwrap();
    for _ in 0..4 {
        http.acquire().await.unwrap();
    }
    let next = http.clone();
    let task = tokio::spawn(async move { next.acquire().await });
    tokio::task::yield_now().await;
    http.cancel.cancel();
    assert_eq!(task.await.unwrap(), Err(TwitchError::Cancelled));
}

#[tokio::test]
async fn cancellation_interrupts_inflight_http_without_retrying() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let http = Arc::new(http(&server));
    let next = http.clone();
    let task = tokio::spawn(async move {
        next.execute(next.request(Method::GET, next.endpoints.tv.clone()), true)
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while server.received_requests().await.unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    http.cancel.cancel();
    assert!(matches!(task.await.unwrap(), Err(TwitchError::Cancelled)));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn transport_refuses_redirects_with_credentials_and_normalizes_json_media_type() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/oauth2/validate"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/leak", server.uri())),
        )
        .mount(&server)
        .await;
    let http = http(&server);
    use twitch_oauth2::client::Client;
    let token = twitch_oauth2::AccessToken::new("testtoken".into());
    assert_eq!(
        http.req(token.validate_token_request())
            .await
            .unwrap()
            .status(),
        302
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}
