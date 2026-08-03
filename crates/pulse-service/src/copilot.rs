//! Service-owned registry for the Copilot's read-only task tools.
//!
//! A tool's model-visible definition, progress label, access policy, and
//! execution handler live together here. The agent loop consumes this registry
//! instead of carrying a second list in its prompt or dispatch match.

use pulse_core::{Store, Task, TaskStatus};
use pulse_llm::{TaskCopilotTask, TaskCopilotToolDefinition};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopilotToolAccess {
    ReadOnly,
    ConfirmedWrite,
}

pub struct CopilotToolExecution {
    pub payload: Value,
    pub tasks: Vec<Task>,
}

type ToolHandler = fn(&Store, &Value) -> Result<CopilotToolExecution, String>;

struct RegisteredCopilotTool {
    definition: TaskCopilotToolDefinition,
    access: CopilotToolAccess,
    progress_label: &'static str,
    handler: ToolHandler,
}

pub struct CopilotToolRegistry {
    tools: Vec<RegisteredCopilotTool>,
}

impl CopilotToolRegistry {
    pub fn read_only_tasks() -> Self {
        Self {
            tools: vec![
                RegisteredCopilotTool {
                    definition: TaskCopilotToolDefinition {
                        name: "list_tasks".into(),
                        description: "List recently updated tasks, optionally restricted to one workflow status.".into(),
                        input_schema: json!({
                            "type": "object",
                            "properties": {
                                "status": { "type": "string", "enum": ["Inbox", "Today", "Next", "Waiting", "Done"] },
                                "limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                            }
                        }),
                    },
                    access: CopilotToolAccess::ReadOnly,
                    progress_label: "Looking through tasks",
                    handler: list_tasks,
                },
                RegisteredCopilotTool {
                    definition: TaskCopilotToolDefinition {
                        name: "search_tasks".into(),
                        description: "Search task titles, notes, projects, and suggested next actions for literal keywords.".into(),
                        input_schema: json!({
                            "type": "object",
                            "required": ["query"],
                            "properties": {
                                "query": { "type": "string", "minLength": 1 },
                                "status": { "type": "string", "enum": ["Inbox", "Today", "Next", "Waiting", "Done"] },
                                "limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                            }
                        }),
                    },
                    access: CopilotToolAccess::ReadOnly,
                    progress_label: "Searching task text",
                    handler: search_tasks,
                },
                RegisteredCopilotTool {
                    definition: TaskCopilotToolDefinition {
                        name: "get_task".into(),
                        description: "Get the current fields for one task by its exact id from a prior tool result.".into(),
                        input_schema: json!({
                            "type": "object",
                            "required": ["id"],
                            "properties": { "id": { "type": "string" } }
                        }),
                    },
                    access: CopilotToolAccess::ReadOnly,
                    progress_label: "Opening task context",
                    handler: get_task,
                },
            ],
        }
    }

    pub fn definitions(&self) -> Vec<TaskCopilotToolDefinition> {
        self.tools.iter().map(|tool| tool.definition.clone()).collect()
    }

    pub fn progress_label(&self, name: &str) -> &'static str {
        self.tools.iter().find(|tool| tool.definition.name == name)
            .map(|tool| tool.progress_label)
            .unwrap_or("Checking a registered tool")
    }

    pub fn execute(&self, store: &Store, name: &str, arguments: &Value) -> Result<CopilotToolExecution, String> {
        let tool = self.tools.iter().find(|tool| tool.definition.name == name)
            .ok_or("unknown copilot tool")?;
        if tool.access != CopilotToolAccess::ReadOnly {
            return Err("this tool requires explicit confirmation".into());
        }
        (tool.handler)(store, arguments)
    }
}

fn list_tasks(store: &Store, arguments: &Value) -> Result<CopilotToolExecution, String> {
    let tasks = store.list_tasks(parse_status(arguments)?).map_err(|error| error.to_string())?
        .into_iter().take(parse_limit(arguments)).collect::<Vec<_>>();
    Ok(CopilotToolExecution {
        payload: json!({ "tasks": tasks.iter().map(task_view).collect::<Vec<_>>() }),
        tasks,
    })
}

fn search_tasks(store: &Store, arguments: &Value) -> Result<CopilotToolExecution, String> {
    let query = arguments.get("query").and_then(Value::as_str).map(str::trim)
        .filter(|query| !query.is_empty()).ok_or("search_tasks requires a query")?;
    let needle = query.to_ascii_lowercase();
    let tasks = store.list_tasks(parse_status(arguments)?).map_err(|error| error.to_string())?
        .into_iter().filter(|task| task_matches(task, &needle))
        .take(parse_limit(arguments)).collect::<Vec<_>>();
    Ok(CopilotToolExecution {
        payload: json!({ "tasks": tasks.iter().map(task_view).collect::<Vec<_>>() }),
        tasks,
    })
}

fn get_task(store: &Store, arguments: &Value) -> Result<CopilotToolExecution, String> {
    let id = arguments.get("id").and_then(Value::as_str).ok_or("get_task requires an id")?;
    let task = store.resolve_task(id).map_err(|error| error.to_string())?;
    Ok(CopilotToolExecution {
        payload: json!({ "task": task_view(&task) }),
        tasks: vec![task],
    })
}

fn parse_limit(arguments: &Value) -> usize {
    arguments.get("limit").and_then(Value::as_u64).unwrap_or(10).clamp(1, 20) as usize
}

fn parse_status(arguments: &Value) -> Result<Option<TaskStatus>, String> {
    match arguments.get("status").and_then(Value::as_str) {
        Some(status) => Ok(Some(TaskStatus::parse(status).ok_or("invalid task status")?)),
        None => Ok(None),
    }
}

fn task_view(task: &Task) -> TaskCopilotTask {
    TaskCopilotTask {
        id: task.id.to_string(),
        title: task.title.clone(),
        status: task.status.as_str().to_string(),
        notes: task.notes.clone(),
        suggested_next_action: task.suggested_next_action.clone(),
        project: task.project.clone(),
        sync_outcome: task.sync_outcome.map(|outcome| outcome.as_str().to_string()),
        updated_at: task.updated_at.to_rfc3339(),
    }
}

fn task_matches(task: &Task, needle: &str) -> bool {
    [Some(task.title.as_str()), task.notes.as_deref(), task.project.as_deref(), task.suggested_next_action.as_deref()]
        .into_iter().flatten().any(|field| field.to_ascii_lowercase().contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_exposes_all_registered_tool_schemas() {
        let registry = CopilotToolRegistry::read_only_tasks();
        let definitions = registry.definitions();
        assert_eq!(definitions.len(), 3);
        assert!(definitions.iter().all(|tool| tool.input_schema.is_object()));
        assert_eq!(registry.progress_label("search_tasks"), "Searching task text");
    }
}
