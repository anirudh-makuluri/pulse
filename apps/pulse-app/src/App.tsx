import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import {
  createTask,
  exportHistory,
  generateSummary,
  getActivityTimeline,
  getSettings,
  getSummary,
  listTasks,
  markDone,
  dueReminders,
  executeOmnibox,
  previewOmnibox,
  reminderAction,
  privacyAcknowledge,
  serviceInfo,
  setSourceEnabled,
  setTaskStatus,
  type SettingsSnapshot,
} from "./api";
import type { ActivityTimeline, Reminder, Task, TaskStatus } from "./types";

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
  const [view, setView] = useState<View>("Inbox");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<ActivityTimeline | null>(null);
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState("…");
  const [loading, setLoading] = useState(false);
  const [settings, setSettings] = useState<SettingsSnapshot | null>(null);
  const [summaryText, setSummaryText] = useState<string>("(loading…)");
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [omniboxOpen, setOmniboxOpen] = useState(false);
  const [omniboxInput, setOmniboxInput] = useState("");
  const [omniboxPreview, setOmniboxPreview] = useState<Awaited<ReturnType<typeof previewOmnibox>> | null>(null);
  const [includeSelection, setIncludeSelection] = useState(false);
  const [due, setDue] = useState<Reminder[]>([]);
  const [omniboxBusy, setOmniboxBusy] = useState(false);
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
      setDue(reminders);
    }).catch(() => undefined);
    poll();
    const id = window.setInterval(poll, 15000);
    return () => window.clearInterval(id);
  }, []);

  async function buildOmniboxPreview() {
    if (!omniboxInput.trim()) return;
    try {
      setOmniboxPreview(await previewOmnibox(omniboxInput, includeSelection));
    } catch (err) { setError(String(err)); }
  }

  async function submitOmnibox() {
    let preview = omniboxPreview;
    if (!preview) {
      if (!omniboxInput.trim()) return;
      try {
        preview = await previewOmnibox(omniboxInput, includeSelection);
        setOmniboxPreview(preview);
      } catch (err) { setError(String(err)); return; }
      if (preview.needs_context_confirmation) return;
    }
    setOmniboxBusy(true);
    try {
      const result = await executeOmnibox(omniboxInput, selectedId, preview.context);
      setInfo(result.message);
      if (result.task) { setSelectedId(result.task.id); setDetail(await getActivityTimeline(result.task.id)); }
      if (result.tasks.length) setTasks(result.tasks);
      await refreshTasks();
      setOmniboxInput(""); setOmniboxPreview(null); setOmniboxOpen(false); setIncludeSelection(false);
    } catch (err) { setError(String(err)); }
    finally { setOmniboxBusy(false); }
  }

  async function actOnReminder(reminder: Reminder, action: "open_context" | "continue_coding" | "snooze" | "done") {
    try {
      await reminderAction(reminder.id, action);
      if (action === "open_context" || action === "continue_coding") {
        setSelectedId(reminder.task_id); setDetail(await getActivityTimeline(reminder.task_id));
      }
      setDue(await dueReminders());
      await refreshTasks();
    } catch (err) { setError(String(err)); }
  }

  async function onAdd(e: FormEvent) {
    e.preventDefault();
    const t = title.trim();
    if (!t) return;
    try {
      const task = await createTask(t, view === "Today");
      setTitle("");
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

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          Pulse <span>·</span>
        </div>
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
        <div className="meta">
          <div>{loading ? "Refreshing…" : "Live"}</div>
          <div>{info}</div>
        </div>
      </aside>

      <main className="main">
        {isTaskView ? (
          <>
            <form className="toolbar" onSubmit={onAdd}>
              <input
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder={
                  view === "Today"
                    ? "Add a task for today…"
                    : "Capture a task…"
                }
              />
              <button type="submit" className="primary">
                Add
              </button>
              <button type="button" onClick={() => void refreshTasks()}>
                Refresh
              </button>
            </form>

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

      <div className="pet-shell" aria-live="polite">
        {due.length ? (
          <section className="reminder-card">
            <div className="reminder-label">Reminder due</div>
            <strong>{due[0].title}</strong>
            <div className="reminder-actions">
              <button onClick={() => void actOnReminder(due[0], "open_context")}>Open Context</button>
              <button onClick={() => void actOnReminder(due[0], "continue_coding")}>Continue in Codex</button>
              <button onClick={() => void actOnReminder(due[0], "snooze")}>Snooze</button>
              <button className="primary" onClick={() => void actOnReminder(due[0], "done")}>Done</button>
            </div>
          </section>
        ) : null}
        {omniboxOpen ? (
          <section className="omnibox" role="dialog" aria-label="Pulse omnibox">
            <div className="omnibox-title">Tell Pulse what to do</div>
            <input autoFocus value={omniboxInput} onChange={(e) => { setOmniboxInput(e.target.value); setOmniboxPreview(null); }} onKeyDown={(e) => { if (e.key === "Escape") setOmniboxOpen(false); }} placeholder="Remind me to review this in 30 minutes" />
            <label className="capture-toggle"><input type="checkbox" checked={includeSelection} onChange={(e) => { setIncludeSelection(e.target.checked); setOmniboxPreview(null); }} /> Include copied selected text</label>
            {omniboxPreview ? <div className="omnibox-preview"><b>{omniboxPreview.parsed.intent.replace(/_/g, " ")}</b>{omniboxPreview.parsed.due_at ? ` · ${new Date(omniboxPreview.parsed.due_at).toLocaleString()}` : null}{omniboxPreview.needs_context_confirmation ? <p>Preview: selected text will be saved only after you confirm.</p> : <p>Only this command will be saved.</p>}</div> : null}
            <div className="omnibox-actions"><button onClick={() => void buildOmniboxPreview()} disabled={!omniboxInput.trim()}>Preview</button><button className="primary" onClick={() => void submitOmnibox()} disabled={!omniboxInput.trim() || omniboxBusy}>{omniboxPreview?.needs_context_confirmation ? "Confirm" : "Run"}</button></div>
          </section>
        ) : null}
        <button className={`pet ${due.length ? "pet-due" : ""}`} onClick={() => setOmniboxOpen((open) => !open)} aria-label="Open Pulse omnibox"><img src="/pulse-firefly-256.png" alt="Pulse firefly" /></button>
      </div>
    </div>
  );
}
