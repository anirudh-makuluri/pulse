export type TaskStatus = "Inbox" | "Today" | "Next" | "Waiting" | "Done";

export interface Task {
  id: string;
  title: string;
  status: TaskStatus;
  source: string;
  confidence: number | null;
  project: string | null;
  notes: string | null;
  suggested_next_action: string | null;
  dedup_key: string | null;
  source_session_id: string | null;
  sync_outcome: "in_progress" | "completed" | "unclear" | null;
  sync_outcome_confidence: number | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

export interface Evidence {
  id: string;
  task_id: string;
  kind: string;
  source_ref: string;
  snippet: string | null;
  metadata_json: string | null;
  observed_at: string;
}

export interface TaskDetail {
  task: Task;
  evidence: Evidence[];
}

export interface Session {
  id: string;
  task_id: string;
  agent: string | null;
  application: string | null;
  repository_path: string | null;
  external_id: string | null;
  source_ref: string | null;
  started_at: string;
  ended_at: string | null;
  created_at: string;
  metadata_json: string;
}

export interface ActivityEvent {
  id: string;
  task_id: string;
  session_id: string | null;
  kind: string;
  summary: string;
  payload_json: string | null;
  source_ref: string | null;
  occurred_at: string;
  created_at: string;
}

export interface Checkpoint {
  id: string;
  task_id: string;
  session_id: string | null;
  summary: string;
  decisions: string[];
  failures: string[];
  next_actions: string[];
  source_ref: string | null;
  created_at: string;
}

export interface Reminder {
  id: string;
  task_id: string;
  title: string;
  due_at: string;
  status: "pending" | "snoozed" | "done" | "cancelled";
  context_json: string;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

export interface Memory {
  id: string;
  task_id: string;
  checkpoint_id: string | null;
  kind: string;
  content: string;
  provenance_json: string;
  created_at: string;
  updated_at: string;
}

export interface Artifact {
  id: string;
  task_id: string;
  session_id: string | null;
  kind: string;
  name: string;
  local_path: string | null;
  content_type: string | null;
  size_bytes: number | null;
  checksum: string | null;
  metadata_json: string;
  created_at: string;
}

export interface ActivityTimeline {
  task: Task;
  evidence: Evidence[];
  sessions: Session[];
  events: ActivityEvent[];
  checkpoints: Checkpoint[];
  reminders: Reminder[];
  memories: Memory[];
  artifacts: Artifact[];
}
