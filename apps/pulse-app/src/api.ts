import { invoke } from "@tauri-apps/api/core";
import type { ActivityTimeline, Reminder, SemanticSearchResult, Task, TaskDetail, TaskStatus } from "./types";

export interface CopilotResult {
  answer: string;
  tasks: Task[];
  backend: string;
}

export interface CopilotOperation {
  operation_id: string;
  conversation_id: string;
  token: string;
  websocket_url: string;
}

export interface CopilotSession {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

export interface CopilotStoredMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  backend: string | null;
  tasks: Task[];
  created_at: string;
}

export interface CopilotSessionDetail {
  session: CopilotSession;
  messages: CopilotStoredMessage[];
}

export interface ContextEnvelope {
  active_app: string | null;
  window_title: string | null;
  captured_at: string;
}

export interface OmniboxPreview {
  parsed: { intent: string; raw: string; subject: string; due_at: string | null };
  context: ContextEnvelope;
}

export interface OmniboxResult {
  message: string;
  task: Task | null;
  reminder: Reminder | null;
  tasks: Task[];
}

export async function previewOmnibox(input: string): Promise<OmniboxPreview> {
  return invoke("preview_omnibox", { input });
}

export async function executeOmnibox(input: string, selectedTaskId: string | null, context: ContextEnvelope): Promise<OmniboxResult> {
  return invoke("execute_omnibox", { input, selectedTaskId, context });
}

export async function dueReminders(): Promise<Reminder[]> { return invoke("due_reminders"); }
export interface RecentSessionSyncResult {
  sessions_reviewed: number;
  sessions_already_imported: number;
  tasks_created: number;
  tasks_updated: number;
  sessions_skipped_unchanged: number;
  sessions_without_actionable_work: number;
  sources_checked: string[];
}
export async function syncRecentSessions(): Promise<RecentSessionSyncResult> { return invoke("sync_recent_sessions"); }
export async function reminderAction(id: string, action: "open_context" | "continue_coding" | "snooze" | "done"): Promise<Reminder> {
  return invoke("reminder_action", { id, action });
}

export async function openTaskContext(taskId: string, mode: "open_context" | "continue_coding"): Promise<void> {
  return invoke("open_task_context", { taskId, mode });
}

export async function setPetExpanded(expanded: boolean): Promise<void> {
  return invoke("set_pet_expanded", { expanded });
}

export async function showMainWindow(): Promise<void> {
  return invoke("show_main_window");
}

export async function showPetContextMenu(): Promise<void> {
  return invoke("show_pet_context_menu");
}

export async function listTasks(status?: TaskStatus): Promise<Task[]> {
  return invoke<Task[]>("list_tasks", { status: status ?? null });
}

export async function getTask(id: string): Promise<TaskDetail> {
  return invoke<TaskDetail>("get_task", { id });
}

export async function getActivityTimeline(id: string): Promise<ActivityTimeline> {
  return invoke<ActivityTimeline>("get_activity_timeline", { id });
}

export async function semanticSearch(query: string): Promise<SemanticSearchResult[]> {
  return invoke<SemanticSearchResult[]>("semantic_search", { query });
}

export async function copilotStart(query: string, conversationId: string | null): Promise<CopilotOperation> {
  return invoke<CopilotOperation>("copilot_start", { query, conversationId });
}

export async function listCopilotSessions(): Promise<CopilotSession[]> {
  const result = await invoke<{ sessions: CopilotSession[] }>("list_copilot_sessions");
  return result.sessions;
}

export async function getCopilotSession(id: string): Promise<CopilotSessionDetail> {
  return invoke<CopilotSessionDetail>("get_copilot_session", { id });
}

export async function createTask(title: string, today: boolean): Promise<Task> {
  return invoke<Task>("create_task", { title, today });
}

export async function setTaskStatus(id: string, status: TaskStatus): Promise<Task> {
  return invoke<Task>("set_task_status", { id, status });
}

export type TaskOutcome = "completed" | "in_progress";

export async function setTaskOutcome(id: string, outcome: TaskOutcome): Promise<Task> {
  return invoke<Task>("set_task_outcome", { id, outcome });
}

export async function markDone(id: string): Promise<Task> {
  return invoke<Task>("mark_done", { id });
}
export async function deleteTask(id: string): Promise<void> { return invoke("delete_task", { id }); }

export async function serviceInfo(): Promise<string> {
  return invoke<string>("service_info");
}

export interface SettingsSnapshot {
  claude_enabled: boolean;
  codex_enabled: boolean;
  privacy_ack: boolean;
  llm_backend: string;
  llm_path: string | null;
  llm_reason: string;
  service_line: string;
  config_path: string;
  data_dir: string;
  show_pet: boolean;
}

export async function getSettings(): Promise<SettingsSnapshot> {
  return invoke<SettingsSnapshot>("get_settings");
}

export async function setSourceEnabled(id: string, enabled: boolean): Promise<void> {
  return invoke("set_source_enabled", { id, enabled });
}

export async function setPetVisible(visible: boolean): Promise<void> {
  return invoke("set_pet_visible", { visible });
}

export async function privacyAcknowledge(): Promise<void> {
  return invoke("privacy_acknowledge");
}

export async function getSummary(date?: string): Promise<string> {
  return invoke<string>("get_summary", { date: date ?? null });
}

export async function generateSummary(date?: string): Promise<string> {
  return invoke<string>("generate_summary", { date: date ?? null });
}

export async function exportHistory(format: "json" | "md"): Promise<string> {
  return invoke<string>("export_history", { format });
}
