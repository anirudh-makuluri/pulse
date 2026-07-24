use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum TaskStatus {
    Inbox,
    Today,
    Next,
    Waiting,
    Done,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "Inbox",
            Self::Today => "Today",
            Self::Next => "Next",
            Self::Waiting => "Waiting",
            Self::Done => "Done",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Inbox" => Some(Self::Inbox),
            "Today" => Some(Self::Today),
            "Next" => Some(Self::Next),
            "Waiting" => Some(Self::Waiting),
            "Done" => Some(Self::Done),
            _ => None,
        }
    }

    pub fn is_open(self) -> bool {
        !matches!(self, Self::Done)
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskSource {
    Manual,
    Claude,
    Codex,
    Unknown,
}

impl TaskSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "manual" => Some(Self::Manual),
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub status: TaskStatus,
    pub source: TaskSource,
    pub confidence: Option<f64>,
    pub project: Option<String>,
    pub notes: Option<String>,
    pub suggested_next_action: Option<String>,
    pub dedup_key: Option<String>,
    pub source_session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewTask {
    pub title: String,
    pub status: TaskStatus,
    pub source: TaskSource,
    pub confidence: Option<f64>,
    pub project: Option<String>,
    pub notes: Option<String>,
    pub suggested_next_action: Option<String>,
    pub dedup_key: Option<String>,
    pub source_session_id: Option<String>,
}

impl NewTask {
    pub fn manual(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            status: TaskStatus::Inbox,
            source: TaskSource::Manual,
            confidence: None,
            project: None,
            notes: None,
            suggested_next_action: None,
            dedup_key: None,
            source_session_id: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskUpdate {
    pub title: Option<String>,
    pub status: Option<TaskStatus>,
    pub notes: Option<String>,
    pub project: Option<String>,
    pub suggested_next_action: Option<String>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: Uuid,
    pub task_id: Uuid,
    pub kind: String,
    pub source_ref: String,
    pub snippet: Option<String>,
    pub metadata_json: Option<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewEvidence {
    pub task_id: Uuid,
    pub kind: String,
    pub source_ref: String,
    pub snippet: Option<String>,
    pub metadata_json: Option<String>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckInKind {
    StillActive,
    IsDone,
    NextStep,
}

impl CheckInKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StillActive => "still_active",
            Self::IsDone => "is_done",
            Self::NextStep => "next_step",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "still_active" => Some(Self::StillActive),
            "is_done" => Some(Self::IsDone),
            "next_step" => Some(Self::NextStep),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckInStatus {
    Open,
    Answered,
}

impl CheckInStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Answered => "answered",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "answered" => Some(Self::Answered),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckIn {
    pub id: Uuid,
    pub task_id: Option<Uuid>,
    pub question: String,
    pub kind: CheckInKind,
    pub status: CheckInStatus,
    pub answer_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceWatermark {
    pub source_ref: String,
    pub path: String,
    pub size_bytes: i64,
    pub mtime_ms: i64,
    pub byte_offset: i64,
    pub last_processed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub id: Uuid,
    pub day: String,
    pub timezone_offset_minutes: i32,
    pub text: String,
    pub highlights_json: String,
    pub evidence_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewCheckIn {
    pub task_id: Option<Uuid>,
    pub question: String,
    pub kind: CheckInKind,
}

/// A bounded period of work contributed by an agent or application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub task_id: Uuid,
    pub agent: Option<String>,
    pub application: Option<String>,
    pub repository_path: Option<String>,
    pub external_id: Option<String>,
    pub source_ref: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub metadata_json: String,
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub task_id: Uuid,
    pub agent: Option<String>,
    pub application: Option<String>,
    pub repository_path: Option<String>,
    pub external_id: Option<String>,
    pub source_ref: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub metadata_json: String,
}

impl NewSession {
    pub fn for_task(task_id: Uuid, started_at: DateTime<Utc>) -> Self {
        Self {
            task_id,
            agent: None,
            application: None,
            repository_path: None,
            external_id: None,
            source_ref: None,
            started_at,
            ended_at: None,
            metadata_json: "{}".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: Uuid,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub kind: String,
    pub summary: String,
    pub payload_json: Option<String>,
    pub source_ref: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewActivityEvent {
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub kind: String,
    pub summary: String,
    pub payload_json: Option<String>,
    pub source_ref: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: Uuid,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub summary: String,
    pub decisions: Vec<String>,
    pub failures: Vec<String>,
    pub next_actions: Vec<String>,
    pub source_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewCheckpoint {
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub summary: String,
    pub decisions: Vec<String>,
    pub failures: Vec<String>,
    pub next_actions: Vec<String>,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderStatus {
    Pending,
    Snoozed,
    Done,
    Cancelled,
}

impl ReminderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Snoozed => "snoozed",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "snoozed" => Some(Self::Snoozed),
            "done" => Some(Self::Done),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: Uuid,
    pub task_id: Uuid,
    pub title: String,
    pub due_at: DateTime<Utc>,
    pub status: ReminderStatus,
    pub context_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewReminder {
    pub task_id: Uuid,
    pub title: String,
    pub due_at: DateTime<Utc>,
    pub context_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: Uuid,
    pub task_id: Uuid,
    pub checkpoint_id: Option<Uuid>,
    pub kind: String,
    pub content: String,
    pub provenance_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewMemory {
    pub task_id: Uuid,
    pub checkpoint_id: Option<Uuid>,
    pub kind: String,
    pub content: String,
    pub provenance_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub kind: String,
    pub name: String,
    pub local_path: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub checksum: Option<String>,
    pub metadata_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewArtifact {
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub kind: String,
    pub name: String,
    pub local_path: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub checksum: Option<String>,
    pub metadata_json: String,
}
