import { invoke } from "@tauri-apps/api/core";
import type { Task, TaskDetail, TaskStatus } from "./types";

export async function listTasks(status?: TaskStatus): Promise<Task[]> {
  return invoke<Task[]>("list_tasks", { status: status ?? null });
}

export async function getTask(id: string): Promise<TaskDetail> {
  return invoke<TaskDetail>("get_task", { id });
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
