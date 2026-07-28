import { FormEvent, useEffect, useRef, useState } from "react";
import { Check, ChevronRight, ExternalLink, LoaderCircle, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
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
  const [reminders, setReminders] = useState<Reminder[]>([]);
  const [resultTasks, setResultTasks] = useState<Task[]>([]);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const surfaced = useRef(new Set<string>());
  const isExpanded = open || reminders.length > 0 || resultTasks.length > 0 || Boolean(message);

  useEffect(() => {
    document.documentElement.classList.add("pet-window");
    return () => document.documentElement.classList.remove("pet-window");
  }, []);

  useEffect(() => {
    void setPetExpanded(isExpanded).catch((error) => setMessage(String(error)));
  }, [isExpanded]);

  useEffect(() => {
    let disposed = false;
    let checking = false;
    const poll = async () => {
      // A session sync can temporarily hold SQLite's write lock. Reminder
      // polling is best-effort, so never turn that transient condition into a
      // visible pet panel (which also resizes the companion window).
      if (checking) return;
      checking = true;
      try {
        const items = await dueReminders();
        if (disposed) return;
        const active = new Set(items.map((item) => `${item.id}:${item.due_at}`));
        surfaced.current.forEach((key) => { if (!active.has(key)) surfaced.current.delete(key); });
        const next = items.find((item) => !surfaced.current.has(`${item.id}:${item.due_at}`));
        if (next) {
          surfaced.current.add(`${next.id}:${next.due_at}`);
        }
        setReminders(items);
      } catch {
        // Keep the pet in its current state and retry on the next interval.
      } finally {
        checking = false;
      }
    };
    void poll();
    const id = window.setInterval(poll, 10_000);
    return () => {
      disposed = true;
      window.clearInterval(id);
    };
  }, []);

  async function submit(event: FormEvent) {
    event.preventDefault();
    let nextPreview = preview;
    if (!nextPreview) {
      if (!input.trim()) return;
      try {
        nextPreview = await previewOmnibox(input);
        setPreview(nextPreview);
      } catch (error) {
        setMessage(String(error));
        return;
      }
    }
    setBusy(true);
    try {
      const result = await executeOmnibox(input, null, nextPreview.context);
      setMessage(result.message);
      setResultTasks(result.tasks);
      setInput("");
      setPreview(null);
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

  function dismissPanel() {
    setOpen(false);
    setPreview(null);
    setResultTasks([]);
    setMessage("");
  }

  return (
    <main className={`pet-app ${isExpanded ? "pet-expanded" : ""}`} aria-live="polite">
      {reminders.length ? (
        <Card className="pet-card reminder-card">
          <CardHeader className="pet-card-header">
            <div className="pet-eyebrow"><Sparkles aria-hidden="true" /> Reminder due</div>
            <CardTitle className="pet-card-title">{reminders[0].title}</CardTitle>
          </CardHeader>
          <CardContent className="pet-card-content reminder-actions">
            <Button size="sm" variant="outline" onClick={() => void runReminder(reminders[0], "open_context")}>Open context</Button>
            <Button size="sm" variant="ghost" onClick={() => void runReminder(reminders[0], "snooze")}>Snooze</Button>
            <Button size="sm" onClick={() => void runReminder(reminders[0], "done")}><Check aria-hidden="true" />Done</Button>
          </CardContent>
        </Card>
      ) : null}

      {open ? (
        <Card className="pet-card omnibox">
          <CardHeader className="pet-card-header">
            <div>
              <CardTitle className="pet-card-title">What should Pulse remember?</CardTitle>
              <CardDescription>Capture the next step before it slips away.</CardDescription>
            </div>
          </CardHeader>
          <CardContent className="pet-card-content">
            <form onSubmit={(event) => void submit(event)} aria-label="Pulse omnibox">
              <Input autoFocus value={input} onChange={(event) => { setInput(event.target.value); setPreview(null); }} onKeyDown={(event) => { if (event.key === "Escape") dismissPanel(); }} placeholder="Review billing pull request" />
              <div className="omnibox-actions">
                <Button type="submit" disabled={!input.trim() || busy}>{busy ? <LoaderCircle className="pet-spinner" aria-hidden="true" /> : null}Save</Button>
                <Button type="button" variant="outline" onClick={() => void showMainWindow()}><ExternalLink aria-hidden="true" />Open full Pulse</Button>
              </div>
            </form>
          </CardContent>
        </Card>
      ) : null}

      {resultTasks.length ? (
        <Card className="pet-card pet-results">
          <CardHeader className="pet-card-header"><div className="pet-eyebrow">Matches</div><CardTitle className="pet-card-title">Related tasks</CardTitle></CardHeader>
          <CardContent className="pet-card-content pet-results-list">
            {resultTasks.slice(0, 4).map((task) => <Button key={task.id} variant="ghost" className="pet-result" onClick={() => void openTaskContext(task.id, "open_context")}><span>{task.title}</span><ChevronRight aria-hidden="true" /></Button>)}
          </CardContent>
        </Card>
      ) : null}
      {message ? <div className="pet-message" role="status">{message}</div> : null}
      <div className="pet-row">
        <Button className={`pet ${reminders.length ? "pet-due" : ""}`} variant="ghost" size="icon" onClick={() => setOpen((value) => !value)} onContextMenu={(event) => { event.preventDefault(); void showPetContextMenu(); }} aria-label="Open Pulse task entry"><img src="/pulse-logo.png" alt="Pulse logo" /></Button>
      </div>
    </main>
  );
}
