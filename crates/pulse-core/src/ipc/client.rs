//! High-level IPC client used by the CLI.

use std::time::Duration;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{PulseError, Result};
use crate::ipc::pipe;
use crate::ipc::rpc::call;
use crate::models::{Evidence, Task, TaskStatus};

pub struct IpcClient {
    stream: std::fs::File,
}

impl IpcClient {
    pub fn connect(pipe_name: &str, timeout: Duration) -> Result<Self> {
        let stream = pipe::connect(pipe_name, timeout)?;
        Ok(Self { stream })
    }

    pub fn ping(&mut self) -> Result<Value> {
        call(&mut self.stream, "ping", json!({}))
    }

    pub fn tasks_list(&mut self, status: Option<TaskStatus>) -> Result<Vec<Task>> {
        let params = match status {
            Some(s) => json!({ "status": [s] }),
            None => json!({}),
        };
        let result = call(&mut self.stream, "tasks.list", params)?;
        let tasks = result
            .get("tasks")
            .cloned()
            .ok_or_else(|| PulseError::Ipc("tasks.list missing tasks".into()))?;
        serde_json::from_value(tasks).map_err(|e| PulseError::Ipc(format!("decode tasks: {e}")))
    }

    pub fn tasks_get(&mut self, id: &str) -> Result<(Task, Vec<Evidence>)> {
        let result = call(&mut self.stream, "tasks.get", json!({ "id": id }))?;
        let task = serde_json::from_value(
            result
                .get("task")
                .cloned()
                .ok_or_else(|| PulseError::Ipc("tasks.get missing task".into()))?,
        )
        .map_err(|e| PulseError::Ipc(format!("decode task: {e}")))?;
        let evidence = result
            .get("evidence")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let evidence: Vec<Evidence> = serde_json::from_value(evidence)
            .map_err(|e| PulseError::Ipc(format!("decode evidence: {e}")))?;
        Ok((task, evidence))
    }

    pub fn tasks_create(
        &mut self,
        title: &str,
        status: Option<TaskStatus>,
        notes: Option<String>,
    ) -> Result<Task> {
        let mut params = json!({ "title": title });
        if let Some(s) = status {
            params["status"] = json!(s);
        }
        if let Some(n) = notes {
            params["notes"] = json!(n);
        }
        let result = call(&mut self.stream, "tasks.create", params)?;
        decode_task(result)
    }

    pub fn tasks_update(
        &mut self,
        id: &str,
        title: Option<String>,
        status: Option<TaskStatus>,
        notes: Option<String>,
    ) -> Result<Task> {
        let mut params = json!({ "id": id });
        if let Some(t) = title {
            params["title"] = json!(t);
        }
        if let Some(s) = status {
            params["status"] = json!(s);
        }
        if let Some(n) = notes {
            params["notes"] = json!(n);
        }
        let result = call(&mut self.stream, "tasks.update", params)?;
        decode_task(result)
    }

    pub fn tasks_done(&mut self, id: &str) -> Result<Task> {
        let result = call(&mut self.stream, "tasks.done", json!({ "id": id }))?;
        decode_task(result)
    }

    pub fn service_status(&mut self) -> Result<Value> {
        call(&mut self.stream, "service.status", json!({}))
    }

    pub fn service_shutdown(&mut self) -> Result<()> {
        call(&mut self.stream, "service.shutdown", json!({}))?;
        Ok(())
    }

    pub fn config_reload(&mut self) -> Result<()> {
        call(&mut self.stream, "config.reload", json!({}))?;
        Ok(())
    }

    pub fn sources_list(&mut self) -> Result<Value> {
        call(&mut self.stream, "sources.list", json!({}))
    }

    pub fn sources_set_enabled(&mut self, id: &str, enabled: bool) -> Result<Value> {
        call(
            &mut self.stream,
            "sources.set_enabled",
            json!({ "id": id, "enabled": enabled }),
        )
    }

    pub fn inference_run_once(&mut self) -> Result<Value> {
        call(&mut self.stream, "inference.run_once", json!({}))
    }
}

fn decode_task(result: Value) -> Result<Task> {
    let task = result
        .get("task")
        .cloned()
        .ok_or_else(|| PulseError::Ipc("response missing task".into()))?;
    serde_json::from_value(task).map_err(|e| PulseError::Ipc(format!("decode task: {e}")))
}

/// Try connect; map common failures.
pub fn try_connect(pipe_name: &str) -> Result<IpcClient> {
    IpcClient::connect(pipe_name, Duration::from_millis(500))
}

/// Helper for tests / callers that already know a UUID.
pub fn id_str(id: Uuid) -> String {
    id.to_string()
}
