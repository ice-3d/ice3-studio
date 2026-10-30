use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rand::RngCore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studio_core::apply::{ApplyRequest, ApplyResult, apply_transaction};
use studio_core::document::Document;
use studio_core::geometry::evaluate_node_content;
use studio_core::ids::NodeId;
use studio_core::validation::validate_document;
use studio_core::{API_VERSION, PROJECT_SCHEMA_VERSION};
use studio_project::{ProjectStore, WorkspacePath};
use tokio::sync::broadcast;
use ts_rs::TS;

use crate::RevisionEvent;
use crate::auth::has_valid_token;
use crate::preview::{PreviewAdapter, export_source};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub workspace_root: PathBuf,
    pub project_id: String,
    pub token: Option<String>,
}

impl ServerConfig {
    pub fn for_tests(project_id: &str) -> Self {
        Self {
            workspace_root: std::env::temp_dir(),
            project_id: project_id.into(),
            token: Some("test-session-token".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RpcRequest {
    pub jsonrpc: String,
    #[ts(type = "unknown")]
    pub id: serde_json::Value,
    pub method: String,
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[ts(type = "unknown")]
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown | null")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown | null")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Capabilities {
    pub api_version: String,
    pub schema_version: u32,
    pub methods: Vec<String>,
}

struct Session {
    workspace_root: PathBuf,
    project_id: String,
    store: ProjectStore,
    document: Document,
    token: String,
    events: broadcast::Sender<RevisionEvent>,
}

#[derive(Clone)]
struct AppState {
    session: Arc<Mutex<Session>>,
}

pub struct TestServer {
    base_url: String,
    token: String,
    state: AppState,
    task: tokio::task::JoinHandle<()>,
    cleanup_root: Option<PathBuf>,
}

impl TestServer {
    pub fn health_url(&self) -> String {
        format!("{}/health", self.base_url)
    }

    pub fn rpc_url(&self) -> String {
        format!("{}/rpc", self.base_url)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn websocket_url(&self) -> String {
        format!(
            "{}/ws?token={}",
            self.base_url.replacen("http://", "ws://", 1),
            self.token
        )
    }

    pub fn schema_url(&self, name: &str) -> String {
        format!("{}/schemas/{name}", self.base_url)
    }

    pub async fn subscribe(&self) -> broadcast::Receiver<RevisionEvent> {
        self.state
            .session
            .lock()
            .expect("session lock")
            .events
            .subscribe()
    }

    pub async fn apply_fixture_batch(&self) -> Result<ApplyResult, String> {
        let mut session = self.state.session.lock().expect("session lock");
        let request = ApplyRequest {
            expected_revision: session.document.revision,
            dry_run: false,
            idempotency_key: "fixture-batch".into(),
            operations: vec![studio_core::operation::Operation::NodeCreate {
                node_id: NodeId::from("node-000001"),
                parent_id: None,
                name: "Ground".into(),
                transform: Default::default(),
                content: Default::default(),
                material_id: None,
                metadata: Default::default(),
            }],
        };
        let result =
            apply_transaction(&mut session.document, request).map_err(|error| error.to_string())?;
        session
            .store
            .write_snapshot(&session.document)
            .map_err(|error| error.to_string())?;
        let _ = session.events.send(RevisionEvent {
            event: "revision.committed".into(),
            revision: result.revision,
        });
        Ok(result)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
        if let Some(root) = &self.cleanup_root {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

pub async fn start_test_server(config: ServerConfig) -> TestServer {
    let unique = random_hex(8);
    let cleanup_root = config
        .workspace_root
        .join(format!("ice3-studio-test-{unique}"));
    std::fs::create_dir_all(&cleanup_root).expect("test workspace");
    let token = config.token.unwrap_or_else(|| random_hex(32));
    let (state, listener) = initialize_state(&cleanup_root, &config.project_id, token.clone())
        .await
        .expect("test server state");
    let address = listener.local_addr().expect("listener address");
    let router = build_router(state.clone());
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    TestServer {
        base_url: format!("http://{address}"),
        token,
        state,
        task,
        cleanup_root: Some(cleanup_root),
    }
}

pub async fn serve(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.token.unwrap_or_else(|| random_hex(32));
    let (state, listener) =
        initialize_state(&config.workspace_root, &config.project_id, token.clone()).await?;
    println!("{}", listener.local_addr()?);
    println!("{token}");
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}

async fn initialize_state(
    root: &std::path::Path,
    project_id: &str,
    token: String,
) -> Result<(AppState, tokio::net::TcpListener), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(root)?;
    let path = WorkspacePath::new(root, project_id)?;
    let store = if path.project_dir().exists() {
        ProjectStore::open(path)?
    } else {
        ProjectStore::create(path)?
    };
    let document = if path_project_exists(root, project_id) {
        store
            .load_current()
            .unwrap_or_else(|_| Document::new(project_id))
    } else {
        Document::new(project_id)
    };
    if store.load_current().is_err() {
        store.write_snapshot(&document)?;
    }
    let (events, _) = broadcast::channel(64);
    let state = AppState {
        session: Arc::new(Mutex::new(Session {
            workspace_root: root.to_path_buf(),
            project_id: project_id.into(),
            store,
            document,
            token,
            events,
        })),
    };
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?;
    Ok((state, listener))
}

fn path_project_exists(root: &std::path::Path, project_id: &str) -> bool {
    root.join(project_id).join("project.json").is_file()
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/rpc", post(rpc))
        .route("/ws", get(websocket))
        .route("/schemas/{name}", get(schema_file))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "apiVersion": API_VERSION,
        "schemaVersion": PROJECT_SCHEMA_VERSION
    }))
}

async fn rpc(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RpcRequest>,
) -> Response {
    let mut session = state.session.lock().expect("session lock");
    if mutating(&request.method) && !has_valid_token(&headers, &session.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": {
                    "code": -32001,
                    "message": "ICE3_STUDIO_AUTH_REQUIRED"
                }
            })),
        )
            .into_response();
    }
    let response = match dispatch(&mut session, &request) {
        Ok(result) => RpcResponse {
            jsonrpc: "2.0".into(),
            id: request.id,
            result: Some(result),
            error: None,
        },
        Err(error) => RpcResponse {
            jsonrpc: "2.0".into(),
            id: request.id,
            result: None,
            error: Some(error),
        },
    };
    (StatusCode::OK, Json(response)).into_response()
}

fn dispatch(session: &mut Session, request: &RpcRequest) -> Result<serde_json::Value, RpcError> {
    match request.method.as_str() {
        "system.health" => Ok(serde_json::json!({
            "status": "ok",
            "apiVersion": API_VERSION,
            "schemaVersion": PROJECT_SCHEMA_VERSION
        })),
        "system.capabilities" => Ok(serde_json::to_value(Capabilities {
            api_version: API_VERSION.into(),
            schema_version: PROJECT_SCHEMA_VERSION,
            methods: vec![
                "system.health".into(),
                "system.capabilities".into(),
                "project.create".into(),
                "project.open".into(),
                "project.get".into(),
                "project.save".into(),
                "project.close".into(),
                "document.get".into(),
                "document.apply".into(),
                "document.undo".into(),
                "document.redo".into(),
                "document.history".into(),
                "scene.get".into(),
                "scene.query".into(),
                "scene.bounds".into(),
                "validation.run".into(),
                "evaluation.evaluate".into(),
                "export.sourceGlb".into(),
                "export.runtimePreview".into(),
            ],
        })
        .expect("serializable capabilities")),
        "project.get" | "document.get" => {
            Ok(serde_json::to_value(&session.document).map_err(serialize_error)?)
        }
        "document.apply" => {
            let apply: ApplyRequest =
                serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
            let result =
                apply_transaction(&mut session.document, apply).map_err(transaction_error)?;
            if !result.dry_run {
                session
                    .store
                    .write_snapshot(&session.document)
                    .map_err(project_error)?;
                let _ = session.events.send(RevisionEvent {
                    event: "revision.committed".into(),
                    revision: result.revision,
                });
            }
            serde_json::to_value(result).map_err(serialize_error)
        }
        "document.undo" => {
            session.document = session.store.undo().map_err(project_error)?;
            let event = RevisionEvent {
                event: "revision.committed".into(),
                revision: session.document.revision,
            };
            let _ = session.events.send(event);
            serde_json::to_value(&session.document).map_err(serialize_error)
        }
        "document.redo" => {
            session.document = session.store.redo().map_err(project_error)?;
            let event = RevisionEvent {
                event: "revision.committed".into(),
                revision: session.document.revision,
            };
            let _ = session.events.send(event);
            serde_json::to_value(&session.document).map_err(serialize_error)
        }
        "document.history" => Ok(serde_json::json!(
            session.store.history().map_err(project_error)?
        )),
        "project.save" => Ok(serde_json::json!({
            "revision": session.document.revision,
            "saved": true
        })),
        "project.close" => Ok(serde_json::json!({ "closed": true })),
        "project.create" => {
            let params: ProjectParams =
                serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
            let path = WorkspacePath::new(&session.workspace_root, &params.project_id)
                .map_err(|error| RpcError::application(-32010, error.to_string()))?;
            let store = ProjectStore::create(path).map_err(project_error)?;
            let document = Document::new(params.project_id.clone());
            store.write_snapshot(&document).map_err(project_error)?;
            session.project_id = params.project_id;
            session.document = document;
            session.store = store;
            Ok(serde_json::json!({ "opened": true }))
        }
        "project.open" => {
            let params: ProjectParams =
                serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
            let path = WorkspacePath::new(&session.workspace_root, &params.project_id)
                .map_err(|error| RpcError::application(-32010, error.to_string()))?;
            let store = ProjectStore::open(path).map_err(project_error)?;
            let document = store.load_current().map_err(project_error)?;
            session.project_id = params.project_id;
            session.document = document;
            session.store = store;
            Ok(serde_json::json!({ "opened": true }))
        }
        "scene.get" => serde_json::to_value(&session.document.scene).map_err(serialize_error),
        "scene.query" => {
            let query: SceneQuery =
                serde_json::from_value(request.params.clone()).unwrap_or_default();
            let nodes = session
                .document
                .scene
                .nodes()
                .iter()
                .filter(|node| {
                    query
                        .parent_id
                        .as_ref()
                        .is_none_or(|parent| node.parent_id.as_ref() == Some(parent))
                })
                .collect::<Vec<_>>();
            serde_json::to_value(nodes).map_err(serialize_error)
        }
        "scene.bounds" => {
            let mut output = BTreeMap::new();
            for node in session.document.scene.nodes() {
                if let Ok(mesh) = evaluate_node_content(&node.content) {
                    output.insert(node.id.as_str().to_owned(), mesh.bounds);
                }
            }
            serde_json::to_value(output).map_err(serialize_error)
        }
        "validation.run" => {
            serde_json::to_value(validate_document(&session.document)).map_err(serialize_error)
        }
        "evaluation.evaluate" => {
            let mut output = BTreeMap::new();
            for node in session.document.scene.nodes() {
                let mesh = evaluate_node_content(&node.content).map_err(geometry_error)?;
                output.insert(node.id.as_str().to_owned(), mesh);
            }
            serde_json::to_value(output).map_err(serialize_error)
        }
        "export.sourceGlb" => {
            let output = export_source(&session.document, session.store.project_dir())
                .map_err(preview_error)?;
            serde_json::to_value(output).map_err(serialize_error)
        }
        "export.runtimePreview" => {
            let cli = std::env::var_os("STUDIO_TOOLCHAIN_CLI")
                .map(PathBuf::from)
                .ok_or_else(|| {
                    RpcError::application(-32020, "STUDIO_TOOLCHAIN_CLI is not configured".into())
                })?;
            let adapter =
                PreviewAdapter::new(&session.workspace_root, cli).map_err(preview_error)?;
            let output = adapter
                .generate(
                    &session.document,
                    session.store.project_dir(),
                    &session.project_id,
                )
                .map_err(preview_error)?;
            serde_json::to_value(output).map_err(serialize_error)
        }
        method => Err(RpcError::application(
            -32601,
            format!("unknown method: {method}"),
        )),
    }
}

async fn websocket(
    State(state): State<AppState>,
    Query(query): Query<HttpToken>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let (authorized, events) = {
        let session = state.session.lock().expect("session lock");
        (
            query.token.as_deref() == Some(session.token.as_str())
                || has_valid_token(&headers, &session.token),
            session.events.subscribe(),
        )
    };
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    upgrade.on_upgrade(move |socket| websocket_loop(socket, events))
}

async fn websocket_loop(mut socket: WebSocket, mut events: broadcast::Receiver<RevisionEvent>) {
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        let Ok(payload) = serde_json::to_string(&event) else {
                            break;
                        };
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn schema_file(AxumPath(name): AxumPath<String>) -> Response {
    if name != "studio-api.schema.json" && name != "studio-types.ts" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas")
        .join(name);
    match tokio::fs::read_to_string(path).await {
        Ok(body) => (StatusCode::OK, body).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpToken {
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectParams {
    project_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SceneQuery {
    parent_id: Option<NodeId>,
}

fn mutating(method: &str) -> bool {
    matches!(
        method,
        "project.create"
            | "project.open"
            | "project.save"
            | "project.close"
            | "document.apply"
            | "document.undo"
            | "document.redo"
            | "export.sourceGlb"
            | "export.runtimePreview"
    )
}

fn random_hex(bytes: usize) -> String {
    let mut data = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut data);
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn invalid_params(error: serde_json::Error) -> RpcError {
    RpcError::application(-32602, error.to_string())
}

fn serialize_error(error: serde_json::Error) -> RpcError {
    RpcError::application(-32603, error.to_string())
}

fn transaction_error(error: studio_core::apply::TransactionError) -> RpcError {
    let data = match error {
        studio_core::apply::TransactionError::ValidationFailed(ref diagnostics) => {
            serde_json::to_value(diagnostics).ok()
        }
        _ => None,
    };
    RpcError {
        code: -32010,
        message: error.to_string(),
        data,
    }
}

fn project_error(error: studio_project::ProjectError) -> RpcError {
    RpcError::application(-32011, error.to_string())
}

fn geometry_error(error: studio_core::geometry::GeometryError) -> RpcError {
    RpcError::application(-32012, error.to_string())
}

fn preview_error(error: crate::preview::PreviewError) -> RpcError {
    RpcError::application(-32013, error.to_string())
}

impl RpcError {
    fn application(code: i64, message: String) -> Self {
        Self {
            code,
            message,
            data: None,
        }
    }
}
