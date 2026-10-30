//! Local API server for ICE3 Studio.

mod auth;
mod events;
pub mod preview;
mod rpc;

pub use events::RevisionEvent;
pub use rpc::{
    Capabilities, RpcError, RpcRequest, RpcResponse, ServerConfig, TestServer, serve,
    start_test_server,
};
