import { type FormEvent, useCallback, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { check } from "@tauri-apps/plugin-updater";
import {
  createTask,
  deleteTask,
  dueReminders,
  getActivityTimeline,
  getSettings,
  getSummary,
  listTasks,
  markDone,
  serviceInfo,
  setTaskOutcome,
  setTaskStatus,
  syncRecentSessions,
} from "@/api";
import { AppSidebar } from "@/components/AppSidebar";
import { CaptureTaskDialog } from "@/components/CaptureTaskDialog";
import { TaskDetail } from "@/components/TaskDetail";
import { WindowBar } from "@/components/WindowBar";
import { HomePage } from "@/components/pages/HomePage";
import { InboxPage } from "@/components/pages/InboxPage";
import { CopilotPage } from "@/components/pages/CopilotPage";
import { SettingsPage } from "@/components/pages/SettingsPage";
import { SourcesPage } from "@/components/pages/SourcesPage";
import { useAppStore } from "@/store/useAppStore";
import type { TaskStatus } from "@/types";
import type { TaskOutcome } from "@/api";

function isSessionSyncBusy(error: unknown): boolean {
  return String(error).includes("Session sync is in progress");
}

export default function App() {
  const view = useAppStore((state) => state.view);
  const taskFilter = useAppStore((state) => state.taskFilter);
  const selectedId = useAppStore((state) => state.selectedId);
  const captureTitle = useAppStore((state) => state.captureTitle);
  const loading = useAppStore((state) => state.loading);
  const info = useAppStore((state) => state.info);
  const setView = useAppStore((state) => state.setView);
  const setTaskFilter = useAppStore((state) => state.setTaskFilter);
  const setTasks = useAppStore((state) => state.setTasks);
  const setHomeTasks = useAppStore((state) => state.setHomeTasks);
  const setSelectedId = useAppStore((state) => state.setSelectedId);
  const setDetail = useAppStore((state) => state.setDetail);
  const setCaptureOpen = useAppStore((state) => state.setCaptureOpen);
  const setCaptureTitle = useAppStore((state) => state.setCaptureTitle);
  const setError = useAppStore((state) => state.setError);
  const setInfo = useAppStore((state) => state.setInfo);
  const setLoading = useAppStore((state) => state.setLoading);
  const setSettings = useAppStore((state) => state.setSettings);
  const setSummaryText = useAppStore((state) => state.setSummaryText);
  const setUpdateStatus = useAppStore((state) => state.setUpdateStatus);
  const setCheckingForUpdate = useAppStore((state) => state.setCheckingForUpdate);
  const setSyncingSessions = useAppStore((state) => state.setSyncingSessions);
  const isTaskView = view === "Inbox";
  const notifiedDue = useRef(new Set<string>());
  const syncingSessionsRef = useRef(false);

  const refreshTasks = useCallback(async () => {
    if (!isTaskView || syncingSessionsRef.current) return;

    setLoading(true);
    setError(null);
    try {
      const [list, service] = await Promise.all([
        listTasks(taskFilter === "All" ? undefined : taskFilter),
        serviceInfo().catch(() => "backend unknown"),
      ]);
      setTasks(list);
      setInfo(service);
      if (selectedId && !list.some((task) => task.id === selectedId)) {
        setSelectedId(null);
        setDetail(null);
      }
    } catch (error) {
      if (!isSessionSyncBusy(error)) setError(String(error));
    } finally {
      setLoading(false);
    }
  }, [isTaskView, selectedId, setDetail, setError, setInfo, setLoading, setSelectedId, setTasks, taskFilter]);

  const refreshSettings = useCallback(async () => {
    try {
      const settings = await getSettings();
      setSettings(settings);
      setInfo(settings.service_line);
    } catch (error) {
      setError(String(error));
    }
  }, [setError, setInfo, setSettings]);

  const refreshHome = useCallback(async () => {
    if (syncingSessionsRef.current) return;

    setLoading(true);
    setError(null);
    try {
      const [tasks, settings, summary, service] = await Promise.all([
        listTasks(),
        getSettings(),
        getSummary(),
        serviceInfo().catch(() => "backend unknown"),
      ]);
      setHomeTasks(tasks);
      setSettings(settings);
      setSummaryText(summary || "No summary for today yet.");
      setInfo(service);
    } catch (error) {
      if (!isSessionSyncBusy(error)) setError(String(error));
    } finally {
      setLoading(false);
    }
  }, [setError, setHomeTasks, setInfo, setLoading, setSettings, setSummaryText]);

  const checkForUpdates = useCallback(async () => {
    setCheckingForUpdate(true);
    setUpdateStatus("Checking for updates…");
    setError(null);
    try {
      const update = await check();
      if (!update) {
        setUpdateStatus("Pulse is up to date.");
        return;
      }

      setUpdateStatus(`Downloading Pulse ${update.version}…`);
      await update.downloadAndInstall();
      setUpdateStatus("Update installed. Restart Pulse to finish.");
    } catch (error) {
      setUpdateStatus(null);
      setError(`Could not update Pulse: ${String(error)}`);
    } finally {
      setCheckingForUpdate(false);
    }
  }, [setCheckingForUpdate, setError, setUpdateStatus]);

  useEffect(() => {
    if (isTaskView) {
      void refreshTasks();
      const interval = window.setInterval(() => void refreshTasks(), 4000);
      return () => window.clearInterval(interval);
    }
    if (view === "Home") {
      void refreshHome();
      const interval = window.setInterval(() => void refreshHome(), 4000);
      return () => window.clearInterval(interval);
    }
    if (view === "Sources" || view === "Settings") void refreshSettings();
  }, [isTaskView, refreshHome, refreshSettings, refreshTasks, view]);

  useEffect(() => {
    if (!selectedId || !isTaskView) {
      if (!isTaskView) setDetail(null);
      return;
    }

    let cancelled = false;
    void getActivityTimeline(selectedId)
      .then((detail) => { if (!cancelled) setDetail(detail); })
      .catch((error) => { if (!cancelled) setError(String(error)); });
    return () => { cancelled = true; };
  }, [isTaskView, selectedId, setDetail, setError]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<string>("pulse://open-task", (event) => {
      setView("Inbox");
      setTaskFilter("All");
      setSelectedId(event.payload);
    }).then((dispose) => { unlisten = dispose; });
    return () => unlisten?.();
  }, [setTaskFilter, setSelectedId, setView]);

  useEffect(() => {
    const poll = () => void dueReminders().then((reminders) => {
      const active = new Set(reminders.map((reminder) => reminder.id));
      notifiedDue.current.forEach((id) => { if (!active.has(id)) notifiedDue.current.delete(id); });
      const firstNew = reminders.find((reminder) => !notifiedDue.current.has(reminder.id));
      if (!firstNew) return;

      notifiedDue.current.add(firstNew.id);
      void (async () => {
        const allowed = await isPermissionGranted() || (await requestPermission()) === "granted";
        if (allowed) sendNotification({ title: "Pulse reminder", body: firstNew.title });
      })().catch(() => undefined);
    }).catch(() => undefined);

    poll();
    const interval = window.setInterval(poll, 15000);
    return () => window.clearInterval(interval);
  }, []);

  async function addTask(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const title = captureTitle.trim();
    if (!title) return;

    try {
      const task = await createTask(title, taskFilter === "Today");
      setCaptureTitle("");
      setCaptureOpen(false);
      setSelectedId(task.id);
      if (isTaskView) await refreshTasks();
      else await refreshHome();
    } catch (error) {
      setError(String(error));
    }
  }

  async function moveTask(status: TaskStatus) {
    if (!selectedId) return;
    try {
      await setTaskStatus(selectedId, status);
      await refreshTasks();
      setDetail(await getActivityTimeline(selectedId));
    } catch (error) {
      setError(String(error));
    }
  }

  async function completeTask() {
    if (!selectedId) return;
    try {
      await markDone(selectedId);
      await refreshTasks();
      setDetail(await getActivityTimeline(selectedId));
    } catch (error) {
      setError(String(error));
    }
  }

  async function updateTaskOutcome(outcome: TaskOutcome) {
    if (!selectedId) return;
    try {
      await setTaskOutcome(selectedId, outcome);
      await refreshTasks();
      setDetail(await getActivityTimeline(selectedId));
    } catch (error) {
      setError(String(error));
    }
  }

  async function deleteSelectedTask() {
    const detail = useAppStore.getState().detail;
    if (!selectedId || !detail) return;
    if (!window.confirm(`Delete “${detail.task.title}”? This also removes its local timeline and reminders.`)) return;

    try {
      await deleteTask(selectedId);
      setSelectedId(null);
      setDetail(null);
      await refreshTasks();
      setInfo("Task deleted.");
    } catch (error) {
      setError(String(error));
    }
  }

  async function syncSessions() {
    syncingSessionsRef.current = true;
    setSyncingSessions(true);
    setError(null);
    try {
      const result = await syncRecentSessions();
      setView("Inbox");
      setTaskFilter("Inbox");
      setTasks(await listTasks("Inbox"));
      setInfo(`Session sync: ${result.tasks_created} added, ${result.tasks_updated} updated; ${result.sessions_skipped_unchanged} unchanged sessions skipped.`);
    } catch (error) {
      setError(String(error));
    } finally {
      syncingSessionsRef.current = false;
      setSyncingSessions(false);
    }
  }

  return (
    <div className="dashboard-shell">
      <div className="sr-only" role="status" aria-live="polite">{loading ? "Refreshing Pulse data." : info}</div>
      <WindowBar />
      <div className={`app ${isTaskView ? "" : "single-pane"}`}>
        <AppSidebar onSyncSessions={() => void syncSessions()} />
        <main className="main">
          {view === "Inbox" ? <InboxPage /> : null}
          {view === "Home" ? <HomePage /> : null}
          {view === "Copilot" ? <CopilotPage /> : null}
          {view === "Sources" ? <SourcesPage onRefresh={refreshSettings} /> : null}
          {view === "Settings" ? <SettingsPage onRefresh={refreshSettings} onCheckForUpdates={() => void checkForUpdates()} /> : null}
        </main>
        {isTaskView ? <TaskDetail onMove={(status) => void moveTask(status)} onOutcome={(outcome) => void updateTaskOutcome(outcome)} onDone={() => void completeTask()} onDelete={() => void deleteSelectedTask()} /> : null}
      </div>
      <CaptureTaskDialog onSubmit={addTask} />
    </div>
  );
}
