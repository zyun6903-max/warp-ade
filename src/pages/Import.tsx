import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { displayProjectName } from "../utils/display";
import {
  dedupeImportCandidates,
  displayImportProjectName,
  importSourceToSessionSource,
  limitLatestPerProject,
} from "../utils/importDisplay";
import { zh } from "../i18n/zh";
import { DismissibleNotice } from "../components/DismissibleNotice";
import type {
  CursorImportCandidate,
  ImportSourceSearchHit,
  Project,
  Session,
  SessionSearchHit,
} from "../types";

type ImportProgress = {
  current: number;
  total: number;
  sourcePath: string;
  status: string;
  done: boolean;
};

type ImportSource = "cursor" | "claude" | "codex";

const SCAN_COMMAND: Record<ImportSource, string> = {
  cursor: "scan_cursor_imports",
  claude: "scan_claude_imports",
  codex: "scan_codex_imports",
};

const IMPORT_COMMAND: Record<ImportSource, string> = {
  cursor: "import_cursor_session",
  claude: "import_claude_session",
  codex: "import_codex_session",
};

type ImportPageProps = {
  onOpenSession?: (sessionId: string) => void;
};

export function ImportPage({ onOpenSession }: ImportPageProps) {
  const [source, setSource] = useState<ImportSource>("cursor");
  const [rawCandidates, setRawCandidates] = useState<CursorImportCandidate[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [recordHits, setRecordHits] = useState<SessionSearchHit[]>([]);
  const [sourceHits, setSourceHits] = useState<ImportSourceSearchHit[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [importProgress, setImportProgress] = useState<ImportProgress | null>(null);
  const searchGenRef = useRef(0);

  const dedupedCandidates = useMemo(
    () => dedupeImportCandidates(rawCandidates),
    [rawCandidates],
  );

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<ImportProgress>("import-progress", (event) => {
      setImportProgress(event.payload);
      if (event.payload.done) {
        setImportProgress(null);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const isSearching = searchQuery.trim().length > 0;

  const displayCandidates = useMemo(() => {
    if (isSearching) return [];
    return limitLatestPerProject(dedupedCandidates);
  }, [dedupedCandidates, isSearching]);

  async function scan(nextSource: ImportSource = source) {
    setLoading(true);
    setMessage(null);
    try {
      const data = await invoke<CursorImportCandidate[]>(SCAN_COMMAND[nextSource]);
      setRawCandidates(data);
      setSelected(new Set());
      const deduped = dedupeImportCandidates(data);
      const displayed = limitLatestPerProject(deduped);
      const foundMessage =
        nextSource === "cursor"
          ? zh.import.foundCursor(displayed.length, deduped.length)
          : nextSource === "claude"
            ? zh.import.foundClaude(displayed.length, deduped.length)
            : zh.import.foundCodex(displayed.length, deduped.length);
      setMessage(foundMessage);
    } catch (err) {
      setMessage(String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    scan(source).catch(console.error);
  }, [source]);

  useEffect(() => {
    const q = searchQuery.trim();
    if (!q) {
      setRecordHits([]);
      setSourceHits([]);
      setSearchLoading(false);
      setSearchError(null);
      return;
    }

    const gen = ++searchGenRef.current;
    setSearchLoading(true);
    setSearchError(null);
    const timer = window.setTimeout(() => {
      Promise.all([
        invoke<SessionSearchHit[]>("search_sessions", {
          query: q,
          limit: 50,
          projectId: null,
          source: importSourceToSessionSource(source),
        }),
        invoke<ImportSourceSearchHit[]>("search_import_sources", {
          source,
          query: q,
          limit: 50,
        }),
      ])
        .then(([records, sources]) => {
          if (gen !== searchGenRef.current) return;
          setRecordHits(records);
          setSourceHits(sources);
        })
        .catch((err) => {
          if (gen !== searchGenRef.current) return;
          setRecordHits([]);
          setSourceHits([]);
          setSearchError(String(err));
        })
        .finally(() => {
          if (gen === searchGenRef.current) {
            setSearchLoading(false);
          }
        });
    }, 300);

    return () => window.clearTimeout(timer);
  }, [searchQuery, source]);

  function cancelSearch() {
    searchGenRef.current += 1;
    setSearchQuery("");
    setSearchLoading(false);
    setRecordHits([]);
    setSourceHits([]);
    setSearchError(null);
  }

  function toggle(path: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  async function openSessionInChat(sessionId: string) {
    const session = await invoke<Session | null>("get_session", { sessionId });
    if (!session?.id) {
      setSearchError(zh.import.openSessionFailed);
      return;
    }
    if (session.source !== "native") {
      const continued = await invoke<Session>("continue_from_import", {
        importedSessionId: session.id,
      });
      onOpenSession?.(continued.id);
      return;
    }
    onOpenSession?.(session.id);
  }

  async function importSelected() {
    setLoading(true);
    setMessage(null);
    try {
      const result = await invoke<{ imported: number; skipped: number }>(
        "batch_import_sessions",
        {
          source,
          sourcePaths: Array.from(selected),
        },
      );
      setMessage(zh.import.result(result.imported, result.skipped));
      await scan(source);
    } catch (e) {
      setMessage(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function openFromSourcePath(sourcePath: string, alreadyImported: boolean) {
    setActionLoading(sourcePath);
    setSearchError(null);
    try {
      let session: Session | null = null;
      if (alreadyImported) {
        session = await invoke<Session | null>("get_session_by_source", { sourcePath });
      } else {
        session = await invoke<Session>(IMPORT_COMMAND[source], { sourcePath });
        await scan(source);
      }
      if (session?.id) {
        await openSessionInChat(session.id);
      } else {
        setSearchError(zh.import.openSessionFailed);
      }
    } catch (err) {
      setSearchError(String(err));
    } finally {
      setActionLoading(null);
    }
  }

  function projectLabelForHit(hit: SessionSearchHit): string {
    const stub: Project = {
      id: hit.session.projectId ?? "",
      name: hit.projectName ?? hit.session.title,
      workspacePath: hit.projectWorkspacePath ?? hit.session.workspacePath,
      sourceOrigin: hit.session.source,
      createdAt: hit.session.createdAt,
      updatedAt: hit.session.updatedAt,
      sessionCount: hit.session.messageCount,
    };
    return displayProjectName(stub);
  }

  const emptyMessage =
    source === "cursor"
      ? zh.import.emptyCursor
      : source === "claude"
        ? zh.import.emptyClaude
        : zh.import.emptyCodex;

  return (
    <div className="page import-page">
      <header className="page-header">
        <h2>{zh.import.title}</h2>
        <p className="muted">{zh.import.subtitle}</p>
        <div className="import-tabs">
          <button
            type="button"
            className={`import-tab ${source === "cursor" ? "active" : ""}`}
            onClick={() => setSource("cursor")}
          >
            Cursor
          </button>
          <button
            type="button"
            className={`import-tab ${source === "claude" ? "active" : ""}`}
            onClick={() => setSource("claude")}
          >
            Claude Code
          </button>
          <button
            type="button"
            className={`import-tab ${source === "codex" ? "active" : ""}`}
            onClick={() => setSource("codex")}
          >
            Codex
          </button>
        </div>
        <div className="import-search">
          <input
            type="search"
            className="import-search-input"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={zh.import.searchPlaceholder}
          />
          {isSearching && (
            <button
              type="button"
              className="btn-ghost import-search-cancel"
              onClick={cancelSearch}
              disabled={!searchLoading && !searchQuery.trim()}
            >
              {searchLoading ? zh.import.cancelSearch : zh.import.clearSearch}
            </button>
          )}
          {isSearching && !searchLoading && (
            <p className="import-search-meta muted">
              {zh.import.searchScanHint(sourceHits.length)} ·{" "}
              {zh.import.searchRecordHint(recordHits.length)}
            </p>
          )}
          {isSearching && searchLoading && (
            <p className="import-search-meta muted">{zh.import.searchLoading}</p>
          )}
        </div>
        {!isSearching && dedupedCandidates.length > 0 && (
          <p className="import-list-hint muted">{zh.import.latestPerProjectHint}</p>
        )}
        <div className="row-actions import-actions">
          <button type="button" className="btn-ghost" onClick={() => scan(source)} disabled={loading}>
            {zh.import.rescan}
          </button>
          <button
            type="button"
            className="btn-primary"
            onClick={importSelected}
            disabled={loading || selected.size === 0}
          >
            {loading ? zh.import.importing : `${zh.import.importSelected}（${selected.size}）`}
          </button>
        </div>
      </header>

      {message && (
        <DismissibleNotice variant="banner" onDismiss={() => setMessage(null)}>
          {message}
        </DismissibleNotice>
      )}
      {importProgress && (
        <div className="import-progress-bar">
          <progress value={importProgress.current} max={importProgress.total || 1} />
          <span className="muted">
            {zh.import.progress(importProgress.current, importProgress.total, importProgress.status)}
          </span>
        </div>
      )}
      {searchError && (
        <DismissibleNotice variant="error" onDismiss={() => setSearchError(null)}>
          {searchError}
        </DismissibleNotice>
      )}

      {isSearching && (
        <section className="card import-table import-record-table">
          <h3 className="import-section-title">{zh.import.searchRecordsTitle}</h3>
          {!searchLoading && recordHits.length === 0 && (
            <p className="muted">{zh.import.searchRecordsEmpty}</p>
          )}
          {!searchLoading && recordHits.length > 0 && (
            <table>
              <thead>
                <tr>
                  <th>{zh.import.project}</th>
                  <th>{zh.import.session}</th>
                  <th>{zh.import.matchedMessage}</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {recordHits.map((hit) => (
                  <tr key={`${hit.session.id}-${hit.matchedSeq}`}>
                    <td className="import-project-name">{projectLabelForHit(hit)}</td>
                    <td title={hit.session.sourcePath ?? hit.session.id}>
                      {hit.session.id.slice(0, 8)}…
                    </td>
                    <td className="import-snippet">{hit.matchedPreview}</td>
                    <td>
                      <button
                        type="button"
                        className="btn-ghost import-row-action"
                        disabled={actionLoading === hit.session.id}
                        onClick={() => void openSessionInChat(hit.session.id)}
                      >
                        {zh.import.openInChat}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      <section className="card import-table">
        <h3 className="import-section-title">
          {isSearching ? zh.import.scanListTitle : zh.import.scanListTitleDefault}
        </h3>
        <table>
          <thead>
            <tr>
              <th></th>
              <th>{zh.import.project}</th>
              <th>{zh.import.session}</th>
              {isSearching ? <th>{zh.import.matchedMessage}</th> : <th>{zh.import.lines}</th>}
              <th>{zh.import.status}</th>
              {isSearching && <th></th>}
            </tr>
          </thead>
          <tbody>
            {isSearching
              ? sourceHits.map((item) => (
                  <tr key={item.sourcePath}>
                    <td>
                      <input
                        type="checkbox"
                        checked={selected.has(item.sourcePath)}
                        disabled={item.alreadyImported}
                        onChange={() => toggle(item.sourcePath)}
                      />
                    </td>
                    <td className="import-project-name" title={item.projectSlug}>
                      {displayImportProjectName(item)}
                    </td>
                    <td title={item.sourcePath}>{item.sessionId.slice(0, 8)}…</td>
                    <td className="import-snippet">{item.matchedPreview}</td>
                    <td>{item.alreadyImported ? zh.import.imported : zh.import.ready}</td>
                    <td>
                      <button
                        type="button"
                        className="btn-ghost import-row-action"
                        disabled={actionLoading === item.sourcePath}
                        onClick={() =>
                          void openFromSourcePath(item.sourcePath, item.alreadyImported)
                        }
                      >
                        {item.alreadyImported ? zh.import.openInChat : zh.import.importAndOpen}
                      </button>
                    </td>
                  </tr>
                ))
              : displayCandidates.map((item) => (
                  <tr key={item.sourcePath}>
                    <td>
                      <input
                        type="checkbox"
                        checked={selected.has(item.sourcePath)}
                        disabled={item.alreadyImported}
                        onChange={() => toggle(item.sourcePath)}
                      />
                    </td>
                    <td className="import-project-name" title={item.projectSlug}>
                      {displayImportProjectName(item)}
                    </td>
                    <td title={item.sourcePath}>{item.sessionId.slice(0, 8)}…</td>
                    <td>{item.messageCountEstimate}</td>
                    <td>{item.alreadyImported ? zh.import.imported : zh.import.ready}</td>
                  </tr>
                ))}
          </tbody>
        </table>
        {!searchLoading &&
          (isSearching ? sourceHits.length === 0 : displayCandidates.length === 0) &&
          !loading && (
            <p className="muted">{isSearching ? zh.import.searchScanEmpty : emptyMessage}</p>
          )}
      </section>
    </div>
  );
}
