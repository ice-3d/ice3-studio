//! Local API server for ICE3 Studio.

mod agent;
mod auth;
pub mod capabilities;
pub mod changeset;
pub mod compute;
mod events;
mod iteration_impact;
pub mod preview;
pub mod preview_packet;
pub mod reference_asset;
mod rpc;
pub mod runtime_tileset;
pub mod scene_session;

pub use events::ServerEvent;
pub use rpc::{
    Capabilities, EvaluatedSceneStats, RpcError, RpcRequest, RpcResponse, ServerConfig, TestServer,
    serve, start_test_server, start_test_server_at,
};
