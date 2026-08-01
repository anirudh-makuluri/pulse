import { TaskPreview } from "@/components/TaskPreview";
import { useAppStore } from "@/store/useAppStore";

export function HomePage() {
  const homeTasks = useAppStore((state) => state.homeTasks);
  const settings = useAppStore((state) => state.settings);
  const summaryText = useAppStore((state) => state.summaryText);
  const error = useAppStore((state) => state.error);
  const openInbox = useAppStore((state) => state.openInbox);
  const openTask = useAppStore((state) => state.openTask);
  const setView = useAppStore((state) => state.setView);

  const focusTasks = homeTasks
    .filter((task) => task.status === "Today" || task.sync_outcome === "in_progress")
    .slice(0, 3);
  const inboxTasks = homeTasks.filter((task) => task.status === "Inbox").slice(0, 3);
  const recentTasks = homeTasks.filter((task) => task.status !== "Done").slice(0, 3);
  const inboxCount = homeTasks.filter((task) => task.status === "Inbox").length;

  return (
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
  );
}
