//! Local API server for ICE3 Studio.

mod auth;
pub mod capabilities;
mod changeset;
pub mod compute;
mod events;
pub mod preview;
mod rpc;
pub mod toolchain;

pub use events::ServerEvent;
pub use rpc::{
    Capabilities, EvaluatedSceneStats, RpcError, RpcRequest, RpcResponse, ServerConfig, TestServer,
    serve, start_test_server, start_test_server_at,
};
