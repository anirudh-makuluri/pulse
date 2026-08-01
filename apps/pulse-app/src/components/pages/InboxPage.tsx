import { TaskPreview } from "@/components/TaskPreview";
import { TASK_FILTERS, useAppStore } from "@/store/useAppStore";

export function InboxPage() {
  const taskFilter = useAppStore((state) => state.taskFilter);
  const tasks = useAppStore((state) => state.tasks);
  const selectedId = useAppStore((state) => state.selectedId);
  const error = useAppStore((state) => state.error);
  const setTaskFilter = useAppStore((state) => state.setTaskFilter);
  const setDetail = useAppStore((state) => state.setDetail);
  const selectTask = useAppStore((state) => state.selectTask);

  return (
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
                selectTask(null);
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
              onOpen={() => selectTask(task.id)}
            />
          ))
        )}
      </div>
    </div>
  );
}
