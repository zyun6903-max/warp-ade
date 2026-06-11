import { folderNameFromPath } from "./display";

export type ImportCandidateLike = {
  sourcePath: string;
  projectSlug: string;
  sessionId: string;
  modifiedAt: number;
  workspacePath?: string;
};

/** Cursor/Claude slug 最后一级常为目录名；带连字符的目录名需合并最后两段 */
export function folderNameFromProjectSlug(slug: string): string {
  const normalized = slug.trim().replace(/^-+/, "");
  if (!normalized) return "—";

  if (folderNameFromPath(normalized)) {
    return folderNameFromPath(normalized)!;
  }

  if (!normalized.startsWith("Users-")) {
    if (normalized.length <= 36) return normalized;
    return `${normalized.slice(0, 18)}…`;
  }

  const parts = normalized.split("-").filter(Boolean);
  if (parts.length < 3) {
    return parts[parts.length - 1] ?? normalized;
  }

  const last = parts[parts.length - 1] ?? "";
  const prev = parts[parts.length - 2] ?? "";
  if (prev && last) {
    return `${prev}-${last}`;
  }
  return last || normalized;
}

export function displayImportProjectName(item: {
  projectSlug: string;
  workspacePath?: string;
}): string {
  const fromWorkspace = folderNameFromPath(item.workspacePath);
  if (fromWorkspace) return fromWorkspace;
  return folderNameFromProjectSlug(item.projectSlug);
}

export function filterImportCandidates<T extends { projectSlug: string; sessionId: string; sourcePath: string }>(
  items: T[],
  query: string,
): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return items;
  return items.filter((item) => {
    const project = displayImportProjectName(item).toLowerCase();
    const haystack = [project, item.projectSlug, item.sessionId, item.sourcePath].join(" ").toLowerCase();
    return haystack.includes(q);
  });
}

export function importSourceToSessionSource(source: "cursor" | "claude" | "codex"): string {
  if (source === "claude") return "claude_code";
  return source;
}

export const IMPORT_LATEST_PER_PROJECT = 5;

export function importProjectKey(item: { projectSlug: string; workspacePath?: string }): string {
  return item.workspacePath?.trim() || item.projectSlug.trim() || "unknown";
}

/** 每个项目仅保留按 modifiedAt 倒序的前 N 条 */
export function limitLatestPerProject<T extends ImportCandidateLike>(
  items: T[],
  perProject = IMPORT_LATEST_PER_PROJECT,
): T[] {
  if (perProject <= 0) return [];
  const byProject = new Map<string, T[]>();
  for (const item of items) {
    const key = importProjectKey(item);
    const bucket = byProject.get(key);
    if (bucket) bucket.push(item);
    else byProject.set(key, [item]);
  }

  const limited: T[] = [];
  for (const bucket of byProject.values()) {
    bucket.sort((a, b) => b.modifiedAt - a.modifiedAt);
    limited.push(...bucket.slice(0, perProject));
  }
  return limited.sort((a, b) => b.modifiedAt - a.modifiedAt);
}

export function dedupeImportCandidates<T extends ImportCandidateLike>(items: T[]): T[] {
  const bySource = new Map<string, T>();
  for (const item of items) {
    if (!bySource.has(item.sourcePath)) {
      bySource.set(item.sourcePath, item);
    }
  }

  const byProjectSession = new Map<string, T>();
  for (const item of bySource.values()) {
    const key = `${item.projectSlug}::${item.sessionId}`;
    const existing = byProjectSession.get(key);
    if (!existing || item.modifiedAt > existing.modifiedAt) {
      byProjectSession.set(key, item);
    }
  }

  return [...byProjectSession.values()].sort((a, b) => b.modifiedAt - a.modifiedAt);
}
