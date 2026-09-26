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
use crate::config::Settings;

pub(super) fn http(server: &MockServer) -> TwitchHttp {
    TwitchHttp::build(
        &Settings::default(),
        Some("testdevice"),
        CancellationToken::new(),
        Endpoints::mock(&server.uri()),
    )
    .unwrap()
}
pub(super) fn session() -> Session {
    serde_json::from_value(json!({"version":1,"client_id":CLIENT_ID,"user_id":42,"device_id":"testdevice","access_token":"testtoken","refresh_token":"testrefresh"})).unwrap()
}
pub(super) fn validation() -> serde_json::Value {
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

pub(super) async fn gql_mock(
    server: &MockServer,
    handler: impl Fn(&serde_json::Value) -> serde_json::Value + Send + Sync + 'static,
) {
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
        json!({"data":null,"errors":[{"message":"server error","path":["missing"]}]});
    assert_eq!(gql_errors(&mut invalid_path, 0), Err(TwitchError::GraphQl));
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
