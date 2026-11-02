mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use studio_server::preview::hold_preview_for_tests;
use studio_server::{ServerConfig, start_test_server};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_stays_responsive_while_native_preview_processing_blocks() {
    let server = start_test_server(ServerConfig::for_tests("native-preview-hold")).await;
    server.apply_fixture_batch().await.unwrap();

    let _preview_delay = hold_preview_for_tests(Duration::from_millis(1500));
    let started = std::time::Instant::now();
    let preview = tokio::spawn({
        let rpc_url = server.rpc_url();
        let token = server.token().to_owned();
        async move {
            reqwest::Client::new()
                .post(rpc_url)
                .bearer_auth(token)
                .json(&serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 3,
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

    let address = server
        .base_url()
        .strip_prefix("http://")
        .unwrap()
        .to_owned();
    let health = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    });
    let preview = preview.await.unwrap();
    let health = health.join().unwrap();
    assert!(health.starts_with("HTTP/1.1 200 OK"), "{health}");
    assert!(preview["result"]["previewPath"].is_string(), "{preview}");
    assert!(started.elapsed() >= Duration::from_millis(1500));
}
