import { invoke } from "@tauri-apps/api/core";
import type { ActivityTimeline, Reminder, Task, TaskDetail, TaskStatus } from "./types";

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

export async function createTask(title: string, today: boolean): Promise<Task> {
  return invoke<Task>("create_task", { title, today });
}

export async function setTaskStatus(id: string, status: TaskStatus): Promise<Task> {
  return invoke<Task>("set_task_status", { id, status });
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
}

export async function getSettings(): Promise<SettingsSnapshot> {
  return invoke<SettingsSnapshot>("get_settings");
}

export async function setSourceEnabled(id: string, enabled: boolean): Promise<void> {
  return invoke("set_source_enabled", { id, enabled });
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
