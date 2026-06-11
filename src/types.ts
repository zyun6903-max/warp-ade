export type MessagePart = {
  partType: string;
  text?: string;
  name?: string;
  input?: unknown;
};

export type MessageView = {
  id: string;
  sessionId: string;
  seq: number;
  role: string;
  parts: MessagePart[];
  preview: string;
  createdAt: number;
  metadata?: Record<string, unknown>;
};

export type Session = {
  id: string;
  title: string;
  source: string;
  sourcePath?: string;
  projectSlug?: string;
  workspacePath?: string;
  projectId?: string;
  continuedFrom?: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
};

export type SessionSearchHit = {
  session: Session;
  projectName?: string;
  projectWorkspacePath?: string;
  matchedPreview: string;
  matchedSeq: number;
  matchedAt: number;
};

export type Project = {
  id: string;
  name: string;
  workspacePath?: string;
  sourceSlug?: string;
  sourceOrigin: string;
  createdAt: number;
  updatedAt: number;
  sessionCount: number;
};

export type Provider = {
  id: string;
  name: string;
  baseUrl: string;
  apiFormat: string;
  models: string[];
  defaultModel: string;
  priority: number;
  enabled: boolean;
  hasKey: boolean;
};

export type CursorImportCandidate = {
  sourcePath: string;
  projectSlug: string;
  sessionId: string;
  title: string;
  messageCountEstimate: number;
  modifiedAt: number;
  alreadyImported: boolean;
  workspacePath?: string;
};

export type ImportSourceSearchHit = {
  sourcePath: string;
  projectSlug: string;
  sessionId: string;
  modifiedAt: number;
  alreadyImported: boolean;
  workspacePath?: string;
  matchedPreview: string;
};

export type ChatStreamEvent = {
  sessionId: string;
  chunk?: string;
  done?: ChatResponse;
  error?: string;
};

export type ChatResponse = {
  content: string;
  providerId: string;
  providerName: string;
  failovered: boolean;
  attempts: number;
  agentPaused?: boolean;
  approvalId?: string;
  pendingAction?: string;
  pendingCommand?: string;
};

export type AppContextSettings = {
  recentTurns: number;
  tokenThreshold: number;
  summaryEnabled: boolean;
  agentMaxIterations: number;
  agentEnabledDefault: boolean;
  shellAutoReadonly: boolean;
  shellAlwaysConfirm: boolean;
  shellExtraAllowlist: string;
  webSearchEnabled: boolean;
  webSearchProvider: "brave" | "tavily";
  webSearchMaxResults: number;
  agentSubagentMaxIterations: number;
  semanticSearchEnabled: boolean;
  semanticSearchModel: string;
  semanticSearchProviderId: string;
  semanticSearchMaxResults: number;
  workspaceOutsideRead: "block" | "confirm" | "allow";
  workspaceOutsideWrite: "block" | "confirm";
};

export type ToolAuditEntry = {
  id: string;
  sessionId?: string;
  toolName: string;
  mode: string;
  inputPreview?: string;
  outputPreview?: string;
  createdAt: number;
};

export type CodeIndexStatus = {
  enabled: boolean;
  workspacePath?: string;
  chunkCount: number;
  fileCount: number;
  lastIndexedAt?: number;
  model: string;
};

export type ShellLogEntry = {
  id: string;
  sessionId?: string;
  command: string;
  mode: string;
  exitCode?: number;
  outputPreview?: string;
  createdAt: number;
};

export type McpServer = {
  id: string;
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  enabled: boolean;
  createdAt: number;
  updatedAt: number;
};

export type McpTestResult = {
  ok: boolean;
  toolCount: number;
  message: string;
  tools: string[];
};

export type AgentToolEvent = {
  sessionId: string;
  callId: string;
  toolName: string;
  status: string;
  preview: string;
};

export type LiveToolStatus = "streaming" | "running" | "done" | "error" | "approval";

export type LiveStep =
  | { kind: "text"; id: string; content: string }
  | { kind: "status"; id: string; label: string }
  | {
      kind: "tool";
      id: string;
      toolName: string;
      status: LiveToolStatus;
      preview: string;
    };

export type QueuedMessage = {
  id: string;
  content: string;
  attachments: ChatAttachment[];
};

export type TestProviderResult = {
  ok: boolean;
  model: string;
  latencyMs: number;
  message: string;
};

export type ProviderUsageRow = {
  providerId: string;
  providerName: string;
  model: string;
  requestCount: number;
  inputTokens: number;
  outputTokens: number;
  testCount: number;
  lastUsedAt?: number;
};

export type AppInfo = {
  name: string;
  version: string;
  dataDir?: string;
};

export type SaveProviderInput = {
  id?: string;
  name: string;
  baseUrl: string;
  apiFormat: string;
  models: string[];
  defaultModel: string;
  priority?: number;
  enabled: boolean;
  apiKey?: string;
};

export type GitChange = {
  path: string;
  status: string;
  staged: boolean;
};

export type WorkspaceInfo = {
  workspacePath?: string;
  isGitRepo: boolean;
  branch?: string;
  branches: string[];
  insertions: number;
  deletions: number;
  changedFiles: number;
  ahead?: number;
  behind?: number;
  unpushedCommits: number;
  changes: GitChange[];
  githubAuthenticated: boolean;
  githubAuthMessage: string;
  source?: string;
  error?: string;
};

export type ChatAttachment = {
  id: string;
  path: string;
  fileName: string;
  kind: string;
  mime: string;
  size: number;
};

export type BatchImportResult = {
  imported: number;
  skipped: number;
};

export type ProjectRuleEntry = {
  label: string;
  path: string;
  chars: number;
};

export type SkillEntry = {
  name: string;
  description: string;
  path: string;
  source: string;
  chars: number;
};

export type ProjectContextBundle = {
  workspacePath: string;
  rules: ProjectRuleEntry[];
  skills: SkillEntry[];
};

export type FileDiffResult = {
  path: string;
  diff: string;
  isNewFile: boolean;
  isDeleted: boolean;
};

export type Page = "chat" | "providers" | "import" | "settings";
