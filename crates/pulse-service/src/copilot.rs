//! Service-owned registry for the Copilot's bounded task tools.
//!
//! A tool's model-visible definition, progress label, access policy, and
//! execution handler live together here. The agent loop consumes this registry
//! instead of carrying a second list in its prompt or dispatch match.

use pulse_core::{NewTask, Store, SyncOutcome, Task, TaskStatus, TaskUpdate};
use pulse_llm::{TaskCopilotTask, TaskCopilotToolDefinition};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopilotToolAccess {
    ReadOnly,
    UserRequestedWrite,
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
    pub fn task_tools() -> Self {
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
                RegisteredCopilotTool {
                    definition: TaskCopilotToolDefinition {
                        name: "create_task".into(),
                        description: "Create one manual task requested by the user. This never deletes or schedules work.".into(),
                        input_schema: json!({
                            "type": "object",
                            "required": ["title"],
                            "properties": {
                                "title": { "type": "string", "minLength": 1, "maxLength": 500 },
                                "status": { "type": "string", "enum": ["Inbox", "Today", "Next", "Waiting", "Done"] },
                                "notes": { "type": "string", "maxLength": 4000 },
                                "project": { "type": "string", "maxLength": 500 },
                                "suggested_next_action": { "type": "string", "maxLength": 1000 },
                                "sync_outcome": { "type": "string", "enum": ["in_progress", "completed", "unclear"] }
                            }
                        }),
                    },
                    access: CopilotToolAccess::UserRequestedWrite,
                    progress_label: "Creating task",
                    handler: create_task,
                },
                RegisteredCopilotTool {
                    definition: TaskCopilotToolDefinition {
                        name: "update_task".into(),
                        description: "Update editable fields on one existing task the user explicitly identified. Use a task id returned by a prior tool result when one is needed.".into(),
                        input_schema: json!({
                            "type": "object",
                            "required": ["id"],
                            "properties": {
                                "id": { "type": "string" },
                                "title": { "type": "string", "minLength": 1, "maxLength": 500 },
                                "status": { "type": "string", "enum": ["Inbox", "Today", "Next", "Waiting", "Done"] },
                                "notes": { "type": "string", "maxLength": 4000 },
                                "project": { "type": "string", "maxLength": 500 },
                                "suggested_next_action": { "type": "string", "maxLength": 1000 },
                                "sync_outcome": { "type": "string", "enum": ["in_progress", "completed", "unclear"] }
                            },
                            "anyOf": [
                                { "required": ["title"] }, { "required": ["status"] }, { "required": ["notes"] },
                                { "required": ["project"] }, { "required": ["suggested_next_action"] }, { "required": ["sync_outcome"] }
                            ]
                        }),
                    },
                    access: CopilotToolAccess::UserRequestedWrite,
                    progress_label: "Updating task",
                    handler: update_task,
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
        match tool.access {
            // A Copilot turn is initiated by the user. Prompt rules additionally
            // require that write tools mirror an explicit request in that turn.
            CopilotToolAccess::ReadOnly | CopilotToolAccess::UserRequestedWrite => {
                (tool.handler)(store, arguments)
            }
        }
    }
}

fn create_task(store: &Store, arguments: &Value) -> Result<CopilotToolExecution, String> {
    let title = required_text(arguments, "title", 500)?;
    let mut task = NewTask::manual(title);
    task.status = optional_status(arguments)?.unwrap_or(TaskStatus::Inbox);
    task.notes = optional_text(arguments, "notes", 4_000)?;
    task.project = optional_text(arguments, "project", 500)?;
    task.suggested_next_action = optional_text(arguments, "suggested_next_action", 1_000)?;
    task.sync_outcome = optional_outcome(arguments)?;
    let task = store.create_task(task).map_err(|error| error.to_string())?;
    Ok(CopilotToolExecution {
        payload: json!({ "task": task_view(&task), "action": "created" }),
        tasks: vec![task],
    })
}

fn update_task(store: &Store, arguments: &Value) -> Result<CopilotToolExecution, String> {
    let id = required_text(arguments, "id", 64)?;
    let update = TaskUpdate {
        title: optional_text(arguments, "title", 500)?,
        status: optional_status(arguments)?,
        notes: optional_text(arguments, "notes", 4_000)?,
        project: optional_text(arguments, "project", 500)?,
        suggested_next_action: optional_text(arguments, "suggested_next_action", 1_000)?,
        sync_outcome: optional_outcome(arguments)?,
        ..Default::default()
    };
    if update.title.is_none() && update.status.is_none() && update.notes.is_none()
        && update.project.is_none() && update.suggested_next_action.is_none() && update.sync_outcome.is_none() {
        return Err("update_task requires at least one editable field".into());
    }
    let task = store.resolve_task(&id).and_then(|task| store.update_task(task.id, update))
        .map_err(|error| error.to_string())?;
    Ok(CopilotToolExecution {
        payload: json!({ "task": task_view(&task), "action": "updated" }),
        tasks: vec![task],
    })
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

fn optional_status(arguments: &Value) -> Result<Option<TaskStatus>, String> {
    match arguments.get("status") {
        Some(Value::String(status)) => TaskStatus::parse(status).map(Some).ok_or("invalid task status".into()),
        Some(_) => Err("status must be a string".into()),
        None => Ok(None),
    }
}

fn optional_outcome(arguments: &Value) -> Result<Option<SyncOutcome>, String> {
    match arguments.get("sync_outcome") {
        Some(Value::String(outcome)) => SyncOutcome::parse(outcome).map(Some).ok_or("invalid task outcome".into()),
        Some(_) => Err("sync_outcome must be a string".into()),
        None => Ok(None),
    }
}

fn required_text<'a>(arguments: &'a Value, field: &str, max_chars: usize) -> Result<&'a str, String> {
    let value = arguments.get(field).and_then(Value::as_str).map(str::trim)
        .filter(|value| !value.is_empty()).ok_or_else(|| format!("{field} requires non-empty text"))?;
    if value.chars().count() > max_chars {
        return Err(format!("{field} must be {max_chars} characters or fewer"));
    }
    Ok(value)
}

fn optional_text(arguments: &Value, field: &str, max_chars: usize) -> Result<Option<String>, String> {
    match arguments.get(field) {
        Some(Value::String(value)) => {
            if value.chars().count() > max_chars {
                return Err(format!("{field} must be {max_chars} characters or fewer"));
            }
            Ok(Some(value.trim().to_string()))
        }
        Some(_) => Err(format!("{field} must be a string")),
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
    use pulse_core::open_in_memory;

    #[test]
    fn registry_exposes_all_registered_tool_schemas() {
        let registry = CopilotToolRegistry::task_tools();
        let definitions = registry.definitions();
        assert_eq!(definitions.len(), 5);
        assert!(definitions.iter().all(|tool| tool.input_schema.is_object()));
        assert_eq!(registry.progress_label("search_tasks"), "Searching task text");
    }

    #[test]
    fn registry_creates_and_updates_a_task() {
        let store = Store::new(open_in_memory().unwrap());
        let registry = CopilotToolRegistry::task_tools();
        let created = registry.execute(&store, "create_task", &json!({
            "title": "Prepare the deployment runbook", "status": "Today", "sync_outcome": "in_progress"
        })).unwrap();
        let id = created.tasks[0].id.to_string();
        assert_eq!(created.tasks[0].status, TaskStatus::Today);
        let updated = registry.execute(&store, "update_task", &json!({
            "id": id, "status": "Done", "sync_outcome": "completed"
        })).unwrap();
        assert_eq!(updated.tasks[0].status, TaskStatus::Done);
        assert_eq!(updated.tasks[0].sync_outcome, Some(SyncOutcome::Completed));
    }
}
