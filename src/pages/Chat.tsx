import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { roleLabel, sourceLabel, zh } from "../i18n/zh";
import { EnvironmentPanel } from "../components/EnvironmentPanel";
import { DismissibleNotice } from "../components/DismissibleNotice";
import { LiveGenerationView } from "../components/LiveGenerationView";
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
  ProjectContextBundle,
  LiveStep,
  LiveToolStatus,
  QueuedMessage,
} from "../types";

const AUTO_PROVIDER = "__auto__";
const PHASE_TOOL = "__phase__";

type ChatMode = "chat" | "agent" | "plan";

const CHAT_MODES: ChatMode[] = ["chat", "agent", "plan"];

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
  onNavigateToProviders?: () => void;
};

function approvalModalTitle(action: string): string {
  if (action === "web_fetch") return zh.chat.approveWebFetch;
  if (action === "outside_read") return zh.chat.approveOutsideRead;
  if (action === "outside_write") return zh.chat.approveOutsideWrite;
  return zh.chat.executeShell;
}

type SessionActivityStatus = "running" | "done";

type SessionActivity = {
  status: SessionActivityStatus;
  projectId: string;
  streamingText: string;
  sendSeq: number;
  liveSteps: LiveStep[];
  currentTextStepId: string | null;
};

function normalizeToolStatus(status: string): LiveToolStatus {
  if (status === "streaming") return "streaming";
  if (status === "start") return "running";
  if (status === "done") return "done";
  if (status === "error") return "error";
  if (status === "approval") return "approval";
  return "running";
}

function stripStatusSteps(steps: LiveStep[]): LiveStep[] {
  return steps.filter((step) => step.kind !== "status");
}

function upsertStatusStep(steps: LiveStep[], id: string, label: string): LiveStep[] {
  return [...stripStatusSteps(steps), { kind: "status", id, label }];
}

function projectActivityStatus(
  activities: Record<string, SessionActivity>,
  projectId: string,
): SessionActivityStatus | null {
  let hasDone = false;
  for (const activity of Object.values(activities)) {
    if (activity.projectId !== projectId) continue;
    if (activity.status === "running") return "running";
    if (activity.status === "done") hasDone = true;
  }
  return hasDone ? "done" : null;
}

export function ChatPage({
  isActive,
  focusSessionId,
  onFocusSessionHandled,
  onNavigateToProviders,
}: ChatPageProps) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [conversationSessions, setConversationSessions] = useState<Session[]>([]);
  const [allProviders, setAllProviders] = useState<Provider[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<MessageView[]>([]);
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [failoverHint, setFailoverHint] = useState<string | null>(null);
  const [sessionActivities, setSessionActivities] = useState<Record<string, SessionActivity>>({});
  const sendSeqRef = useRef(0);
  const sendInFlightRef = useRef(false);
  const blockMessagesFetchRef = useRef<string | null>(null);
  const pendingSendRollbackRef = useRef<{
    sendSeq: number;
    sessionId: string;
    rawContent: string;
    attachments: ChatAttachment[];
    optimisticId: string;
  } | null>(null);
  const activeSessionIdRef = useRef<string | null>(null);
  const sessionQueuesRef = useRef<Record<string, QueuedMessage[]>>({});
  const [sessionQueues, setSessionQueues] = useState<Record<string, QueuedMessage[]>>({});
  const [selectedProviderId, setSelectedProviderId] = useState<string>(AUTO_PROVIDER);
  const bottomRef = useRef<HTMLDivElement>(null);
  const messageScrollRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);
  const [renamingSessionId, setRenamingSessionId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [newWorkspaceOpen, setNewWorkspaceOpen] = useState(false);
  const [newWorkspacePath, setNewWorkspacePath] = useState("");
  const [chatMode, setChatMode] = useState<ChatMode>("agent");
  const [shellModal, setShellModal] = useState<{ action: string; payload: string } | null>(null);
  const [attachments, setAttachments] = useState<ChatAttachment[]>([]);
  const [attachmentSaving, setAttachmentSaving] = useState(false);
  const [projectContext, setProjectContext] = useState<ProjectContextBundle | null>(null);
  const [changesRefresh, setChangesRefresh] = useState(0);
  const [importedBannerDismissed, setImportedBannerDismissed] = useState(false);

  const usableProviders = allProviders.filter((p) => p.enabled && p.hasKey);

  const { conversations } = useMemo(() => splitProjects(projects), [projects]);
  const navProjects = useMemo(() => sortProjectsForNav(projects), [projects]);

  const activeProject = projects.find((p) => p.id === activeProjectId);
  const activeSession =
    conversationSessions.find((s) => s.id === activeSessionId) ??
    sessions.find((s) => s.id === activeSessionId);
  const conversationSlot = conversationSessions.slice(0, 1);

  const activeSessionActivity = activeSessionId
    ? sessionActivities[activeSessionId]
    : undefined;
  const isActiveGenerating = activeSessionActivity?.status === "running";
  const streamingText = activeSessionActivity?.streamingText ?? "";
  const liveStepCount = activeSessionActivity?.liveSteps.length ?? 0;
  const anySessionGenerating = useMemo(
    () => Object.values(sessionActivities).some((a) => a.status === "running"),
    [sessionActivities],
  );

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

  function isNearMessageBottom(threshold = 96): boolean {
    const el = messageScrollRef.current;
    if (!el) return true;
    return el.scrollHeight - el.scrollTop - el.clientHeight <= threshold;
  }

  function scrollMessagesToBottom(behavior: ScrollBehavior = "auto") {
    const el = messageScrollRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior });
  }

  function handleMessageScroll() {
    if (isActiveGenerating) return;
    stickToBottomRef.current = isNearMessageBottom();
  }

  function markStickToBottom() {
    stickToBottomRef.current = true;
  }

  function rollbackFailedSend(
    rollback: NonNullable<typeof pendingSendRollbackRef.current>,
    options?: { error?: string | null },
  ) {
    if (activeSessionIdRef.current !== rollback.sessionId) return;
    blockMessagesFetchRef.current = null;
    setMessages((prev) => prev.filter((message) => message.id !== rollback.optimisticId));
    setInput(rollback.rawContent);
    setAttachments(rollback.attachments);
    if (options && "error" in options) {
      setError(options.error ?? null);
    }
  }

  useLayoutEffect(() => {
    markStickToBottom();
    scrollMessagesToBottom("auto");
  }, [activeSessionId]);

  useLayoutEffect(() => {
    if (!stickToBottomRef.current && !isActiveGenerating) return;
    if (isActiveGenerating) {
      stickToBottomRef.current = true;
    }
    scrollMessagesToBottom(isActiveGenerating ? "auto" : "smooth");
    const raf = requestAnimationFrame(() => {
      scrollMessagesToBottom("auto");
    });
    return () => cancelAnimationFrame(raf);
  }, [messages, isActiveGenerating, streamingText, liveStepCount]);

  const activeQueue = activeSessionId ? (sessionQueues[activeSessionId] ?? []) : [];

  function syncSessionQueues(next: Record<string, QueuedMessage[]>) {
    sessionQueuesRef.current = next;
    setSessionQueues(next);
  }

  function enqueueMessage(sessionId: string, content: string, messageAttachments: ChatAttachment[]) {
    const prev = sessionQueuesRef.current[sessionId] ?? [];
    syncSessionQueues({
      ...sessionQueuesRef.current,
      [sessionId]: [
        ...prev,
        {
          id: `queue-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          content,
          attachments: messageAttachments,
        },
      ],
    });
  }

  function removeQueuedMessage(sessionId: string, queueId: string) {
    const prev = sessionQueuesRef.current[sessionId] ?? [];
    syncSessionQueues({
      ...sessionQueuesRef.current,
      [sessionId]: prev.filter((item) => item.id !== queueId),
    });
  }

  function dequeueMessage(sessionId: string): QueuedMessage | null {
    const queue = sessionQueuesRef.current[sessionId] ?? [];
    if (queue.length === 0) return null;
    const [first, ...rest] = queue;
    syncSessionQueues({
      ...sessionQueuesRef.current,
      [sessionId]: rest,
    });
    return first;
  }

  function patchSessionPhase(
    sessionId: string,
    sendSeq: number,
    label: string,
    phaseId: string,
  ) {
    setSessionActivities((prev) => {
      const current = prev[sessionId];
      if (!current || current.status !== "running" || current.sendSeq !== sendSeq) {
        return prev;
      }
      return {
        ...prev,
        [sessionId]: {
          ...current,
          liveSteps: upsertStatusStep(current.liveSteps, phaseId, label),
        },
      };
    });
  }

  function clearSessionActivity(sessionId: string) {
    setSessionActivities((prev) => {
      if (!prev[sessionId]) return prev;
      const next = { ...prev };
      delete next[sessionId];
      return next;
    });
  }

  function startSessionActivity(sessionId: string, projectId: string, sendSeq: number) {
    setSessionActivities((prev) => ({
      ...prev,
      [sessionId]: {
        status: "running",
        projectId,
        streamingText: "",
        sendSeq,
        liveSteps: [{ kind: "status", id: "boot", label: zh.chat.agentStarting }],
        currentTextStepId: null,
      },
    }));
  }

  function appendSessionStream(sessionId: string, sendSeq: number, chunk: string) {
    setSessionActivities((prev) => {
      const current = prev[sessionId];
      if (!current || current.status !== "running" || current.sendSeq !== sendSeq) {
        return prev;
      }

      const liveSteps = stripStatusSteps([...current.liveSteps]);
      let currentTextStepId = current.currentTextStepId;
      if (!currentTextStepId) {
        currentTextStepId = `text-${liveSteps.length}-${Date.now()}`;
        liveSteps.push({ kind: "text", id: currentTextStepId, content: "" });
      }

      const textIdx = liveSteps.findIndex(
        (step) => step.kind === "text" && step.id === currentTextStepId,
      );
      if (textIdx >= 0 && liveSteps[textIdx].kind === "text") {
        const textStep = liveSteps[textIdx];
        liveSteps[textIdx] = { ...textStep, content: textStep.content + chunk };
      }

      return {
        ...prev,
        [sessionId]: {
          ...current,
          streamingText: current.streamingText + chunk,
          liveSteps,
          currentTextStepId,
        },
      };
    });
  }

  function applyAgentToolEvent(event: AgentToolEvent) {
    const { sessionId, callId, toolName, status, preview } = event;
    setSessionActivities((prev) => {
      const current = prev[sessionId];
      if (!current || current.status !== "running") return prev;

      const toolId = callId || `${toolName}-${current.liveSteps.length}`;

      if (toolName === PHASE_TOOL) {
        return {
          ...prev,
          [sessionId]: {
            ...current,
            liveSteps: upsertStatusStep(
              current.liveSteps,
              toolId,
              preview.trim() || zh.chat.thinking,
            ),
          },
        };
      }

      const liveSteps = stripStatusSteps([...current.liveSteps]);
      const toolStatus = normalizeToolStatus(status);
      const existingIdx = liveSteps.findIndex((step) => step.kind === "tool" && step.id === toolId);

      if (existingIdx >= 0) {
        const existing = liveSteps[existingIdx];
        if (existing.kind === "tool") {
          liveSteps[existingIdx] = {
            ...existing,
            toolName: toolName || existing.toolName,
            status: toolStatus,
            preview: preview.trim() || existing.preview,
          };
        }
      } else {
        liveSteps.push({
          kind: "tool",
          id: toolId,
          toolName,
          status: toolStatus,
          preview,
        });
      }

      return {
        ...prev,
        [sessionId]: {
          ...current,
          liveSteps,
          currentTextStepId: null,
        },
      };
    });
  }

  function finishSessionActivity(sessionId: string, viewing: boolean) {
    setSessionActivities((prev) => {
      const current = prev[sessionId];
      if (!current) return prev;
      if (viewing) {
        const next = { ...prev };
        delete next[sessionId];
        return next;
      }
      return {
        ...prev,
        [sessionId]: { ...current, status: "done", streamingText: "", currentTextStepId: null },
      };
    });
  }

  function subscribeChatStream(
    sessionId: string,
    sendSeq: number,
    onFailover?: (providerName: string, attempts: number) => void,
  ) {
    return listen<ChatStreamEvent>("chat-stream", (event) => {
      const payload = event.payload;
      if (payload.sessionId !== sessionId) return;
      if (sendSeq !== sendSeqRef.current) return;
      if (payload.chunk) {
        appendSessionStream(sessionId, sendSeq, payload.chunk);
      }
      if (payload.error && activeSessionIdRef.current === sessionId) {
        setError(payload.error);
      }
      if (payload.done?.failovered && onFailover) {
        onFailover(payload.done.providerName, payload.done.attempts);
      }
    });
  }

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

  async function pickWorkspaceFolder() {
    try {
      const selected = await invoke<string | null>("pick_workspace_directory", {
        title: zh.chat.pickWorkspaceFolder,
      });
      if (typeof selected === "string" && selected.trim()) {
        setNewWorkspacePath(selected);
      }
    } catch (e) {
      setError(String(e));
    }
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
      clearSessionActivity(sessionId);
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
    const convId = await resolveConversationsProjectId();
    const inWorkspace =
      Boolean(activeProjectId && convId && activeProjectId !== convId);

    if (activeSessionId) {
      const sessionInCurrentProject =
        !inWorkspace ||
        activeSession?.projectId === activeProjectId ||
        sessions.some((s) => s.id === activeSessionId);
      if (sessionInCurrentProject) {
        return activeSessionId;
      }
    }

    if (inWorkspace && activeProjectId) {
      const listed = await invoke<Session[]>("list_sessions", { projectId: activeProjectId });
      const existing = [...listed].sort((a, b) => b.updatedAt - a.updatedAt)[0];
      if (existing) {
        setActiveSessionId(existing.id);
        return existing.id;
      }
      const session = await invoke<Session>("create_session", {
        title: "新对话",
        projectId: activeProjectId,
      });
      await refreshSessions(activeProjectId, session.id);
      return session.id;
    }

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
        setChatMode(settings.agentEnabledDefault ? "agent" : "chat");
      } catch {
        /* ignore */
      }
    })().catch((e) => setError(String(e)));
  }, [isActive]);

  useEffect(() => {
    if (!isActive) return;
    const unlisten = listen<AgentToolEvent>("agent-tool", (event) => {
      const p = event.payload;
      applyAgentToolEvent(p);
      if (
        p.status === "done" &&
        ["write_file", "apply_patch", "delete_file"].includes(p.toolName)
      ) {
        setChangesRefresh((n) => n + 1);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [isActive]);

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
    if (blockMessagesFetchRef.current === activeSessionId) {
      return;
    }
    invoke<MessageView[]>("get_messages", { sessionId: activeSessionId })
      .then(setMessages)
      .catch((e) => setError(String(e)));
  }, [activeSessionId]);

  useEffect(() => {
    setImportedBannerDismissed(false);
  }, [activeSessionId]);

  useEffect(() => {
    if (!activeSessionId) {
      setProjectContext(null);
      return;
    }
    invoke<ProjectContextBundle | null>("get_project_context", { sessionId: activeSessionId })
      .then(setProjectContext)
      .catch(() => setProjectContext(null));
  }, [activeSessionId, activeSession?.workspacePath, activeProject?.workspacePath]);

  async function handleCancelGeneration() {
    if (!anySessionGenerating) return;
    const rollback = pendingSendRollbackRef.current;
    sendSeqRef.current += 1;
    try {
      await invoke("cancel_chat_generation");
    } catch {
      /* ignore */
    }
    if (rollback) {
      rollbackFailedSend(rollback);
      pendingSendRollbackRef.current = null;
    }
    setSessionActivities((prev) => {
      const next = { ...prev };
      const queueNext = { ...sessionQueuesRef.current };
      for (const [sessionId, activity] of Object.entries(prev)) {
        if (activity.status === "running") {
          delete next[sessionId];
          delete queueNext[sessionId];
        }
      }
      syncSessionQueues(queueNext);
      return next;
    });
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
    const projectId =
      activeSession?.projectId ??
      activeProjectId ??
      conversations?.id ??
      "";
    setShellModal(null);

    setError(null);
    setFailoverHint(null);
    const sendSeq = ++sendSeqRef.current;
    if (projectId) {
      startSessionActivity(sessionId, projectId, sendSeq);
    }

    const unlisten = await subscribeChatStream(sessionId, sendSeq, (providerName, attempts) => {
      setFailoverHint(zh.chat.failoveredHint(providerName, attempts));
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
      if (activeSessionIdRef.current === sessionId) {
        setMessages(updated);
      }

      if (!run) {
        setFailoverHint(zh.chat.rejectShell);
      } else if (!response.agentPaused) {
        setFailoverHint(zh.chat.shellResumed);
      }
    } catch (e) {
      if (activeSessionIdRef.current === sessionId) {
        setError(String(e));
      }
    } finally {
      unlisten();
      if (sendSeq === sendSeqRef.current && projectId) {
        finishSessionActivity(sessionId, activeSessionIdRef.current === sessionId);
        setChangesRefresh((n) => n + 1);
      }
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

  async function handleContinueFromImport() {
    if (!activeSessionId || activeSession?.source === "native") return;
    setError(null);
    try {
      const continued = await invoke<Session>("continue_from_import", {
        importedSessionId: activeSessionId,
      });
      if (activeProjectId) {
        await refreshSessions(activeProjectId, continued.id);
      }
      setActiveSessionId(continued.id);
    } catch (e) {
      setError(String(e));
    }
  }

  async function drainMessageQueue(sessionId: string) {
    if (sendInFlightRef.current) return;
    const next = dequeueMessage(sessionId);
    if (!next) return;
    const outgoing = formatMessageWithAttachments(next.content, next.attachments);
    await executeSend(outgoing, next.attachments, next.content);
  }

  async function executeSend(
    outgoing: string,
    pendingAttachments: ChatAttachment[],
    rawContent: string,
  ) {
    sendInFlightRef.current = true;
    setError(null);
    setFailoverHint(null);
    const sendSeq = ++sendSeqRef.current;

    let sessionId = "";
    let sessionProjectId = "";
    let unlisten: (() => void) | null = null;

    try {
      const wasEmpty = messages.length === 0;
      const convId = conversations?.id ?? null;
      const workspaceProjectId =
        activeProjectId && convId && activeProjectId !== convId ? activeProjectId : null;

      sessionId = await ensureSession();
      sessionProjectId = workspaceProjectId ?? convId ?? activeProjectId ?? "";

      if (sessionProjectId) {
        startSessionActivity(sessionId, sessionProjectId, sendSeq);
      }
      blockMessagesFetchRef.current = sessionId;

      const optimisticUser: MessageView = {
        id: `optimistic-user-${sendSeq}`,
        sessionId,
        seq: messages.length + 1,
        role: "user",
        parts: [{ partType: "text", text: outgoing }],
        preview: outgoing.slice(0, 120),
        createdAt: Math.floor(Date.now() / 1000),
      };
      setMessages((prev) => [...prev, optimisticUser]);
      pendingSendRollbackRef.current = {
        sendSeq,
        sessionId,
        rawContent,
        attachments: pendingAttachments,
        optimisticId: optimisticUser.id,
      };
      markStickToBottom();
      setInput("");
      setAttachments([]);

      unlisten = await subscribeChatStream(sessionId, sendSeq, (providerName, attempts) => {
        setFailoverHint(zh.chat.failoveredHint(providerName, attempts));
      });

      patchSessionPhase(sessionId, sendSeq, zh.chat.awaitingBackend, "await-backend");

      const providerId =
        selectedProviderId === AUTO_PROVIDER ? null : selectedProviderId;

      const response = await invoke<ChatResponse>("send_message", {
        sessionId,
        content: outgoing,
        providerId,
        autoFailover: true,
        agentMode: chatMode !== "chat",
        planMode: chatMode === "plan",
      });

      if (response.agentPaused && response.pendingCommand) {
        setShellModal({
          action: response.pendingAction ?? "shell",
          payload: response.pendingCommand,
        });
      }

      pendingSendRollbackRef.current = null;

      const updatedProjects = await refreshProjects(workspaceProjectId ?? convId ?? undefined);
      const updatedConvId =
        updatedProjects.find(isConversationsProject)?.id ?? convId;

      if (wasEmpty) {
        const title =
          rawContent.trim().slice(0, 80) ||
          pendingAttachments[0]?.fileName ||
          "新对话";
        await invoke<Session>("rename_session", { sessionId, title });
      }

      const updated = await invoke<MessageView[]>("get_messages", { sessionId });
      if (activeSessionIdRef.current === sessionId) {
        blockMessagesFetchRef.current = null;
        setMessages(updated);
      }

      const resolvedProjectId = workspaceProjectId ?? updatedConvId;
      if (resolvedProjectId === updatedConvId && updatedConvId) {
        await refreshConversationSessions(sessionId, updatedConvId);
      } else if (resolvedProjectId) {
        await refreshSessions(resolvedProjectId, sessionId);
        if (activeSessionIdRef.current === sessionId) {
          setActiveProjectId(resolvedProjectId);
        }
      }

      if (response.failovered && activeSessionIdRef.current === sessionId) {
        setFailoverHint(
          zh.chat.failoveredHint(response.providerName, response.attempts),
        );
      }

      if (!response.content && activeSessionIdRef.current === sessionId) {
        setError(zh.chat.providerEmpty);
      }
    } catch (e) {
      const rollback = pendingSendRollbackRef.current;
      if (rollback?.sendSeq === sendSeq) {
        rollbackFailedSend(rollback, { error: String(e) });
        pendingSendRollbackRef.current = null;
      }
    } finally {
      sendInFlightRef.current = false;
      unlisten?.();
      if (sendSeq === sendSeqRef.current && sessionId && sessionProjectId) {
        finishSessionActivity(sessionId, activeSessionIdRef.current === sessionId);
        setChangesRefresh((n) => n + 1);
      }
      if (sendSeq === sendSeqRef.current && sessionId) {
        await drainMessageQueue(sessionId);
      }
    }
  }

  async function handleSend() {
    if (activeSession && activeSession.source !== "native") {
      setError(zh.chat.importedReadOnly);
      return;
    }

    const content = input.trim();
    if (!content && attachments.length === 0) {
      setError(zh.chat.noInput);
      return;
    }

    const pendingAttachments = attachments;
    const outgoing = formatMessageWithAttachments(content, pendingAttachments);

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

    if (anySessionGenerating && !isActiveGenerating) {
      setError(zh.chat.waitOtherSession);
      return;
    }

    if (isActiveGenerating || sendInFlightRef.current) {
      const sessionId = activeSessionId ?? (await ensureSession());
      enqueueMessage(sessionId, content, pendingAttachments);
      setInput("");
      setAttachments([]);
      setError(null);
      return;
    }

    await executeSend(outgoing, pendingAttachments, content);
  }

  const headerWorkspace =
    activeSession?.workspacePath ?? activeProject?.workspacePath ?? undefined;

  const isImportedReadOnly = activeSession != null && activeSession.source !== "native";
  const providerReady = usableProviders.length > 0;
  const canSend =
    providerReady &&
    !isImportedReadOnly &&
    !attachmentSaving &&
    !(anySessionGenerating && !isActiveGenerating);

  const selectedProvider =
    selectedProviderId === AUTO_PROVIDER
      ? usableProviders[0]
      : usableProviders.find((p) => p.id === selectedProviderId);

  function renderActivityDot(
    status: SessionActivityStatus | null,
    title: string,
    className = "nav-activity-dot",
  ) {
    if (!status) return null;
    return (
      <span
        className={`${className} ${status === "running" ? "running" : "done"}`}
        title={title}
        aria-hidden={status === "done"}
      />
    );
  }

  function selectSession(session: Session, projectId?: string | null) {
    if (sessionActivities[session.id]?.status === "done") {
      clearSessionActivity(session.id);
    }
    setActiveProjectId(projectId ?? conversations?.id ?? activeProjectId);
    setActiveSessionId(session.id);
  }

  function renderSessionRow(session: Session, project?: Project) {
    const inConversations = project ? isConversationsProject(project) : false;
    const sessionActivity = sessionActivities[session.id]?.status ?? null;
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
            <span className="nav-thread-title">
              {renderActivityDot(
                sessionActivity,
                sessionActivity === "running"
                  ? zh.chat.sessionRunning
                  : zh.chat.sessionDone,
                "nav-activity-dot nav-thread-activity",
              )}
              <span className="nav-thread-title-text">
                {displaySessionTitle(session, project)}
              </span>
            </span>
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
                  const projectActivity = projectActivityStatus(sessionActivities, project.id);
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
                          {renderActivityDot(
                            projectActivity,
                            projectActivity === "running"
                              ? zh.chat.projectRunning
                              : zh.chat.projectDone,
                            "nav-activity-dot nav-project-activity",
                          )}
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
          <div className="chat-header-row">
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
            {headerWorkspace && (
              <p className="project-context-hint muted" title={
                projectContext
                  ? [
                      ...projectContext.rules.map((r) => r.path),
                      ...projectContext.skills.map((s) => s.path),
                    ].join("\n")
                  : undefined
              }>
                {projectContext
                  ? projectContext.rules.length + projectContext.skills.length > 0
                    ? zh.chat.projectContextLoaded(
                        projectContext.rules.length,
                        projectContext.skills.length,
                      )
                    : zh.chat.projectContextEmpty
                  : zh.chat.projectContextNoWorkspace}
              </p>
            )}
            {activeSession && activeSession.source !== "native" && !importedBannerDismissed && (
              <DismissibleNotice
                variant="banner"
                className="imported-readonly-banner"
                onDismiss={() => setImportedBannerDismissed(true)}
              >
                <p className="provenance-row">
                  <span className={`badge badge-${activeSession.source}`}>
                    {sourceLabel(activeSession.source)}
                  </span>
                  <span className="muted">{zh.chat.provenanceImported}</span>
                </p>
                <p className="muted">{zh.chat.importedReadOnlyHint}</p>
                <button
                  type="button"
                  className="btn-primary btn-sm"
                  onClick={() => void handleContinueFromImport()}
                >
                  {zh.chat.continueFromImport}
                </button>
              </DismissibleNotice>
            )}
            {activeSession?.continuedFrom && activeSession.source === "native" && (
              <p className="provenance-row muted">{zh.chat.continuedFrom}</p>
            )}
          </div>
          </div>
          {error && (
            <DismissibleNotice
              variant="error"
              className="chat-header-status"
              onDismiss={() => setError(null)}
            >
              {error}
            </DismissibleNotice>
          )}
          {!error && failoverHint && (
            <DismissibleNotice
              variant="info"
              className="chat-header-status"
              onDismiss={() => setFailoverHint(null)}
            >
              {failoverHint}
            </DismissibleNotice>
          )}
        </header>

        <div className="message-scroll" ref={messageScrollRef} onScroll={handleMessageScroll}>
          {messages.length === 0 && !isActiveGenerating && (
            <div className="empty-hero">
              <div className="empty-glow" />
              <div className="empty-icon">◈</div>
              <h2>warp-ade</h2>
              <p>{zh.chat.emptyHint}</p>
              {!providerReady && (
                <div className="empty-onboarding">
                  <p className="muted">
                    {allProviders.length > 0 ? zh.chat.missingKeyHint : zh.chat.configureProviderFirst}
                  </p>
                  {onNavigateToProviders && (
                    <button type="button" className="btn-primary" onClick={onNavigateToProviders}>
                      {zh.chat.goConfigureProvider}
                    </button>
                  )}
                </div>
              )}
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
            {isActiveGenerating && (
              <article className="message-row message-row-assistant">
                <div className="message-avatar">AI</div>
                <div className="message-bubble message-assistant live-generation-bubble">
                  <div className="message-role">{roleLabel("assistant")}</div>
                  <LiveGenerationView steps={activeSessionActivity?.liveSteps ?? []} />
                </div>
              </article>
            )}
            <div ref={bottomRef} />
          </div>
        </div>

        <footer className="composer-dock">
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
            {activeQueue.length > 0 && (
              <ul className="composer-queue" aria-label={zh.chat.queueTitle}>
                {activeQueue.map((item, index) => (
                  <li key={item.id} className="composer-queue-item">
                    <span className="composer-queue-index">{index + 1}</span>
                    <span className="composer-queue-text">
                      {item.content.trim() ||
                        item.attachments.map((file) => attachmentLabel(file)).join(" · ") ||
                        "…"}
                    </span>
                    <button
                      type="button"
                      className="composer-queue-remove"
                      onClick={() =>
                        activeSessionId && removeQueuedMessage(activeSessionId, item.id)
                      }
                      title={zh.chat.queueRemove}
                      aria-label={zh.chat.queueRemove}
                    >
                      ×
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <textarea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder={isActiveGenerating ? zh.chat.placeholderQueue : zh.chat.placeholder}
              rows={1}
              disabled={attachmentSaving || isImportedReadOnly}
              onPaste={(e) => {
                const files = e.clipboardData?.files;
                if (files && files.length > 0) {
                  e.preventDefault();
                  void addFilesAsAttachments(files);
                }
              }}
              onKeyDown={(e) => {
                if (e.key !== "Enter" || e.shiftKey) return;
                if (e.nativeEvent.isComposing || e.keyCode === 229) return;
                e.preventDefault();
                void handleSend();
              }}
            />
            <div className="composer-toolbar">
              <div className="composer-toolbar-left">
                <label
                  className={`composer-pill composer-mode-pill composer-mode-${chatMode}`}
                  title={zh.chat.chatModeHint[chatMode]}
                >
                  <span className="composer-pill-icon" aria-hidden="true">
                    {chatMode === "agent" ? "∞" : chatMode === "plan" ? "◈" : "T"}
                  </span>
                  <select
                    className="composer-pill-select"
                    value={chatMode}
                    onChange={(e) => setChatMode(e.target.value as ChatMode)}
                    aria-label={zh.chat.chatModeGroup}
                  >
                    {CHAT_MODES.map((mode) => (
                      <option key={mode} value={mode} title={zh.chat.chatModeHint[mode]}>
                        {zh.chat.chatModeLabel[mode]}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="composer-pill composer-provider-pill">
                  <span className="provider-dot" data-ready={providerReady} />
                  <select
                    className="composer-pill-select composer-provider-select"
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
                        <option value={AUTO_PROVIDER}>{zh.chat.autoProviderShort}</option>
                        {usableProviders.map((p) => (
                          <option key={p.id} value={p.id}>
                            {p.name} · {p.defaultModel}
                          </option>
                        ))}
                      </>
                    )}
                  </select>
                </label>
              </div>
              <div className="composer-toolbar-actions">
                {isActiveGenerating ? (
                  <button
                    type="button"
                    className="composer-stop-btn"
                    onClick={() => void handleCancelGeneration()}
                    title={zh.chat.cancelGeneration}
                    aria-label={zh.chat.cancelGeneration}
                  >
                    ■
                  </button>
                ) : null}
                <button type="button" className="btn-primary btn-sm" onClick={handleSend} disabled={!canSend}>
                  {zh.chat.send}
                </button>
              </div>
            </div>
          </div>
        </footer>
      </section>

      <EnvironmentPanel
        isActive={isActive}
        workspacePath={headerWorkspace}
        source={activeProject?.sourceOrigin}
        refreshToken={changesRefresh}
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
            <p className="muted">{zh.chat.workspacePathHint}</p>
            <div className="workspace-path-row">
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
              <button
                type="button"
                className="btn-ghost"
                onClick={() => void pickWorkspaceFolder()}
              >
                {zh.chat.pickWorkspaceFolder}
              </button>
            </div>
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
