use std::fs;

use studio_server::{ServerConfig, TestServer, start_test_server};

pub const SUCCESS_TOOLCHAIN: &str = "import fs from 'node:fs'; const a=process.argv.slice(2); const i=a.indexOf('--output'); fs.copyFileSync(a[1], a[i+1]);";
pub const FAILED_TOOLCHAIN: &str = "process.stderr.write('planned failure'); process.exit(7);";

pub async fn start_with_toolchain(project_id: &str, body: &str) -> TestServer {
    let server = start_test_server(ServerConfig::for_tests(project_id)).await;
    replace_toolchain_body(&server, body).await;
    server
}

pub async fn replace_toolchain_body(server: &TestServer, body: &str) {
    let path = server.workspace_root().join("test-toolchain.mjs");
    fs::write(&path, body).unwrap();
    server.set_toolchain_cli(path).await;
}

pub async fn rpc_with_bearer(
    server: &TestServer,
    token: &str,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(token)
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

pub async fn rpc_with_approval_cookie(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
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
