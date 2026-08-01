import { setSourceEnabled } from "@/api";
import { Switch } from "@/components/ui/switch";
import { useAppStore } from "@/store/useAppStore";

type SourcesPageProps = {
  onRefresh: () => Promise<void>;
};

export function SourcesPage({ onRefresh }: SourcesPageProps) {
  const settings = useAppStore((state) => state.settings);
  const error = useAppStore((state) => state.error);
  const setError = useAppStore((state) => state.setError);

  async function updateSource(source: "claude" | "codex", enabled: boolean) {
    try {
      await setSourceEnabled(source, enabled);
      await onRefresh();
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div className="panel-page section-page">
      <div className="section-header">
        <div>
          <div className="eyebrow">Sources</div>
          <p>Choose which local session data Pulse watches.</p>
        </div>
        <button type="button" className="text-button" onClick={() => void onRefresh()}>
          Refresh
        </button>
      </div>
      {error ? <div className="error">{error}</div> : null}
      {!settings ? (
        <div className="empty-list">Loading sources…</div>
      ) : (
        <div className="section-content sources-page">
          <section className="home-card source-card">
            <div>
              <h2>Claude</h2>
              <p>Watch local Claude session files and infer task candidates with their evidence.</p>
            </div>
            <div className="source-toggle">
              <span>{settings.claude_enabled ? "Watching" : "Off"}</span>
              <Switch
                checked={settings.claude_enabled}
                onCheckedChange={(enabled) => void updateSource("claude", enabled)}
                aria-label="Watch Claude session files"
              />
            </div>
          </section>
          <section className="home-card source-card">
            <div>
              <h2>Codex</h2>
              <p>Watch local Codex session files and infer task candidates with their evidence.</p>
            </div>
            <div className="source-toggle">
              <span>{settings.codex_enabled ? "Watching" : "Off"}</span>
              <Switch
                checked={settings.codex_enabled}
                onCheckedChange={(enabled) => void updateSource("codex", enabled)}
                aria-label="Watch Codex session files"
              />
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
