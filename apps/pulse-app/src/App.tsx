import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import {
  createTask,
  getTask,
  listTasks,
  markDone,
  serviceInfo,
  setTaskStatus,
} from "./api";
import type { Task, TaskDetail, TaskStatus } from "./types";

type View = "Inbox" | "Today" | "Next" | "Waiting" | "Done" | "All";

const VIEWS: View[] = ["Inbox", "Today", "Next", "Waiting", "Done", "All"];

function shortId(id: string): string {
  return id.slice(0, 8);
}

function sourceClass(source: string): string {
  const s = source.toLowerCase();
  if (s === "claude") return "source-claude";
  if (s === "codex") return "source-codex";
  return "source-manual";
}

export default function App() {
  const [view, setView] = useState<View>("Inbox");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<TaskDetail | null>(null);
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState("…");
  const [loading, setLoading] = useState(false);

  const statusFilter = useMemo(
    () => (view === "All" ? undefined : (view as TaskStatus)),
    [view],
  );

  const refresh = useCallback(async () => {
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
  }, [selectedId, statusFilter]);

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => void refresh(), 4000);
    return () => window.clearInterval(id);
  }, [refresh]);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    void getTask(selectedId)
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  async function onAdd(e: FormEvent) {
    e.preventDefault();
    const t = title.trim();
    if (!t) return;
    try {
      const task = await createTask(t, view === "Today");
      setTitle("");
      setSelectedId(task.id);
      await refresh();
    } catch (err) {
      setError(String(err));
    }
  }

  async function move(status: TaskStatus) {
    if (!selectedId) return;
    try {
      await setTaskStatus(selectedId, status);
      await refresh();
      const d = await getTask(selectedId);
      setDetail(d);
    } catch (err) {
      setError(String(err));
    }
  }

  async function done() {
    if (!selectedId) return;
    try {
      await markDone(selectedId);
      await refresh();
      const d = await getTask(selectedId);
      setDetail(d);
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
          {VIEWS.map((v) => (
            <button
              key={v}
              className={view === v ? "active" : ""}
              onClick={() => setView(v)}
            >
              {v}
            </button>
          ))}
        </nav>
        <div className="meta">
          <div>{loading ? "Refreshing…" : "Live"}</div>
          <div>{info}</div>
          <div>Poll every 4s</div>
        </div>
      </aside>

      <main className="main">
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
          <button type="button" onClick={() => void refresh()}>
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
                  {t.project ? <span className="pill">{t.project}</span> : null}
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
      </main>

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
              <h3>Evidence</h3>
              {detail.evidence.length === 0 ? (
                <div className="empty-list" style={{ padding: 12 }}>
                  No evidence linked.
                </div>
              ) : (
                detail.evidence.map((ev) => (
                  <div className="evidence" key={ev.id}>
                    <div>
                      <strong>{ev.kind}</strong> · {ev.source_ref}
                    </div>
                    <div style={{ color: "var(--muted)", marginTop: 4 }}>
                      {new Date(ev.observed_at).toLocaleString()}
                    </div>
                    {ev.snippet ? (
                      <div style={{ marginTop: 8 }}>{ev.snippet}</div>
                    ) : null}
                  </div>
                ))
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
  );
}
