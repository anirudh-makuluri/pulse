//! JSON-RPC 2.0 types for Pulse IPC.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const JSONRPC_VERSION: &str = "2.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Request {
    pub fn new(id: impl Into<Value>, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.into(),
            id: id.into(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcErrorObject>,
}

impl Response {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(id: Value, error: RpcErrorObject) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcErrorObject {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// JSON-RPC / Pulse application error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum RpcCode {
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
    TaskNotFound = -32001,
    InvalidTransition = -32002,
    CheckInNotFound = -32003,
    ServiceBusy = -32004,
    ConfigError = -32005,
    Unavailable = -32006,
}

impl RpcCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl RpcErrorObject {
    pub fn new(code: RpcCode, message: impl Into<String>) -> Self {
        Self {
            code: code.as_i32(),
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(code: RpcCode, message: impl Into<String>, data: Value) -> Self {
        Self {
            code: code.as_i32(),
            message: message.into(),
            data: Some(data),
        }
    }
}

/// Handler implemented by the service.
pub trait RpcHandler: Send + Sync {
    fn handle(&self, method: &str, params: Value) -> std::result::Result<Value, RpcErrorObject>;
}

/// Serve one request/response cycle on a connected stream.
pub fn serve_one<H: RpcHandler, S: std::io::Read + std::io::Write>(
    handler: &H,
    stream: &mut S,
) -> crate::error::Result<()> {
    use super::frame::{read_frame, write_json};

    let body = match read_frame(stream) {
        Ok(b) => b,
        Err(e) => {
            // Cannot form a proper JSON-RPC response without an id.
            return Err(e);
        }
    };

    let req: Request = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            let resp = Response::failure(
                Value::Null,
                RpcErrorObject::new(RpcCode::ParseError, format!("parse error: {e}")),
            );
            write_json(stream, &resp)?;
            return Ok(());
        }
    };

    if req.jsonrpc != JSONRPC_VERSION {
        let resp = Response::failure(
            req.id,
            RpcErrorObject::new(RpcCode::InvalidRequest, "jsonrpc must be \"2.0\""),
        );
        write_json(stream, &resp)?;
        return Ok(());
    }

    let resp = match handler.handle(&req.method, req.params) {
        Ok(result) => Response::success(req.id, result),
        Err(err) => Response::failure(req.id, err),
    };
    write_json(stream, &resp)?;
    Ok(())
}

/// Call a method and return the result value (or error).
pub fn call<S: std::io::Read + std::io::Write>(
    stream: &mut S,
    method: &str,
    params: Value,
) -> crate::error::Result<Value> {
    use super::frame::{read_json, write_json};
    use crate::error::PulseError;

    let req = Request::new(1, method, params);
    write_json(stream, &req)?;
    let resp: Response = read_json(stream)?;
    if let Some(err) = resp.error {
        return Err(PulseError::Ipc(format!(
            "rpc {}: {} ({})",
            method, err.message, err.code
        )));
    }
    resp.result
        .ok_or_else(|| PulseError::Ipc("rpc response missing result".into()))
}
