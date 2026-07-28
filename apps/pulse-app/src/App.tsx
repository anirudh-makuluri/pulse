import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import {
  createTask,
  deleteTask,
  exportHistory,
  generateSummary,
  getActivityTimeline,
  getSettings,
  getSummary,
  listTasks,
  markDone,
  dueReminders,
  privacyAcknowledge,
  serviceInfo,
  setSourceEnabled,
  setTaskStatus,
  syncRecentSessions,
  type SettingsSnapshot,
} from "./api";
import type { ActivityTimeline, Task, TaskStatus } from "./types";

type View =
  | "Inbox"
  | "Today"
  | "Next"
  | "Waiting"
  | "Done"
  | "All"
  | "Summary"
  | "Settings";

const TASK_VIEWS: Array<Exclude<View, "Summary" | "Settings">> = [
  "Inbox",
  "Today",
  "Next",
  "Waiting",
  "Done",
  "All",
];

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
  const [view, setView] = useState<View>("Inbox");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<ActivityTimeline | null>(null);
  const [title, setTitle] = useState("");
  const [captureOpen, setCaptureOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState("…");
  const [loading, setLoading] = useState(false);
  const [settings, setSettings] = useState<SettingsSnapshot | null>(null);
  const [summaryText, setSummaryText] = useState<string>("(loading…)");
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [syncingSessions, setSyncingSessions] = useState(false);
  const notifiedDue = useRef(new Set<string>());

  const isTaskView = TASK_VIEWS.includes(view as (typeof TASK_VIEWS)[number]);
  const statusFilter = useMemo(
    () => (view === "All" || !isTaskView ? undefined : (view as TaskStatus)),
    [view, isTaskView],
  );
  const timeline = useMemo(() => (detail ? timelineEntries(detail) : []), [detail]);

  const refreshTasks = useCallback(async () => {
    if (!isTaskView) return;
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
      setError(String(e));
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

  const refreshSummary = useCallback(async () => {
    try {
      const text = await getSummary();
      setSummaryText(text || "(no summary for today yet)");
    } catch (e) {
      setSummaryText(String(e));
    }
  }, []);

  useEffect(() => {
    if (isTaskView) {
      void refreshTasks();
      const id = window.setInterval(() => void refreshTasks(), 4000);
      return () => window.clearInterval(id);
    }
    if (view === "Settings") {
      void refreshSettings();
    }
    if (view === "Summary") {
      void refreshSummary();
    }
  }, [view, isTaskView, refreshTasks, refreshSettings, refreshSummary]);

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
      setView("All");
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
      const task = await createTask(t, view === "Today");
      setTitle("");
      setCaptureOpen(false);
      setSelectedId(task.id);
      await refreshTasks();
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
    setSyncingSessions(true);
    setError(null);
    try {
      const result = await syncRecentSessions();
      setView("Inbox");
      setTasks(await listTasks("Inbox"));
      setInfo(
        `Session sync: ${result.tasks_created} added, ${result.tasks_updated} updated; ${result.sessions_skipped_unchanged} unchanged sessions skipped.`,
      );
    } catch (err) {
      setError(String(err));
    } finally {
      setSyncingSessions(false);
    }
  }

  return (
    <div className="dashboard-shell">
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
          <span className="window-context" data-tauri-drag-region>Local-first task memory</span>
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
            aria-label="Close window"
            onMouseDown={(event) => event.stopPropagation()}
            onClick={() => void desktopWindow.close()}
          />
        </div>
      </header>

      <div className="app">
        <aside className="sidebar">
        <nav className="nav">
          {TASK_VIEWS.map((v) => (
            <button
              key={v}
              className={view === v ? "active" : ""}
              onClick={() => setView(v)}
            >
              {v}
            </button>
          ))}
          <button
            className={view === "Summary" ? "active" : ""}
            onClick={() => setView("Summary")}
          >
            Summary
          </button>
          <button
            className={view === "Settings" ? "active" : ""}
            onClick={() => setView("Settings")}
          >
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
          <div className="meta">
            <div>{loading ? "Refreshing…" : "Live"}</div>
            <div>{info}</div>
          </div>
        </div>
        </aside>

        <main className="main">
        {isTaskView ? (
          <>
            {error ? <div className="error">{error}</div> : null}

            <div className="list">
              {tasks.length === 0 ? (
                <div className="empty-list">No tasks in {view}.</div>
              ) : (
                tasks.map((t) => (
                  <button
                    key={t.id}
                    className={`task ${selectedId === t.id ? "selected" : ""}`}
                    onClick={() => setSelectedId(t.id)}
                  >
                    <div className="task-title">{t.title}</div>
                    <div className="task-meta">
                      <span className="pill">{shortId(t.id)}</span>
                      <span className="pill">{t.status}</span>
                      <span className={`pill ${sourceClass(t.source)}`}>
                        {t.source}
                      </span>
                      {outcomeLabel(t.sync_outcome) ? (
                        <span className={`pill outcome-${t.sync_outcome}`}>
                          {outcomeLabel(t.sync_outcome)}
                        </span>
                      ) : null}
                      {t.project ? (
                        <span className="pill">{t.project}</span>
                      ) : null}
                      {t.confidence != null ? (
                        <span className="pill">
                          conf {(t.confidence * 100).toFixed(0)}%
                        </span>
                      ) : null}
                    </div>
                  </button>
                ))
              )}
            </div>
          </>
        ) : null}

        {view === "Summary" ? (
          <div className="panel-page">
            <div className="toolbar">
              <strong>Today’s summary</strong>
              <button
                type="button"
                className="primary"
                onClick={() =>
                  void generateSummary()
                    .then(setSummaryText)
                    .catch((e) => setError(String(e)))
                }
              >
                Generate
              </button>
              <button type="button" onClick={() => void refreshSummary()}>
                Reload
              </button>
            </div>
            {error ? <div className="error">{error}</div> : null}
            <pre className="panel-body">{summaryText}</pre>
          </div>
        ) : null}

        {view === "Settings" ? (
          <div className="panel-page">
            <div className="toolbar">
              <strong>Settings</strong>
              <button type="button" onClick={() => void refreshSettings()}>
                Refresh
              </button>
            </div>
            {error ? <div className="error">{error}</div> : null}
            {!settings ? (
              <div className="empty-list">Loading settings…</div>
            ) : (
              <div className="settings">
                <section>
                  <h3>Sources</h3>
                  <label className="toggle">
                    <input
                      type="checkbox"
                      checked={settings.claude_enabled}
                      onChange={(e) =>
                        void setSourceEnabled("claude", e.target.checked)
                          .then(refreshSettings)
                          .catch((err) => setError(String(err)))
                      }
                    />
                    Claude session tracking
                  </label>
                  <label className="toggle">
                    <input
                      type="checkbox"
                      checked={settings.codex_enabled}
                      onChange={(e) =>
                        void setSourceEnabled("codex", e.target.checked)
                          .then(refreshSettings)
                          .catch((err) => setError(String(err)))
                      }
                    />
                    Codex session tracking
                  </label>
                </section>

                <section>
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

                <section>
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

                <section>
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

        <aside className="detail">
        {!isTaskView ? (
          <div className="empty">
            {view === "Summary"
              ? "Generate a clean end-of-day recap from your task list."
              : "Toggle sources, privacy, and export history."}
          </div>
        ) : !detail ? (
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
              placeholder={view === "Today" ? "Add a task for today…" : "What needs your attention?"}
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
