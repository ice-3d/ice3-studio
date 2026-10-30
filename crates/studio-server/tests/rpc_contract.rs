use futures_util::StreamExt;
use studio_server::{ServerConfig, start_test_server};

#[tokio::test]
async fn health_reports_api_and_schema_versions() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let body: serde_json::Value = reqwest::get(server.health_url())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["apiVersion"], "1.0");
    assert_eq!(body["schemaVersion"], 1);
}

#[tokio::test]
async fn write_requires_the_local_session_token() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "document.apply",
            "params": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn unknown_methods_return_a_stable_json_rpc_error() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "unknown.method",
            "params": {}
        }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601);
}

#[tokio::test]
async fn publishes_a_revision_event_after_a_successful_commit() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let (mut websocket, _) = tokio_tungstenite::connect_async(server.websocket_url())
        .await
        .unwrap();
    server.apply_fixture_batch().await.unwrap();
    let message = tokio::time::timeout(std::time::Duration::from_secs(1), websocket.next())
        .await
        .unwrap()
        .unwrap();
    let message = message.unwrap();
    let body: serde_json::Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
    assert_eq!(body["revision"], 1);
    assert_eq!(body["event"], "revision.committed");
}

#[tokio::test]
async fn publishes_generated_api_schemas() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let schema = reqwest::get(server.schema_url("studio-api.schema.json"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(schema.contains("\"apiVersion\": \"1.0\""));
    assert!(schema.contains("node.create"));
}
