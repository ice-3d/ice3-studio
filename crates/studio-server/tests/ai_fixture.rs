use std::path::Path;

use studio_server::{ServerConfig, TestServer, start_test_server};

async fn rpc(
    server: &TestServer,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
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

#[tokio::test]
async fn deterministic_ai_client_builds_and_reloads_wastewater_fixture() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/wastewater-mini/ai-plan.json");
    let plan: serde_json::Value = serde_json::from_slice(&std::fs::read(fixture).unwrap()).unwrap();
    let transactions = plan["transactions"].as_array().unwrap();

    let first = rpc(&server, 1, "document.apply", transactions[0].clone()).await;
    assert_eq!(first["result"]["revision"], 1);

    let rejected = rpc(&server, 2, "document.apply", transactions[1].clone()).await;
    assert_eq!(rejected["error"]["code"], -32010);
    assert!(rejected["error"]["data"].is_array());

    let committed = rpc(&server, 3, "document.apply", transactions[2].clone()).await;
    assert_eq!(committed["result"]["revision"], 2);

    let document = rpc(&server, 4, "document.get", serde_json::json!({})).await;
    let nodes = document["result"]["scene"]["nodes"].as_array().unwrap();
    let count_role = |role: &str| {
        nodes
            .iter()
            .filter(|node| node["metadata"]["role"] == role)
            .count()
    };
    assert_eq!(count_role("aerationTank"), 2);
    assert_eq!(count_role("secondaryClarifier"), 2);
    assert_eq!(document["result"]["relations"].as_array().unwrap().len(), 4);
    assert_eq!(document["result"]["materials"].as_array().unwrap().len(), 6);

    let source = rpc(&server, 5, "export.sourceGlb", serde_json::json!({})).await;
    let source_path = source["result"]["sourcePath"].as_str().unwrap();
    assert!(Path::new(source_path).is_file());
    assert_eq!(source["result"]["revision"], 2);

    let undo = rpc(&server, 6, "document.undo", serde_json::json!({})).await;
    assert_eq!(undo["result"]["revision"], 3);
    assert_eq!(
        undo["result"]["scene"]["nodes"].as_array().unwrap().len(),
        1
    );
    let redo = rpc(&server, 7, "document.redo", serde_json::json!({})).await;
    assert_eq!(redo["result"]["revision"], 4);
    assert_eq!(
        redo["result"]["scene"]["nodes"].as_array().unwrap().len(),
        nodes.len()
    );
}
