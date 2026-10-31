use std::collections::{BTreeMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::rejection::JsonRejection;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path as AxumPath, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rand::RngCore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studio_core::apply::{ApplyRequest, ApplyResult, apply_transaction};
use studio_core::document::Document;
use studio_core::geometry::Bounds;
use studio_core::gpu_contract::{GeometryBackend, GeometryBackendMode};
use studio_core::ids::NodeId;
use studio_core::large_scene::evaluate::{
    EvaluatedScene, evaluate_large_scene_with_cache_and_evaluator, evaluated_scene_bounds,
};
use studio_core::validation::{DiagnosticCode, validate_document};
use studio_core::{API_VERSION, PROJECT_SCHEMA_VERSION};
use studio_gpu::NativeGpuBackend;
use studio_project::geometry_cache::GeometryDiskCache;
use studio_project::{ProjectStore, WorkspacePath};
use tokio::sync::broadcast;
use tower_http::services::{ServeDir, ServeFile};
use ts_rs::TS;

use crate::ServerEvent;
use crate::auth::{approval_cookie, bearer_token};
use crate::capabilities::{WriteMode, authorize_write};
use crate::compute::{ComputeEvaluator, ComputeService, ComputeStatus};
use crate::preview::{PreviewAdapter, export_source};
use crate::toolchain::{ToolchainService, ToolchainStatus};

const MAX_RPC_BODY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub workspace_root: PathBuf,
    pub project_id: String,
    pub token: Option<String>,
    pub fixture: Option<PathBuf>,
    pub port: u16,
    pub toolchain_cli: Option<PathBuf>,
    pub approval_token: Option<String>,
    pub write_mode: WriteMode,
    pub viewer_dir: Option<PathBuf>,
    pub compute_mode: GeometryBackendMode,
}

impl ServerConfig {
    pub fn for_tests(project_id: &str) -> Self {
        Self {
            workspace_root: std::env::temp_dir(),
            project_id: project_id.into(),
            token: Some("test-session-token".into()),
            fixture: None,
            port: 0,
            toolchain_cli: None,
            approval_token: None,
            write_mode: WriteMode::Autonomous,
            viewer_dir: None,
            compute_mode: GeometryBackendMode::Cpu,
        }
    }

    pub fn review_required(project_id: &str) -> Self {
        Self {
            token: Some("test-agent-token".into()),
            approval_token: Some("test-approval-token".into()),
            write_mode: WriteMode::ReviewRequired,
            ..Self::for_tests(project_id)
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
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
    pub toolchain: ToolchainStatus,
    pub compute: ComputeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EvaluatedSceneStats {
    pub source_nodes: usize,
    pub expanded_nodes: usize,
    pub unique_meshes: usize,
    pub effective_triangles: usize,
    pub unique_triangles: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub generated_meshes: usize,
}

pub(crate) struct Session {
    pub(crate) workspace_root: PathBuf,
    pub(crate) project_id: String,
    pub(crate) store: ProjectStore,
    pub(crate) document: Document,
    pub(crate) token: String,
    pub(crate) events: broadcast::Sender<ServerEvent>,
    pub(crate) closed: bool,
    pub(crate) toolchain_cli: Option<PathBuf>,
    pub(crate) toolchain: Arc<ToolchainService>,
    pub(crate) compute: Arc<ComputeService>,
    pub(crate) approval_token: String,
    pub(crate) write_mode: WriteMode,
    pub(crate) bootstrap_token: Option<String>,
    pub(crate) bootstrap_expires_at_ms: u64,
    pub(crate) bootstrap_consumed: bool,
}

#[derive(Clone)]
struct AppState {
    session: Arc<Mutex<Session>>,
}

struct SessionCredentials {
    agent_token: String,
    approval_token: String,
    write_mode: WriteMode,
    bootstrap_token: Option<String>,
}

pub struct TestServer {
    base_url: String,
    token: String,
    state: AppState,
    task: tokio::task::JoinHandle<()>,
    cleanup_root: Option<PathBuf>,
    approval_token: String,
}

impl TestServer {
    pub fn health_url(&self) -> String {
        format!("{}/health", self.base_url)
    }

    pub fn rpc_url(&self) -> String {
        format!("{}/rpc", self.base_url)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn agent_token(&self) -> &str {
        &self.token
    }

    pub fn approval_token(&self) -> &str {
        &self.approval_token
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

    pub fn workspace_root(&self) -> &Path {
        self.cleanup_root
            .as_deref()
            .expect("test server workspace should exist")
    }

    pub async fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.state
            .session
            .lock()
            .expect("session lock")
            .events
            .subscribe()
    }

    pub async fn set_toolchain_cli(&self, path: PathBuf) {
        let mut session = self.state.session.lock().expect("session lock");
        session.toolchain_cli = Some(path.clone());
        session.toolchain.reconfigure(path);
    }

    pub async fn stop(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
        self.cleanup_root = None;
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
        let history_event = serde_json::json!({
            "kind": "apply",
            "request": &request
        });
        let previous = session.document.clone();
        let result =
            apply_transaction(&mut session.document, request).map_err(|error| error.to_string())?;
        if let Err(error) = session
            .store
            .write_snapshot_with_history(&session.document, &history_event)
        {
            session.document = previous;
            return Err(error.to_string());
        }
        let _ = session.events.send(ServerEvent::RevisionCommitted {
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
    let approval_token = match config.write_mode {
        WriteMode::Autonomous => token.clone(),
        WriteMode::ReviewRequired => config
            .approval_token
            .unwrap_or_else(|| "test-approval-token".into()),
    };
    let bootstrap_token = Some("test-bootstrap-token".into());
    let viewer_dir = config.viewer_dir.clone();
    let (state, listener) = initialize_state(
        &cleanup_root,
        &config.project_id,
        SessionCredentials {
            agent_token: token.clone(),
            approval_token: approval_token.clone(),
            write_mode: config.write_mode,
            bootstrap_token,
        },
        config.fixture.as_deref(),
        config.port,
        config.toolchain_cli,
        config.compute_mode,
    )
    .await
    .expect("test server state");
    let address = listener.local_addr().expect("listener address");
    let router = build_router(state.clone(), viewer_dir);
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    TestServer {
        base_url: format!("http://{address}"),
        token,
        approval_token,
        state,
        task,
        cleanup_root: Some(cleanup_root),
    }
}

pub async fn start_test_server_at(root: &Path) -> TestServer {
    let config = ServerConfig::review_required("restart-project");
    std::fs::create_dir_all(root).expect("restart workspace");
    let token = config.token.expect("test agent token");
    let approval_token = config.approval_token.expect("test approval token");
    let (state, listener) = initialize_state(
        root,
        &config.project_id,
        SessionCredentials {
            agent_token: token.clone(),
            approval_token: approval_token.clone(),
            write_mode: config.write_mode,
            bootstrap_token: Some("test-bootstrap-token".into()),
        },
        None,
        config.port,
        config.toolchain_cli,
        config.compute_mode,
    )
    .await
    .expect("restart test server state");
    let address = listener.local_addr().expect("listener address");
    let task_state = state.clone();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, build_router(task_state, None)).await;
    });
    TestServer {
        base_url: format!("http://{address}"),
        token,
        approval_token,
        state,
        task,
        cleanup_root: None,
    }
}

pub async fn serve(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let token = config
        .token
        .or_else(|| std::env::var("STUDIO_SESSION_TOKEN").ok())
        .or_else(|| std::env::var("STUDIO_AGENT_TOKEN").ok())
        .unwrap_or_else(|| random_hex(32));
    let approval_token = config
        .approval_token
        .or_else(|| std::env::var("STUDIO_APPROVAL_TOKEN").ok())
        .unwrap_or_else(|| random_hex(32));
    let bootstrap_token = (config.write_mode == WriteMode::ReviewRequired).then(|| random_hex(32));
    let toolchain_cli = config
        .toolchain_cli
        .or_else(|| std::env::var_os("STUDIO_TOOLCHAIN_CLI").map(PathBuf::from));
    let viewer_dir = config
        .viewer_dir
        .or_else(|| std::env::var_os("STUDIO_VIEWER_DIR").map(PathBuf::from));
    let (state, listener) = initialize_state(
        &config.workspace_root,
        &config.project_id,
        SessionCredentials {
            agent_token: token.clone(),
            approval_token,
            write_mode: config.write_mode,
            bootstrap_token: bootstrap_token.clone(),
        },
        config.fixture.as_deref(),
        config.port,
        toolchain_cli,
        config.compute_mode,
    )
    .await?;
    println!("{}", listener.local_addr()?);
    println!("{token}");
    if let Some(bootstrap) = bootstrap_token {
        println!(
            "http://{}/session/bootstrap?token={bootstrap}",
            listener.local_addr()?
        );
    }
    axum::serve(listener, build_router(state, viewer_dir)).await?;
    Ok(())
}

async fn initialize_state(
    root: &std::path::Path,
    project_id: &str,
    credentials: SessionCredentials,
    fixture: Option<&std::path::Path>,
    port: u16,
    toolchain_cli: Option<PathBuf>,
    compute_mode: GeometryBackendMode,
) -> Result<(AppState, tokio::net::TcpListener), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(root)?;
    let path = WorkspacePath::new(root, project_id)?;
    let project_exists = path_project_exists(root, project_id);
    let store = if project_exists {
        ProjectStore::open(path)?
    } else {
        ProjectStore::create(path)?
    };
    let mut document = if project_exists {
        store.load_current()?
    } else {
        Document::new(project_id)
    };
    if project_exists {
        // Existing projects are already persisted.
    } else {
        store.write_snapshot(&document)?;
        if let Some(fixture) = fixture {
            seed_fixture(&mut document, &store, root, fixture)?;
        }
    }
    let (events, _) = broadcast::channel(64);
    let toolchain = Arc::new(ToolchainService::new_with_launch_root(
        store.project_dir(),
        root,
        toolchain_cli.clone(),
    ));
    let gpu: Arc<dyn GeometryBackend> = Arc::new(NativeGpuBackend::new(GeometryBackendMode::Gpu));
    let compute = Arc::new(ComputeService::new(
        crate::compute::ComputePolicy {
            mode: compute_mode,
            ..crate::compute::ComputePolicy::default()
        },
        (compute_mode != GeometryBackendMode::Cpu).then_some(gpu),
    ));
    let _ = pollster::block_on(compute.initialize());
    let state = AppState {
        session: Arc::new(Mutex::new(Session {
            workspace_root: root.to_path_buf(),
            project_id: project_id.into(),
            store,
            document,
            token: credentials.agent_token,
            events,
            closed: false,
            toolchain_cli,
            toolchain,
            compute,
            approval_token: credentials.approval_token,
            write_mode: credentials.write_mode,
            bootstrap_token: credentials.bootstrap_token,
            bootstrap_expires_at_ms: now_ms().saturating_add(60_000),
            bootstrap_consumed: false,
        })),
    };
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await?;
    Ok((state, listener))
}

fn path_project_exists(root: &std::path::Path, project_id: &str) -> bool {
    root.join(project_id).join("project.json").is_file()
}

fn build_router(state: AppState, viewer_dir: Option<PathBuf>) -> Router {
    let router = Router::new()
        .route("/health", get(health))
        .route("/session/bootstrap", get(bootstrap_session))
        .route(
            "/rpc",
            post(rpc).layer(DefaultBodyLimit::max(MAX_RPC_BODY_BYTES)),
        )
        .route("/ws", get(websocket))
        .route("/schemas/{name}", get(schema_file))
        .route("/previews/{revision}", get(preview_file))
        .route("/previews/candidates/{digest}", get(candidate_preview_file))
        .with_state(state);
    if let Some(viewer_dir) = viewer_dir {
        router
            .nest_service("/assets", ServeDir::new(viewer_dir.join("assets")))
            .fallback_service(ServeFile::new(viewer_dir.join("index.html")))
    } else {
        router
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "apiVersion": API_VERSION,
        "schemaVersion": PROJECT_SCHEMA_VERSION
    }))
}

async fn bootstrap_session(
    State(state): State<AppState>,
    Query(query): Query<BootstrapQuery>,
) -> Response {
    let cookie = {
        let mut session = state.session.lock().expect("session lock");
        let valid = !session.bootstrap_consumed
            && query.token.as_deref() == session.bootstrap_token.as_deref()
            && now_ms() <= session.bootstrap_expires_at_ms
            && session.write_mode == WriteMode::ReviewRequired;
        if !valid {
            return StatusCode::FORBIDDEN.into_response();
        }
        session.bootstrap_consumed = true;
        format!(
            "ice3_studio_approval={}; HttpOnly; SameSite=Strict; Path=/",
            session.approval_token
        )
    };
    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("valid cookie header"),
    );
    response
}

async fn rpc(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<RpcRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match request {
        Ok(request) => request,
        Err(error) => {
            let code = if matches!(error, JsonRejection::JsonSyntaxError(_)) {
                -32700
            } else {
                -32600
            };
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": code,
                        "message": format!("ICE3_STUDIO_API_INVALID: {error}"),
                        "diagnosticCode": "ICE3_STUDIO_API_INVALID"
                    }
                })),
            )
                .into_response();
        }
    };
    if request.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": {
                    "code": -32600,
                    "message": "ICE3_STUDIO_API_INVALID: jsonrpc must be 2.0",
                    "diagnosticCode": "ICE3_STUDIO_API_INVALID"
                }
            })),
        )
            .into_response();
    }
    let authorized = if mutating(&request.method) {
        let session = state.session.lock().expect("session lock");
        authorize_write(
            session.write_mode,
            &request.method,
            bearer_token(&headers),
            approval_cookie(&headers),
            &session.token,
            &session.approval_token,
        )
        .is_some()
    } else {
        true
    };
    if !authorized {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": {
                    "code": -32001,
                    "message": "ICE3_STUDIO_AUTH_REQUIRED",
                    "diagnosticCode": "ICE3_STUDIO_AUTH_REQUIRED"
                }
            })),
        )
            .into_response();
    }
    let request_id = request.id.clone();
    if let Some(result) = dispatch_unlocked_preview(&state, &request).await {
        let response = match result {
            Ok(result) => RpcResponse {
                jsonrpc: "2.0".into(),
                id: request_id,
                result: Some(result),
                error: None,
            },
            Err(error) => RpcResponse {
                jsonrpc: "2.0".into(),
                id: request_id,
                result: None,
                error: Some(error),
            },
        };
        return (StatusCode::OK, Json(response)).into_response();
    }
    let response = match tokio::task::spawn_blocking(move || {
        let mut session = state.session.lock().expect("session lock");
        dispatch(&mut session, &request)
    })
    .await
    {
        Ok(Ok(result)) => RpcResponse {
            jsonrpc: "2.0".into(),
            id: request_id,
            result: Some(result),
            error: None,
        },
        Ok(Err(error)) => RpcResponse {
            jsonrpc: "2.0".into(),
            id: request_id,
            result: None,
            error: Some(error),
        },
        Err(error) => RpcResponse {
            jsonrpc: "2.0".into(),
            id: request_id,
            result: None,
            error: Some(RpcError::with_diagnostic(
                -32603,
                DiagnosticCode::ApiInvalid.as_str(),
                format!("request worker failed: {error}"),
                None,
            )),
        },
    };
    (StatusCode::OK, Json(response)).into_response()
}

async fn dispatch_unlocked_preview(
    state: &AppState,
    request: &RpcRequest,
) -> Option<Result<serde_json::Value, RpcError>> {
    if request.method != "export.runtimePreview" {
        return None;
    }
    let params: RuntimePreviewParams = serde_json::from_value(request.params.clone()).ok()?;
    if let Some(digest) = params.candidate_digest {
        let prepared = {
            let mut session = state.session.lock().expect("session lock");
            if session.closed {
                return Some(Err(RpcError::with_diagnostic(
                    -32010,
                    DiagnosticCode::ProjectNotFound.as_str(),
                    "ICE3_STUDIO_PROJECT_NOT_FOUND: no project is open".into(),
                    None,
                )));
            }
            (
                crate::changeset::prepare_candidate_preview(&mut session, &digest),
                Arc::clone(&session.toolchain),
            )
        };
        let (prepared, toolchain) = prepared;
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return Some(Err(error)),
        };
        let result = tokio::task::spawn_blocking(move || {
            PreviewAdapter::from_service(toolchain).generate_candidate(
                &prepared.document,
                &prepared.project_dir,
                &prepared.digest,
            )
        })
        .await
        .unwrap_or_else(|error| {
            Err(crate::preview::PreviewError::Toolchain(
                studio_toolchain_client::ToolchainClientError::Crashed {
                    message: format!("preview worker failed: {error}"),
                },
            ))
        });
        return Some(match result {
            Ok(preview) => serde_json::to_value(preview).map_err(serialize_error),
            Err(error) => {
                let mut session = state.session.lock().expect("session lock");
                if let Err(record_error) = crate::changeset::record_candidate_preview_failure(
                    &mut session,
                    &digest,
                    error.to_string(),
                ) {
                    return Some(Err(record_error));
                }
                Err(preview_error(error))
            }
        });
    }
    let (document, project_dir, project_id, toolchain, closed) = {
        let session = state.session.lock().expect("session lock");
        (
            session.document.clone(),
            session.store.project_dir().to_path_buf(),
            session.project_id.clone(),
            Arc::clone(&session.toolchain),
            session.closed,
        )
    };
    if closed {
        return Some(Err(RpcError::with_diagnostic(
            -32010,
            DiagnosticCode::ProjectNotFound.as_str(),
            "ICE3_STUDIO_PROJECT_NOT_FOUND: no project is open".into(),
            None,
        )));
    }
    Some(
        tokio::task::spawn_blocking(move || {
            PreviewAdapter::from_service(toolchain)
                .generate(&document, &project_dir, &project_id)
                .map_err(preview_error)
                .and_then(|preview| serde_json::to_value(preview).map_err(serialize_error))
        })
        .await
        .unwrap_or_else(|error| {
            Err(RpcError::with_diagnostic(
                -32603,
                DiagnosticCode::RuntimePreviewFailed.as_str(),
                format!("preview worker failed: {error}"),
                None,
            ))
        }),
    )
}

fn dispatch(session: &mut Session, request: &RpcRequest) -> Result<serde_json::Value, RpcError> {
    if session.closed
        && !matches!(
            request.method.as_str(),
            "system.health" | "system.capabilities" | "project.create" | "project.open"
        )
    {
        return Err(RpcError::with_diagnostic(
            -32010,
            DiagnosticCode::ProjectNotFound.as_str(),
            "ICE3_STUDIO_PROJECT_NOT_FOUND: no project is open".into(),
            None,
        ));
    }
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
                "evaluation.stats".into(),
                "export.sourceGlb".into(),
                "export.runtimePreview".into(),
                "changeset.create".into(),
                "changeset.list".into(),
                "changeset.get".into(),
                "changeset.addOperations".into(),
                "changeset.removeOperations".into(),
                "changeset.evaluate".into(),
                "changeset.getCandidate".into(),
                "changeset.commit".into(),
                "changeset.discard".into(),
            ],
            toolchain: session.toolchain.status(),
            compute: session.compute.status(),
        })
        .expect("serializable capabilities")),
        "project.get" | "document.get" => {
            Ok(serde_json::to_value(&session.document).map_err(serialize_error)?)
        }
        "document.apply" => {
            let apply: ApplyRequest =
                serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
            let history_event = serde_json::json!({
                "kind": "apply",
                "request": &apply
            });
            let previous = session.document.clone();
            let result =
                apply_transaction(&mut session.document, apply).map_err(transaction_error)?;
            if !result.dry_run {
                if let Err(error) = session
                    .store
                    .write_snapshot_with_history(&session.document, &history_event)
                {
                    session.document = previous;
                    return Err(project_error(error));
                }
                let _ = session.events.send(ServerEvent::RevisionCommitted {
                    revision: result.revision,
                });
            }
            serde_json::to_value(result).map_err(serialize_error)
        }
        "document.undo" => {
            session.document = session.store.undo().map_err(project_error)?;
            let event = ServerEvent::RevisionCommitted {
                revision: session.document.revision,
            };
            let _ = session.events.send(event);
            serde_json::to_value(&session.document).map_err(serialize_error)
        }
        "document.redo" => {
            session.document = session.store.redo().map_err(project_error)?;
            let event = ServerEvent::RevisionCommitted {
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
        "project.close" => {
            session.store.close();
            session.closed = true;
            Ok(serde_json::json!({ "closed": true }))
        }
        "project.create" => {
            let params: ProjectParams =
                serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
            let path = WorkspacePath::new(&session.workspace_root, &params.project_id).map_err(
                |error| {
                    RpcError::with_diagnostic(
                        -32010,
                        DiagnosticCode::ApiInvalid.as_str(),
                        format!("ICE3_STUDIO_API_INVALID: {error}"),
                        None,
                    )
                },
            )?;
            let project_dir = path.project_dir().to_path_buf();
            let store = ProjectStore::create(path).map_err(project_error)?;
            let document = Document::new(params.project_id.clone());
            store.write_snapshot(&document).map_err(project_error)?;
            let toolchain = Arc::new(ToolchainService::new_with_launch_root(
                &project_dir,
                &session.workspace_root,
                session.toolchain_cli.clone(),
            ));
            session.toolchain.shutdown();
            session.store.close();
            session.project_id = params.project_id;
            session.document = document;
            session.store = store;
            session.toolchain = toolchain;
            session.closed = false;
            Ok(serde_json::json!({ "opened": true }))
        }
        "project.open" => {
            let params: ProjectParams =
                serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
            let path = WorkspacePath::new(&session.workspace_root, &params.project_id).map_err(
                |error| {
                    RpcError::with_diagnostic(
                        -32010,
                        DiagnosticCode::ApiInvalid.as_str(),
                        format!("ICE3_STUDIO_API_INVALID: {error}"),
                        None,
                    )
                },
            )?;
            let same_project = session.store.project_dir() == path.project_dir();
            let project_dir = path.project_dir().to_path_buf();
            if same_project {
                session.store.close();
                session.closed = true;
            }
            let store = ProjectStore::open(path).map_err(project_error)?;
            let document = store.load_current().map_err(project_error)?;
            let toolchain = Arc::new(ToolchainService::new_with_launch_root(
                &project_dir,
                &session.workspace_root,
                session.toolchain_cli.clone(),
            ));
            if !same_project {
                session.store.close();
            }
            session.toolchain.shutdown();
            session.project_id = params.project_id;
            session.document = document;
            session.store = store;
            session.toolchain = toolchain;
            session.closed = false;
            Ok(serde_json::json!({ "opened": true }))
        }
        "scene.get" => serde_json::to_value(&session.document.scene).map_err(serialize_error),
        "scene.query" => {
            let query: SceneQuery =
                serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
            let (_, bounds_by_id) = evaluate_session_scene(session)?;
            let relation_nodes = if query.relation_type.is_some() || query.related_node_id.is_some()
            {
                let mut nodes = HashSet::new();
                for relation in &session.document.relations {
                    if query
                        .relation_type
                        .as_ref()
                        .is_some_and(|kind| relation.relation_type != *kind)
                    {
                        continue;
                    }
                    if query.related_node_id.as_ref().is_some_and(|node_id| {
                        relation.from_node_id != *node_id && relation.to_node_id != *node_id
                    }) {
                        continue;
                    }
                    nodes.insert(relation.from_node_id.clone());
                    nodes.insert(relation.to_node_id.clone());
                }
                Some(nodes)
            } else {
                None
            };
            let nodes = session
                .document
                .scene
                .nodes()
                .iter()
                .filter(|node| {
                    query
                        .node_id
                        .as_ref()
                        .is_none_or(|node_id| node.id == *node_id)
                })
                .filter(|node| {
                    query
                        .parent_id
                        .as_ref()
                        .is_none_or(|parent| node.parent_id.as_ref() == Some(parent))
                })
                .filter(|node| {
                    query
                        .metadata
                        .iter()
                        .all(|(key, value)| node.metadata.get(key) == Some(value))
                })
                .filter(|node| {
                    relation_nodes
                        .as_ref()
                        .is_none_or(|nodes| nodes.contains(&node.id))
                })
                .filter(|node| {
                    query.bounds.is_none_or(|bounds| {
                        bounds_by_id
                            .get(node.id.as_str())
                            .copied()
                            .is_some_and(|candidate| bounds_intersect(candidate, bounds))
                    })
                })
                .collect::<Vec<_>>();
            serde_json::to_value(nodes).map_err(serialize_error)
        }
        "scene.bounds" => {
            let (_, output) = evaluate_session_scene(session)?;
            serde_json::to_value(output).map_err(serialize_error)
        }
        "validation.run" => {
            serde_json::to_value(validate_document(&session.document)).map_err(serialize_error)
        }
        "evaluation.evaluate" => {
            let (evaluated, _) = evaluate_session_scene(session)?;
            let mut output = BTreeMap::new();
            for node in evaluated.nodes {
                if let Some(mesh_key) = node.mesh_key
                    && let Some(mesh) = evaluated.meshes.get(&mesh_key)
                {
                    output.insert(node.id, mesh.clone());
                }
            }
            serde_json::to_value(output).map_err(serialize_error)
        }
        "evaluation.stats" => {
            let (evaluated, _) = evaluate_session_scene(session)?;
            serde_json::to_value(EvaluatedSceneStats {
                source_nodes: evaluated.stats.source_nodes,
                expanded_nodes: evaluated.stats.expanded_nodes,
                unique_meshes: evaluated.stats.unique_meshes,
                effective_triangles: evaluated.stats.effective_triangles,
                unique_triangles: evaluated.stats.unique_triangles,
                cache_hits: evaluated.stats.cache_hits,
                cache_misses: evaluated.stats.cache_misses,
                generated_meshes: evaluated.stats.generated_meshes,
            })
            .map_err(serialize_error)
        }
        "export.sourceGlb" => {
            let output = export_source(&session.document, session.store.project_dir())
                .map_err(preview_error)?;
            serde_json::to_value(output).map_err(serialize_error)
        }
        "export.runtimePreview" => {
            let params: RuntimePreviewParams =
                serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
            if let Some(digest) = params.candidate_digest {
                return crate::changeset::export_candidate_preview(session, &digest)
                    .and_then(|preview| serde_json::to_value(preview).map_err(serialize_error));
            }
            let adapter = PreviewAdapter::from_service(Arc::clone(&session.toolchain));
            let output = adapter
                .generate(
                    &session.document,
                    session.store.project_dir(),
                    &session.project_id,
                )
                .map_err(preview_error)?;
            serde_json::to_value(output).map_err(serialize_error)
        }
        method
            if matches!(
                method,
                "changeset.create"
                    | "changeset.list"
                    | "changeset.get"
                    | "changeset.addOperations"
                    | "changeset.removeOperations"
                    | "changeset.evaluate"
                    | "changeset.getCandidate"
                    | "changeset.commit"
                    | "changeset.discard"
            ) =>
        {
            crate::changeset::dispatch(session, method, request.params.clone())
        }
        method => Err(RpcError::with_diagnostic(
            -32601,
            DiagnosticCode::ApiInvalid.as_str(),
            format!("ICE3_STUDIO_API_INVALID: unknown method: {method}"),
            None,
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
                || bearer_token(&headers) == Some(session.token.as_str()),
            session.events.subscribe(),
        )
    };
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    upgrade.on_upgrade(move |socket| websocket_loop(socket, events))
}

async fn websocket_loop(mut socket: WebSocket, mut events: broadcast::Receiver<ServerEvent>) {
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

async fn preview_file(
    State(state): State<AppState>,
    AxumPath(revision): AxumPath<u64>,
) -> Response {
    let path = {
        let session = state.session.lock().expect("session lock");
        session
            .store
            .project_dir()
            .join("cache/toolchain/preview")
            .join(format!("{revision:08}.glb"))
    };
    match tokio::fs::read(path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "model/gltf-binary")],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn candidate_preview_file(
    State(state): State<AppState>,
    AxumPath(digest): AxumPath<String>,
) -> Response {
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = {
        let session = state.session.lock().expect("session lock");
        session
            .store
            .project_dir()
            .join("cache/toolchain/candidates")
            .join(format!("{digest}.glb"))
    };
    match tokio::fs::read(path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "model/gltf-binary")],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn seed_fixture(
    document: &mut Document,
    store: &ProjectStore,
    workspace_root: &Path,
    fixture: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = workspace_root.canonicalize()?;
    let fixture = if fixture.is_dir() {
        fixture.join("ai-plan.json")
    } else {
        fixture.to_path_buf()
    }
    .canonicalize()?;
    if !fixture.starts_with(&root) {
        return Err("fixture must be inside the configured workspace".into());
    }
    let plan: FixturePlan = serde_json::from_slice(&std::fs::read(fixture)?)?;
    for request in plan.transactions {
        let dry_run = request.dry_run;
        let history_event = serde_json::json!({
            "kind": "apply",
            "request": &request
        });
        match apply_transaction(document, request) {
            Ok(_) if dry_run => {}
            Ok(_) => store.write_snapshot_with_history(document, &history_event)?,
            Err(_) if dry_run => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct FixturePlan {
    transactions: Vec<ApplyRequest>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpToken {
    token: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct BootstrapQuery {
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectParams {
    project_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimePreviewParams {
    #[serde(default)]
    #[serde(rename = "revision")]
    _revision: Option<u64>,
    candidate_digest: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SceneQuery {
    node_id: Option<NodeId>,
    parent_id: Option<NodeId>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
    relation_type: Option<String>,
    related_node_id: Option<NodeId>,
    bounds: Option<Bounds>,
}

fn bounds_intersect(left: Bounds, right: Bounds) -> bool {
    left.min.x <= right.max.x
        && left.max.x >= right.min.x
        && left.min.y <= right.max.y
        && left.max.y >= right.min.y
        && left.min.z <= right.max.z
        && left.max.z >= right.min.z
}

fn evaluate_session_scene(
    session: &Session,
) -> Result<(EvaluatedScene, BTreeMap<String, Bounds>), RpcError> {
    let mut cache = GeometryDiskCache::new(session.store.project_dir()).map_err(|error| {
        RpcError::with_diagnostic(
            -32012,
            DiagnosticCode::GeometryCacheFailed.as_str(),
            format!("{}: {error}", DiagnosticCode::GeometryCacheFailed.as_str()),
            None,
        )
    })?;
    let evaluator = ComputeEvaluator::new(&session.compute);
    let evaluated = evaluate_large_scene_with_cache_and_evaluator(
        &session.document,
        &mut cache,
        Some(&evaluator),
    )
    .map_err(large_scene_error)?;
    session
        .compute
        .record_cache_hits(evaluated.stats.cache_hits);
    let bounds = evaluated_scene_bounds(&evaluated);
    Ok((evaluated, bounds))
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
            | "changeset.create"
            | "changeset.addOperations"
            | "changeset.removeOperations"
            | "changeset.evaluate"
            | "changeset.getCandidate"
            | "changeset.commit"
            | "changeset.discard"
    )
}

fn random_hex(bytes: usize) -> String {
    let mut data = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut data);
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn invalid_params(error: serde_json::Error) -> RpcError {
    RpcError::with_diagnostic(
        -32602,
        "ICE3_STUDIO_API_INVALID",
        format!("ICE3_STUDIO_API_INVALID: {error}"),
        None,
    )
}

fn serialize_error(error: serde_json::Error) -> RpcError {
    RpcError::with_diagnostic(
        -32603,
        "ICE3_STUDIO_API_INVALID",
        format!("ICE3_STUDIO_API_INVALID: {error}"),
        None,
    )
}

pub(crate) fn transaction_error(error: studio_core::apply::TransactionError) -> RpcError {
    let (diagnostic_code, data) = match &error {
        studio_core::apply::TransactionError::RevisionConflict { .. } => {
            (DiagnosticCode::RevisionConflict, None)
        }
        studio_core::apply::TransactionError::ValidationFailed(diagnostics) => {
            let code = diagnostics
                .first()
                .map(|diagnostic| diagnostic.code.clone())
                .unwrap_or(DiagnosticCode::OperationInvalid);
            (code, serde_json::to_value(diagnostics).ok())
        }
        studio_core::apply::TransactionError::OperationInvalid {
            operation_index,
            code,
            object_id,
            pointer,
            message,
        } => (
            code.clone(),
            Some(serde_json::json!({
                "operationIndex": operation_index,
                "objectId": object_id,
                "pointer": pointer,
                "message": message
            })),
        ),
        studio_core::apply::TransactionError::EmptyTransaction
        | studio_core::apply::TransactionError::IdempotencyKeyReused => {
            (DiagnosticCode::OperationInvalid, None)
        }
        studio_core::apply::TransactionError::Serialization(_) => {
            (DiagnosticCode::ApiInvalid, None)
        }
    };
    RpcError::with_diagnostic(
        -32010,
        diagnostic_code.as_str(),
        format!("{}: {error}", diagnostic_code.as_str()),
        data,
    )
}

pub(crate) fn project_error(error: studio_project::ProjectError) -> RpcError {
    let (code, diagnostic) = match &error {
        studio_project::ProjectError::Missing => (-32010, DiagnosticCode::ProjectNotFound),
        studio_project::ProjectError::Locked => (-32011, DiagnosticCode::ProjectLocked),
        _ => (-32011, DiagnosticCode::PersistenceFailed),
    };
    RpcError::with_diagnostic(
        code,
        diagnostic.as_str(),
        format!("{}: {error}", diagnostic.as_str()),
        None,
    )
}

fn large_scene_error(error: studio_core::large_scene::evaluate::LargeSceneError) -> RpcError {
    use studio_core::large_scene::evaluate::LargeSceneError;

    let code = match &error {
        LargeSceneError::Parameter(_) => DiagnosticCode::ParameterInvalid,
        LargeSceneError::Connection(_) => DiagnosticCode::ConnectionInvalid,
        LargeSceneError::Layout(_) => DiagnosticCode::InstanceSetInvalid,
        LargeSceneError::Geometry(_) => DiagnosticCode::GeometryInvalid,
        LargeSceneError::InvalidComponentReference => DiagnosticCode::ComponentInvalid,
        LargeSceneError::NodeNotFound(_) => DiagnosticCode::ReferenceInvalid,
        LargeSceneError::Backend(error) => match error {
            studio_core::gpu_contract::BackendError::Unavailable { .. } => {
                DiagnosticCode::GpuUnavailable
            }
            studio_core::gpu_contract::BackendError::Incompatible { .. } => {
                DiagnosticCode::GpuIncompatible
            }
            studio_core::gpu_contract::BackendError::BudgetExceeded => {
                DiagnosticCode::GpuBudgetExceeded
            }
            studio_core::gpu_contract::BackendError::Kernel { .. } => {
                DiagnosticCode::GpuKernelFailed
            }
            studio_core::gpu_contract::BackendError::DeviceLost { .. } => {
                DiagnosticCode::GpuDeviceLost
            }
            studio_core::gpu_contract::BackendError::Parity => DiagnosticCode::GpuParityFailed,
        },
    };
    RpcError::with_diagnostic(
        -32012,
        code.as_str(),
        format!("{}: {error}", code.as_str()),
        None,
    )
}

fn preview_error(error: crate::preview::PreviewError) -> RpcError {
    let diagnostic = match &error {
        crate::preview::PreviewError::Export(_) => DiagnosticCode::ExportFailed,
        crate::preview::PreviewError::Toolchain(
            studio_toolchain_client::ToolchainClientError::Unavailable { .. },
        ) => DiagnosticCode::ToolchainUnavailable,
        crate::preview::PreviewError::Toolchain(
            studio_toolchain_client::ToolchainClientError::Incompatible { .. },
        ) => DiagnosticCode::ToolchainIncompatible,
        crate::preview::PreviewError::Toolchain(
            studio_toolchain_client::ToolchainClientError::Timeout { .. },
        ) => DiagnosticCode::ToolchainTimeout,
        crate::preview::PreviewError::Toolchain(
            studio_toolchain_client::ToolchainClientError::Protocol { .. }
            | studio_toolchain_client::ToolchainClientError::Crashed { .. },
        ) => DiagnosticCode::ToolchainProtocolInvalid,
        crate::preview::PreviewError::Toolchain(
            studio_toolchain_client::ToolchainClientError::Rejected { .. },
        ) => DiagnosticCode::ToolchainJobFailed,
        _ => DiagnosticCode::RuntimePreviewFailed,
    };
    RpcError::with_diagnostic(
        -32013,
        diagnostic.as_str(),
        format!("{}: {error}", diagnostic.as_str()),
        None,
    )
}

impl RpcError {
    pub(crate) fn with_diagnostic(
        code: i64,
        diagnostic_code: &str,
        message: String,
        data: Option<serde_json::Value>,
    ) -> Self {
        Self {
            code,
            message,
            diagnostic_code: Some(diagnostic_code.into()),
            data,
        }
    }
}
