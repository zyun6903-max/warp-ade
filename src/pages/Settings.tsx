import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { zh } from "../i18n/zh";
import type {
  AppContextSettings,
  AppInfo,
  CodeIndexStatus,
  McpServer,
  McpTestResult,
  Provider,
  ShellLogEntry,
  ToolAuditEntry,
} from "../types";

function formatLogTime(ts: number): string {
  return new Date(ts * 1000).toLocaleString();
}

type McpDraft = {
  id?: string;
  name: string;
  command: string;
  argsText: string;
  enabled: boolean;
};

const emptyMcpDraft = (): McpDraft => ({
  name: "",
  command: "",
  argsText: "",
  enabled: true,
});

export function SettingsPage() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<AppContextSettings | null>(null);
  const [shellLogs, setShellLogs] = useState<ShellLogEntry[]>([]);
  const [toolAuditLogs, setToolAuditLogs] = useState<ToolAuditEntry[]>([]);
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [mcpDraft, setMcpDraft] = useState<McpDraft | null>(null);
  const [mcpTesting, setMcpTesting] = useState<string | null>(null);
  const [hasWebSearchKey, setHasWebSearchKey] = useState(false);
  const [webSearchKey, setWebSearchKey] = useState("");
  const [webSearchTesting, setWebSearchTesting] = useState(false);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [semanticWorkspace, setSemanticWorkspace] = useState("");
  const [semanticStatus, setSemanticStatus] = useState<CodeIndexStatus | null>(null);
  const [semanticRebuilding, setSemanticRebuilding] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function refreshMcp() {
    const list = await invoke<McpServer[]>("list_mcp_servers");
    setMcpServers(list);
  }

  useEffect(() => {
    invoke<AppInfo>("get_app_info").then(setInfo).catch(console.error);
    invoke<AppContextSettings>("get_context_settings")
      .then(setSettings)
      .catch(console.error);
    invoke<ShellLogEntry[]>("list_shell_audit_log", { limit: 50 })
      .then(setShellLogs)
      .catch(console.error);
    invoke<ToolAuditEntry[]>("list_tool_audit_log", { limit: 50 })
      .then(setToolAuditLogs)
      .catch(console.error);
    void refreshMcp().catch(console.error);
    invoke<boolean>("has_web_search_key")
      .then(setHasWebSearchKey)
      .catch(console.error);
    invoke<Provider[]>("list_providers")
      .then(setProviders)
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

  async function saveMcpDraft() {
    if (!mcpDraft || !mcpDraft.name.trim() || !mcpDraft.command.trim()) return;
    try {
      const args = mcpDraft.argsText
        .split("\n")
        .map((l) => l.trim())
        .filter(Boolean);
      await invoke<McpServer>("save_mcp_server", {
        input: {
          id: mcpDraft.id ?? null,
          name: mcpDraft.name.trim(),
          command: mcpDraft.command.trim(),
          args,
          env: {},
          enabled: mcpDraft.enabled,
        },
      });
      setMcpDraft(null);
      await refreshMcp();
      setMessage(zh.settings.mcpSave);
    } catch (e) {
      setMessage(String(e));
    }
  }

  async function testMcp(id: string) {
    setMcpTesting(id);
    try {
      const result = await invoke<McpTestResult>("test_mcp_server", { id });
      setMessage(result.ok ? zh.settings.mcpTestOk(result.toolCount) : result.message);
    } catch (e) {
      setMessage(String(e));
    } finally {
      setMcpTesting(null);
    }
  }

  async function deleteMcp(id: string) {
    try {
      await invoke("delete_mcp_server", { id });
      await refreshMcp();
    } catch (e) {
      setMessage(String(e));
    }
  }

  async function importCursorMcp() {
    try {
      const count = await invoke<number>("import_cursor_mcp_servers");
      await refreshMcp();
      setMessage(zh.settings.mcpImportOk(count));
    } catch (e) {
      setMessage(String(e));
    }
  }

  function editMcp(server: McpServer) {
    setMcpDraft({
      id: server.id,
      name: server.name,
      command: server.command,
      argsText: server.args.join("\n"),
      enabled: server.enabled,
    });
  }

  async function saveWebSearchKey() {
    if (!webSearchKey.trim()) return;
    try {
      await invoke("save_web_search_key", { key: webSearchKey.trim() });
      setWebSearchKey("");
      setHasWebSearchKey(true);
      setMessage(zh.settings.webSearchKeySaved);
    } catch (e) {
      setMessage(String(e));
    }
  }

  async function testWebSearch() {
    setWebSearchTesting(true);
    try {
      const preview = await invoke<string>("test_web_search");
      setMessage(preview);
    } catch (e) {
      setMessage(String(e));
    } finally {
      setWebSearchTesting(false);
    }
  }

  async function refreshSemanticStatus() {
    const path = semanticWorkspace.trim();
    if (!path) {
      setSemanticStatus(null);
      return;
    }
    try {
      const status = await invoke<CodeIndexStatus>("get_semantic_index_status", {
        workspacePath: path,
      });
      setSemanticStatus(status);
    } catch (e) {
      setMessage(String(e));
    }
  }

  async function rebuildSemanticIndex() {
    const path = semanticWorkspace.trim();
    if (!path) return;
    setSemanticRebuilding(true);
    try {
      const status = await invoke<CodeIndexStatus>("rebuild_semantic_index", {
        workspacePath: path,
      });
      setSemanticStatus(status);
      setMessage(
        zh.settings.semanticSearchStatus(status.fileCount, status.chunkCount),
      );
    } catch (e) {
      setMessage(String(e));
    } finally {
      setSemanticRebuilding(false);
    }
  }

  const embeddingProviders = providers.filter(
    (p) => p.enabled && p.hasKey && p.apiFormat !== "anthropic_messages",
  );

  return (
    <div className="page">
      <header className="page-header">
        <h2>{zh.settings.title}</h2>
        <p className="muted">{zh.settings.subtitle}</p>
      </header>

      {message && <div className="info-banner">{message}</div>}

      <section className="card settings-grid">
        <div>
          <h3>{zh.settings.application}</h3>
          <dl>
            <dt>{zh.settings.name}</dt>
            <dd>{info?.name ?? zh.appName}</dd>
            <dt>{zh.settings.version}</dt>
            <dd>{info?.version ?? "0.1.0"}</dd>
            <dt>{zh.settings.dataDir}</dt>
            <dd>{info?.dataDir ?? zh.settings.loading}</dd>
          </dl>
        </div>
        <div>
          <h3>{zh.settings.storage}</h3>
          <ul>
            {zh.settings.storageItems.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
        <div>
          <h3>{zh.settings.coming}</h3>
          <ul>
            {zh.settings.comingItems.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
      </section>

      {settings && (
        <section className="card settings-form">
          <h3>{zh.settings.contextTitle}</h3>
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
                setSettings({ ...settings, tokenThreshold: Number(e.target.value) || 60000 })
              }
            />
          </label>
          <label className="settings-field settings-checkbox">
            <input
              type="checkbox"
              checked={settings.summaryEnabled}
              onChange={(e) => setSettings({ ...settings, summaryEnabled: e.target.checked })}
            />
            <span>{zh.settings.summaryEnabled}</span>
          </label>

          <h3>{zh.settings.agentTitle}</h3>
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

          <h3>{zh.settings.shellTitle}</h3>
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
              rows={4}
              value={settings.shellExtraAllowlist}
              onChange={(e) =>
                setSettings({ ...settings, shellExtraAllowlist: e.target.value })
              }
              placeholder={"pnpm test\ncargo check"}
            />
          </label>

          <h3>{zh.settings.workspaceOutsideTitle}</h3>
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

          <button type="button" className="btn-primary" onClick={() => void saveSettings()}>
            {zh.settings.saveSettings}
          </button>
        </section>
      )}

      <section className="card settings-mcp">
        <div className="settings-mcp-header">
          <div>
            <h3>{zh.settings.mcpTitle}</h3>
            <p className="muted">{zh.settings.mcpSubtitle}</p>
          </div>
          <div className="settings-mcp-actions">
            <button type="button" className="btn-ghost" onClick={() => void importCursorMcp()}>
              {zh.settings.mcpImportCursor}
            </button>
            <button type="button" className="btn-primary" onClick={() => setMcpDraft(emptyMcpDraft())}>
              {zh.settings.mcpAdd}
            </button>
          </div>
        </div>

        {mcpDraft && (
          <div className="mcp-draft card">
            <label className="settings-field">
              <span>{zh.settings.mcpName}</span>
              <input
                value={mcpDraft.name}
                onChange={(e) => setMcpDraft({ ...mcpDraft, name: e.target.value })}
              />
            </label>
            <label className="settings-field">
              <span>{zh.settings.mcpCommand}</span>
              <input
                value={mcpDraft.command}
                placeholder="npx"
                onChange={(e) => setMcpDraft({ ...mcpDraft, command: e.target.value })}
              />
            </label>
            <label className="settings-field settings-field-wide">
              <span>{zh.settings.mcpArgs}</span>
              <textarea
                className="settings-textarea"
                rows={3}
                value={mcpDraft.argsText}
                placeholder={"-y\n@modelcontextprotocol/server-filesystem\n/Users/you/project"}
                onChange={(e) => setMcpDraft({ ...mcpDraft, argsText: e.target.value })}
              />
            </label>
            <label className="settings-field settings-checkbox">
              <input
                type="checkbox"
                checked={mcpDraft.enabled}
                onChange={(e) => setMcpDraft({ ...mcpDraft, enabled: e.target.checked })}
              />
              <span>{zh.settings.mcpEnabled}</span>
            </label>
            <div className="workspace-modal-actions">
              <button type="button" className="btn-ghost" onClick={() => setMcpDraft(null)}>
                {zh.chat.cancel}
              </button>
              <button type="button" className="btn-primary" onClick={() => void saveMcpDraft()}>
                {zh.settings.mcpSave}
              </button>
            </div>
          </div>
        )}

        {mcpServers.length === 0 ? (
          <p className="muted">{zh.settings.mcpEmpty}</p>
        ) : (
          <ul className="mcp-server-list">
            {mcpServers.map((server) => (
              <li key={server.id} className="mcp-server-item">
                <div className="mcp-server-main">
                  <strong>{server.name}</strong>
                  <span className="muted mcp-server-cmd">
                    {server.command} {server.args.join(" ")}
                  </span>
                  {!server.enabled && <span className="mcp-disabled-tag">已禁用</span>}
                </div>
                <div className="mcp-server-actions">
                  <button
                    type="button"
                    className="btn-ghost"
                    disabled={mcpTesting === server.id}
                    onClick={() => void testMcp(server.id)}
                  >
                    {mcpTesting === server.id ? zh.settings.mcpTesting : zh.settings.mcpTest}
                  </button>
                  <button type="button" className="btn-ghost" onClick={() => editMcp(server)}>
                    {zh.providers.editBtn}
                  </button>
                  <button
                    type="button"
                    className="btn-ghost danger"
                    onClick={() => void deleteMcp(server.id)}
                  >
                    {zh.settings.mcpDelete}
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>

      {settings && (
        <section className="card settings-web-search">
          <h3>{zh.settings.webSearchTitle}</h3>
          <p className="muted">{zh.settings.webSearchSubtitle}</p>
          <label className="settings-field settings-checkbox">
            <input
              type="checkbox"
              checked={settings.webSearchEnabled}
              onChange={(e) =>
                setSettings({ ...settings, webSearchEnabled: e.target.checked })
              }
            />
            <span>{zh.settings.webSearchEnabled}</span>
          </label>
          <label className="settings-field">
            <span>{zh.settings.webSearchProvider}</span>
            <select
              value={settings.webSearchProvider}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  webSearchProvider: e.target.value as "brave" | "tavily",
                })
              }
            >
              <option value="brave">{zh.settings.webSearchProviderBrave}</option>
              <option value="tavily">{zh.settings.webSearchProviderTavily}</option>
            </select>
          </label>
          <label className="settings-field">
            <span>{zh.settings.webSearchMaxResults}</span>
            <input
              type="number"
              min={1}
              max={20}
              value={settings.webSearchMaxResults}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  webSearchMaxResults: Number(e.target.value) || 5,
                })
              }
            />
          </label>
          <label className="settings-field settings-field-wide">
            <span>
              {zh.settings.webSearchApiKey}{" "}
              <span className="muted">
                ({hasWebSearchKey ? zh.settings.webSearchApiKeySaved : zh.settings.webSearchApiKeyEmpty})
              </span>
            </span>
            <div className="settings-inline-row">
              <input
                type="password"
                value={webSearchKey}
                placeholder={zh.settings.webSearchApiKeyPlaceholder}
                onChange={(e) => setWebSearchKey(e.target.value)}
              />
              <button
                type="button"
                className="btn-ghost"
                disabled={!webSearchKey.trim()}
                onClick={() => void saveWebSearchKey()}
              >
                {zh.settings.webSearchSaveKey}
              </button>
            </div>
          </label>
          <div className="settings-inline-row">
            <button type="button" className="btn-primary" onClick={() => void saveSettings()}>
              {zh.settings.saveSettings}
            </button>
            <button
              type="button"
              className="btn-ghost"
              disabled={webSearchTesting}
              onClick={() => void testWebSearch()}
            >
              {webSearchTesting ? zh.settings.webSearchTesting : zh.settings.webSearchTest}
            </button>
          </div>
        </section>
      )}

      {settings && (
        <section className="card settings-semantic-search">
          <h3>{zh.settings.semanticSearchTitle}</h3>
          <p className="muted">{zh.settings.semanticSearchSubtitle}</p>
          <label className="settings-field settings-checkbox">
            <input
              type="checkbox"
              checked={settings.semanticSearchEnabled}
              onChange={(e) =>
                setSettings({ ...settings, semanticSearchEnabled: e.target.checked })
              }
            />
            <span>{zh.settings.semanticSearchEnabled}</span>
          </label>
          <label className="settings-field">
            <span>{zh.settings.semanticSearchModel}</span>
            <input
              value={settings.semanticSearchModel}
              placeholder="text-embedding-3-small"
              onChange={(e) =>
                setSettings({ ...settings, semanticSearchModel: e.target.value })
              }
            />
          </label>
          <label className="settings-field">
            <span>{zh.settings.semanticSearchProvider}</span>
            <select
              value={settings.semanticSearchProviderId}
              onChange={(e) =>
                setSettings({ ...settings, semanticSearchProviderId: e.target.value })
              }
            >
              <option value="">{zh.settings.semanticSearchProviderAuto}</option>
              {embeddingProviders.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <label className="settings-field">
            <span>{zh.settings.semanticSearchMaxResults}</span>
            <input
              type="number"
              min={1}
              max={20}
              value={settings.semanticSearchMaxResults}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  semanticSearchMaxResults: Number(e.target.value) || 8,
                })
              }
            />
          </label>
          <p className="muted">{zh.settings.semanticSearchWorkspaceHint}</p>
          <label className="settings-field settings-field-wide">
            <span>{zh.settings.semanticSearchWorkspacePath}</span>
            <div className="settings-inline-row">
              <input
                value={semanticWorkspace}
                placeholder="/Users/you/project"
                onChange={(e) => setSemanticWorkspace(e.target.value)}
              />
              <button
                type="button"
                className="btn-ghost"
                disabled={!semanticWorkspace.trim()}
                onClick={() => void refreshSemanticStatus()}
              >
                刷新状态
              </button>
            </div>
          </label>
          {semanticStatus && semanticWorkspace.trim() && (
            <p className="muted">
              {zh.settings.semanticSearchStatus(
                semanticStatus.fileCount,
                semanticStatus.chunkCount,
              )}
              {semanticStatus.lastIndexedAt
                ? ` · ${formatLogTime(semanticStatus.lastIndexedAt)}`
                : ""}
            </p>
          )}
          <div className="settings-inline-row">
            <button type="button" className="btn-primary" onClick={() => void saveSettings()}>
              {zh.settings.saveSettings}
            </button>
            <button
              type="button"
              className="btn-ghost"
              disabled={semanticRebuilding || !semanticWorkspace.trim()}
              onClick={() => void rebuildSemanticIndex()}
            >
              {semanticRebuilding
                ? zh.settings.semanticSearchRebuilding
                : zh.settings.semanticSearchRebuild}
            </button>
          </div>
        </section>
      )}

      <section className="card settings-tool-audit">
        <h3>{zh.settings.toolAuditTitle}</h3>
        {toolAuditLogs.length === 0 ? (
          <p className="muted">{zh.settings.toolAuditEmpty}</p>
        ) : (
          <ul className="shell-audit-list">
            {toolAuditLogs.map((log) => (
              <li key={log.id} className="shell-audit-item">
                <div className="shell-audit-meta">
                  <span className="shell-audit-mode">{log.toolName}</span>
                  <span className="shell-audit-mode">{zh.settings.toolAuditMode(log.mode)}</span>
                  <span className="shell-audit-time">{formatLogTime(log.createdAt)}</span>
                </div>
                {log.inputPreview && (
                  <pre className="shell-audit-command">{log.inputPreview}</pre>
                )}
                {log.outputPreview && (
                  <pre className="shell-audit-preview">{log.outputPreview}</pre>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="card settings-shell-audit">
        <h3>{zh.settings.shellAuditTitle}</h3>
        {shellLogs.length === 0 ? (
          <p className="muted">{zh.settings.shellAuditEmpty}</p>
        ) : (
          <ul className="shell-audit-list">
            {shellLogs.map((log) => (
              <li key={log.id} className="shell-audit-item">
                <div className="shell-audit-meta">
                  <span className="shell-audit-mode">{zh.settings.shellAuditMode(log.mode)}</span>
                  <span className="shell-audit-time">{formatLogTime(log.createdAt)}</span>
                  {log.exitCode != null && (
                    <span className="shell-audit-exit">exit={log.exitCode}</span>
                  )}
                </div>
                <pre className="shell-audit-command">{log.command}</pre>
                {log.outputPreview && (
                  <pre className="shell-audit-preview">{log.outputPreview}</pre>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
