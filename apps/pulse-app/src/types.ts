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
