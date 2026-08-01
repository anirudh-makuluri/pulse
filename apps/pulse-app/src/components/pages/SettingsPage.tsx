import { exportHistory, privacyAcknowledge, setPetVisible } from "@/api";
import { Switch } from "@/components/ui/switch";
import { useAppStore } from "@/store/useAppStore";

type SettingsPageProps = {
  onRefresh: () => Promise<void>;
  onCheckForUpdates: () => void;
};

export function SettingsPage({ onRefresh, onCheckForUpdates }: SettingsPageProps) {
  const settings = useAppStore((state) => state.settings);
  const error = useAppStore((state) => state.error);
  const exportPath = useAppStore((state) => state.exportPath);
  const updateStatus = useAppStore((state) => state.updateStatus);
  const checkingForUpdate = useAppStore((state) => state.checkingForUpdate);
  const setError = useAppStore((state) => state.setError);
  const setExportPath = useAppStore((state) => state.setExportPath);

  async function acknowledgePrivacy() {
    try {
      await privacyAcknowledge();
      await onRefresh();
    } catch (err) {
      setError(String(err));
    }
  }

  async function exportData(format: "json" | "md") {
    try {
      setExportPath(await exportHistory(format));
    } catch (err) {
      setError(String(err));
    }
  }

  async function updatePetVisibility(visible: boolean) {
    try {
      await setPetVisible(visible);
      await onRefresh();
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div className="panel-page section-page">
      <div className="section-header">
        <div>
          <div className="eyebrow">Settings</div>
          <p>Manage privacy, exports, and local app details.</p>
        </div>
        <button type="button" className="text-button" onClick={() => void onRefresh()}>
          Refresh
        </button>
      </div>
      {error ? <div className="error">{error}</div> : null}
      {!settings ? (
        <div className="empty-list">Loading settings…</div>
      ) : (
        <div className="section-content settings">
          <section className="home-card settings-card">
            <h3>Privacy / LLM</h3>
            <p className="muted">
              Backend: <code>{settings.llm_backend}</code>
              {settings.llm_path ? <> · <code>{settings.llm_path}</code></> : null}
            </p>
            <p className="muted">{settings.llm_reason}</p>
            <p className="muted">Privacy ack: {settings.privacy_ack ? "yes" : "no (heuristic only)"}</p>
            {!settings.privacy_ack ? (
              <button type="button" className="primary" onClick={() => void acknowledgePrivacy()}>
                Acknowledge remote LLM risk
              </button>
            ) : null}
          </section>

          <section className="home-card settings-card">
            <h3>Export</h3>
            <div className="task-actions">
              <button type="button" onClick={() => void exportData("json")}>Export JSON</button>
              <button type="button" onClick={() => void exportData("md")}>Export Markdown</button>
            </div>
            {exportPath ? <p className="muted">Wrote: <code>{exportPath}</code></p> : null}
          </section>

          <section className="home-card settings-card desktop-pet-setting">
            <div className="desktop-pet-copy">
              <h3>Desktop companion</h3>
              <p className="muted">Keep the Pulse pet at the bottom-right of your screen. Pulse remains available from the system tray when it is hidden.</p>
            </div>
            <Switch checked={settings.show_pet} onCheckedChange={(visible) => void updatePetVisibility(visible)} aria-label="Show desktop pet" />
          </section>

          <section className="home-card settings-card">
            <h3>Software update</h3>
            <p className="muted">Check GitHub Releases for a signed Pulse update and install it automatically.</p>
            <button type="button" className="primary" onClick={onCheckForUpdates} disabled={checkingForUpdate}>
              {checkingForUpdate ? "Checking for updates…" : "Check for updates"}
            </button>
            {updateStatus ? <p className="muted" role="status">{updateStatus}</p> : null}
          </section>

          <section className="home-card settings-card">
            <h3>Paths</h3>
            <p className="muted">Data: <code>{settings.data_dir}</code></p>
            <p className="muted">Config: <code>{settings.config_path}</code></p>
            <p className="muted">{settings.service_line}</p>
          </section>
        </div>
      )}
    </div>
  );
}
