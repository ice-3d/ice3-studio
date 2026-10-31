mod common;

use studio_server::{ServerConfig, TestServer, start_test_server};

use common::{rpc_with_approval_cookie, rpc_with_bearer};

#[tokio::test]
async fn manifest_access_matches_request_authority() {
    let server = start_test_server(ServerConfig::review_required("agent-manifest-access")).await;
    let read_only =
        unauthenticated_rpc(&server, "system.agentManifest", serde_json::json!({})).await;
    assert_eq!(
        read_only["result"]["access"]["canRead"], true,
        "{read_only}"
    );
    assert_eq!(read_only["result"]["access"]["canPropose"], false);
    assert_eq!(read_only["result"]["access"]["canCommit"], false);

    let agent = rpc_with_bearer(
        &server,
        server.agent_token(),
        "system.agentManifest",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(agent["result"]["access"]["canRead"], true, "{agent}");
    assert_eq!(agent["result"]["access"]["canPropose"], true);
    assert_eq!(agent["result"]["access"]["canCommit"], false);
    assert_eq!(agent["result"]["access"]["canApplyDirectly"], false);

    let approval =
        rpc_with_approval_cookie(&server, "system.agentManifest", serde_json::json!({})).await;
    assert_eq!(approval["result"]["access"]["canRead"], true, "{approval}");
    assert_eq!(approval["result"]["access"]["canPropose"], true);
    assert_eq!(approval["result"]["access"]["canCommit"], true);
    assert_eq!(approval["result"]["access"]["canApplyDirectly"], true);

    let both =
        rpc_with_bearer_and_approval(&server, "system.agentManifest", serde_json::json!({})).await;
    assert_eq!(
        both["result"]["access"]["canCommit"], true,
        "approval capability must not be hidden by a simultaneous agent token: {both}"
    );

    let expected_tools = [
        "system.agentManifest",
        "scene.context",
        "scene.search",
        "evaluation.query",
        "changeset.create",
        "changeset.addOperations",
        "changeset.evaluate",
        "changeset.getCandidate",
        "changeset.commit",
        "document.apply",
        "instance.patch",
        "component.patch",
        "connection.patch",
        "material.patch",
    ];
    let tools = agent["result"]["tools"].as_array().unwrap();
    for expected in expected_tools {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == expected)
            .unwrap_or_else(|| panic!("missing tool {expected}: {agent}"));
        assert!(
            tool["inputSchemaRef"]
                .as_str()
                .is_some_and(|value| value.contains("#/types/")),
            "tool {expected} must reference a generated schema definition: {tool}"
        );
    }
}

#[tokio::test]
async fn autonomous_manifest_exposes_direct_write_access() {
    let server = start_test_server(ServerConfig::for_tests("agent-manifest-autonomous")).await;
    let manifest = rpc_with_bearer(
        &server,
        server.token(),
        "system.agentManifest",
        serde_json::json!({}),
    )
    .await;

    assert_eq!(
        manifest["result"]["access"]["canPropose"], true,
        "{manifest}"
    );
    assert_eq!(manifest["result"]["access"]["canCommit"], true);
    assert_eq!(manifest["result"]["access"]["canApplyDirectly"], true);
    assert_eq!(manifest["result"]["access"]["canDiscard"], true);
}

async fn unauthenticated_rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

async fn rpc_with_bearer_and_approval(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.agent_token())
        .header(
            reqwest::header::COOKIE,
            format!("ice3_studio_approval={}", server.approval_token()),
        )
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}
