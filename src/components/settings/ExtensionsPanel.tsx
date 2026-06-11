import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { zh } from "../../i18n/zh";
import type { McpServer, McpTestResult, SkillListItem } from "../../types";

const emptyMcpForm = {
  name: "",
  command: "",
  argsText: "",
  enabled: true,
};

export function ExtensionsPanel() {
  const [skills, setSkills] = useState<SkillListItem[]>([]);
  const [skillsDir, setSkillsDir] = useState("");
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [mcpForm, setMcpForm] = useState(emptyMcpForm);
  const [editingMcpId, setEditingMcpId] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [testingMcpId, setTestingMcpId] = useState<string | null>(null);

  const refreshSkills = useCallback(async () => {
    try {
      setSkills(await invoke<SkillListItem[]>("list_all_skills", { workspacePath: null }));
    } catch (e) {
      console.error(e);
      setSkills([]);
    }
  }, []);

  const refreshMcp = useCallback(async () => {
    try {
      setMcpServers(await invoke<McpServer[]>("list_mcp_servers"));
    } catch (e) {
      console.error(e);
      setMcpServers([]);
    }
  }, []);

  useEffect(() => {
    void refreshSkills();
    void refreshMcp();
    invoke<string>("get_user_skills_dir")
      .then(setSkillsDir)
      .catch(console.error);
  }, [refreshSkills, refreshMcp]);

  async function toggleSkill(skill: SkillListItem) {
    try {
      await invoke("set_skill_enabled", { skillPath: skill.path, enabled: !skill.enabled });
      await refreshSkills();
    } catch (e) {
      setMessage(String(e));
    }
  }

  async function deleteSkill(skill: SkillListItem) {
    if (skill.source !== "user") return;
    try {
      await invoke("delete_user_skill", { skillPath: skill.path });
      await refreshSkills();
      setMessage(zh.settings.skillsDeleted);
    } catch (e) {
      setMessage(String(e));
    }
  }

  async function revealSkill(skill: SkillListItem) {
    try {
      await invoke("reveal_skill_path", { skillPath: skill.path });
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

  async function testMcp(id: string) {
    setTestingMcpId(id);
    try {
      const result = await invoke<McpTestResult>("test_mcp_server", { id });
      setMessage(result.message || zh.settings.mcpTestOk(result.toolCount));
    } catch (e) {
      setMessage(String(e));
    } finally {
      setTestingMcpId(null);
    }
  }

  async function saveMcp() {
    if (!mcpForm.name.trim() || !mcpForm.command.trim()) {
      setMessage(zh.settings.mcpNeedName);
      return;
    }
    const args = mcpForm.argsText
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
    try {
      await invoke("save_mcp_server", {
        input: {
          id: editingMcpId ?? undefined,
          name: mcpForm.name.trim(),
          command: mcpForm.command.trim(),
          args,
          env: {},
          enabled: mcpForm.enabled,
        },
      });
      setMcpForm(emptyMcpForm);
      setEditingMcpId(null);
      await refreshMcp();
      setMessage(zh.settings.mcpSaved);
    } catch (e) {
      setMessage(String(e));
    }
  }

  async function deleteMcp(id: string) {
    try {
      await invoke("delete_mcp_server", { id });
      if (editingMcpId === id) {
        setEditingMcpId(null);
        setMcpForm(emptyMcpForm);
      }
      await refreshMcp();
    } catch (e) {
      setMessage(String(e));
    }
  }

  function editMcp(server: McpServer) {
    setEditingMcpId(server.id);
    setMcpForm({
      name: server.name,
      command: server.command,
      argsText: server.args.join("\n"),
      enabled: server.enabled,
    });
  }

  return (
    <div className="extensions-panel">
      {message && (
        <p className="extensions-message muted" onClick={() => setMessage(null)}>
          {message}
        </p>
      )}

      <section className="settings-panel-section">
        <div className="extensions-section-head">
          <div>
            <h3>{zh.settings.skillsTitle}</h3>
            <p className="muted settings-hint">{zh.settings.skillsSubtitle}</p>
            {skillsDir && (
              <p className="muted settings-hint" title={skillsDir}>
                {zh.settings.skillsDir}: {skillsDir}
              </p>
            )}
          </div>
          <button type="button" className="btn-ghost btn-sm" onClick={() => void refreshSkills()}>
            ↻
          </button>
        </div>
        {skills.length === 0 ? (
          <p className="muted">{zh.settings.skillsEmpty}</p>
        ) : (
          <ul className="extensions-skill-list">
            {skills.map((skill) => (
              <li key={skill.path} className="extensions-skill-item">
                <div className="extensions-skill-main">
                  <span className="extensions-skill-name">{skill.name}</span>
                  <span className="extensions-skill-source">{skill.source}</span>
                  {!skill.enabled && (
                    <span className="extensions-skill-disabled">{zh.settings.skillsDisabled}</span>
                  )}
                </div>
                {skill.description && (
                  <p className="muted extensions-skill-desc">{skill.description}</p>
                )}
                <div className="extensions-skill-actions">
                  <button type="button" className="btn-ghost btn-sm" onClick={() => void toggleSkill(skill)}>
                    {skill.enabled ? zh.settings.skillsDisable : zh.settings.skillsEnable}
                  </button>
                  <button type="button" className="btn-ghost btn-sm" onClick={() => void revealSkill(skill)}>
                    {zh.settings.skillsReveal}
                  </button>
                  {skill.source === "user" && (
                    <button
                      type="button"
                      className="btn-ghost btn-sm extensions-danger"
                      onClick={() => void deleteSkill(skill)}
                    >
                      {zh.settings.skillsDelete}
                    </button>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="settings-panel-section settings-mcp-section">
        <div className="extensions-section-head">
          <div>
            <h3>{zh.settings.mcpTitle}</h3>
            <p className="muted settings-hint">{zh.settings.mcpSubtitle}</p>
          </div>
          <button type="button" className="btn-ghost btn-sm" onClick={() => void importCursorMcp()}>
            {zh.settings.mcpImportCursor}
          </button>
        </div>

        {mcpServers.length === 0 ? (
          <p className="muted">{zh.settings.mcpEmpty}</p>
        ) : (
          <ul className="settings-mcp-list">
            {mcpServers.map((server) => (
              <li key={server.id} className="settings-mcp-item">
                <div className="settings-mcp-item-main">
                  <span className="settings-mcp-name">{server.name}</span>
                  <span className="muted settings-mcp-cmd">
                    {server.command} {server.args.join(" ")}
                  </span>
                  {!server.enabled && <span className="extensions-skill-disabled">{zh.settings.skillsDisabled}</span>}
                </div>
                <div className="extensions-skill-actions">
                  <button type="button" className="btn-ghost btn-sm" onClick={() => editMcp(server)}>
                    {zh.providers.editBtn}
                  </button>
                  <button
                    type="button"
                    className="btn-ghost btn-sm"
                    disabled={testingMcpId === server.id}
                    onClick={() => void testMcp(server.id)}
                  >
                    {testingMcpId === server.id ? zh.settings.mcpTesting : zh.settings.mcpTest}
                  </button>
                  <button
                    type="button"
                    className="btn-ghost btn-sm extensions-danger"
                    onClick={() => void deleteMcp(server.id)}
                  >
                    {zh.settings.mcpDelete}
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}

        <div className="settings-mcp-form">
          <h4>{editingMcpId ? zh.providers.edit : zh.settings.mcpAdd}</h4>
          <label className="settings-field">
            <span>{zh.settings.mcpName}</span>
            <input
              value={mcpForm.name}
              onChange={(e) => setMcpForm({ ...mcpForm, name: e.target.value })}
            />
          </label>
          <label className="settings-field">
            <span>{zh.settings.mcpCommand}</span>
            <input
              value={mcpForm.command}
              onChange={(e) => setMcpForm({ ...mcpForm, command: e.target.value })}
              placeholder="npx"
            />
          </label>
          <label className="settings-field">
            <span>{zh.settings.mcpArgs}</span>
            <textarea
              className="settings-textarea"
              rows={3}
              value={mcpForm.argsText}
              onChange={(e) => setMcpForm({ ...mcpForm, argsText: e.target.value })}
            />
          </label>
          <label className="settings-field settings-checkbox">
            <input
              type="checkbox"
              checked={mcpForm.enabled}
              onChange={(e) => setMcpForm({ ...mcpForm, enabled: e.target.checked })}
            />
            <span>{zh.settings.mcpEnabled}</span>
          </label>
          <div className="extensions-form-actions">
            {editingMcpId && (
              <button
                type="button"
                className="btn-ghost btn-sm"
                onClick={() => {
                  setEditingMcpId(null);
                  setMcpForm(emptyMcpForm);
                }}
              >
                {zh.providers.cancel}
              </button>
            )}
            <button type="button" className="btn-primary btn-sm" onClick={() => void saveMcp()}>
              {zh.settings.mcpSave}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
