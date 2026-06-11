import type { Project, Session } from "../types";

export function folderNameFromPath(path?: string): string | undefined {
  if (!path?.trim()) return undefined;
  const trimmed = path.trim().replace(/\/+$/, "");
  const name = trimmed.split("/").filter(Boolean).pop();
  return name || undefined;
}

export function isConversationsProject(project: Project): boolean {
  return (
    project.sourceOrigin === "conversations" ||
    (project.name === "对话" && !project.workspacePath?.trim())
  );
}

export function displayProjectName(project: Project): string {
  const fromPath = folderNameFromPath(project.workspacePath);
  if (fromPath) return fromPath;
  if (isConversationsProject(project)) return "对话";
  return project.name.trim() || "未命名项目";
}

export function displaySessionTitle(session: Session, project?: Project): string {
  if (project && isConversationsProject(project)) {
    const title = session.title.trim();
    return title || "新对话";
  }
  const workspacePath = session.workspacePath?.trim() || project?.workspacePath?.trim();
  const folder = folderNameFromPath(workspacePath);
  if (folder) return folder;
  const title = session.title.trim();
  if (title && title !== "新对话") return title;
  return session.title.trim() || "新对话";
}

export function splitProjects(projects: Project[]) {
  const conversations =
    projects.find(isConversationsProject) ??
    projects.find((p) => p.name === "对话" && !p.workspacePath?.trim());
  const workspaceProjects = projects.filter((p) => p.id !== conversations?.id);
  return { conversations, workspaceProjects };
}

export function sortProjectsForNav(projects: Project[]): Project[] {
  const { conversations, workspaceProjects } = splitProjects(projects);
  const rest = [...workspaceProjects].sort((a, b) => b.updatedAt - a.updatedAt);
  return conversations ? [conversations, ...rest] : rest;
}
