import { invoke } from "@tauri-apps/api/core";
import type { ActivityTimeline, Task, TaskDetail, TaskStatus } from "./types";

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
