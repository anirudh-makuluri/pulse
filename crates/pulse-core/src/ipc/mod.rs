//! IPC framing, JSON-RPC, named pipes, and PID helpers.

pub mod client;
pub mod frame;
pub mod pid;
pub mod pipe;
pub mod rpc;

pub use client::{try_connect, IpcClient};
pub use pid::{
    live_service_pid, process_is_live, read_pid_file, remove_pid_file_if_matches, write_pid_file,
    ServicePidFile,
};
pub use rpc::{call, serve_one, Request, Response, RpcCode, RpcErrorObject, RpcHandler};
