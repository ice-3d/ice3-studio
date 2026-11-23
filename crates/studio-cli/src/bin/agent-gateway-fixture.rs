use std::error::Error;
use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[derive(Clone)]
struct FixtureConfig {
    studio_url: String,
    token: String,
    project_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = parse_args()?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    println!("AGENT_GATEWAY_URL=ws://{address}/agent");
    io::stdout().flush()?;
    loop {
        let (stream, _) = listener.accept().await?;
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, config).await {
                eprintln!("agent gateway fixture: {error}");
            }
        });
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    config: FixtureConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut socket = accept_async(stream).await?;
    let client = reqwest::Client::builder().no_proxy().build()?;
    while let Some(message) = socket.next().await {
        let message = message?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(text.as_str())?;
        match value["type"].as_str() {
            Some("ping") => {
                send(
                    &mut socket,
                    json!({
                        "type": "pong",
                        "protocolVersion": 1
                    }),
                )
                .await?;
            }
            Some("run.cancel") => {
                let run_id = value["runId"].as_str().unwrap_or("run-1");
                send(
                    &mut socket,
                    json!({
                        "type": "run.finished",
                        "protocolVersion": 1,
                        "runId": run_id
                    }),
                )
                .await?;
            }
            Some("intent.submit") => {
                handle_intent(&mut socket, &client, &config, &value).await?;
            }
            _ => {
                return Err("unsupported fixture message".into());
            }
        }
    }
    Ok(())
}

async fn handle_intent(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    client: &reqwest::Client,
    config: &FixtureConfig,
    value: &Value,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let request_id = value["requestId"].as_str().ok_or("requestId is missing")?;
    let intent = &value["intent"];
    if intent["projectId"] != config.project_id
        || !intent["revision"].is_u64()
        || !intent["selectionIds"].is_array()
        || !intent["camera"]["position"].is_array()
    {
        return Err("intent context is invalid".into());
    }
    let run_id = format!("run-{}", now_suffix());
    send(
        socket,
        json!({
            "type": "run.started",
            "protocolVersion": 1,
            "requestId": request_id,
            "runId": run_id
        }),
    )
    .await?;
    send(
        socket,
        json!({
            "type": "plan.updated",
            "protocolVersion": 1,
            "runId": run_id,
            "summary": "Planning the edit"
        }),
    )
    .await?;
    send(
        socket,
        json!({
            "type": "tool.updated",
            "protocolVersion": 1,
            "runId": run_id,
            "toolName": "scene.search",
            "phase": "started",
            "summary": "Searching the scene"
        }),
    )
    .await?;

    if intent["text"]
        .as_str()
        .is_some_and(|text| text.contains("[disconnect]"))
    {
        tokio::time::sleep(Duration::from_millis(100)).await;
        socket.close(None).await?;
        return Ok(());
    }

    send(
        socket,
        json!({
            "type": "message.delta",
            "protocolVersion": 1,
            "runId": run_id,
            "text": "Found the clarifier"
        }),
    )
    .await?;
    send(
        socket,
        json!({
            "type": "tool.updated",
            "protocolVersion": 1,
            "runId": run_id,
            "toolName": "scene.search",
            "phase": "finished",
            "summary": "Found the clarifier"
        }),
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(600)).await;

    match create_change_set(client, config, intent["revision"].as_u64().unwrap_or(0)).await {
        Ok(change_set_id) => {
            send(
                socket,
                json!({
                    "type": "changeset.linked",
                    "protocolVersion": 1,
                    "runId": run_id,
                    "changeSetId": change_set_id,
                    "entryIds": []
                }),
            )
            .await?;
            send(
                socket,
                json!({
                    "type": "run.finished",
                    "protocolVersion": 1,
                    "runId": run_id
                }),
            )
            .await?;
        }
        Err(error) => {
            send(
                socket,
                json!({
                    "type": "run.failed",
                    "protocolVersion": 1,
                    "runId": run_id,
                    "diagnosticCode": "ICE3_AGENT_RUN_FAILED",
                    "message": error.to_string()
                }),
            )
            .await?;
        }
    }
    Ok(())
}

async fn create_change_set(
    client: &reqwest::Client,
    config: &FixtureConfig,
    base_revision: u64,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let suffix = now_suffix();
    let created = rpc(
        client,
        config,
        json!({
            "baseRevision": base_revision,
            "title": "Agent E2E",
            "idempotencyKey": format!("agent-e2e-create-{suffix}")
        }),
        "changeset.create",
    )
    .await?;
    let change_set_id = created["id"]
        .as_str()
        .ok_or("ChangeSet ID is missing")?
        .to_owned();
    let version = created["version"].as_u64().ok_or("version is missing")?;
    let added = rpc(
        client,
        config,
        json!({
            "changeSetId": change_set_id,
            "expectedVersion": version,
            "idempotencyKey": format!("agent-e2e-add-{suffix}"),
            "entries": [{
                "source": { "kind": "agent", "label": "deterministic-fixture" },
                "operation": {
                    "type": "node.update",
                    "nodeId": "node-000001",
                    "name": "Agent E2E Ground",
                    "transform": null,
                    "visible": null,
                    "metadata": null
                }
            }]
        }),
        "changeset.addOperations",
    )
    .await?;
    let version = added["version"].as_u64().ok_or("version is missing")?;
    let evaluated = rpc(
        client,
        config,
        json!({
            "changeSetId": change_set_id,
            "expectedVersion": version,
            "includeCandidate": false
        }),
        "changeset.evaluate",
    )
    .await?;
    if evaluated["status"] != "ready" {
        return Err(format!("ChangeSet evaluation failed: {evaluated}").into());
    }
    Ok(change_set_id)
}

async fn rpc(
    client: &reqwest::Client,
    config: &FixtureConfig,
    params: Value,
    method: &str,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    let response = client
        .post(format!("{}/rpc", config.studio_url))
        .bearer_auth(&config.token)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": method,
            "method": method,
            "params": params
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    if let Some(error) = response.get("error") {
        return Err(format!("Studio RPC failed: {error}").into());
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "RPC result is missing".into())
}

async fn send(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    value: Value,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    socket.send(Message::Text(value.to_string().into())).await?;
    Ok(())
}

fn parse_args() -> Result<FixtureConfig, Box<dyn Error>> {
    let mut studio_url = None;
    let mut token = None;
    let mut project_id = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--studio-url" => studio_url = args.next(),
            "--token" => token = args.next(),
            "--project" => project_id = args.next(),
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    Ok(FixtureConfig {
        studio_url: studio_url.ok_or("--studio-url is required")?,
        token: token.ok_or("--token is required")?,
        project_id: project_id.ok_or("--project is required")?,
    })
}

fn now_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
