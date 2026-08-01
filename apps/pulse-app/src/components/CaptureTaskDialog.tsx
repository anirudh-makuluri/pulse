import type { FormEvent } from "react";
import { useAppStore } from "@/store/useAppStore";

type CaptureTaskDialogProps = {
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
};

export function CaptureTaskDialog({ onSubmit }: CaptureTaskDialogProps) {
  const captureOpen = useAppStore((state) => state.captureOpen);
  const captureTitle = useAppStore((state) => state.captureTitle);
  const taskFilter = useAppStore((state) => state.taskFilter);
  const setCaptureOpen = useAppStore((state) => state.setCaptureOpen);
  const setCaptureTitle = useAppStore((state) => state.setCaptureTitle);

  if (!captureOpen) return null;

  return (
    <div className="capture-backdrop" role="presentation" onMouseDown={() => setCaptureOpen(false)}>
      <form
        className="capture-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="capture-task-title"
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={onSubmit}
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
          value={captureTitle}
          onChange={(event) => setCaptureTitle(event.target.value)}
          onKeyDown={(event) => { if (event.key === "Escape") setCaptureOpen(false); }}
          placeholder={taskFilter === "Today" ? "Add a task for today…" : "What needs your attention?"}
        />
        <div className="capture-dialog-actions">
          <button type="button" onClick={() => setCaptureOpen(false)}>Cancel</button>
          <button type="submit" className="primary">Add task</button>
        </div>
      </form>
    </div>
  );
}
