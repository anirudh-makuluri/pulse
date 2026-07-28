import { FormEvent, useEffect, useRef, useState } from "react";
import {
  dueReminders,
  executeOmnibox,
  openTaskContext,
  previewOmnibox,
  reminderAction,
  setPetExpanded,
  showPetContextMenu,
  showMainWindow,
} from "./api";
import type { Reminder, Task } from "./types";

type ReminderAction = "open_context" | "continue_coding" | "snooze" | "done";

export default function Pet() {
  const [open, setOpen] = useState(false);
  const [input, setInput] = useState("");
  const [preview, setPreview] = useState<Awaited<ReturnType<typeof previewOmnibox>> | null>(null);
  const [includeSelection, setIncludeSelection] = useState(false);
  const [reminders, setReminders] = useState<Reminder[]>([]);
  const [resultTasks, setResultTasks] = useState<Task[]>([]);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const surfaced = useRef(new Set<string>());

  useEffect(() => {
    document.documentElement.classList.add("pet-window");
    return () => document.documentElement.classList.remove("pet-window");
  }, []);

  useEffect(() => {
    void setPetExpanded(open || reminders.length > 0 || resultTasks.length > 0).catch((error) => setMessage(String(error)));
  }, [open, reminders.length, resultTasks.length]);

  useEffect(() => {
    const poll = () => void dueReminders().then((items) => {
      const active = new Set(items.map((item) => `${item.id}:${item.due_at}`));
      surfaced.current.forEach((key) => { if (!active.has(key)) surfaced.current.delete(key); });
      const next = items.find((item) => !surfaced.current.has(`${item.id}:${item.due_at}`));
      if (next) {
        surfaced.current.add(`${next.id}:${next.due_at}`);
      }
      setReminders(items);
    }).catch((error) => setMessage(`Reminder check failed: ${String(error)}`));
    poll();
    const id = window.setInterval(poll, 10_000);
    return () => window.clearInterval(id);
  }, []);

  async function submit(event: FormEvent) {
    event.preventDefault();
    let nextPreview = preview;
    if (!nextPreview) {
      if (!input.trim()) return;
      try {
        nextPreview = await previewOmnibox(input, includeSelection);
        setPreview(nextPreview);
      } catch (error) {
        setMessage(String(error));
        return;
      }
      if (nextPreview.needs_context_confirmation) return;
    }
    setBusy(true);
    try {
      const result = await executeOmnibox(input, null, nextPreview.context);
      setMessage(result.message);
      setResultTasks(result.tasks);
      setInput("");
      setPreview(null);
      setIncludeSelection(false);
      if (!result.tasks.length) setOpen(false);
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function runReminder(reminder: Reminder, action: ReminderAction) {
    try {
      await reminderAction(reminder.id, action);
      if (action === "open_context" || action === "continue_coding") {
        await openTaskContext(reminder.task_id, action);
      }
      setReminders(await dueReminders());
      setMessage(action === "done" ? "Reminder completed." : action === "snooze" ? "Reminder snoozed for 30 minutes." : "Opening activity context.");
    } catch (error) {
      setMessage(String(error));
    }
  }

  return (
    <main className={`pet-app ${open || reminders.length ? "pet-expanded" : ""}`} aria-live="polite">
      {reminders.length ? (
        <section className="reminder-card">
          <div className="reminder-label">Reminder due</div>
          <strong>{reminders[0].title}</strong>
          <div className="reminder-actions">
            <button onClick={() => void runReminder(reminders[0], "open_context")}>Open Context</button>
            <button onClick={() => void runReminder(reminders[0], "snooze")}>Snooze</button>
            <button className="primary" onClick={() => void runReminder(reminders[0], "done")}>Done</button>
          </div>
        </section>
      ) : null}

      {open ? (
        <form className="omnibox" onSubmit={(event) => void submit(event)} aria-label="Pulse omnibox">
          <div className="omnibox-title">What should Pulse remember?</div>
          <input autoFocus value={input} onChange={(event) => { setInput(event.target.value); setPreview(null); }} onKeyDown={(event) => { if (event.key === "Escape") setOpen(false); }} placeholder="Add review billing PR" />
         <div className="omnibox-actions"><button className="primary" type="submit" disabled={!input.trim() || busy}>Save</button><button type="button" onClick={() => void showMainWindow()}>Open full Pulse</button></div>
        </form>
      ) : null}

      {resultTasks.length ? <section className="pet-results"><div className="reminder-label">Matches</div>{resultTasks.slice(0, 4).map((task) => <button key={task.id} onClick={() => void openTaskContext(task.id, "open_context")}>{task.title}</button>)}</section> : null}
      {/* {message ? <div className="pet-message">{message}</div> : null} */}
      <div className="pet-row">
        <button className={`pet ${reminders.length ? "pet-due" : ""}`} onClick={() => setOpen((value) => !value)} onContextMenu={(event) => { event.preventDefault(); void showPetContextMenu(); }} aria-label="Open Pulse task entry"><img src="/pulse-firefly-256.png" alt="Pulse firefly" /></button>
      </div>
    </main>
  );
}
