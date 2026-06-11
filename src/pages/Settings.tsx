import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { zh } from "../i18n/zh";
import { ExtensionsPanel } from "../components/settings/ExtensionsPanel";
import { DismissibleNotice } from "../components/DismissibleNotice";
import type { AppContextSettings, AppInfo } from "../types";

type SettingsTab = "general" | "context" | "agent" | "shell" | "workspace" | "extensions";

const SETTINGS_TABS: SettingsTab[] = [
  "general",
  "context",
  "agent",
  "shell",
  "workspace",
  "extensions",
];

export function SettingsPage() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<AppContextSettings | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");

  useEffect(() => {
    invoke<AppInfo>("get_app_info").then(setInfo).catch(console.error);
    invoke<AppContextSettings>("get_context_settings")
      .then(setSettings)
      .catch(console.error);
  }, []);

  async function saveSettings() {
    if (!settings) return;
    try {
      await invoke("save_context_settings_cmd", { settings });
      setMessage(zh.settings.savedSettings);
    } catch (e) {
      setMessage(String(e));
    }
  }

  function renderEditableFooter() {
    return (
      <div className="settings-panel-footer">
        <button type="button" className="btn-primary" onClick={() => void saveSettings()}>
          {zh.settings.saveSettings}
        </button>
      </div>
    );
  }

  function renderPanelContent() {
    switch (activeTab) {
      case "general":
        return (
          <div className="settings-panel-body">
            <section className="settings-panel-section">
              <h3>{zh.settings.application}</h3>
              <dl className="settings-dl">
                <dt>{zh.settings.name}</dt>
                <dd>{info?.name ?? zh.appName}</dd>
                <dt>{zh.settings.version}</dt>
                <dd>{info?.version ?? "0.1.0"}</dd>
                <dt>{zh.settings.dataDir}</dt>
                <dd>{info?.dataDir ?? zh.settings.loading}</dd>
              </dl>
            </section>
            <section className="settings-panel-section">
              <h3>{zh.settings.storage}</h3>
              <ul className="settings-list">
                {zh.settings.storageItems.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </section>
            <section className="settings-panel-section">
              <h3>{zh.settings.coming}</h3>
              <ul className="settings-list">
                {zh.settings.comingItems.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </section>
          </div>
        );

      case "context":
        if (!settings) {
          return <p className="muted settings-panel-loading">{zh.settings.loading}</p>;
        }
        return (
          <>
            <div className="settings-panel-body settings-form">
              <label className="settings-field">
                <span>{zh.settings.recentTurns}</span>
                <input
                  type="number"
                  min={2}
                  max={50}
                  value={settings.recentTurns}
                  onChange={(e) =>
                    setSettings({ ...settings, recentTurns: Number(e.target.value) || 12 })
                  }
                />
              </label>
              <label className="settings-field">
                <span>{zh.settings.tokenThreshold}</span>
                <input
                  type="number"
                  min={10000}
                  step={5000}
                  value={settings.tokenThreshold}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      tokenThreshold: Number(e.target.value) || 60000,
                    })
                  }
                />
              </label>
              <label className="settings-field settings-checkbox">
                <input
                  type="checkbox"
                  checked={settings.summaryEnabled}
                  onChange={(e) =>
                    setSettings({ ...settings, summaryEnabled: e.target.checked })
                  }
                />
                <span>{zh.settings.summaryEnabled}</span>
              </label>
            </div>
            {renderEditableFooter()}
          </>
        );

      case "agent":
        if (!settings) {
          return <p className="muted settings-panel-loading">{zh.settings.loading}</p>;
        }
        return (
          <>
            <div className="settings-panel-body settings-form">
              <label className="settings-field">
                <span>{zh.settings.agentMaxIterations}</span>
                <input
                  type="number"
                  min={1}
                  max={50}
                  value={settings.agentMaxIterations}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      agentMaxIterations: Number(e.target.value) || 25,
                    })
                  }
                />
              </label>
              <label className="settings-field">
                <span>{zh.settings.agentSubagentMaxIterations}</span>
                <input
                  type="number"
                  min={1}
                  max={30}
                  value={settings.agentSubagentMaxIterations}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      agentSubagentMaxIterations: Number(e.target.value) || 12,
                    })
                  }
                />
              </label>
              <label className="settings-field settings-checkbox">
                <input
                  type="checkbox"
                  checked={settings.agentEnabledDefault}
                  onChange={(e) =>
                    setSettings({ ...settings, agentEnabledDefault: e.target.checked })
                  }
                />
                <span>{zh.settings.agentEnabledDefault}</span>
              </label>
            </div>
            {renderEditableFooter()}
          </>
        );

      case "shell":
        if (!settings) {
          return <p className="muted settings-panel-loading">{zh.settings.loading}</p>;
        }
        return (
          <>
            <div className="settings-panel-body settings-form">
              <label className="settings-field settings-checkbox">
                <input
                  type="checkbox"
                  checked={settings.shellAutoReadonly}
                  disabled={settings.shellAlwaysConfirm}
                  onChange={(e) =>
                    setSettings({ ...settings, shellAutoReadonly: e.target.checked })
                  }
                />
                <span>{zh.settings.shellAutoReadonly}</span>
              </label>
              <label className="settings-field settings-checkbox">
                <input
                  type="checkbox"
                  checked={settings.shellAlwaysConfirm}
                  onChange={(e) =>
                    setSettings({ ...settings, shellAlwaysConfirm: e.target.checked })
                  }
                />
                <span>{zh.settings.shellAlwaysConfirm}</span>
              </label>
              <label className="settings-field settings-field-wide">
                <span>{zh.settings.shellExtraAllowlist}</span>
                <textarea
                  className="settings-textarea"
                  rows={6}
                  value={settings.shellExtraAllowlist}
                  onChange={(e) =>
                    setSettings({ ...settings, shellExtraAllowlist: e.target.value })
                  }
                  placeholder={"my-cli verify\n./scripts/check.sh"}
                />
                <p className="muted settings-hint">{zh.settings.shellBuiltinAllowlistHint}</p>
              </label>
            </div>
            {renderEditableFooter()}
          </>
        );

      case "workspace":
        if (!settings) {
          return <p className="muted settings-panel-loading">{zh.settings.loading}</p>;
        }
        return (
          <>
            <div className="settings-panel-body settings-form">
              <label className="settings-field">
                <span>{zh.settings.workspaceOutsideRead}</span>
                <select
                  value={settings.workspaceOutsideRead}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      workspaceOutsideRead: e.target.value as "block" | "confirm" | "allow",
                    })
                  }
                >
                  <option value="block">{zh.settings.workspaceOutsideReadBlock}</option>
                  <option value="confirm">{zh.settings.workspaceOutsideReadConfirm}</option>
                  <option value="allow">{zh.settings.workspaceOutsideReadAllow}</option>
                </select>
              </label>
              <label className="settings-field">
                <span>{zh.settings.workspaceOutsideWrite}</span>
                <select
                  value={settings.workspaceOutsideWrite}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      workspaceOutsideWrite: e.target.value as "block" | "confirm",
                    })
                  }
                >
                  <option value="block">{zh.settings.workspaceOutsideWriteBlock}</option>
                  <option value="confirm">{zh.settings.workspaceOutsideWriteConfirm}</option>
                </select>
              </label>
            </div>
            {renderEditableFooter()}
          </>
        );

      case "extensions":
        return (
          <div className="settings-panel-body">
            <ExtensionsPanel />
          </div>
        );

      default:
        return null;
    }
  }

  return (
    <div className="page settings-page">
      <header className="page-header">
        <h2>{zh.settings.title}</h2>
        <p className="muted">{zh.settings.subtitle}</p>
      </header>

      {message && (
        <DismissibleNotice variant="banner" onDismiss={() => setMessage(null)}>
          {message}
        </DismissibleNotice>
      )}

      <div className="settings-layout">
        <nav className="settings-nav card" aria-label={zh.settings.title}>
          {SETTINGS_TABS.map((tab) => (
            <button
              key={tab}
              type="button"
              className={`settings-nav-item ${activeTab === tab ? "active" : ""}`}
              onClick={() => setActiveTab(tab)}
            >
              {zh.settings.tabs[tab]}
            </button>
          ))}
        </nav>

        <section className="card settings-panel">
          <header className="settings-panel-header">
            <h3>{zh.settings.tabs[activeTab]}</h3>
            <p className="muted">{zh.settings.tabDesc[activeTab]}</p>
          </header>
          {renderPanelContent()}
        </section>
      </div>
    </div>
  );
}
