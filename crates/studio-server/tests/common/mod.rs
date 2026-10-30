use std::fs;

use studio_server::{ServerConfig, TestServer, start_test_server};

pub const SUCCESS_TOOLCHAIN: &str =
    "import fs from 'node:fs'; const a=process.argv.slice(2); const i=a.indexOf('--output'); fs.copyFileSync(a[1], a[i+1]);";
pub const FAILED_TOOLCHAIN: &str =
    "process.stderr.write('planned failure'); process.exit(7);";

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
