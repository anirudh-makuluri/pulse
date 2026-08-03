import { type FormEvent, useEffect, useRef, useState } from "react";
import { ArrowUpRight, History, Plus } from "lucide-react";
import { copilotStart, getCopilotSession, listCopilotSessions, type CopilotResult, type CopilotSession, type CopilotStoredMessage } from "@/api";
import { useAppStore } from "@/store/useAppStore";

const SUGGESTIONS = [
  "What should I work on today?",
  "What is blocked?",
  "Show tasks related to authentication",
  "Which tasks are in progress?",
];

type ChatMessage = {
  id: string;
  question: string;
  result?: CopilotResult;
  error?: string;
  progress: string[];
};

export function CopilotPage() {
  const openTask = useAppStore((state) => state.openTask);
  const [question, setQuestion] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [sessions, setSessions] = useState<CopilotSession[]>([]);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const hasConversation = messages.length > 0;

  useEffect(() => () => socketRef.current?.close(), []);

  async function ask(event?: FormEvent<HTMLFormElement>) {
    event?.preventDefault();
    const query = question.trim();
    if (!query || loading) return;

    const id = crypto.randomUUID();
    setMessages((current) => [...current, { id, question: query, progress: ["Connecting to Task Copilot…"] }]);
    setQuestion("");
    setLoading(true);
    try {
      const operation = await copilotStart(query, conversationId);
      setConversationId(operation.conversation_id);
      const socket = new WebSocket(operation.websocket_url);
      socketRef.current = socket;
      let settled = false;
      const updateMessage = (update: (message: ChatMessage) => ChatMessage) => {
        setMessages((current) => current.map((message) => message.id === id ? update(message) : message));
      };
      socket.onopen = () => {
        socket.send(JSON.stringify({ operation_id: operation.operation_id, token: operation.token }));
      };
      socket.onmessage = (event) => {
        try {
          const progress = JSON.parse(String(event.data)) as { event?: string; message?: string; result?: CopilotResult };
          if (progress.event === "final" && progress.result) {
            settled = true;
            updateMessage((message) => ({ ...message, result: progress.result, progress: [] }));
            socket.close();
            setLoading(false);
            return;
          }
          if (progress.event === "error") {
            settled = true;
            updateMessage((message) => ({ ...message, error: progress.message || "Task Copilot could not complete the request.", progress: [] }));
            socket.close();
            setLoading(false);
            return;
          }
          if (progress.message) {
            const progressMessage = progress.message;
            updateMessage((message) => ({ ...message, progress: [...message.progress, progressMessage].slice(-4) }));
          }
        } catch {
          // Ignore malformed local progress frames; close/error still surface a failure.
        }
      };
      socket.onerror = () => {
        if (!settled) {
          settled = true;
          updateMessage((message) => ({ ...message, error: "Lost the local Task Copilot progress connection.", progress: [] }));
          setLoading(false);
        }
      };
      socket.onclose = () => {
        if (!settled && socketRef.current === socket) {
          settled = true;
          updateMessage((message) => ({ ...message, error: "Task Copilot closed before returning an answer.", progress: [] }));
          setLoading(false);
        }
      };
    } catch (reason) {
      setMessages((current) => current.map((message) => (
        message.id === id ? { ...message, error: String(reason), progress: [] } : message
      )));
      setLoading(false);
    }
  }

  async function toggleHistory() {
    if (historyOpen) {
      setHistoryOpen(false);
      return;
    }
    setHistoryOpen(true);
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      setSessions(await listCopilotSessions());
    } catch (reason) {
      setHistoryError(String(reason));
    } finally {
      setHistoryLoading(false);
    }
  }

  async function openSession(id: string) {
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      const detail = await getCopilotSession(id);
      setConversationId(detail.session.id);
      setMessages(messagesFromHistory(detail.messages));
      setHistoryOpen(false);
    } catch (reason) {
      setHistoryError(String(reason));
    } finally {
      setHistoryLoading(false);
    }
  }

  function startNewSession() {
    if (loading) return;
    setConversationId(null);
    setMessages([]);
    setQuestion("");
    setHistoryOpen(false);
  }

  const composer = (
    <form className="copilot-form" onSubmit={(event) => void ask(event)}>
      <label className="sr-only" htmlFor="copilot-question">Ask about your tasks</label>
      <div className="copilot-input-row">
        <input
          id="copilot-question"
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
          placeholder="Ask about your tasks"
          maxLength={1000}
          autoFocus
        />
        <button className="primary" type="submit" disabled={!question.trim() || loading}>
          {loading ? "Thinking…" : "Ask"}
        </button>
      </div>
    </form>
  );

  return (
    <div className={`panel-page copilot-page ${hasConversation ? "copilot-chat-state" : "copilot-empty-state"}`}>
      <div className="copilot-top-actions">
        <button type="button" onClick={() => void toggleHistory()} aria-expanded={historyOpen}>
          <History aria-hidden="true" /> History
        </button>
      </div>
      {historyOpen ? (
        <aside className="copilot-history" aria-label="Recent Copilot sessions">
          <div className="copilot-history-heading">
            <strong>Recent sessions</strong>
            <button type="button" onClick={startNewSession} disabled={loading}><Plus aria-hidden="true" /> New</button>
          </div>
          {historyLoading ? <p>Loading sessions…</p> : null}
          {historyError ? <p className="copilot-history-error">{historyError}</p> : null}
          {!historyLoading && !historyError && sessions.length === 0 ? <p>No saved Copilot sessions yet.</p> : null}
          {sessions.map((session) => (
            <button className="copilot-history-session" key={session.id} type="button" onClick={() => void openSession(session.id)}>
              <strong>{session.title}</strong>
              <small>{new Date(session.updated_at).toLocaleString()}</small>
            </button>
          ))}
        </aside>
      ) : null}
      {hasConversation ? (
        <>
          <div className="copilot-transcript" aria-live="polite">
            {messages.map((message) => (
              <article className="copilot-message" key={message.id}>
                <div className="copilot-user-message">{message.question}</div>
                {message.result ? (
                  <section className="copilot-answer">
                    <div className="copilot-answer-heading">
                      <span>Task Copilot</span>
                      <span>{message.result.backend === "heuristic" ? "Local fallback" : `Powered by ${message.result.backend}`}</span>
                    </div>
                    <p>{message.result.answer}</p>
                    <div className="copilot-citations">
                      <h2>Supporting tasks</h2>
                      {message.result.tasks.length ? message.result.tasks.map((task) => (
                        <button key={task.id} type="button" onClick={() => openTask(task)}>
                          <span>
                            <strong>{task.title}</strong>
                            <small>{task.status}{task.suggested_next_action ? ` · Next: ${task.suggested_next_action}` : ""}</small>
                          </span>
                          <ArrowUpRight aria-hidden="true" />
                        </button>
                      )) : <p className="copilot-no-citations">No supporting task was identified for this answer.</p>}
                    </div>
                  </section>
                ) : null}
                {message.error ? <div className="error">{message.error}</div> : null}
                {!message.result && !message.error ? (
                  <div className="copilot-progress" aria-label="Task Copilot progress">
                    {message.progress.map((progress, index) => <div key={`${progress}-${index}`}>{progress}</div>)}
                  </div>
                ) : null}
              </article>
            ))}
          </div>
          <div className="copilot-composer-dock">{composer}</div>
        </>
      ) : (
        <div className="copilot-welcome">
          <div className="copilot-welcome-copy">
            <div className="eyebrow">Task Copilot</div>
            <h1>What can I help you move forward?</h1>
            <p>Ask about your tasks, priorities, or blockers. I can read your work, but I can’t change it.</p>
          </div>
          {composer}
          <div className="copilot-empty">
            <p>Try one of these:</p>
            <div className="copilot-suggestions">
              {SUGGESTIONS.map((suggestion) => (
                <button key={suggestion} type="button" onClick={() => setQuestion(suggestion)}>{suggestion}</button>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function messagesFromHistory(messages: CopilotStoredMessage[]): ChatMessage[] {
  const turns: ChatMessage[] = [];
  for (const message of messages) {
    if (message.role === "user") {
      turns.push({ id: message.id, question: message.content, progress: [] });
      continue;
    }
    const latest = turns[turns.length - 1];
    if (latest) {
      latest.result = { answer: message.content, tasks: message.tasks, backend: message.backend || "heuristic" };
    }
  }
  return turns;
}
