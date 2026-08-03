import { Database, House, Inbox as InboxIcon, Settings as SettingsIcon, Sparkles } from "lucide-react";
import { useAppStore } from "@/store/useAppStore";

type AppSidebarProps = {
  onSyncSessions: () => void;
};

export function AppSidebar({ onSyncSessions }: AppSidebarProps) {
  const view = useAppStore((state) => state.view);
  const syncingSessions = useAppStore((state) => state.syncingSessions);
  const setView = useAppStore((state) => state.setView);
  const setCaptureOpen = useAppStore((state) => state.setCaptureOpen);
  const openInbox = useAppStore((state) => state.openInbox);

  return (
    <aside className="sidebar">
      <nav className="nav" aria-label="Primary navigation">
        <button className={view === "Home" ? "active" : ""} onClick={() => setView("Home")}>
          <House aria-hidden="true" />
          Home
        </button>
        <button className={view === "Inbox" ? "active" : ""} onClick={() => openInbox()}>
          <InboxIcon aria-hidden="true" />
          Inbox
        </button>
        <button className={view === "Copilot" ? "active" : ""} onClick={() => setView("Copilot")}>
          <Sparkles aria-hidden="true" />
          Task Copilot
        </button>
        <button className={view === "Sources" ? "active" : ""} onClick={() => setView("Sources")}>
          <Database aria-hidden="true" />
          Sources
        </button>
        <div className="nav-section-label">System</div>
        <button className={view === "Settings" ? "active" : ""} onClick={() => setView("Settings")}>
          <SettingsIcon aria-hidden="true" />
          Settings
        </button>
      </nav>
      <div className="sidebar-footer">
        <button className="capture-task" type="button" onClick={() => setCaptureOpen(true)}>
          Capture task
        </button>
        <button className="session-sync" type="button" onClick={onSyncSessions} disabled={syncingSessions}>
          {syncingSessions ? "Syncing sessions..." : "Sync latest sessions"}
        </button>
      </div>
    </aside>
  );
}
