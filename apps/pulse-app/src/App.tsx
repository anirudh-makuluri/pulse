import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { check } from "@tauri-apps/plugin-updater";
import { Database, House, Inbox as InboxIcon, Settings as SettingsIcon } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  createTask,
  deleteTask,
  exportHistory,
  getActivityTimeline,
  getSettings,
  getSummary,
  listTasks,
  markDone,
  dueReminders,
  privacyAcknowledge,
  serviceInfo,
  setPetVisible,
  setSourceEnabled,
  setTaskStatus,
  syncRecentSessions,
  type SettingsSnapshot,
} from "./api";
import type { ActivityTimeline, Task, TaskStatus } from "./types";

type View =
  | "Home"
  | "Inbox"
  | "Sources"
  | "Settings";

type TaskFilter = TaskStatus | "All";

const TASK_FILTERS: TaskFilter[] = [
  "Inbox",
  "Today",
  "Next",
  "Waiting",
  "Done",
  "All",
];

const isDevelopment = import.meta.env.DEV;

function shortId(id: string): string {
  return id.slice(0, 8);
}

function sourceClass(source: string): string {
  const s = source.toLowerCase();
  if (s === "claude") return "source-claude";
  if (s === "codex") return "source-codex";
  return "source-manual";
}

function outcomeLabel(outcome: Task["sync_outcome"]): string | null {
  if (outcome === "in_progress") return "In progress";
  if (outcome === "completed") return "Completed";
  if (outcome === "unclear") return "Unclear";
  return null;
}

function isSessionSyncBusy(error: unknown): boolean {
  return String(error).includes("Session sync is in progress");
}

function TaskPreview({
  task,
  onOpen,
  compact = false,
  selected = false,
}: {
  task: Task;
  onOpen: () => void;
  compact?: boolean;
  selected?: boolean;
}) {
  return (
    <button className={compact ? "home-task" : `task ${selected ? "selected" : ""}`} onClick={onOpen}>
      <div className="task-title">{task.title}</div>
      <div className="task-meta">
        {!compact ? <span className="pill">{shortId(task.id)}</span> : null}
        <span className="pill">{task.status}</span>
        <span className={`pill ${sourceClass(task.source)}`}>{task.source}</span>
        {outcomeLabel(task.sync_outcome) ? (
          <span className={`pill outcome-${task.sync_outcome}`}>
            {outcomeLabel(task.sync_outcome)}
          </span>
        ) : null}
        {task.project ? <span className="pill">{task.project}</span> : null}
        {!compact && task.confidence != null ? (
          <span className="pill">conf {(task.confidence * 100).toFixed(0)}%</span>
        ) : null}
      </div>
      {compact && task.suggested_next_action ? (
        <div className="home-task-next">{task.suggested_next_action}</div>
      ) : null}
    </button>
  );
}

type TimelineEntry = {
  id: string;
  at: string;
  kind: string;
  title: string;
  detail?: string;
};

function timelineEntries(timeline: ActivityTimeline): TimelineEntry[] {
  const entries: TimelineEntry[] = [
    ...timeline.sessions.map((session) => ({
      id: `session-${session.id}`,
      at: session.started_at,
      kind: "Session",
      title:
        [session.agent, session.application].filter(Boolean).join(" · ") ||
        "Work session",
      detail: session.repository_path ?? session.source_ref ?? undefined,
    })),
    ...timeline.events.map((event) => ({
      id: `event-${event.id}`,
      at: event.occurred_at,
      kind: event.kind,
      title: event.summary,
      detail: event.source_ref ?? undefined,
    })),
    ...timeline.checkpoints.map((checkpoint) => ({
      id: `checkpoint-${checkpoint.id}`,
      at: checkpoint.created_at,
      kind: "Checkpoint",
      title: checkpoint.summary,
      detail: checkpoint.next_actions.length
        ? `Next: ${checkpoint.next_actions.join(" · ")}`
        : undefined,
    })),
    ...timeline.evidence.map((evidence) => ({
      id: `evidence-${evidence.id}`,
      at: evidence.observed_at,
      kind: "Evidence",
      title: evidence.snippet ?? evidence.kind,
      detail: evidence.source_ref,
    })),
    ...timeline.reminders.map((reminder) => ({
      id: `reminder-${reminder.id}`,
      at: reminder.due_at,
      kind: `Reminder · ${reminder.status}`,
      title: reminder.title,
    })),
    ...timeline.memories.map((memory) => ({
      id: `memory-${memory.id}`,
      at: memory.created_at,
      kind: `Memory · ${memory.kind}`,
      title: memory.content,
    })),
    ...timeline.artifacts.map((artifact) => ({
      id: `artifact-${artifact.id}`,
      at: artifact.created_at,
      kind: `Artifact · ${artifact.kind}`,
      title: artifact.name,
      detail: artifact.local_path ?? undefined,
    })),
  ];
  return entries.sort((a, b) => b.at.localeCompare(a.at));
}

export default function App() {
  const desktopWindow = getCurrentWebviewWindow();
  const [view, setView] = useState<View>("Home");
  const [taskFilter, setTaskFilter] = useState<TaskFilter>("Inbox");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [homeTasks, setHomeTasks] = useState<Task[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<ActivityTimeline | null>(null);
  const [title, setTitle] = useState("");
  const [captureOpen, setCaptureOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [updateStatus, setUpdateStatus] = useState<string | null>(null);
  const [checkingForUpdate, setCheckingForUpdate] = useState(false);
  const [info, setInfo] = useState("…");
  const [loading, setLoading] = useState(false);
  const [settings, setSettings] = useState<SettingsSnapshot | null>(null);
  const [summaryText, setSummaryText] = useState<string>("(loading…)");
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [syncingSessions, setSyncingSessions] = useState(false);
  const notifiedDue = useRef(new Set<string>());
  const syncingSessionsRef = useRef(false);

  const isTaskView = view === "Inbox";
  const statusFilter = useMemo(
    () => (taskFilter === "All" ? undefined : taskFilter),
    [taskFilter],
  );
  const timeline = useMemo(() => (detail ? timelineEntries(detail) : []), [detail]);
  const focusTasks = useMemo(
    () => homeTasks.filter((task) => task.status === "Today" || task.sync_outcome === "in_progress").slice(0, 3),
    [homeTasks],
  );
  const inboxTasks = useMemo(
    () => homeTasks.filter((task) => task.status === "Inbox").slice(0, 3),
    [homeTasks],
  );
  const recentTasks = useMemo(
    () => homeTasks.filter((task) => task.status !== "Done").slice(0, 3),
    [homeTasks],
  );
  const inboxCount = useMemo(
    () => homeTasks.filter((task) => task.status === "Inbox").length,
    [homeTasks],
  );

  const refreshTasks = useCallback(async () => {
    if (!isTaskView || syncingSessionsRef.current) return;
    setLoading(true);
    setError(null);
    try {
      const [list, svc] = await Promise.all([
        listTasks(statusFilter),
        serviceInfo().catch(() => "backend unknown"),
      ]);
      setTasks(list);
      setInfo(svc);
      if (selectedId && !list.some((t) => t.id === selectedId)) {
        setSelectedId(null);
        setDetail(null);
      }
    } catch (e) {
      // The service rejects reads while it holds the database lock for a
      // session sync. Keep the currently rendered task list in place until
      // the next successful refresh instead of flashing a transient error.
      if (!isSessionSyncBusy(e)) setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [isTaskView, selectedId, statusFilter]);

  const refreshSettings = useCallback(async () => {
    try {
      const s = await getSettings();
      setSettings(s);
      setInfo(s.service_line);
    } catch (e) {
      setError(String(e));
    }
  }, []);

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
      // Windows exits Pulse while the signed NSIS installer applies the update.
      setUpdateStatus("Update installed. Restart Pulse to finish.");
    } catch (err) {
      setUpdateStatus(null);
      setError(`Could not update Pulse: ${String(err)}`);
    } finally {
      setCheckingForUpdate(false);
    }
  }, []);

  const refreshHome = useCallback(async () => {
    if (syncingSessionsRef.current) return;
    setLoading(true);
    setError(null);
    try {
      const [allTasks, sourceSettings, text, svc] = await Promise.all([
        listTasks(),
        getSettings(),
        getSummary(),
        serviceInfo().catch(() => "backend unknown"),
      ]);
      setHomeTasks(allTasks);
      setSettings(sourceSettings);
      setSummaryText(text || "No summary for today yet.");
      setInfo(svc);
    } catch (e) {
      if (!isSessionSyncBusy(e)) setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (isTaskView) {
      void refreshTasks();
      const id = window.setInterval(() => void refreshTasks(), 4000);
      return () => window.clearInterval(id);
    }
    if (view === "Home") {
      void refreshHome();
      const id = window.setInterval(() => void refreshHome(), 4000);
      return () => window.clearInterval(id);
    }
    if (view === "Sources" || view === "Settings") {
      void refreshSettings();
    }
  }, [view, isTaskView, refreshTasks, refreshSettings, refreshHome]);

  useEffect(() => {
    if (!selectedId || !isTaskView) {
      if (!isTaskView) setDetail(null);
      return;
    }
    let cancelled = false;
    void getActivityTimeline(selectedId)
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, isTaskView]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<string>("pulse://open-task", (event) => {
      setView("Inbox");
      setTaskFilter("All");
      setSelectedId(event.payload);
    }).then((fn) => { unlisten = fn; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    const poll = () => void dueReminders().then((reminders) => {
      const active = new Set(reminders.map((reminder) => reminder.id));
      notifiedDue.current.forEach((id) => { if (!active.has(id)) notifiedDue.current.delete(id); });
      const firstNew = reminders.find((reminder) => !notifiedDue.current.has(reminder.id));
      if (firstNew) {
        notifiedDue.current.add(firstNew.id);
        void (async () => {
          const allowed = await isPermissionGranted() || (await requestPermission()) === "granted";
          if (allowed) sendNotification({ title: "Pulse reminder", body: firstNew.title });
        })().catch(() => undefined);
      }
    }).catch(() => undefined);
    poll();
    const id = window.setInterval(poll, 15000);
    return () => window.clearInterval(id);
  }, []);

  async function onAdd(e: FormEvent) {
    e.preventDefault();
    const t = title.trim();
    if (!t) return;
    try {
      const task = await createTask(t, taskFilter === "Today");
      setTitle("");
      setCaptureOpen(false);
      setSelectedId(task.id);
      if (isTaskView) {
        await refreshTasks();
      } else {
        await refreshHome();
      }
    } catch (err) {
      setError(String(err));
    }
  }

  async function move(status: TaskStatus) {
    if (!selectedId) return;
    try {
      await setTaskStatus(selectedId, status);
      await refreshTasks();
      setDetail(await getActivityTimeline(selectedId));
    } catch (err) {
      setError(String(err));
    }
  }

  async function done() {
    if (!selectedId) return;
    try {
      await markDone(selectedId);
      await refreshTasks();
      setDetail(await getActivityTimeline(selectedId));
    } catch (err) {
      setError(String(err));
    }
  }

  async function deleteSelectedTask() {
    if (!selectedId || !detail) return;
    if (!window.confirm(`Delete “${detail.task.title}”? This also removes its local timeline and reminders.`)) return;
    try {
      await deleteTask(selectedId);
      setSelectedId(null);
      setDetail(null);
      await refreshTasks();
      setInfo("Task deleted.");
    } catch (err) {
      setError(String(err));
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
      setInfo(
        `Session sync: ${result.tasks_created} added, ${result.tasks_updated} updated; ${result.sessions_skipped_unchanged} unchanged sessions skipped.`,
      );
    } catch (err) {
      setError(String(err));
    } finally {
      syncingSessionsRef.current = false;
      setSyncingSessions(false);
    }
  }

  function openTask(task: Task) {
    setView("Inbox");
    setTaskFilter(task.status);
    setSelectedId(task.id);
  }

  function openInbox(filter: TaskFilter = "Inbox") {
    setView("Inbox");
    setTaskFilter(filter);
    setSelectedId(null);
    setDetail(null);
  }

  return (
    <div className="dashboard-shell">
      <div className="sr-only" role="status" aria-live="polite">
        {loading ? "Refreshing Pulse data." : info}
      </div>
      <header
        className="window-bar"
        data-tauri-drag-region
        onMouseDown={(event) => {
          if (event.button === 0) void desktopWindow.startDragging();
        }}
        onDoubleClick={() => void desktopWindow.toggleMaximize()}
      >
        <div className="window-identity" data-tauri-drag-region>
          <img className="window-mark" src="/pulse-logo.png" alt="" aria-hidden="true" />
          <span>Pulse</span>
          {isDevelopment && <span className="environment-label">( dev )</span>}
        </div>
        <div className="window-controls">
          <button
            className="window-control minimize"
            type="button"
            aria-label="Minimize window"
            onMouseDown={(event) => event.stopPropagation()}
            onClick={() => void desktopWindow.minimize()}
          />
          <button
            className="window-control maximize"
            type="button"
            aria-label="Maximize or restore window"
            onMouseDown={(event) => event.stopPropagation()}
            onClick={() => void desktopWindow.toggleMaximize()}
          />
          <button
            className="window-control close"
            type="button"
            aria-label="Hide Pulse window"
            onMouseDown={(event) => event.stopPropagation()}
            onClick={() => void desktopWindow.hide()}
          />
        </div>
      </header>

      <div className={`app ${isTaskView ? "" : "single-pane"}`}>
        <aside className="sidebar">
        <nav className="nav" aria-label="Primary navigation">
          <button
            className={view === "Home" ? "active" : ""}
            onClick={() => setView("Home")}
          >
            <House aria-hidden="true" />
            Home
          </button>
          <button
            className={view === "Inbox" ? "active" : ""}
            onClick={() => openInbox()}
          >
            <InboxIcon aria-hidden="true" />
            Inbox
          </button>
          <button
            className={view === "Sources" ? "active" : ""}
            onClick={() => setView("Sources")}
          >
            <Database aria-hidden="true" />
            Sources
          </button>
          <div className="nav-section-label">System</div>
          <button
            className={view === "Settings" ? "active" : ""}
            onClick={() => setView("Settings")}
          >
            <SettingsIcon aria-hidden="true" />
            Settings
          </button>
        </nav>
        <div className="sidebar-footer">
          <button className="capture-task" type="button" onClick={() => setCaptureOpen(true)}>
            Capture task
          </button>
          <button className="session-sync" type="button" onClick={() => void syncSessions()} disabled={syncingSessions}>
            {syncingSessions ? "Syncing sessions..." : "Sync latest sessions"}
          </button>
        </div>
        </aside>

        <main className="main">
        {isTaskView ? (
          <div className="panel-page section-page inbox-page">
            <div className="section-header">
              <div>
                <div className="eyebrow">Inbox</div>
                <p>Review and organize captured tasks.</p>
              </div>
              <div className="task-filters" aria-label="Task filters">
                {TASK_FILTERS.map((filter) => (
                  <button
                    key={filter}
                    type="button"
                    className={taskFilter === filter ? "active" : ""}
                    onClick={() => {
                      setTaskFilter(filter);
                      setSelectedId(null);
                      setDetail(null);
                    }}
                  >
                    {filter}
                  </button>
                ))}
              </div>
            </div>
            {error ? <div className="error">{error}</div> : null}

            <div className="list section-list">
              {tasks.length === 0 ? (
                <div className="empty-list">No tasks in {taskFilter}.</div>
              ) : (
                tasks.map((task) => (
                  <TaskPreview
                    key={task.id}
                    task={task}
                    selected={selectedId === task.id}
                    onOpen={() => setSelectedId(task.id)}
                  />
                ))
              )}
            </div>
          </div>
        ) : null}

        {view === "Home" ? (
          <div className="panel-page section-page home-panel">
            <div className="home-header">
              <div>
                <div className="eyebrow">Home</div>
                <p>Stay on top of what needs your attention.</p>
              </div>
              <button type="button" className="primary" onClick={() => openInbox()}>
                Open inbox
              </button>
            </div>
            {error ? <div className="error">{error}</div> : null}
            <div className="home-grid">
              <section className="home-card home-card-focus home-card-wide">
                <div className="home-card-heading">
                  <div>
                    <h2>Focus now</h2>
                    <p>Today’s work and sessions that are still in progress.</p>
                  </div>
                  <button type="button" className="text-button" onClick={() => openInbox("Today")}>View today</button>
                </div>
                <div className="home-task-list">
                  {focusTasks.length ? focusTasks.map((task) => (
                    <TaskPreview key={task.id} task={task} compact onOpen={() => openTask(task)} />
                  )) : <div className="home-empty">No active focus yet. Move a task to Today when you’re ready to start.</div>}
                </div>
              </section>

              <section className="home-card home-card-triage">
                <div className="home-card-heading">
                  <div>
                    <h2>Needs triage</h2>
                    <p>{inboxCount === 1 ? "1 task is waiting in Inbox." : `${inboxCount} tasks are waiting in Inbox.`}</p>
                  </div>
                  <button type="button" className="text-button" onClick={() => openInbox("Inbox")}>Review</button>
                </div>
                <div className="home-task-list">
                  {inboxTasks.length ? inboxTasks.map((task) => (
                    <TaskPreview key={task.id} task={task} compact onOpen={() => openTask(task)} />
                  )) : <div className="home-empty">Your Inbox is clear.</div>}
                </div>
              </section>

              <section className="home-card home-card-continue">
                <div className="home-card-heading">
                  <div>
                    <h2>Continue working</h2>
                    <p>Recently updated unfinished tasks.</p>
                  </div>
                  <button type="button" className="text-button" onClick={() => openInbox("All")}>View all</button>
                </div>
                <div className="home-task-list">
                  {recentTasks.length ? recentTasks.map((task) => (
                    <TaskPreview key={task.id} task={task} compact onOpen={() => openTask(task)} />
                  )) : <div className="home-empty">No unfinished tasks to continue.</div>}
                </div>
              </section>

              <section className="home-card home-card-sources">
                <div className="home-card-heading">
                  <div>
                    <h2>Source health</h2>
                    <p>Session tracking is private and local by default.</p>
                  </div>
                  <button type="button" className="text-button" onClick={() => setView("Sources")}>Manage</button>
                </div>
                <div className="source-statuses">
                  <div><span className={`source-indicator ${settings?.claude_enabled ? "enabled" : ""}`} />Claude <span>{settings?.claude_enabled ? "Watching" : "Off"}</span></div>
                  <div><span className={`source-indicator ${settings?.codex_enabled ? "enabled" : ""}`} />Codex <span>{settings?.codex_enabled ? "Watching" : "Off"}</span></div>
                </div>
              </section>

              <section className="home-card home-card-recap home-card-wide">
                <div className="home-card-heading">
                  <div>
                    <h2>Today’s recap</h2>
                    <p>Your saved daily summary.</p>
                  </div>
                </div>
                <p className="summary-preview">{summaryText}</p>
              </section>
            </div>
          </div>
        ) : null}

        {view === "Sources" ? (
          <div className="panel-page section-page">
            <div className="section-header">
              <div>
                <div className="eyebrow">Sources</div>
                <p>Choose which local session data Pulse watches.</p>
              </div>
              <button type="button" className="text-button" onClick={() => void refreshSettings()}>
                Refresh
              </button>
            </div>
            {error ? <div className="error">{error}</div> : null}
            {!settings ? (
              <div className="empty-list">Loading sources…</div>
            ) : (
              <div className="section-content sources-page">
                <section className="home-card source-card">
                  <div>
                    <h2>Claude</h2>
                    <p>Watch local Claude session files and infer task candidates with their evidence.</p>
                  </div>
                  <label className="toggle source-toggle">
                    <span>{settings.claude_enabled ? "Watching" : "Off"}</span>
                    <input
                      type="checkbox"
                      checked={settings.claude_enabled}
                      onChange={(e) =>
                        void setSourceEnabled("claude", e.target.checked)
                          .then(refreshSettings)
                          .catch((err) => setError(String(err)))
                      }
                    />
                  </label>
                </section>
                <section className="home-card source-card">
                  <div>
                    <h2>Codex</h2>
                    <p>Watch local Codex session files and infer task candidates with their evidence.</p>
                  </div>
                  <label className="toggle source-toggle">
                    <span>{settings.codex_enabled ? "Watching" : "Off"}</span>
                    <input
                      type="checkbox"
                      checked={settings.codex_enabled}
                      onChange={(e) =>
                        void setSourceEnabled("codex", e.target.checked)
                          .then(refreshSettings)
                          .catch((err) => setError(String(err)))
                      }
                    />
                  </label>
                </section>
              </div>
            )}
          </div>
        ) : null}

        {view === "Settings" ? (
          <div className="panel-page section-page">
            <div className="section-header">
              <div>
                <div className="eyebrow">Settings</div>
                <p>Manage privacy, exports, and local app details.</p>
              </div>
              <button type="button" className="text-button" onClick={() => void refreshSettings()}>
                Refresh
              </button>
            </div>
            {error ? <div className="error">{error}</div> : null}
            {!settings ? (
              <div className="empty-list">Loading settings…</div>
            ) : (
              <div className="section-content settings">
                <section className="home-card settings-card">
                  <h3>Privacy / LLM</h3>
                  <p className="muted">
                    Backend: <code>{settings.llm_backend}</code>
                    {settings.llm_path ? (
                      <>
                        {" "}
                        · <code>{settings.llm_path}</code>
                      </>
                    ) : null}
                  </p>
                  <p className="muted">{settings.llm_reason}</p>
                  <p className="muted">
                    Privacy ack:{" "}
                    {settings.privacy_ack ? "yes" : "no (heuristic only)"}
                  </p>
                  {!settings.privacy_ack ? (
                    <button
                      type="button"
                      className="primary"
                      onClick={() =>
                        void privacyAcknowledge()
                          .then(refreshSettings)
                          .catch((err) => setError(String(err)))
                      }
                    >
                      Acknowledge remote LLM risk
                    </button>
                  ) : null}
                </section>

                <section className="home-card settings-card">
                  <h3>Export</h3>
                  <div className="task-actions">
                    <button
                      type="button"
                      onClick={() =>
                        void exportHistory("json")
                          .then((p) => setExportPath(p))
                          .catch((err) => setError(String(err)))
                      }
                    >
                      Export JSON
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        void exportHistory("md")
                          .then((p) => setExportPath(p))
                          .catch((err) => setError(String(err)))
                      }
                    >
                      Export Markdown
                    </button>
                  </div>
                  {exportPath ? (
                    <p className="muted">
                      Wrote: <code>{exportPath}</code>
                    </p>
                  ) : null}
                </section>

                <section className="home-card settings-card desktop-pet-setting">
                  <div className="desktop-pet-copy">
                    <h3>Desktop companion</h3>
                    <p className="muted">Keep the Pulse pet at the bottom-right of your screen. Pulse remains available from the system tray when it is hidden.</p>
                  </div>
                  <Switch
                    checked={settings.show_pet}
                    onCheckedChange={(visible) =>
                      void setPetVisible(visible)
                        .then(refreshSettings)
                        .catch((err) => setError(String(err)))
                    }
                    aria-label="Show desktop pet"
                  />
                </section>

                <section className="home-card settings-card">
                  <h3>Software update</h3>
                  <p className="muted">Check GitHub Releases for a signed Pulse update and install it automatically.</p>
                  <button type="button" className="primary" onClick={() => void checkForUpdates()} disabled={checkingForUpdate}>
                    {checkingForUpdate ? "Checking for updates…" : "Check for updates"}
                  </button>
                  {updateStatus ? <p className="muted" role="status">{updateStatus}</p> : null}
                </section>

                <section className="home-card settings-card">
                  <h3>Paths</h3>
                  <p className="muted">
                    Data: <code>{settings.data_dir}</code>
                  </p>
                  <p className="muted">
                    Config: <code>{settings.config_path}</code>
                  </p>
                  <p className="muted">{settings.service_line}</p>
                </section>
              </div>
            )}
          </div>
        ) : null}
        </main>

        {isTaskView ? (
        <aside className="detail">
        {!detail ? (
          <div className="empty">Select a task to see detail and evidence.</div>
        ) : (
          <>
            <h2>{detail.task.title}</h2>
            <div className="task-meta">
              <span className="pill">{detail.task.status}</span>
              <span className={`pill ${sourceClass(detail.task.source)}`}>
                {detail.task.source}
              </span>
              {outcomeLabel(detail.task.sync_outcome) ? (
                <span className={`pill outcome-${detail.task.sync_outcome}`}>
                  {outcomeLabel(detail.task.sync_outcome)}
                </span>
              ) : null}
              {detail.task.confidence != null ? (
                <span className="pill">
                  conf {(detail.task.confidence * 100).toFixed(0)}%
                </span>
              ) : null}
            </div>

            <div className="task-actions">
              <button onClick={() => void move("Today")}>Today</button>
              <button onClick={() => void move("Next")}>Next</button>
              <button onClick={() => void move("Waiting")}>Waiting</button>
              <button onClick={() => void move("Inbox")}>Inbox</button>
              <button className="primary" onClick={() => void done()}>
                Done
              </button>
              <button className="danger" onClick={() => void deleteSelectedTask()}>
                Delete
              </button>
            </div>

            {detail.task.notes ? (
              <div className="detail-section">
                <h3>Notes</h3>
                <pre>{detail.task.notes}</pre>
              </div>
            ) : null}

            {detail.task.suggested_next_action ? (
              <div className="detail-section">
                <h3>Suggested next</h3>
                <pre>{detail.task.suggested_next_action}</pre>
              </div>
            ) : null}

            <div className="detail-section">
              <h3>Chronological timeline</h3>
              {timeline.length === 0 ? (
                <div className="empty-list" style={{ padding: 12 }}>
                  No activity recorded yet.
                </div>
              ) : (
                <div className="timeline">
                  {timeline.map((entry) => (
                    <article className="timeline-item" key={entry.id}>
                      <div className="timeline-dot" />
                      <div className="timeline-content">
                        <div className="timeline-meta">
                          <span>{entry.kind}</span>
                          <time>{new Date(entry.at).toLocaleString()}</time>
                        </div>
                        <div className="timeline-title">{entry.title}</div>
                        {entry.detail ? (
                          <div className="timeline-detail">{entry.detail}</div>
                        ) : null}
                      </div>
                    </article>
                  ))}
                </div>
              )}
            </div>

            <div className="detail-section">
              <h3>Ids</h3>
              <pre>
                {detail.task.id}
                {"\n"}
                updated {new Date(detail.task.updated_at).toLocaleString()}
              </pre>
            </div>
          </>
        )}
        </aside>
        ) : null}

      </div>
      {captureOpen ? (
        <div className="capture-backdrop" role="presentation" onMouseDown={() => setCaptureOpen(false)}>
          <form
            className="capture-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="capture-task-title"
            onMouseDown={(event) => event.stopPropagation()}
            onSubmit={onAdd}
          >
            <div className="capture-dialog-heading">
              <div>
                <div className="eyebrow">New task</div>
                <h2 id="capture-task-title">Capture task</h2>
              </div>
              <button type="button" className="dialog-close" onClick={() => setCaptureOpen(false)} aria-label="Close task capture" />
            </div>
            <input
              autoFocus
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              onKeyDown={(event) => { if (event.key === "Escape") setCaptureOpen(false); }}
              placeholder={taskFilter === "Today" ? "Add a task for today…" : "What needs your attention?"}
            />
            <div className="capture-dialog-actions">
              <button type="button" onClick={() => setCaptureOpen(false)}>Cancel</button>
              <button type="submit" className="primary">Add task</button>
            </div>
          </form>
        </div>
      ) : null}
    </div>
  );
}
