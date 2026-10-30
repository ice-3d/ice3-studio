use std::path::PathBuf;

use studio_server::toolchain::ToolchainService;
use studio_server::{ServerConfig, TestServer, start_test_server};
use studio_toolchain_client::ToolchainState;

const FAKE_SIDECAR: &str = r#"
import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { createInterface } from 'node:readline';

const args = process.argv.slice(2);
const workspace = args[args.indexOf('--workspace') + 1];
const mode = process.env.FAKE_SIDECAR_MODE ?? 'ready';
const lines = createInterface({ input: process.stdin });
lines.on('line', async (line) => {
  const request = JSON.parse(line);
  if (request.method === 'toolchain.capabilities') {
    process.stdout.write(JSON.stringify({
      jsonrpc: '2.0',
      id: request.id,
      result: {
        contractVersion: mode === 'incompatible' ? '2.0.0' : '1.0.0',
        packageVersion: '0.2.0',
        operations: ['toolchain.capabilities', 'asset.validate', 'asset.prepare', 'asset.preview', 'asset.partition', '$/cancel'],
        inputFormats: ['glb', 'gltf'],
        features: { lod: true, partition: true, tileset: true, renderBatching: true, cancellation: true },
        limits: { maxMessageBytes: 8388608, maxConcurrentJobs: 2, maxSourceBytes: 2147483648 }
      }
    }) + '\n');
    return;
  }
  if (request.method === 'asset.preview') {
    if (mode === 'crash-once') {
      const marker = `${workspace}/crashed-once`;
      try {
        await readFile(marker);
      } catch {
        await writeFile(marker, '1');
        process.exit(9);
      }
    }
    if (mode === 'failed') {
      process.stdout.write(JSON.stringify({
        jsonrpc: '2.0',
        id: request.id,
        error: { code: -32603, message: 'failed', data: { code: 'ICE3_TOOL_SIDECAR_JOB_FAILED', message: 'failed', retryable: false } }
      }) + '\n');
      return;
    }
    if (mode === 'slow') await new Promise((resolve) => setTimeout(resolve, 500));
    const source = join(workspace, request.params.sourcePath);
    const output = join(workspace, request.params.outputPath);
    await mkdir(dirname(output), { recursive: true });
    await copyFile(source, output);
    const bytes = await readFile(output);
    const hash = createHash('sha256').update(bytes).digest('hex');
    await writeFile(`${dirname(output)}/manifest.json`, '{}\n');
    process.stdout.write(JSON.stringify({
      jsonrpc: '2.0',
      id: request.id,
      result: {
        assetId: request.params.assetId,
        contentHash: `sha256:${hash}`,
        profileVersion: 'ice3-prepared-glb-v1',
        manifestPath: `${dirname(request.params.outputPath)}/manifest.json`,
        primaryPath: request.params.outputPath,
        byteLength: bytes.byteLength,
        diagnostics: [],
        warnings: []
      }
    }) + '\n');
  }
});
"#;

async fn rpc(server: &TestServer, method: &str, params: serde_json::Value) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
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

async fn install_sidecar(server: &TestServer, mode: &str) {
    let script = server_project_dir(server).join("fake-sidecar.mjs");
    std::fs::write(
        &script,
        format!("process.env.FAKE_SIDECAR_MODE = '{mode}';\n{FAKE_SIDECAR}"),
    )
    .unwrap();
    server.set_toolchain_cli(script).await;
}

fn server_project_dir(server: &TestServer) -> PathBuf {
    std::fs::read_dir(server.workspace_root())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.join("project.json").is_file())
        .expect("test project directory")
}

async fn revision(server: &TestServer) -> u64 {
    rpc(server, "document.get", serde_json::json!({})).await["result"]["revision"]
        .as_u64()
        .unwrap()
}

#[tokio::test]
async fn preview_uses_the_sidecar_and_preserves_revision() {
    let server = start_test_server(ServerConfig::for_tests("toolchain-preview")).await;
    install_sidecar(&server, "ready").await;
    let before = revision(&server).await;
    let preview = rpc(&server, "export.runtimePreview", serde_json::json!({})).await;
    assert_eq!(preview["result"]["assetId"], "toolchain-preview");
    assert_eq!(revision(&server).await, before);
}

#[tokio::test]
async fn preview_uses_the_current_project_after_a_project_switch() {
    let server = start_test_server(ServerConfig::for_tests("toolchain-project-a")).await;
    install_sidecar(&server, "ready").await;
    let first = rpc(&server, "export.runtimePreview", serde_json::json!({})).await;
    assert!(first["result"]["previewPath"].is_string(), "{first}");

    let created = rpc(
        &server,
        "project.create",
        serde_json::json!({ "projectId": "toolchain-project-b" }),
    )
    .await;
    assert_eq!(created["result"]["opened"], true, "{created}");
    let second = rpc(&server, "export.runtimePreview", serde_json::json!({})).await;
    assert!(second["result"]["previewPath"].is_string(), "{second}");
}

#[tokio::test]
async fn incompatible_sidecar_disables_preview_but_not_document_reads() {
    let server = start_test_server(ServerConfig::for_tests("toolchain-incompatible")).await;
    install_sidecar(&server, "incompatible").await;
    let document = rpc(&server, "document.get", serde_json::json!({})).await;
    assert!(document.get("result").is_some());
    let preview = rpc(&server, "export.runtimePreview", serde_json::json!({})).await;
    assert_eq!(
        preview["error"]["diagnosticCode"],
        "ICE3_STUDIO_TOOLCHAIN_INCOMPATIBLE"
    );
}

#[tokio::test]
async fn capabilities_expose_toolchain_state() {
    let server = start_test_server(ServerConfig::for_tests("toolchain-capabilities")).await;
    install_sidecar(&server, "ready").await;
    let response = rpc(&server, "system.capabilities", serde_json::json!({})).await;
    assert_eq!(response["result"]["toolchain"]["state"], "ready");
    assert_eq!(response["result"]["toolchain"]["contractVersion"], "1.0.0");
}

#[tokio::test]
async fn document_reads_stay_responsive_during_slow_preview() {
    let server = start_test_server(ServerConfig::for_tests("toolchain-concurrency")).await;
    install_sidecar(&server, "slow").await;
    let preview = tokio::spawn({
        let rpc_url = server.rpc_url();
        let token = server.token().to_owned();
        async move {
            reqwest::Client::new()
                .post(rpc_url)
                .bearer_auth(token)
                .json(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": "preview",
                    "method": "export.runtimePreview",
                    "params": {}
                }))
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap()
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let document = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        rpc(&server, "document.get", serde_json::json!({})),
    )
    .await
    .expect("document read must not wait for preview");
    assert!(document.get("result").is_some());
    assert!(preview.await.unwrap().get("result").is_some());
}

#[tokio::test]
async fn sidecar_crash_is_retried_once_without_changing_revision() {
    let server = start_test_server(ServerConfig::for_tests("toolchain-restart")).await;
    install_sidecar(&server, "crash-once").await;
    let before = revision(&server).await;
    let preview = rpc(&server, "export.runtimePreview", serde_json::json!({})).await;
    assert!(preview.get("result").is_some());
    assert_eq!(revision(&server).await, before);
}

#[test]
fn stopped_sidecar_restarts_lazily_on_the_next_preview() {
    let root = tempfile::tempdir().unwrap();
    let script = root.path().join("fake-sidecar.mjs");
    std::fs::write(
        &script,
        format!("process.env.FAKE_SIDECAR_MODE = 'ready';\n{FAKE_SIDECAR}"),
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("cache")).unwrap();
    std::fs::write(root.path().join("cache/source.glb"), b"glTFfixture").unwrap();
    let service = ToolchainService::new(root.path(), Some(script));
    assert_eq!(service.status().state, ToolchainState::Ready);
    service.shutdown();
    assert_eq!(service.status().state, ToolchainState::Unavailable);
    let output = root.path().join("cache/toolchain/result.glb");
    service
        .preview(&root.path().join("cache/source.glb"), &output, "lazy")
        .unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), b"glTFfixture");
    assert_eq!(service.status().state, ToolchainState::Ready);
}

#[test]
fn concurrent_preview_requests_share_one_sidecar_process() {
    let root = tempfile::tempdir().unwrap();
    let script = root.path().join("fake-sidecar.mjs");
    std::fs::write(
        &script,
        format!("process.env.FAKE_SIDECAR_MODE = 'slow';\n{FAKE_SIDECAR}"),
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("cache")).unwrap();
    std::fs::write(root.path().join("cache/one.glb"), b"one").unwrap();
    std::fs::write(root.path().join("cache/two.glb"), b"two").unwrap();
    let service = ToolchainService::new(root.path(), Some(script));
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            service.preview(
                &root.path().join("cache/one.glb"),
                &root.path().join("cache/toolchain/one.glb"),
                "one",
            )
        });
        let second = scope.spawn(|| {
            service.preview(
                &root.path().join("cache/two.glb"),
                &root.path().join("cache/toolchain/two.glb"),
                "two",
            )
        });
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
    });
    assert!(started.elapsed() < std::time::Duration::from_millis(900));
}
