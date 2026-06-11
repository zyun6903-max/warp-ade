import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { sourceLabel, zh } from "../i18n/zh";
import { DiffViewer } from "./DiffViewer";
import type { FileDiffResult, WorkspaceInfo } from "../types";

type EnvironmentPanelProps = {
  workspacePath?: string;
  source?: string;
  isActive: boolean;
  refreshToken?: number;
};

const DIFF_PANEL_COLLAPSED_KEY = "warp-ade.diffPanelCollapsed";

function readDiffPanelCollapsed(): boolean {
  try {
    return localStorage.getItem(DIFF_PANEL_COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

function writeDiffPanelCollapsed(collapsed: boolean) {
  try {
    localStorage.setItem(DIFF_PANEL_COLLAPSED_KEY, collapsed ? "1" : "0");
  } catch {
    /* ignore */
  }
}

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

export function EnvironmentPanel({
  workspacePath,
  source,
  isActive,
  refreshToken = 0,
}: EnvironmentPanelProps) {
  const [info, setInfo] = useState<WorkspaceInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [showMeta, setShowMeta] = useState(false);
  const [branchBusy, setBranchBusy] = useState(false);
  const [gitBusy, setGitBusy] = useState<"commit" | "push" | null>(null);
  const [commitMessage, setCommitMessage] = useState("");
  const [gitFeedback, setGitFeedback] = useState<string | null>(null);
  const [collapsed, setCollapsedState] = useState(readDiffPanelCollapsed);
  const setCollapsed = (next: boolean) => {
    setCollapsedState(next);
    writeDiffPanelCollapsed(next);
  };
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [fileDiff, setFileDiff] = useState<FileDiffResult | null>(null);
  const [diffLoading, setDiffLoading] = useState(false);
  const [diffError, setDiffError] = useState<string | null>(null);

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

  const loadDiff = useCallback(
    async (path: string) => {
      if (!workspacePath?.trim()) return;
      setSelectedPath(path);
      setDiffLoading(true);
      setDiffError(null);
      try {
        const result = await invoke<FileDiffResult>("get_git_file_diff", {
          workspacePath,
          filePath: path,
        });
        setFileDiff(result);
      } catch (e) {
        setFileDiff(null);
        setDiffError(String(e));
      } finally {
        setDiffLoading(false);
      }
    },
    [workspacePath],
  );

  useEffect(() => {
    if (!isActive || !workspacePath?.trim()) {
      setInfo(null);
      return;
    }
    refresh().catch(console.error);
  }, [isActive, workspacePath, refresh, refreshToken]);

  useEffect(() => {
    if (!isActive || !workspacePath?.trim()) return;
    const timer = window.setInterval(() => {
      refresh().catch(console.error);
    }, 8000);
    return () => window.clearInterval(timer);
  }, [isActive, workspacePath, refresh]);

  useEffect(() => {
    setSelectedPath(null);
    setFileDiff(null);
    setDiffError(null);
    setShowMeta(false);
    setCommitMessage("");
    setGitFeedback(null);
  }, [workspacePath]);

  useEffect(() => {
    if (!info?.changes.length) {
      setSelectedPath(null);
      setFileDiff(null);
      return;
    }
    if (selectedPath && info.changes.some((c) => c.path === selectedPath)) {
      void loadDiff(selectedPath);
      return;
    }
    const first = info.changes[0]?.path;
    if (first) {
      void loadDiff(first);
    }
  }, [info?.changes, refreshToken]);

  async function handleBranchChange(nextBranch: string) {
    if (!workspacePath || !info?.branch || nextBranch === info.branch) return;
    setBranchBusy(true);
    setGitFeedback(null);
    try {
      await invoke("checkout_git_branch", { workspacePath, branch: nextBranch });
      await refresh();
    } catch (e) {
      setGitFeedback(String(e));
    } finally {
      setBranchBusy(false);
    }
  }

  async function handleCommit() {
    if (!workspacePath?.trim()) return;
    setGitBusy("commit");
    setGitFeedback(null);
    try {
      await invoke("commit_git_changes", {
        workspacePath,
        message: commitMessage.trim(),
      });
      setCommitMessage("");
      setGitFeedback(zh.env.commitOk);
      await refresh();
    } catch (e) {
      setGitFeedback(String(e));
    } finally {
      setGitBusy(null);
    }
  }

  async function handlePush() {
    if (!workspacePath?.trim()) return;
    setGitBusy("push");
    setGitFeedback(null);
    try {
      await invoke("push_git_branch", { workspacePath });
      setGitFeedback(zh.env.pushOk);
      await refresh();
    } catch (e) {
      setGitFeedback(String(e));
    } finally {
      setGitBusy(null);
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
  const changeCount = info.changes.length;
  const hasLocalChanges = changeCount > 0 || info.changedFiles > 0;
  const hasUnpushed = (info.unpushedCommits ?? 0) > 0 || (info.ahead ?? 0) > 0;
  const gitActionBusy = gitBusy !== null || branchBusy;

  if (collapsed) {
    return (
      <aside className="env-shell env-shell-collapsed" aria-label={zh.env.changesTitle}>
        <button
          type="button"
          className="env-rail-tab"
          onClick={() => setCollapsed(false)}
          title={`${zh.env.expand}${changeCount > 0 ? ` · ${changeCount}` : ""}`}
          aria-label={zh.env.expand}
        >
          <span className="env-rail-icon" aria-hidden="true">
            ±
          </span>
          {changeCount > 0 && <span className="env-rail-badge">{changeCount}</span>}
        </button>
      </aside>
    );
  }

  return (
    <aside className="env-shell env-shell-diff">
      <button
        type="button"
        className="env-rail-tab env-rail-tab-close"
        onClick={() => setCollapsed(true)}
        title={zh.env.collapse}
        aria-label={zh.env.collapse}
      >
        <span className="env-rail-label">{zh.env.collapseShort}</span>
      </button>

      <div className="env-panel env-panel-diff">
        <header className="env-diff-header">
          <div>
            <h2>{zh.env.changesTitle}</h2>
            <p className="env-diff-sub muted">
              <span className="diff-add">+{info.insertions}</span>{" "}
              <span className="diff-del">-{info.deletions}</span>
              {hasLocalChanges ? ` · ${syncSummary(info)}` : ` · ${zh.env.clean}`}
            </p>
          </div>
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
          </div>
        </header>

        <div className="env-git-toolbar">
          <div className="env-git-row">
            {info.branches.length > 0 ? (
              <select
                className="env-branch-select env-git-branch"
                value={info.branch ?? ""}
                disabled={gitActionBusy}
                onChange={(e) => void handleBranchChange(e.target.value)}
                title={zh.env.branch}
                aria-label={zh.env.branch}
              >
                {info.branches.map((branch) => (
                  <option key={branch} value={branch}>
                    {branch}
                  </option>
                ))}
              </select>
            ) : (
              <span className="env-git-branch env-git-branch-label">{info.branch ?? "—"}</span>
            )}
            <button
              type="button"
              className="btn-ghost btn-sm env-git-btn"
              disabled={!hasLocalChanges || gitActionBusy || !commitMessage.trim()}
              onClick={() => void handleCommit()}
              title={zh.env.commit}
            >
              {gitBusy === "commit" ? zh.env.committing : zh.env.commit}
            </button>
            <button
              type="button"
              className="btn-ghost btn-sm env-git-btn"
              disabled={!hasUnpushed || gitActionBusy}
              onClick={() => void handlePush()}
              title={hasUnpushed ? zh.env.push : zh.env.nothingToPush}
            >
              {gitBusy === "push" ? zh.env.pushing : zh.env.push}
            </button>
          </div>
          {hasLocalChanges && (
            <input
              type="text"
              className="env-commit-input"
              value={commitMessage}
              disabled={gitActionBusy}
              onChange={(e) => setCommitMessage(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && commitMessage.trim() && !gitActionBusy) {
                  e.preventDefault();
                  void handleCommit();
                }
              }}
              placeholder={zh.env.commitPlaceholder}
            />
          )}
          {gitFeedback && <p className="env-git-feedback muted">{gitFeedback}</p>}
        </div>

        <div className="env-diff-body">
          {changeCount === 0 ? (
            <p className="env-changes-empty muted">{zh.env.noChanges}</p>
          ) : (
            <>
              <ul className="env-file-list">
                {info.changes.map((change) => (
                  <li key={`${change.status}-${change.path}`}>
                    <button
                      type="button"
                      className={`env-file-item ${selectedPath === change.path ? "active" : ""}`}
                      onClick={() => void loadDiff(change.path)}
                    >
                      <span className={`change-badge ${changeBadgeClass(change.status)}`}>
                        {change.status}
                      </span>
                      <span className="change-path" title={change.path}>
                        {change.path}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>

              <div className="env-diff-viewer-wrap">
                {diffLoading && <p className="muted env-diff-status">{zh.env.diffLoading}</p>}
                {diffError && !diffLoading && (
                  <p className="env-diff-error">{diffError}</p>
                )}
                {!diffLoading && fileDiff && (
                  <>
                    <div className="env-diff-file-label" title={fileDiff.path}>
                      {fileDiff.path}
                      {fileDiff.isNewFile && (
                        <span className="env-diff-tag">{zh.env.diffNewFile}</span>
                      )}
                      {fileDiff.isDeleted && (
                        <span className="env-diff-tag">{zh.env.diffDeleted}</span>
                      )}
                    </div>
                    <DiffViewer diff={fileDiff.diff} />
                  </>
                )}
                {!diffLoading && !fileDiff && !diffError && (
                  <p className="muted env-diff-status">{zh.env.diffSelectFile}</p>
                )}
              </div>
            </>
          )}
        </div>

        <div className="env-meta-toggle">
          <button
            type="button"
            className="btn-ghost btn-sm"
            onClick={() => setShowMeta((v) => !v)}
          >
            {showMeta ? zh.env.hideMeta : zh.env.showMeta}
          </button>
        </div>

        {showMeta && (
          <div className="env-card env-meta-card">
            <div className="env-rows">
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
            <section className="env-source">
              <h3>{zh.env.source}</h3>
              <p className="env-source-value">{sourceText}</p>
              <p className="env-workspace-path" title={workspacePath}>
                {workspacePath}
              </p>
            </section>
          </div>
        )}
      </div>
    </aside>
  );
}
