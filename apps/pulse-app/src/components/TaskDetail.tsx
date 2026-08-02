import { outcomeLabel, sourceClass } from "@/components/TaskPreview";
import type { TaskOutcome } from "@/api";
import { useAppStore } from "@/store/useAppStore";
import type { ActivityTimeline, TaskStatus } from "@/types";
import type { ReactNode } from "react";

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
      title: [session.agent, session.application].filter(Boolean).join(" · ") || "Work session",
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
      detail: checkpoint.next_actions.length ? `Next: ${checkpoint.next_actions.join(" · ")}` : undefined,
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

type TaskDetailProps = {
  onMove: (status: TaskStatus) => void;
  onOutcome: (outcome: TaskOutcome) => void;
  onDone: () => void;
  onDelete: () => void;
};

export function TaskDetail({ onMove, onOutcome, onDone, onDelete }: TaskDetailProps) {
  const detail = useAppStore((state) => state.detail);

  if (!detail) {
    return <aside className="detail"><div className="empty">Select a task to see detail and evidence.</div></aside>;
  }

  const timeline = timelineEntries(detail);
  const outcome = outcomeLabel(detail.task.sync_outcome);

  return (
    <aside className="detail">
      <h2>{detail.task.title}</h2>
      <div className="task-meta">
        <span className="pill">{detail.task.status}</span>
        <span className={`pill ${sourceClass(detail.task.source)}`}>{detail.task.source}</span>
        {outcome ? <span className={`pill outcome-${detail.task.sync_outcome}`}>{outcome}</span> : null}
        {detail.task.confidence != null ? <span className="pill">conf {(detail.task.confidence * 100).toFixed(0)}%</span> : null}
      </div>

      <div className="task-actions">
        <button onClick={() => onMove("Today")}>Today</button>
        <button onClick={() => onMove("Next")}>Next</button>
        <button onClick={() => onMove("Waiting")}>Waiting</button>
        <button onClick={() => onMove("Inbox")}>Inbox</button>
        <button className="primary" onClick={onDone}>Done</button>
        <button className="danger" onClick={onDelete}>Delete</button>
      </div>

      <div className="task-outcome" aria-label="Task outcome">
        <span>Outcome</span>
        <button
          className={detail.task.sync_outcome === "in_progress" ? "selected" : ""}
          onClick={() => onOutcome("in_progress")}
        >
          In progress
        </button>
        <button
          className={detail.task.sync_outcome === "completed" ? "selected" : ""}
          onClick={() => onOutcome("completed")}
        >
          Completed
        </button>
      </div>

      {detail.task.notes ? <DetailSection title="Notes"><pre>{detail.task.notes}</pre></DetailSection> : null}
      {detail.task.suggested_next_action ? <DetailSection title="Suggested next"><pre>{detail.task.suggested_next_action}</pre></DetailSection> : null}

      <DetailSection title="Chronological timeline">
        {timeline.length === 0 ? (
          <div className="empty-list" style={{ padding: 12 }}>No activity recorded yet.</div>
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
                  {entry.detail ? <div className="timeline-detail">{entry.detail}</div> : null}
                </div>
              </article>
            ))}
          </div>
        )}
      </DetailSection>

      <DetailSection title="Ids">
        <pre>{detail.task.id}{"\n"}updated {new Date(detail.task.updated_at).toLocaleString()}</pre>
      </DetailSection>
    </aside>
  );
}

function DetailSection({ title, children }: { title: string; children: ReactNode }) {
  return <div className="detail-section"><h3>{title}</h3>{children}</div>;
}
