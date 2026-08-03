import { create } from "zustand";
import type { SettingsSnapshot } from "@/api";
import type { ActivityTimeline, Task, TaskStatus } from "@/types";

export type View = "Home" | "Inbox" | "Copilot" | "Sources" | "Settings";
export type TaskFilter = TaskStatus | "All";

export const TASK_FILTERS: TaskFilter[] = [
  "Inbox",
  "Today",
  "Next",
  "Waiting",
  "Done",
  "All",
];

type AppState = {
  view: View;
  taskFilter: TaskFilter;
  tasks: Task[];
  homeTasks: Task[];
  selectedId: string | null;
  detail: ActivityTimeline | null;
  captureOpen: boolean;
  captureTitle: string;
  error: string | null;
  info: string;
  loading: boolean;
  settings: SettingsSnapshot | null;
  summaryText: string;
  exportPath: string | null;
  updateStatus: string | null;
  checkingForUpdate: boolean;
  syncingSessions: boolean;
  setView: (view: View) => void;
  setTaskFilter: (taskFilter: TaskFilter) => void;
  setTasks: (tasks: Task[]) => void;
  setHomeTasks: (tasks: Task[]) => void;
  setSelectedId: (id: string | null) => void;
  setDetail: (detail: ActivityTimeline | null) => void;
  setCaptureOpen: (open: boolean) => void;
  setCaptureTitle: (title: string) => void;
  setError: (error: string | null) => void;
  setInfo: (info: string) => void;
  setLoading: (loading: boolean) => void;
  setSettings: (settings: SettingsSnapshot | null) => void;
  setSummaryText: (summaryText: string) => void;
  setExportPath: (exportPath: string | null) => void;
  setUpdateStatus: (updateStatus: string | null) => void;
  setCheckingForUpdate: (checking: boolean) => void;
  setSyncingSessions: (syncing: boolean) => void;
  openInbox: (filter?: TaskFilter) => void;
  openTask: (task: Task) => void;
  selectTask: (id: string | null) => void;
};

export const useAppStore = create<AppState>((set) => ({
  view: "Home",
  taskFilter: "Inbox",
  tasks: [],
  homeTasks: [],
  selectedId: null,
  detail: null,
  captureOpen: false,
  captureTitle: "",
  error: null,
  info: "…",
  loading: false,
  settings: null,
  summaryText: "(loading…)",
  exportPath: null,
  updateStatus: null,
  checkingForUpdate: false,
  syncingSessions: false,
  setView: (view) => set({ view }),
  setTaskFilter: (taskFilter) => set({ taskFilter }),
  setTasks: (tasks) => set({ tasks }),
  setHomeTasks: (homeTasks) => set({ homeTasks }),
  setSelectedId: (selectedId) => set({ selectedId }),
  setDetail: (detail) => set({ detail }),
  setCaptureOpen: (captureOpen) => set({ captureOpen }),
  setCaptureTitle: (captureTitle) => set({ captureTitle }),
  setError: (error) => set({ error }),
  setInfo: (info) => set({ info }),
  setLoading: (loading) => set({ loading }),
  setSettings: (settings) => set({ settings }),
  setSummaryText: (summaryText) => set({ summaryText }),
  setExportPath: (exportPath) => set({ exportPath }),
  setUpdateStatus: (updateStatus) => set({ updateStatus }),
  setCheckingForUpdate: (checkingForUpdate) => set({ checkingForUpdate }),
  setSyncingSessions: (syncingSessions) => set({ syncingSessions }),
  openInbox: (taskFilter = "Inbox") => set({
    view: "Inbox",
    taskFilter,
    selectedId: null,
    detail: null,
  }),
  openTask: (task) => set({
    view: "Inbox",
    taskFilter: task.status,
    selectedId: task.id,
    detail: null,
  }),
  selectTask: (selectedId) => set({ selectedId, detail: null }),
}));
