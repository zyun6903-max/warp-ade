import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { sourceLabel, zh } from "../i18n/zh";
import type { WorkspaceInfo } from "../types";

type EnvironmentPanelProps = {
  workspacePath?: string;
  source?: string;
  isActive: boolean;
};

function syncSummary(info: WorkspaceInfo): string {
  if (!info.isGitRepo) {
    return info.error ?? zh.env.noGit;
  }
  if (info.unpushedCommits > 0) {
    return zh.env.unpushed(info.unpushedCommits);
  }
  if (info.ahead && info.ahead > 0) {
    return zh.env.ahead(info.ahead);
  }
  if (info.behind && info.behind > 0) {
    return zh.env.behind(info.behind);
  }
  if (info.changedFiles > 0) {
    return zh.env.uncommitted(info.changedFiles);
  }
  return zh.env.clean;
}

function changeBadgeClass(status: string): string {
  if (status === "?") return "change-untracked";
  return `change-${status.replace(/[^a-zA-Z0-9]/g, "")}`;
}

export function EnvironmentPanel({ workspacePath, source, isActive }: EnvironmentPanelProps) {
  const [info, setInfo] = useState<WorkspaceInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [showChanges, setShowChanges] = useState(false);
  const [branchBusy, setBranchBusy] = useState(false);
  const [collapsed, setCollapsed] = useState(false);

  const refresh = useCallback(async () => {
    if (!workspacePath?.trim()) {
      setInfo(null);
      return;
    }
    setLoading(true);
    try {
      const data = await invoke<WorkspaceInfo>("get_workspace_info", {
        workspacePath: workspacePath,
        source: source ?? null,
      });
      setInfo(data);
    } catch (e) {
      setInfo({
        isGitRepo: false,
        branches: [],
        insertions: 0,
        deletions: 0,
        changedFiles: 0,
        unpushedCommits: 0,
        changes: [],
        githubAuthenticated: false,
        githubAuthMessage: String(e),
        error: String(e),
      });
    } finally {
      setLoading(false);
    }
  }, [workspacePath, source]);

  useEffect(() => {
    if (!isActive || !workspacePath?.trim()) {
      setInfo(null);
      return;
    }
    refresh().catch(console.error);
  }, [isActive, workspacePath, refresh]);

  useEffect(() => {
    if (!isActive || !workspacePath?.trim()) return;
    const timer = window.setInterval(() => {
      refresh().catch(console.error);
    }, 5000);
    return () => window.clearInterval(timer);
  }, [isActive, workspacePath, refresh]);

  useEffect(() => {
    setShowChanges(false);
    setCollapsed(false);
  }, [workspacePath]);

  async function handleBranchChange(nextBranch: string) {
    if (!workspacePath || !info?.branch || nextBranch === info.branch) return;
    setBranchBusy(true);
    try {
      await invoke("checkout_git_branch", { workspacePath, branch: nextBranch });
      await refresh();
    } catch (e) {
      console.error(e);
    } finally {
      setBranchBusy(false);
    }
  }

  if (!workspacePath?.trim()) {
    return null;
  }

  const gitReady = Boolean(info?.isGitRepo && info.branch);
  if (!gitReady && !loading) {
    return null;
  }

  if (!gitReady && loading) {
    return null;
  }

  if (!info) {
    return null;
  }

  const sourceText = info.source ? sourceLabel(info.source) : zh.env.noSource;

  if (collapsed) {
    return (
      <aside className="env-shell env-shell-collapsed" aria-label={zh.env.title}>
        <button
          type="button"
          className="env-rail-tab"
          onClick={() => setCollapsed(false)}
          title={`${zh.env.expand}${info.branch ? ` · ${info.branch}` : ""}`}
          aria-label={zh.env.expand}
        >
          <span className="env-rail-icon" aria-hidden="true">
            ⎇
          </span>
        </button>
      </aside>
    );
  }

  return (
    <aside className="env-shell">
      <div className="env-panel">
        <div className="env-card">
          <header className="env-header">
            <h2>{zh.env.title}</h2>
            <div className="env-header-actions">
              <button
                type="button"
                className="btn-ghost btn-icon env-refresh"
                onClick={() => refresh()}
                disabled={loading}
                title={zh.env.refresh}
              >
                ↻
              </button>
              <button
                type="button"
                className="btn-ghost btn-icon env-collapse"
                onClick={() => setCollapsed(true)}
                title={zh.env.collapse}
              >
                ›
              </button>
            </div>
          </header>

          <div className="env-rows">
            <button
              type="button"
              className={`env-row env-row-btn ${showChanges ? "expanded" : ""}`}
              onClick={() => setShowChanges((v) => !v)}
            >
              <span className="env-row-left">
                <span className="env-icon">±</span>
                <span>{zh.env.changes}</span>
              </span>
              <span className="env-diff-stats">
                <span className="diff-add">+{info.insertions}</span>
                <span className="diff-del">-{info.deletions}</span>
              </span>
            </button>

            {showChanges && info.changes.length > 0 && (
              <div className="env-changes-list">
                {info.changes.map((change) => (
                  <div key={`${change.status}-${change.path}`} className="env-change-item">
                    <span className={`change-badge ${changeBadgeClass(change.status)}`}>
                      {change.status}
                    </span>
                    <span className="change-path" title={change.path}>
                      {change.path}
                    </span>
                  </div>
                ))}
              </div>
            )}

            {showChanges && info.changes.length === 0 && (
              <p className="env-changes-empty muted">{zh.env.noChanges}</p>
            )}

            <div className="env-row">
              <span className="env-row-left">
                <span className="env-icon">⌂</span>
                <span>{zh.env.local}</span>
              </span>
            </div>

            <div className="env-row env-row-branch">
              <span className="env-row-left">
                <span className="env-icon">⎇</span>
                <span>{zh.env.branch}</span>
              </span>
              {info.branches.length > 0 ? (
                <select
                  className="env-branch-select"
                  value={info.branch ?? ""}
                  disabled={branchBusy}
                  onChange={(e) => handleBranchChange(e.target.value)}
                >
                  {info.branches.map((branch) => (
                    <option key={branch} value={branch}>
                      {branch}
                    </option>
                  ))}
                </select>
              ) : (
                <span className="env-muted">{info.branch ?? "—"}</span>
              )}
            </div>

            <div className="env-row">
              <span className="env-row-left">
                <span className="env-icon">↑</span>
                <span>{zh.env.sync}</span>
              </span>
              <span className="env-muted env-sync-text">{syncSummary(info)}</span>
            </div>

            <div className="env-row">
              <span className="env-row-left">
                <span className="env-icon">◉</span>
                <span>GitHub CLI</span>
              </span>
              <span
                className={`env-muted ${info.githubAuthenticated ? "env-ok" : "env-warn"}`}
                title={info.githubAuthMessage}
              >
                {info.githubAuthenticated ? zh.env.ghOk : zh.env.ghFail}
              </span>
            </div>
          </div>

          <div className="env-divider" />

          <section className="env-source">
            <h3>{zh.env.source}</h3>
            <p className="env-source-value">{sourceText}</p>
            <p className="env-workspace-path" title={workspacePath}>
              {workspacePath}
            </p>
          </section>
        </div>
      </div>
    </aside>
  );
}
