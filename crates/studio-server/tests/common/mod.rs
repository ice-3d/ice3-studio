#![allow(
    dead_code,
    reason = "integration tests use different subsets of shared helpers"
)]

use std::fs;

use studio_server::{ServerConfig, TestServer, start_test_server};

pub const SUCCESS_TOOLCHAIN: &str = r#"
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { createInterface } from 'node:readline';
const args = process.argv.slice(2);
const workspace = args[args.indexOf('--workspace') + 1];
createInterface({ input: process.stdin }).on('line', async (line) => {
  const request = JSON.parse(line);
  if (request.method === 'toolchain.capabilities') {
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result: {
      contractVersion: '1.0.0', packageVersion: '0.2.0',
      operations: ['toolchain.capabilities', 'asset.preview'], inputFormats: ['glb'],
      features: { lod: false, partition: false, tileset: false, renderBatching: false, cancellation: true },
      limits: { maxMessageBytes: 8388608, maxConcurrentJobs: 2, maxSourceBytes: 2147483648 }
    } }) + '\n');
  } else if (request.method === 'asset.preview') {
    if (!request.params.outputPath.startsWith('cache/toolchain/')) {
      process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, error: {
        code: -32603, message: 'asset outputs must be inside cache/toolchain',
        data: { code: 'ICE3_TOOL_SIDECAR_PATH_INVALID', message: 'asset outputs must be inside cache/toolchain', retryable: false }
      } }) + '\n');
      return;
    }
    if (process.argv[1]?.includes('slow-toolchain')) {
      await new Promise((resolve) => setTimeout(resolve, 600));
    }
    const source = `${workspace}/${request.params.sourcePath}`;
    const output = `${workspace}/${request.params.outputPath}`;
    fs.mkdirSync(output.slice(0, output.lastIndexOf('/')), { recursive: true });
    fs.copyFileSync(source, output);
    const bytes = fs.readFileSync(output);
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result: {
      assetId: request.params.assetId,
      contentHash: `sha256:${createHash('sha256').update(bytes).digest('hex')}`,
      profileVersion: 'ice3-prepared-glb-v1',
      manifestPath: `${request.params.outputPath}.manifest.json`,
      primaryPath: request.params.outputPath,
      byteLength: bytes.byteLength, diagnostics: [], warnings: []
    } }) + '\n');
  }
});
"#;
pub const FAILED_TOOLCHAIN: &str = r#"
import { createInterface } from 'node:readline';
createInterface({ input: process.stdin }).on('line', (line) => {
  const request = JSON.parse(line);
  if (request.method === 'toolchain.capabilities') {
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result: {
      contractVersion: '1.0.0', packageVersion: '0.2.0',
      operations: ['toolchain.capabilities', 'asset.preview'], inputFormats: ['glb'],
      features: { lod: false, partition: false, tileset: false, renderBatching: false, cancellation: true },
      limits: { maxMessageBytes: 8388608, maxConcurrentJobs: 2, maxSourceBytes: 2147483648 }
    } }) + '\n');
  } else if (request.method === 'asset.preview') {
    process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, error: {
      code: -32603, message: 'planned failure',
      data: { code: 'ICE3_TOOL_SIDECAR_JOB_FAILED', message: 'planned failure', retryable: false }
    } }) + '\n');
  }
});
"#;

pub async fn rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    rpc_with_bearer(server, server.token(), method, params).await
}

pub async fn create_and_evaluate_candidate(server: &TestServer) -> serde_json::Value {
    let created = rpc(
        server,
        "changeset.create",
        serde_json::json!({ "baseRevision": 0, "title": "candidate preview" }),
    )
    .await;
    let change_set_id = created["result"]["id"].as_str().unwrap();
    rpc(
        server,
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 0,
            "entries": [{
                "source": { "kind": "agent", "label": "native-preview" },
                "operation": {
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Candidate tank",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": { "type": "group" },
                    "material_id": null,
                    "metadata": {}
                }
            }]
        }),
    )
    .await;
    let evaluated = rpc(
        server,
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "includeCandidate": false
        }),
    )
    .await;
    assert_eq!(evaluated["result"]["status"], "ready");
    evaluated["result"].clone()
}

pub async fn start_with_toolchain(project_id: &str, body: &str) -> TestServer {
    let server = start_test_server(ServerConfig::for_tests(project_id)).await;
    replace_toolchain_body(&server, body).await;
    server
}

pub async fn replace_toolchain_body(server: &TestServer, body: &str) {
    let project = std::fs::read_dir(server.workspace_root())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.join("project.json").is_file())
        .unwrap();
    let path = project.join("test-toolchain.mjs");
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
