import type { Task } from "@/types";

function shortId(id: string): string {
  return id.slice(0, 8);
}

export function sourceClass(source: string): string {
  const normalized = source.toLowerCase();
  if (normalized === "claude") return "source-claude";
  if (normalized === "codex") return "source-codex";
  return "source-manual";
}

export function outcomeLabel(outcome: Task["sync_outcome"]): string | null {
  if (outcome === "in_progress") return "In progress";
  if (outcome === "completed") return "Completed";
  if (outcome === "unclear") return "Unclear";
  return null;
}

type TaskPreviewProps = {
  task: Task;
  onOpen: () => void;
  compact?: boolean;
  selected?: boolean;
};

export function TaskPreview({
  task,
  onOpen,
  compact = false,
  selected = false,
}: TaskPreviewProps) {
  const outcome = outcomeLabel(task.sync_outcome);

  return (
    <button className={compact ? "home-task" : `task ${selected ? "selected" : ""}`} onClick={onOpen}>
      <div className="task-title">{task.title}</div>
      <div className="task-meta">
        {/* {!compact ? <span className="pill">{shortId(task.id)}</span> : null} */}
        <span className="pill">{task.status}</span>
        <span className={`pill ${sourceClass(task.source)}`}>{task.source}</span>
        {outcome ? <span className={`pill outcome-${task.sync_outcome}`}>{outcome}</span> : null}
        {task.project ? <span className="pill">{task.project}</span> : null}
        {/* {!compact && task.confidence != null ? (
          <span className="pill">conf {(task.confidence * 100).toFixed(0)}%</span>
        ) : null} */}
      </div>
      {compact && task.suggested_next_action ? (
        <div className="home-task-next">{task.suggested_next_action}</div>
      ) : null}
    </button>
  );
}
