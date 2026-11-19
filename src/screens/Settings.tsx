import { Card } from "../components/Card";
import { SettingsIcon } from "../components/Icons";
import "./Settings.css";

export function Settings() {
  return (
    <div className="settings">
      <h2 className="settings-title">
        <SettingsIcon /> Settings
      </h2>

      <div className="settings-grid">
        <Card className="settings-card">
          <h3>Appearance</h3>
          <p>Dark mode is the only available theme in this prototype.</p>
          <div className="settings-row">
            <span>Theme</span>
            <span className="settings-value">Dark</span>
          </div>
        </Card>

        <Card className="settings-card">
          <h3>Local authority</h3>
          <p>Authorized lab root for file broker demonstrations.</p>
          <div className="settings-row">
            <span>Root</span>
            <code className="settings-value">C:\Users\tonig\paige-desktop-lab</code>
          </div>
        </Card>

        <Card className="settings-card">
          <h3>Connected services</h3>
          <p>Cloud integrations are not yet activated.</p>
          <div className="settings-row">
            <span>Linear</span>
            <span className="settings-value settings-value-pending">Not connected</span>
          </div>
          <div className="settings-row">
            <span>GitHub</span>
            <span className="settings-value settings-value-pending">Not connected</span>
          </div>
          <div className="settings-row">
            <span>Supabase</span>
            <span className="settings-value settings-value-pending">Not connected</span>
          </div>
          <div className="settings-row">
            <span>Vercel</span>
            <span className="settings-value settings-value-pending">Not connected</span>
          </div>
        </Card>

        <Card className="settings-card">
          <h3>About</h3>
          <div className="settings-row">
            <span>Version</span>
            <span className="settings-value">1B-UI prototype</span>
          </div>
          <div className="settings-row">
            <span>Build</span>
            <span className="settings-value">Tauri 2 + React + Rust</span>
          </div>
        </Card>
      </div>
    </div>
  );
}
