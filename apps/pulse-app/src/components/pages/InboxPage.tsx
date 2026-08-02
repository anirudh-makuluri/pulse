import { useEffect, useState } from "react";
import { Search } from "lucide-react";
import { semanticSearch } from "@/api";
import { TaskPreview } from "@/components/TaskPreview";
import type { SemanticSearchResult } from "@/types";
import { TASK_FILTERS, useAppStore } from "@/store/useAppStore";

const SEARCH_DEBOUNCE_MS = 350;
const MIN_QUERY_LENGTH = 3;

export function InboxPage() {
  const taskFilter = useAppStore((state) => state.taskFilter);
  const tasks = useAppStore((state) => state.tasks);
  const selectedId = useAppStore((state) => state.selectedId);
  const error = useAppStore((state) => state.error);
  const setTaskFilter = useAppStore((state) => state.setTaskFilter);
  const setDetail = useAppStore((state) => state.setDetail);
  const selectTask = useAppStore((state) => state.selectTask);
  const [query, setQuery] = useState("");
  const [semanticResults, setSemanticResults] = useState<SemanticSearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);

  useEffect(() => {
    const trimmedQuery = query.trim();
    if (trimmedQuery.length < MIN_QUERY_LENGTH) {
      setSemanticResults([]);
      setSearching(false);
      setSearchError(null);
      return;
    }

    let current = true;
    const timeout = window.setTimeout(() => {
      setSearching(true);
      setSearchError(null);
      void semanticSearch(trimmedQuery)
        .then((results) => {
          if (current) setSemanticResults(results);
        })
        .catch((error) => {
          if (current) {
            setSemanticResults([]);
            setSearchError(String(error));
          }
        })
        .finally(() => {
          if (current) setSearching(false);
        });
    }, SEARCH_DEBOUNCE_MS);

    return () => {
      current = false;
      window.clearTimeout(timeout);
    };
  }, [query]);

  const hasSearchQuery = query.trim().length >= MIN_QUERY_LENGTH;

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
      <div className="semantic-search">
        <Search aria-hidden="true" />
        <input
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search related activities"
          aria-label="Search related activities"
        />
        {searching ? <span className="semantic-search-status">Searching…</span> : null}
      </div>
      {hasSearchQuery ? (
        <section className="semantic-results" aria-live="polite">
          <div className="semantic-results-heading">
            <span>Related activities</span>
            <span>Semantic search</span>
          </div>
          {searchError ? <div className="semantic-search-error">{searchError}</div> : null}
          {!searching && !searchError && semanticResults.length === 0 ? (
            <div className="semantic-empty">No related activities found yet.</div>
          ) : null}
          {semanticResults.map((result) => (
            <div className="semantic-result" key={result.task.id}>
              <TaskPreview
                task={result.task}
                selected={selectedId === result.task.id}
                onOpen={() => selectTask(result.task.id)}
              />
              <span className="semantic-match">Matched {result.source_type}</span>
            </div>
          ))}
        </section>
      ) : null}
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
