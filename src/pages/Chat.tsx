import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { roleLabel, sourceLabel, zh } from "../i18n/zh";
import { EnvironmentPanel } from "../components/EnvironmentPanel";
import { MessageBranch } from "../components/MessageBranch";
import {
  displayProjectName,
  displaySessionTitle,
  isConversationsProject,
  sortProjectsForNav,
  splitProjects,
} from "../utils/display";
import {
  attachmentLabel,
  fileToBase64,
  formatMessageWithAttachments,
} from "../utils/attachments";
import type {
  AgentToolEvent,
  ChatResponse,
  ChatStreamEvent,
  MessageView,
  Project,
  Provider,
  Session,
  ChatAttachment,
} from "../types";

const AUTO_PROVIDER = "__auto__";

function toEpochMs(ts: number): number {
  return ts < 1e12 ? ts * 1000 : ts;
}

function formatRelativeTime(ts: number): string {
  const diff = Date.now() - toEpochMs(ts);
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 14) return `${days} 天前`;
  const weeks = Math.floor(days / 7);
  return `${weeks} 周前`;
}

type ChatPageProps = {
  isActive: boolean;
  focusSessionId?: string | null;
  onFocusSessionHandled?: () => void;
};

function approvalModalTitle(action: string): string {
  if (action === "web_fetch") return zh.chat.approveWebFetch;
  if (action === "outside_read") return zh.chat.approveOutsideRead;
  if (action === "outside_write") return zh.chat.approveOutsideWrite;
  return zh.chat.executeShell;
}

export function ChatPage({ isActive, focusSessionId, onFocusSessionHandled }: ChatPageProps) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [conversationSessions, setConversationSessions] = useState<Session[]>([]);
  const [allProviders, setAllProviders] = useState<Provider[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<MessageView[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [failoverHint, setFailoverHint] = useState<string | null>(null);
  const [streamingText, setStreamingText] = useState("");
  const sendSeqRef = useRef(0);
  const [selectedProviderId, setSelectedProviderId] = useState<string>(AUTO_PROVIDER);
  const bottomRef = useRef<HTMLDivElement>(null);
  const [renamingSessionId, setRenamingSessionId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [newWorkspaceOpen, setNewWorkspaceOpen] = useState(false);
  const [newWorkspacePath, setNewWorkspacePath] = useState("");
  const [agentMode, setAgentMode] = useState(false);
  const [agentToolHint, setAgentToolHint] = useState<string | null>(null);
  const [shellModal, setShellModal] = useState<{ action: string; payload: string } | null>(null);
  const [attachments, setAttachments] = useState<ChatAttachment[]>([]);
  const [attachmentSaving, setAttachmentSaving] = useState(false);

  const usableProviders = allProviders.filter((p) => p.enabled && p.hasKey);

  const { conversations } = useMemo(() => splitProjects(projects), [projects]);
  const navProjects = useMemo(() => sortProjectsForNav(projects), [projects]);

  const activeProject = projects.find((p) => p.id === activeProjectId);
  const activeSession =
    conversationSessions.find((s) => s.id === activeSessionId) ??
    sessions.find((s) => s.id === activeSessionId);
  const conversationSlot = conversationSessions.slice(0, 1);

  async function resolveConversationsProjectId(): Promise<string | null> {
    const fromState = projects.find(isConversationsProject)?.id;
    if (fromState) return fromState;
    const data = await invoke<Project[]>("list_projects");
    return data.find(isConversationsProject)?.id ?? null;
  }

  async function refreshProjects(preferredId?: string) {
    const data = await invoke<Project[]>("list_projects");
    setProjects(data);
    setActiveProjectId((current) => {
      if (preferredId && data.some((p) => p.id === preferredId)) {
        return preferredId;
      }
      if (current && data.some((p) => p.id === current)) {
        return current;
      }
      const conv = data.find((p) => isConversationsProject(p));
      if (conv) return conv.id;
      return data[0]?.id ?? null;
    });
    return data;
  }

  async function refreshConversationSessions(
    preferredSessionId?: string,
    convProjectId?: string | null,
  ) {
    const convId =
      convProjectId ??
      projects.find(isConversationsProject)?.id ??
      (await invoke<Project[]>("list_projects")).find(isConversationsProject)?.id;
    if (!convId) {
      setConversationSessions([]);
      if (activeProjectId && activeProjectId === convProjectId) {
        setSessions([]);
      }
      return [];
    }
    const data = await invoke<Session[]>("list_sessions", { projectId: convId });
    const sorted = [...data].sort((a, b) => b.updatedAt - a.updatedAt);
    const slot = sorted.slice(0, 1);
    setConversationSessions(slot);
    if (activeProjectId === convId) {
      setSessions(slot);
      setActiveSessionId((current) => {
        if (preferredSessionId && slot.some((s) => s.id === preferredSessionId)) {
          return preferredSessionId;
        }
        if (current && slot.some((s) => s.id === current)) {
          return current;
        }
        return slot[0]?.id ?? null;
      });
    }
    return slot;
  }

  async function refreshSessions(projectId: string | null, preferredSessionId?: string) {
    if (!projectId) {
      setSessions([]);
      setActiveSessionId(null);
      return [];
    }
    const data = await invoke<Session[]>("list_sessions", { projectId });
    setSessions(data);
    setActiveSessionId((current) => {
      if (preferredSessionId && data.some((s) => s.id === preferredSessionId)) {
        return preferredSessionId;
      }
      if (current && data.some((s) => s.id === current)) {
        return current;
      }
      return data[0]?.id ?? null;
    });
    return data;
  }

  async function refreshProviders() {
    const data = await invoke<Provider[]>("list_providers");
    setAllProviders(data);
    const usable = data.filter((p) => p.enabled && p.hasKey);
    setSelectedProviderId((current) => {
      if (current === AUTO_PROVIDER) {
        return AUTO_PROVIDER;
      }
      return usable.some((p) => p.id === current) ? current : AUTO_PROVIDER;
    });
    return data;
  }

  function openNewWorkspaceForm() {
    setNewWorkspacePath("");
    setNewWorkspaceOpen(true);
  }

  async function submitNewWorkspaceProject() {
    const workspacePath = newWorkspacePath.trim();
    if (!workspacePath) {
      setError(zh.chat.workspacePathPrompt);
      return;
    }
    const name =
      workspacePath
        .replace(/\/+$/, "")
        .split("/")
        .filter(Boolean)
        .pop() ?? zh.chat.newProject;
    try {
      const project = await invoke<Project>("create_project", {
        name,
        workspacePath,
      });
      setNewWorkspaceOpen(false);
      setNewWorkspacePath("");
      await refreshProjects(project.id);
      setActiveProjectId(project.id);
      await refreshSessions(project.id);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleNewConversationSession() {
    const convId = await resolveConversationsProjectId();
    if (!convId) {
      setError(zh.chat.noProjects);
      return;
    }
    const existing = conversationSlot[0];
    if (existing) {
      setActiveProjectId(convId);
      setActiveSessionId(existing.id);
      setMessages([]);
      setError(null);
      setFailoverHint(null);
      return;
    }
    try {
      const session = await invoke<Session>("create_session", {
        title: "新对话",
        projectId: convId,
      });
      setActiveProjectId(convId);
      await refreshConversationSessions(session.id, convId);
      setMessages([]);
      setError(null);
      setFailoverHint(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleNewWorkspaceSession(projectId: string) {
    try {
      const session = await invoke<Session>("create_session", {
        title: "新对话",
        projectId,
      });
      setActiveProjectId(projectId);
      await refreshSessions(projectId, session.id);
      await refreshProjects(projectId);
      setMessages([]);
      setError(null);
      setFailoverHint(null);
    } catch (e) {
      setError(String(e));
    }
  }

  function startRename(session: Session, project?: Project) {
    setRenamingSessionId(session.id);
    setRenameDraft(displaySessionTitle(session, project));
  }

  async function commitRename(session: Session, project?: Project) {
    const title = renameDraft.trim();
    setRenamingSessionId(null);
    if (!title) {
      setError(zh.chat.renameSessionPrompt);
      return;
    }
    const current = displaySessionTitle(session, project);
    if (title === current) return;
    try {
      await invoke<Session>("rename_session", { sessionId: session.id, title });
      const convId = await resolveConversationsProjectId();
      if (project?.id === convId || session.projectId === convId) {
        await refreshConversationSessions(session.id, convId);
      } else if (session.projectId) {
        await refreshSessions(session.projectId, session.id);
      }
      await refreshProjects();
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDeleteSession(sessionId: string, project?: Project) {
    try {
      await invoke("delete_session", { sessionId });
      const updatedProjects = await refreshProjects();
      const conv = updatedProjects.find(isConversationsProject);
      const sessionProjectId = project?.id ?? activeProjectId;

      if (sessionProjectId && sessionProjectId !== conv?.id) {
        await refreshSessions(sessionProjectId);
      }
      await refreshConversationSessions(undefined, conv?.id ?? null);

      if (activeSessionId === sessionId) {
        setActiveSessionId(null);
        setMessages([]);
      }
      setError(null);
      setFailoverHint(zh.chat.deletedOk);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleSaveSessionToWorkspace(session: Session) {
    const defaultPath = session.workspacePath ?? activeProject?.workspacePath ?? "";
    const workspacePath = window.prompt(zh.chat.workspacePathPrompt, defaultPath);
    if (!workspacePath?.trim()) return;
    try {
      await invoke<Session>("save_session_to_workspace", {
        sessionId: session.id,
        workspacePath: workspacePath.trim(),
      });
      await refreshProjects();
      await refreshConversationSessions();
      if (activeProjectId && activeProjectId !== conversations?.id) {
        await refreshSessions(activeProjectId);
      }
      setError(null);
      setFailoverHint(zh.chat.savedToWorkspace);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDeleteProject(project: Project) {
    if (isConversationsProject(project)) {
      setError(zh.chat.conversationsNotDeletable);
      return;
    }
    try {
      await invoke("delete_project", { projectId: project.id });
      const updatedProjects = await refreshProjects();
      const conv = updatedProjects.find(isConversationsProject);
      if (activeProjectId === project.id) {
        setActiveProjectId(conv?.id ?? null);
        setActiveSessionId(null);
        setMessages([]);
        if (conv?.id) {
          await refreshConversationSessions(undefined, conv.id);
        } else {
          setSessions([]);
        }
      } else {
        await refreshConversationSessions(undefined, conv?.id ?? null);
        if (activeProjectId) {
          await refreshSessions(activeProjectId);
        }
      }
      setError(null);
      setFailoverHint(zh.chat.deletedOk);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleSaveProjectToWorkspace(project: Project) {
    const workspacePath = window.prompt(zh.chat.workspacePathPrompt, project.workspacePath ?? "");
    if (!workspacePath?.trim()) return;
    try {
      await invoke<Project>("save_project_to_workspace", {
        projectId: project.id,
        workspacePath: workspacePath.trim(),
      });
      await refreshProjects(project.id);
      setFailoverHint(zh.chat.savedToWorkspace);
    } catch (e) {
      setError(String(e));
    }
  }

  async function ensureSession(): Promise<string> {
    if (activeSessionId) {
      return activeSessionId;
    }

    const convId = await resolveConversationsProjectId();
    if (!convId) {
      throw new Error(zh.chat.noProjects);
    }

    const listed = await invoke<Session[]>("list_sessions", { projectId: convId });
    const existing = listed[0];
    if (existing) {
      setActiveProjectId(convId);
      setActiveSessionId(existing.id);
      setConversationSessions(listed.slice(0, 1));
      return existing.id;
    }

    const session = await invoke<Session>("create_session", {
      title: "新对话",
      projectId: convId,
    });
    setActiveProjectId(convId);
    await refreshConversationSessions(session.id, convId);
    return session.id;
  }

  useEffect(() => {
    if (!isActive) return;
    (async () => {
      await refreshProjects();
      await refreshProviders();
      await refreshConversationSessions();
      try {
        const settings = await invoke<{ agentEnabledDefault: boolean }>("get_context_settings");
        setAgentMode(settings.agentEnabledDefault);
      } catch {
        /* ignore */
      }
    })().catch((e) => setError(String(e)));
  }, [isActive]);

  useEffect(() => {
    if (!isActive) return;
    const unlisten = listen<AgentToolEvent>("agent-tool", (event) => {
      const p = event.payload;
      if (activeSessionId && p.sessionId !== activeSessionId) return;
      setAgentToolHint(`${p.toolName} · ${p.status}`);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [isActive, activeSessionId]);

  useEffect(() => {
    if (!isActive || !focusSessionId) return;
    let cancelled = false;
    (async () => {
      try {
        const session = await invoke<Session | null>("get_session", {
          sessionId: focusSessionId,
        });
        if (cancelled) return;
        if (!session) {
          setError(zh.import.openSessionFailed);
          onFocusSessionHandled?.();
          return;
        }
        const data = await refreshProjects(session.projectId ?? undefined);
        const conv = data.find(isConversationsProject);
        const projectId = session.projectId ?? conv?.id ?? null;
        if (projectId && conv?.id && projectId === conv.id) {
          await refreshConversationSessions(session.id, conv.id);
        } else if (projectId) {
          await refreshSessions(projectId, session.id);
        }
        if (projectId) setActiveProjectId(projectId);
        setActiveSessionId(session.id);
        setError(null);
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) onFocusSessionHandled?.();
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isActive, focusSessionId, onFocusSessionHandled]);

  useEffect(() => {
    if (!isActive || !activeProjectId) return;
    if (conversations?.id && activeProjectId === conversations.id) {
      refreshConversationSessions().catch((e) => setError(String(e)));
      return;
    }
    refreshSessions(activeProjectId).catch((e) => setError(String(e)));
  }, [isActive, activeProjectId, conversations?.id]);

  useEffect(() => {
    if (!activeSessionId) {
      setMessages([]);
      return;
    }
    invoke<MessageView[]>("get_messages", { sessionId: activeSessionId })
      .then(setMessages)
      .catch((e) => setError(String(e)));
  }, [activeSessionId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, loading, streamingText]);

  async function handleCancelGeneration() {
    if (!loading) return;
    sendSeqRef.current += 1;
    try {
      await invoke("cancel_chat_generation");
    } catch {
      /* ignore */
    }
    setLoading(false);
    setStreamingText("");
    setFailoverHint(null);
  }

  async function approveShell(run: boolean) {
    if (!shellModal || !activeSessionId) {
      setShellModal(null);
      return;
    }
    const payload = shellModal.payload;
    const action = shellModal.action;
    const sessionId = activeSessionId;
    setShellModal(null);

    setLoading(true);
    setError(null);
    setFailoverHint(null);
    setStreamingText("");
    const sendSeq = ++sendSeqRef.current;

    const unlisten = await listen<ChatStreamEvent>("chat-stream", (event) => {
      if (sendSeq !== sendSeqRef.current) return;
      const payload = event.payload;
      if (payload.chunk) {
        setStreamingText((current) => current + payload.chunk);
      }
      if (payload.error) {
        setError(payload.error);
      }
      if (payload.done?.failovered) {
        setFailoverHint(
          zh.chat.failoveredHint(payload.done.providerName, payload.done.attempts),
        );
      }
    });

    try {
      const response = await invoke<ChatResponse>("execute_agent_shell", {
        sessionId,
        command: payload,
        approved: run,
        action,
      });

      if (response.agentPaused && response.pendingCommand) {
        setShellModal({
          action: response.pendingAction ?? "shell",
          payload: response.pendingCommand,
        });
      }

      const updated = await invoke<MessageView[]>("get_messages", { sessionId });
      setMessages(updated);

      if (!run) {
        setFailoverHint(zh.chat.rejectShell);
      } else if (!response.agentPaused) {
        setFailoverHint(zh.chat.shellResumed);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      unlisten();
      if (sendSeq === sendSeqRef.current) {
        setStreamingText("");
        setLoading(false);
        setAgentToolHint(null);
      }
    }
  }

  async function handleExportSession() {
    if (!activeSessionId) {
      setError(zh.chat.noSession);
      return;
    }
    try {
      const md = await invoke<string>("export_session_markdown", {
        sessionId: activeSessionId,
      });
      const title =
        activeSession?.title?.replace(/[^\w\u4e00-\u9fff-]+/g, "_").slice(0, 40) || "session";
      const blob = new Blob([md], { type: "text/markdown;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${title}.md`;
      a.click();
      URL.revokeObjectURL(url);
      setFailoverHint(zh.chat.exportOk);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function addFilesAsAttachments(files: FileList | File[]) {
    const list = Array.from(files);
    if (list.length === 0) return;
    setAttachmentSaving(true);
    setError(null);
    try {
      const sessionId = await ensureSession();
      const saved: ChatAttachment[] = [];
      for (const file of list) {
        const dataBase64 = await fileToBase64(file);
        const attachment = await invoke<ChatAttachment>("save_chat_attachment", {
          input: {
            sessionId,
            workspacePath: activeSession?.workspacePath ?? activeProject?.workspacePath ?? null,
            fileName: file.name || "paste.bin",
            dataBase64,
          },
        });
        saved.push(attachment);
      }
      setAttachments((prev) => [...prev, ...saved]);
    } catch (e) {
      setError(`${zh.chat.attachmentPasteFailed}: ${String(e)}`);
    } finally {
      setAttachmentSaving(false);
    }
  }

  async function handleSend() {
    if (loading) return;

    const content = input.trim();
    if (!content && attachments.length === 0) {
      setError(zh.chat.noInput);
      return;
    }

    const outgoing = formatMessageWithAttachments(content, attachments);
    const pendingAttachments = attachments;

    let latest = allProviders;
    if (isActive) {
      latest = await invoke<Provider[]>("list_providers");
      setAllProviders(latest);
    }
    const usable = latest.filter((p) => p.enabled && p.hasKey);
    if (usable.length === 0) {
      setError(latest.length > 0 ? zh.chat.missingKeyHint : zh.chat.noProvider);
      return;
    }

    setLoading(true);
    setError(null);
    setFailoverHint(null);
    setStreamingText("");
    setInput("");
    setAttachments([]);
    const sendSeq = ++sendSeqRef.current;

    const unlisten = await listen<ChatStreamEvent>("chat-stream", (event) => {
      if (sendSeq !== sendSeqRef.current) return;
      const payload = event.payload;
      if (payload.chunk) {
        setStreamingText((current) => current + payload.chunk);
      }
      if (payload.error) {
        setError(payload.error);
      }
      if (payload.done?.failovered) {
        setFailoverHint(
          zh.chat.failoveredHint(payload.done.providerName, payload.done.attempts),
        );
      }
    });

    try {
      const wasEmpty = messages.length === 0;
      const sessionId = await ensureSession();
      const providerId =
        selectedProviderId === AUTO_PROVIDER ? null : selectedProviderId;

      const response = await invoke<ChatResponse>("send_message", {
        sessionId,
        content: outgoing,
        providerId,
        autoFailover: true,
        agentMode,
      });

      if (response.agentPaused && response.pendingCommand) {
        setShellModal({
          action: response.pendingAction ?? "shell",
          payload: response.pendingCommand,
        });
      }

      const updatedProjects = await refreshProjects();
      const conv = updatedProjects.find(isConversationsProject);
      const convId = conv?.id ?? null;

      if (wasEmpty && convId) {
        const title = content.trim().slice(0, 80) || attachments[0]?.fileName || "新对话";
        await invoke<Session>("rename_session", { sessionId, title });
      }

      const updated = await invoke<MessageView[]>("get_messages", { sessionId });
      setMessages(updated);

      if (convId) {
        await refreshConversationSessions(sessionId, convId);
        setActiveProjectId(convId);
      }
      if (activeProjectId && activeProjectId !== convId) {
        await refreshSessions(activeProjectId, sessionId);
      }

      if (response.failovered) {
        setFailoverHint(
          zh.chat.failoveredHint(response.providerName, response.attempts),
        );
      }

      if (!response.content) {
        setError(zh.chat.providerEmpty);
      }
    } catch (e) {
      if (sendSeq === sendSeqRef.current) {
        setError(String(e));
        setInput(content);
        setAttachments(pendingAttachments);
      }
    } finally {
      unlisten();
      if (sendSeq === sendSeqRef.current) {
        setStreamingText("");
        setLoading(false);
        setAgentToolHint(null);
      }
    }
  }

  const headerWorkspace =
    activeSession?.workspacePath ?? activeProject?.workspacePath ?? undefined;

  const selectedProvider =
    selectedProviderId === AUTO_PROVIDER
      ? usableProviders[0]
      : usableProviders.find((p) => p.id === selectedProviderId);

  const providerReady = usableProviders.length > 0;

  function selectSession(session: Session, projectId?: string | null) {
    setActiveProjectId(projectId ?? conversations?.id ?? activeProjectId);
    setActiveSessionId(session.id);
  }

  function renderSessionRow(session: Session, project?: Project) {
    const inConversations = project ? isConversationsProject(project) : false;
    const canSaveToWorkspace =
      !inConversations && !session.workspacePath?.trim() && !project?.workspacePath?.trim();
    return (
      <div
        key={session.id}
        className={`nav-thread-wrap ${inConversations ? "nav-thread-wrap-conversation" : ""} ${session.id === activeSessionId ? "active" : ""}`}
      >
        <button
          type="button"
          className={`nav-thread ${session.id === activeSessionId ? "active" : ""}`}
          onClick={() => selectSession(session, project?.id ?? conversations?.id)}
          onDoubleClick={() => startRename(session, project)}
        >
          {renamingSessionId === session.id ? (
            <input
              className="nav-thread-rename"
              value={renameDraft}
              autoFocus
              onClick={(e) => e.stopPropagation()}
              onChange={(e) => setRenameDraft(e.target.value)}
              onKeyDown={(e) => {
                e.stopPropagation();
                if (e.key === "Enter") {
                  e.preventDefault();
                  void commitRename(session, project);
                }
                if (e.key === "Escape") {
                  setRenamingSessionId(null);
                }
              }}
              onBlur={() => void commitRename(session, project)}
            />
          ) : (
            <span className="nav-thread-title">{displaySessionTitle(session, project)}</span>
          )}
          <span className="nav-thread-meta">
            {!inConversations && (
              <span className={`badge badge-${session.source}`}>{sourceLabel(session.source)}</span>
            )}
            <span>
              {session.messageCount} {zh.chat.msgs} · {formatRelativeTime(session.updatedAt)}
            </span>
          </span>
        </button>
        <div className="nav-thread-actions">
          {canSaveToWorkspace && (
            <button
              type="button"
              className="nav-action-btn"
              title={zh.chat.saveToWorkspace}
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                void handleSaveSessionToWorkspace(session);
              }}
            >
              ↗
            </button>
          )}
          <button
            type="button"
            className="nav-action-btn"
            title={zh.chat.renameSession}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              startRename(session, project);
            }}
          >
            ✎
          </button>
          <button
            type="button"
            className="nav-action-btn nav-action-danger"
            title={zh.chat.deleteSession}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              void handleDeleteSession(session.id, project);
            }}
          >
            ×
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="chat-layout">
      <aside className="nav-sidebar">
        <div className="sidebar-header">
          <div>
            <p className="sidebar-label">{zh.chat.conversations}</p>
            <p className="sidebar-count muted">{zh.chat.oneConversationHint}</p>
          </div>
          {conversationSlot.length === 0 && (
            <button
              type="button"
              className="btn-ghost btn-icon"
              onClick={() => void handleNewConversationSession()}
              title={zh.chat.newSession}
            >
              +
            </button>
          )}
        </div>

        <div className="nav-tree">
          {conversationSlot.length === 0 ? (
            <div className="nav-threads-empty">
              <p className="muted">{zh.chat.noSessions}</p>
              <button type="button" className="nav-new-thread" onClick={() => void handleNewConversationSession()}>
                + {zh.chat.newSession}
              </button>
            </div>
          ) : (
            renderSessionRow(conversationSlot[0], conversations)
          )}

          <div className="nav-section">
            <div className="nav-section-header">
              <span>{zh.chat.workspaceProjects}</span>
              <button
                type="button"
                className="btn-ghost btn-icon nav-section-add"
                onClick={openNewWorkspaceForm}
                title={zh.chat.newWorkspaceProject}
              >
                +
              </button>
            </div>
            {navProjects.filter((project) => !isConversationsProject(project)).length === 0 ? (
              <p className="sidebar-empty muted">{zh.chat.noWorkspaceProjectsYet}</p>
            ) : (
              navProjects
                .filter((project) => !isConversationsProject(project))
                .map((project) => {
                  const isOpen = project.id === activeProjectId;
                  return (
                    <div key={project.id} className={`nav-project-group ${isOpen ? "open" : ""}`}>
                      <div className="nav-project-row-wrap">
                        <button
                          type="button"
                          className={`nav-project-row ${isOpen ? "active" : ""}`}
                          onClick={() => setActiveProjectId(project.id)}
                        >
                          <span className="nav-project-icon">▣</span>
                          <span className="nav-project-name">{displayProjectName(project)}</span>
                          <span className="nav-project-count">{project.sessionCount}</span>
                        </button>
                        <div className="nav-project-actions">
                          {!project.workspacePath?.trim() && (
                            <button
                              type="button"
                              className="nav-action-btn"
                              title={zh.chat.saveToWorkspace}
                              onClick={(e) => {
                                e.stopPropagation();
                                void handleSaveProjectToWorkspace(project);
                              }}
                            >
                              ↗
                            </button>
                          )}
                          <button
                            type="button"
                            className="nav-action-btn nav-action-danger"
                            title={zh.chat.deleteProject}
                            onMouseDown={(e) => e.stopPropagation()}
                            onClick={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              void handleDeleteProject(project);
                            }}
                          >
                            ×
                          </button>
                        </div>
                      </div>

                      {isOpen && (
                        <div className="nav-threads">
                          {sessions.length === 0 ? (
                            <p className="sidebar-empty muted">{zh.chat.noSessions}</p>
                          ) : (
                            sessions.map((session) => renderSessionRow(session, project))
                          )}
                          <button
                            type="button"
                            className="nav-new-thread"
                            onClick={() => void handleNewWorkspaceSession(project.id)}
                          >
                            + {zh.chat.newSession}
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })
            )}
          </div>
        </div>
      </aside>

      <section className="chat-main">
        <header className="chat-header">
          <div className="chat-header-info">
            {activeSession &&
            conversations?.id &&
            activeProjectId === conversations.id &&
            renamingSessionId === activeSession.id ? (
              <input
                className="chat-title-rename"
                value={renameDraft}
                autoFocus
                aria-label={zh.chat.renameSession}
                onChange={(e) => setRenameDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void commitRename(activeSession, conversations);
                  }
                  if (e.key === "Escape") {
                    setRenamingSessionId(null);
                  }
                }}
                onBlur={() => void commitRename(activeSession, conversations)}
              />
            ) : (
              <h1
                className={
                  conversations?.id && activeProjectId === conversations.id
                    ? "chat-title-editable"
                    : undefined
                }
                title={
                  activeSession && conversations?.id && activeProjectId === conversations.id
                    ? zh.chat.renameSession
                    : undefined
                }
                onDoubleClick={() => {
                  if (activeSession && conversations?.id && activeProjectId === conversations.id) {
                    startRename(activeSession, conversations);
                  }
                }}
              >
                {activeSession
                  ? displaySessionTitle(activeSession, activeProject)
                  : zh.chat.selectSession}
              </h1>
            )}
            {headerWorkspace && <p className="path-chip">{headerWorkspace}</p>}
            {activeSession && activeSession.source !== "native" && (
              <p className="provenance-row">
                <span className={`badge badge-${activeSession.source}`}>
                  {sourceLabel(activeSession.source)}
                </span>
                <span className="muted">{zh.chat.provenanceImported}</span>
              </p>
            )}
            {activeSession?.continuedFrom && (
              <p className="provenance-row muted">{zh.chat.continuedFrom}</p>
            )}
          </div>
          <div className="chat-header-actions">
            <label className="agent-mode-toggle" title={zh.chat.agentMode}>
              <input
                type="checkbox"
                checked={agentMode}
                onChange={(e) => setAgentMode(e.target.checked)}
              />
              <span>Agent</span>
            </label>
            <button
              type="button"
              className="btn-ghost btn-sm"
              disabled={!activeSessionId}
              onClick={() => void handleExportSession()}
            >
              {zh.chat.exportSession}
            </button>
          </div>
          <div className="provider-pill">
            <span className="provider-dot" data-ready={providerReady} />
            <select
              className="provider-select"
              value={providerReady ? selectedProviderId : ""}
              onChange={(e) => setSelectedProviderId(e.target.value)}
              aria-label={zh.chat.selectProvider}
              disabled={!providerReady}
            >
              {!providerReady ? (
                <option value="">
                  {allProviders.length > 0
                    ? zh.chat.missingKeyHint
                    : zh.chat.configureProviderFirst}
                </option>
              ) : (
                <>
                  <option value={AUTO_PROVIDER}>{zh.chat.autoProvider}</option>
                  {usableProviders.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name} · {p.defaultModel}
                    </option>
                  ))}
                </>
              )}
            </select>
          </div>
        </header>

        <div className="message-scroll">
          {messages.length === 0 && !loading && (
            <div className="empty-hero">
              <div className="empty-glow" />
              <div className="empty-icon">◈</div>
              <h2>warp-ade</h2>
              <p>{zh.chat.emptyHint}</p>
              {selectedProvider && (
                <div className="empty-meta">当前模型 · {selectedProvider.defaultModel}</div>
              )}
            </div>
          )}

          <div className="message-list">
            {messages.map((message) => (
              <article key={message.id} className={`message-row message-row-${message.role}`}>
                <div className="message-avatar">{message.role === "user" ? "你" : "AI"}</div>
                <div className={`message-bubble message-${message.role}`}>
                  <div className="message-role">
                    {roleLabel(message.role)}
                    {message.metadata?.partial ? (
                      <span className="message-partial-tag">{zh.chat.partialMessage}</span>
                    ) : null}
                    {message.metadata?.subagent ? (
                      <span className="message-branch-tag">子 Agent</span>
                    ) : null}
                  </div>
                  <MessageBranch message={message} />
                </div>
              </article>
            ))}
            {loading && (
              <article className="message-row message-row-assistant">
                <div className="message-avatar">AI</div>
                <div
                  className={`message-bubble message-assistant ${streamingText ? "" : "loading-bubble"}`}
                >
                  {streamingText ? (
                    <>
                      <div className="message-role">{roleLabel("assistant")}</div>
                      <div className="message-body streaming-body">{streamingText}</div>
                    </>
                  ) : (
                    <>
                      <span className="typing-dots">
                        <span />
                        <span />
                        <span />
                      </span>
                      {zh.chat.thinking}
                    </>
                  )}
                </div>
              </article>
            )}
            <div ref={bottomRef} />
          </div>
        </div>

        <footer className="composer-dock">
          {agentToolHint && <div className="info-toast">{zh.chat.agentToolRunning}: {agentToolHint}</div>}
          {failoverHint && <div className="info-toast">{failoverHint}</div>}
          {error && <div className="error-toast">{error}</div>}
          <div
            className="composer-card"
            onDragOver={(e) => {
              e.preventDefault();
              e.currentTarget.classList.add("composer-drag-over");
            }}
            onDragLeave={(e) => {
              e.currentTarget.classList.remove("composer-drag-over");
            }}
            onDrop={(e) => {
              e.preventDefault();
              e.currentTarget.classList.remove("composer-drag-over");
              if (e.dataTransfer.files.length > 0) {
                void addFilesAsAttachments(e.dataTransfer.files);
              }
            }}
          >
            {attachments.length > 0 && (
              <ul className="composer-attachments">
                {attachments.map((item) => (
                  <li key={item.id} className="composer-attachment-chip">
                    <span title={item.path}>{attachmentLabel(item)}</span>
                    <button
                      type="button"
                      className="btn-ghost btn-sm"
                      onClick={() =>
                        setAttachments((prev) => prev.filter((a) => a.id !== item.id))
                      }
                    >
                      {zh.chat.attachmentRemove}
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <textarea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder={zh.chat.placeholder}
              rows={1}
              disabled={loading || attachmentSaving}
              onPaste={(e) => {
                const files = e.clipboardData?.files;
                if (files && files.length > 0) {
                  e.preventDefault();
                  void addFilesAsAttachments(files);
                }
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  handleSend();
                }
              }}
            />
            <div className="composer-toolbar">
              <span className="composer-hint">
                {attachmentSaving ? zh.chat.attachmentSaving : zh.chat.attachmentHint}
                {" · "}
                Enter 发送 · Shift+Enter 换行
              </span>
              {loading ? (
                <button
                  type="button"
                  className="btn-ghost btn-sm composer-cancel"
                  onClick={() => void handleCancelGeneration()}
                >
                  {zh.chat.cancelGeneration}
                </button>
              ) : null}
              <button type="button" className="btn-primary" onClick={handleSend} disabled={loading}>
                {loading ? zh.chat.sending : zh.chat.send}
              </button>
            </div>
          </div>
        </footer>
      </section>

      <EnvironmentPanel
        isActive={isActive}
        workspacePath={headerWorkspace}
        source={activeProject?.sourceOrigin}
      />

      {shellModal && (
        <div className="workspace-modal-backdrop" role="presentation">
          <div className="workspace-modal card" role="dialog">
            <h3>{approvalModalTitle(shellModal.action)}</h3>
            <pre className="shell-command-preview">{shellModal.payload}</pre>
            <div className="workspace-modal-actions">
              <button type="button" className="btn-ghost" onClick={() => void approveShell(false)}>
                {zh.chat.rejectShell}
              </button>
              <button type="button" className="btn-primary" onClick={() => void approveShell(true)}>
                {approvalModalTitle(shellModal.action)}
              </button>
            </div>
          </div>
        </div>
      )}

      {newWorkspaceOpen && (
        <div
          className="workspace-modal-backdrop"
          role="presentation"
          onClick={() => setNewWorkspaceOpen(false)}
        >
          <div
            className="workspace-modal card"
            role="dialog"
            aria-labelledby="new-workspace-title"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 id="new-workspace-title">{zh.chat.createWorkspaceProject}</h3>
            <p className="muted">{zh.chat.workspacePathPrompt}</p>
            <input
              type="text"
              className="workspace-modal-input"
              value={newWorkspacePath}
              placeholder={zh.chat.workspacePathExample}
              autoFocus
              onChange={(e) => setNewWorkspacePath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void submitNewWorkspaceProject();
                }
                if (e.key === "Escape") {
                  setNewWorkspaceOpen(false);
                }
              }}
            />
            <div className="workspace-modal-actions">
              <button type="button" className="btn-ghost" onClick={() => setNewWorkspaceOpen(false)}>
                {zh.chat.cancel}
              </button>
              <button
                type="button"
                className="btn-primary"
                onClick={() => void submitNewWorkspaceProject()}
              >
                {zh.chat.create}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
