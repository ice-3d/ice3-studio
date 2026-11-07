//! Minimal Model Context Protocol server over stdio.
//!
//! It does not model anything itself: it exposes the tools already declared by a
//! running `studio serve` instance, so the manifest stays the single source of
//! truth for what an agent may call. Only `tools` is implemented; MCP resources,
//! prompts and sampling are deliberately out of scope.

use std::error::Error;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

/// Protocol revision this server speaks. Clients negotiate down if they must.
const PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "ice3-studio";

pub struct McpConfig {
    /// Base URL of a running `studio serve` instance, without a trailing slash.
    pub url: String,
    /// Local session token used for every forwarded call.
    pub token: String,
    /// When set, start a Studio server for this workspace if the URL is down.
    pub spawn: Option<SpawnTarget>,
}

pub struct SpawnTarget {
    pub workspace: PathBuf,
    pub project: String,
    pub port: u16,
}

pub fn run(config: McpConfig) -> Result<(), Box<dyn Error>> {
    // Loopback traffic must never honour HTTP_PROXY; a local proxy would answer
    // 502 and the MCP server would look unreachable.
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .no_proxy()
        .build()?;
    // Keep the handle alive for the whole session: dropping it stops the server.
    let _spawned = ensure_backend(&client, &config)?;
    let mut server = McpServer {
        client,
        base_url: config.url.trim_end_matches('/').to_string(),
        token: config.token,
        tools: None,
    };

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Some(response) = server.handle_line(&line) else {
            continue;
        };
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}

/// A Studio server started by this process; it dies with the MCP session.
struct SpawnedServer {
    child: std::process::Child,
}

impl Drop for SpawnedServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn reachable(client: &reqwest::blocking::Client, url: &str) -> bool {
    client
        .get(format!("{url}/health"))
        .timeout(Duration::from_secs(1))
        .send()
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

/// Start Studio only when the caller asked for it and nothing answers yet.
/// An already-running server always wins, so a hand-started instance is never
/// replaced underneath the user.
fn ensure_backend(
    client: &reqwest::blocking::Client,
    config: &McpConfig,
) -> Result<Option<SpawnedServer>, Box<dyn Error>> {
    if reachable(client, &config.url) {
        return Ok(None);
    }
    let Some(target) = &config.spawn else {
        return Ok(None);
    };
    let log = std::fs::File::create(std::env::temp_dir().join("ice3-studio-serve.log"))?;
    let child = std::process::Command::new(std::env::current_exe()?)
        .arg("serve")
        .arg("--workspace")
        .arg(&target.workspace)
        .arg("--project")
        .arg(&target.project)
        .arg("--port")
        .arg(target.port.to_string())
        .env("STUDIO_TOKEN", &config.token)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(log))
        .spawn()?;
    let mut guard = SpawnedServer { child };
    for _ in 0..300 {
        if reachable(client, &config.url) {
            return Ok(Some(guard));
        }
        if let Some(status) = guard.child.try_wait()? {
            return Err(format!("studio serve exited during startup with {status}").into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("studio serve did not become ready within 30s".into())
}

struct McpServer {
    client: reqwest::blocking::Client,
    base_url: String,
    token: String,
    /// Tool list is stable for a session; rebuilding it would refetch the schema.
    tools: Option<Vec<Value>>,
}

impl McpServer {
    /// Returns `None` for notifications, which must not receive a response.
    fn handle_line(&mut self, line: &str) -> Option<String> {
        let request: Value = match serde_json::from_str(line) {
            Ok(request) => request,
            Err(error) => {
                return Some(rpc_error(
                    Value::Null,
                    -32700,
                    format!("parse error: {error}"),
                ));
            }
        };
        let id = request.get("id").cloned();
        let method = request["method"].as_str().unwrap_or_default().to_string();
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        // Notifications (`notifications/initialized`, `notifications/cancelled`)
        // carry no id and must stay silent.
        let id = id?;

        let outcome = match method.as_str() {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
            })),
            "ping" => Ok(json!({})),
            "tools/list" => self.ensure_tools().map(|tools| json!({ "tools": tools })),
            "tools/call" => Ok(self.call_tool(&params)),
            other => Err((-32601, format!("unknown method: {other}"))),
        };

        Some(match outcome {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
            Err((code, message)) => rpc_error(id, code, message),
        })
    }

    fn ensure_tools(&mut self) -> Result<Vec<Value>, (i64, String)> {
        if let Some(tools) = &self.tools {
            return Ok(tools.clone());
        }
        let manifest = self
            .rpc("system.agentManifest", json!({}))
            .map_err(|message| (-32603, message))?;
        let schema_ref = manifest["schemaUrl"]
            .as_str()
            .unwrap_or("/schemas/studio-api.schema.json");
        let schema = self.fetch_schema(schema_ref)?;

        let mut tools = Vec::new();
        for tool in manifest["tools"].as_array().cloned().unwrap_or_default() {
            // MCP has no way to show "temporarily unavailable"; exposing such a
            // tool only invites calls that must fail.
            if tool["available"].as_bool() == Some(false) {
                continue;
            }
            tools.push(to_mcp_tool(&tool, &schema));
        }
        self.tools = Some(tools.clone());
        Ok(tools)
    }

    fn call_tool(&mut self, params: &Value) -> Value {
        let name = params["name"].as_str().unwrap_or_default().to_string();
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match self.rpc(&name, arguments) {
            Ok(result) => {
                let text =
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
                let mut payload = json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": false,
                });
                // `structuredContent` must be an object; arrays and scalars only
                // travel as text.
                if result.is_object() {
                    payload["structuredContent"] = result;
                }
                payload
            }
            Err(message) => json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true,
            }),
        }
    }

    fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        let response = self
            .client
            .post(format!("{}/rpc", self.base_url))
            .bearer_auth(&self.token)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params,
            }))
            .send()
            .map_err(|error| format!("studio is unreachable: {error}"))?;
        let body: Value = response
            .json()
            .map_err(|error| format!("studio returned invalid JSON: {error}"))?;
        if let Some(error) = body.get("error") {
            return Err(error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| error.to_string()));
        }
        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }

    fn fetch_schema(&self, reference: &str) -> Result<Value, (i64, String)> {
        let response = self
            .client
            .get(format!("{}{reference}", self.base_url))
            .send()
            .map_err(|error| {
                (
                    -32603,
                    format!("schema {reference} is unreachable: {error}"),
                )
            })?;
        if !response.status().is_success() {
            return Err((
                -32603,
                format!("schema {reference} returned {}", response.status()),
            ));
        }
        response
            .json()
            .map_err(|error| (-32603, format!("schema {reference} is invalid: {error}")))
    }
}

fn rpc_error(id: Value, code: i64, message: String) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
    .to_string()
}

fn to_mcp_tool(tool: &Value, schema: &Value) -> Value {
    let name = tool["name"].as_str().unwrap_or_default();
    let description = tool["summary"].as_str().unwrap_or_default();
    let mut mcp = json!({
        "name": name,
        "description": description,
        "inputSchema": inline_schema(tool, "inputSchemaRef", schema),
        "annotations": annotations(tool),
    });
    if let Some(output) = tool.get("outputSchemaRef").and_then(Value::as_str) {
        mcp["outputSchema"] = resolve(schema, output);
    }
    mcp
}

/// MCP requires an inline schema; the manifest only carries a `#/types/...`
/// reference into the published schema document.
fn inline_schema(tool: &Value, field: &str, schema: &Value) -> Value {
    tool.get(field)
        .and_then(Value::as_str)
        .map(|reference| resolve(schema, reference))
        .unwrap_or_else(|| json!({ "type": "object", "additionalProperties": true }))
}

fn resolve(schema: &Value, reference: &str) -> Value {
    reference
        .split_once('#')
        .and_then(|(_, pointer)| schema.pointer(pointer).cloned())
        .unwrap_or_else(|| json!({ "type": "object", "additionalProperties": true }))
}

/// Advertise the same guarantees the JSON-RPC contract already promises, so a
/// client can decide whether a call needs confirmation without knowing Studio.
fn annotations(tool: &Value) -> Value {
    let side_effect = tool["sideEffect"].as_str().unwrap_or("read");
    json!({
        "readOnlyHint": side_effect == "read",
        "destructiveHint": side_effect == "commit" || side_effect == "direct_write",
        "idempotentHint": tool["idempotency"].as_str() == Some("required"),
        "openWorldHint": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Value {
        json!({
            "types": {
                "SceneSearchRequest": {
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                }
            }
        })
    }

    #[test]
    fn inlines_the_referenced_input_schema() {
        let tool = json!({
            "name": "scene.search",
            "summary": "Search the scene.",
            "sideEffect": "read",
            "idempotency": "none",
            "available": true,
            "inputSchemaRef": "/schemas/studio-api.schema.json#/types/SceneSearchRequest",
        });
        let mcp = to_mcp_tool(&tool, &schema());
        assert_eq!(mcp["name"], "scene.search");
        assert_eq!(mcp["inputSchema"]["properties"]["query"]["type"], "string");
        assert_eq!(mcp["annotations"]["readOnlyHint"], true);
    }

    #[test]
    fn falls_back_to_an_open_object_when_the_reference_is_missing() {
        let tool = json!({ "name": "x", "summary": "y", "sideEffect": "read" });
        let mcp = to_mcp_tool(&tool, &schema());
        assert_eq!(mcp["inputSchema"]["type"], "object");
        assert_eq!(mcp["annotations"]["readOnlyHint"], true);
    }

    #[test]
    fn marks_writes_and_idempotency_in_annotations() {
        let tool = json!({
            "name": "changeset.create",
            "summary": "Create a proposal.",
            "sideEffect": "proposal",
            "idempotency": "required",
        });
        let annotations = annotations(&tool);
        assert_eq!(annotations["readOnlyHint"], false);
        assert_eq!(annotations["destructiveHint"], false);
        assert_eq!(annotations["idempotentHint"], true);
    }

    #[test]
    fn notifications_are_not_answered() {
        let mut server = McpServer {
            client: reqwest::blocking::Client::new(),
            base_url: "http://127.0.0.1:1".into(),
            token: "t".into(),
            tools: None,
        };
        let response =
            server.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert!(response.is_none());
    }

    /// Blocking reqwest cannot run inside an async context, so the live tests
    /// drive the server from a plain `#[test]` and only use the runtime to boot it.
    fn live_server() -> (tokio::runtime::Runtime, studio_server::TestServer) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let server = runtime.block_on(studio_server::start_test_server(
            studio_server::ServerConfig::for_tests("mcp-live"),
        ));
        (runtime, server)
    }

    fn live_mcp(server: &studio_server::TestServer) -> McpServer {
        McpServer {
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("client"),
            base_url: server.base_url().trim_end_matches('/').to_string(),
            token: server.token().to_string(),
            tools: None,
        }
    }

    #[test]
    fn lists_tools_with_inlined_schemas_from_a_live_manifest() {
        let (_runtime, server) = live_server();
        let mut mcp = live_mcp(&server);
        let tools = mcp.ensure_tools().expect("tools");

        let search = tools
            .iter()
            .find(|tool| tool["name"] == "scene.search")
            .expect("scene.search is exposed");
        assert_eq!(search["inputSchema"]["type"], "object");
        assert!(
            search["inputSchema"]["properties"].get("query").is_some(),
            "the schema should be inlined, not referenced: {search}"
        );
        assert_eq!(search["annotations"]["readOnlyHint"], true);
    }

    #[test]
    fn forwards_a_tool_call_to_the_studio_rpc() {
        let (_runtime, server) = live_server();
        let mut mcp = live_mcp(&server);
        let response = mcp.call_tool(&json!({ "name": "scene.search", "arguments": {} }));

        assert_eq!(response["isError"], false, "{response}");
        assert_eq!(response["structuredContent"]["items"], json!([]));
        assert_eq!(response["content"][0]["type"], "text");
    }

    #[test]
    fn surfaces_a_tool_failure_as_an_error_result() {
        let (_runtime, server) = live_server();
        let mut mcp = live_mcp(&server);
        let response = mcp.call_tool(&json!({
            "name": "scene.search",
            "arguments": { "limit": 0 }
        }));

        assert_eq!(response["isError"], true, "{response}");
        assert!(
            !response["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .is_empty()
        );
    }

    #[test]
    fn unknown_methods_return_a_json_rpc_error() {
        let mut server = McpServer {
            client: reqwest::blocking::Client::new(),
            base_url: "http://127.0.0.1:1".into(),
            token: "t".into(),
            tools: None,
        };
        let response = server
            .handle_line(r#"{"jsonrpc":"2.0","id":7,"method":"resources/list"}"#)
            .expect("requests are answered");
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["id"], 7);
        assert_eq!(parsed["error"]["code"], -32601);
    }
}
