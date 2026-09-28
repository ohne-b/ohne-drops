//! Server-only diagnostics. Never format requests, URLs, headers or upstream values.
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::TwitchError;

pub(super) fn kind(value: Option<&Value>) -> &'static str {
    match value {
        None => "missing",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

pub(super) fn invalid(
    operation: &'static str,
    reason: &'static str,
    value: Option<&Value>,
) -> TwitchError {
    tracing::warn!(
        operation,
        reason,
        actual_type = kind(value),
        "Invalid upstream response"
    );
    TwitchError::InvalidResponse
}

pub(super) fn json(body: &[u8], status: u16) -> Result<Value, TwitchError> {
    serde_json::from_slice(body).map_err(|error| {
        // Transitive serde_json features can also produce Data errors (e.g. its
        // reserved RawValue key). Their Display may echo an upstream value.
        let reason = if error.is_syntax() || error.is_eof() {
            error.to_string()
        } else {
            "JSON value decoding failed (details withheld)".to_owned()
        };
        tracing::warn!(
            status, bytes = body.len(), category = ?error.classify(),
            line = error.line(), column = error.column(), %reason,
            "Upstream response is not valid JSON"
        );
        TwitchError::InvalidResponse
    })
}

pub(super) fn operation(request: &Value) -> &'static str {
    // Request variables and arbitrary operation names must never enter logs.
    match request["operationName"].as_str().unwrap_or_default() {
        "Inventory" => "Inventory",
        "DirectoryPage_Game" => "DirectoryPage_Game",
        "VideoPlayerStreamInfoOverlayChannel" => "VideoPlayerStreamInfoOverlayChannel",
        "DropCurrentSessionContext" => "DropCurrentSessionContext",
        "DropsPage_ClaimDropRewards" => "DropsPage_ClaimDropRewards",
        "DropsHighlightService_AvailableDrops" => "DropsHighlightService_AvailableDrops",
        "OnsiteNotifications_DeleteNotification" => "OnsiteNotifications_DeleteNotification",
        _ if request.is_array() => "batch",
        _ => "unknown",
    }
}

pub(super) fn graphql(response: &Value, operation: &'static str, attempt: u32, index: usize) {
    let errors = response["errors"].as_array();
    if response.get("error").is_some() || errors.is_some_and(|v| !v.is_empty()) {
        let messages: Vec<_> = errors
            .into_iter()
            .flatten()
            .take(4)
            .map(
                |error| match error["message"].as_str().unwrap_or_default() {
                    message @ ("Unauthorized"
                    | "unauthorized"
                    | "authentication required"
                    | "invalid oauth token"
                    | "service error"
                    | "PersistedQueryNotFound"
                    | "service timeout"
                    | "service unavailable"
                    | "context deadline exceeded"
                    | "server error") => message,
                    _ => "unrecognized (withheld)",
                },
            )
            .collect();
        // Correlate repeated unknown errors without retaining arbitrary text that
        // could echo credentials. Cap output, even for a hostile errors array.
        let mut fingerprint = Sha256::new();
        fingerprint.update(response["error"].to_string());
        fingerprint.update(response["errors"].to_string());
        let fingerprint = hex::encode(fingerprint.finalize());
        tracing::warn!(operation, attempt = attempt + 1, batch_index = index,
            error_count = errors.map_or(0, Vec::len), ?messages, %fingerprint,
            top_level_error = response.get("error").is_some(), "GraphQL response contains errors");
    }
}

pub(super) fn network(error: &reqwest::Error, stage: &'static str, attempt: usize) {
    // Error Display/source Display can contain proxy credentials and request URLs.
    // Preserve typed transport causes instead of attempting blacklist redaction.
    let mut source = std::error::Error::source(error);
    let mut io_kind = None;
    let mut os_code = None;
    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            io_kind = Some(io.kind());
            os_code = io.raw_os_error();
        }
        source = cause.source();
    }
    tracing::warn!(
        stage,
        attempt,
        timeout = error.is_timeout(),
        connect = error.is_connect(),
        body = error.is_body(),
        decode = error.is_decode(),
        request = error.is_request(),
        ?io_kind,
        ?os_code,
        "Upstream transport failed"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::twitch::{
        TwitchClient,
        operations::Operation,
        tests::{http, session},
    };
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tracing::instrument::WithSubscriber;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[derive(Clone, Default)]
    struct Writer(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Writer {
        fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync + 'static {
            let writer = self.clone();
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(move || writer.clone())
                .finish()
        }
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    #[tokio::test]
    async fn malformed_progress_logs_field_and_type_without_exposing_values_or_changing_public_error()
     {
        for (drop, expected) in [
            (
                json!({"dropID":"private-id", "currentMinutesWatched":null}),
                "currentMinutesWatched must fit u32",
            ),
            (
                json!({"dropID":"", "currentMinutesWatched":2}),
                "dropID must be a nonempty string",
            ),
            (
                json!({"dropID":"private-id", "currentMinutesWatched":"private-secret"}),
                "currentMinutesWatched must fit u32",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/gql"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"data":{"currentUser":{"dropCurrentSession":drop}}})),
                )
                .mount(&server)
                .await;
            let client = TwitchClient::new(Arc::new(http(&server)), &session());
            let output = Writer::default();
            let error = client
                .current_drop(123456)
                .with_subscriber(output.subscriber())
                .await
                .unwrap_err();
            assert_eq!(error.to_string(), "Twitch returned an invalid response");
            let text = output.text();
            assert!(
                text.contains("CurrentDrop") && text.contains(expected),
                "{text}"
            );
            assert!(
                !text.contains("private-")
                    && !text.contains("testtoken")
                    && !text.contains("123456")
            );
        }
    }

    #[tokio::test]
    async fn json_and_graphql_failures_include_operation_and_safe_details() {
        for (body, expected) in [
            (
                "<html>private-secret</html>",
                "expected value at line 1 column 1",
            ),
            (
                r#"{"errors":[{"message":"PersistedQueryNotFound"}]}"#,
                "PersistedQueryNotFound",
            ),
            (
                r#"{"errors":[{"message":"private-secret https://user:password@proxy/?token=testtoken"}]}"#,
                "unrecognized (withheld)",
            ),
            ("null", "expected response object"),
            (
                r#"{"$serde_json::private::RawValue":987654321}"#,
                "JSON value decoding failed (details withheld)",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/gql"))
                .respond_with(ResponseTemplate::new(200).set_body_string(body))
                .mount(&server)
                .await;
            let client = TwitchClient::new(Arc::new(http(&server)), &session());
            let output = Writer::default();
            assert!(
                client
                    .gql(Operation::CurrentDrop.request(json!({"channelID":"private-variable"})))
                    .with_subscriber(output.subscriber())
                    .await
                    .is_err()
            );
            let text = output.text();
            assert!(
                text.contains("DropCurrentSessionContext") && text.contains(expected),
                "{text}"
            );
            for secret in [
                "private-",
                "password",
                "testtoken",
                "<html>",
                "987654321",
                "RawValue",
            ] {
                assert!(!text.contains(secret), "{text}");
            }
        }
    }

    #[test]
    fn diagnostics_are_bounded_and_do_not_echo_arbitrary_keys_paths_or_error_text() {
        let output = Writer::default();
        let hostile = json!({"errors": (0..1000).map(|_| json!({
            "message":"private-secret", "path":["private-path"], "extensions":{"private-key":"private-value"}
        })).collect::<Vec<_>>()});
        tracing::subscriber::with_default(output.subscriber(), || {
            graphql(&hostile, "Inventory", 0, 0);
            invalid("CurrentDrop", "expected string", Some(&hostile));
        });
        let text = output.text();
        assert!(text.contains("error_count=1000") && text.contains("fingerprint="));
        assert!(!text.contains("private-") && text.len() < 1500);
        assert_eq!(
            operation(&json!({"operationName":"private-operation"})),
            "unknown"
        );
    }

    #[tokio::test]
    async fn partial_catalog_logs_rejection_reason_without_exposing_account_records() {
        for stale in [true, false] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/gql"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    json!({"data":{"currentUser":{"inventory":{
                        "dropCampaignsInProgress":[{"id":"private-id"}], "gameEventDrops":[]
                    }}}}),
                ))
                .mount(&server)
                .await;
            Mock::given(method("GET")).and(path("/catalog"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "lastUpdatedAt": (chrono::Utc::now() - chrono::Duration::minutes(if stale {31} else {0})).to_rfc3339(),
                    "data":[{"rewards":[null]}]
                }))).mount(&server).await;
            let client = TwitchClient::new(Arc::new(http(&server)), &session());
            let output = Writer::default();
            let inventory = client
                .inventory()
                .with_subscriber(output.subscriber())
                .await
                .unwrap();
            assert!(!inventory.status.available);
            let text = output.text();
            assert!(
                text.contains("Account inventory is partial")
                    && text.contains("malformed_records=1"),
                "{text}"
            );
            assert!(
                text.contains(if stale {
                    "outside the freshness window"
                } else {
                    "invalid_records=1"
                }),
                "{text}"
            );
            assert!(!text.contains("private-id") && !text.contains("testtoken"));
        }
    }

    #[tokio::test]
    async fn batch_diagnostics_are_capped_and_preserve_partial_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/gql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec![
                json!({
                    "data":{"user":null}, "errors":[{"message":"server error", "path":["user"]}]
                });
                20
            ]))
            .mount(&server)
            .await;
        let client = TwitchClient::new(Arc::new(http(&server)), &session());
        let output = Writer::default();
        let result = client
            .batch(vec![Operation::StreamInfo.request(json!({})); 20])
            .with_subscriber(output.subscriber())
            .await
            .unwrap();
        assert_eq!(result.len(), 20);
        let text = output.text();
        assert_eq!(
            text.matches("GraphQL response contains errors").count(),
            4,
            "{text}"
        );
        assert!(
            text.contains("omitted=16") && text.contains("VideoPlayerStreamInfoOverlayChannel"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn http_and_transport_failures_log_status_or_typed_cause_without_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("Set-Cookie", "private-cookie")
                    .set_body_string("private-body"),
            )
            .mount(&server)
            .await;
        let http = http(&server);
        let output = Writer::default();
        let url = format!("{}/private-path?token=private-secret", server.uri());
        http.execute(http.client.get(&url), false)
            .with_subscriber(output.subscriber())
            .await
            .unwrap();
        // A malformed request fails without contacting any network service.
        let error = http
            .client
            .get("http://[private-secret")
            .send()
            .await
            .unwrap_err();
        tracing::subscriber::with_default(output.subscriber(), || network(&error, "send", 1));
        let text = output.text();
        assert!(
            text.contains("status=403") && text.contains("Upstream transport failed"),
            "{text}"
        );
        assert!(!text.contains("private-") && !text.contains(&server.uri()));
    }
}
