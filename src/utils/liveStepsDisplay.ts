import type { LiveStep, LiveToolStatus } from "../types";
import { parseApplyPatchPreview, parseWriteFilePreview } from "./toolPreviewFormat";

const PHASE_TOOL = "__phase__";

export type ToolStep = Extract<LiveStep, { kind: "tool" }>;

export type LiveDisplayItem =
  | { kind: "text"; step: Extract<LiveStep, { kind: "text" }> }
  | { kind: "status"; step: Extract<LiveStep, { kind: "status" }> }
  | { kind: "tool"; step: ToolStep }
  | { kind: "tool-group"; id: string; toolName: string; steps: ToolStep[] };

function stepHasContent(step: LiveStep): boolean {
  if (step.kind === "status") return step.label.trim().length > 0;
  if (step.kind === "text") return step.content.trim().length > 0;
  if (step.kind === "tool" && step.toolName === PHASE_TOOL) {
    return step.preview.trim().length > 0;
  }
  if (step.kind === "tool") return true;
  return false;
}

function isActiveToolStatus(status: LiveToolStatus): boolean {
  return status === "streaming" || status === "running" || status === "approval";
}

function collapseStatusSteps(steps: LiveStep[]): LiveStep[] {
  const out: LiveStep[] = [];
  let statusRun: LiveStep[] = [];

  const flushStatus = () => {
    if (statusRun.length === 0) return;
    out.push(statusRun[statusRun.length - 1]!);
    statusRun = [];
  };

  for (const step of steps) {
    const isStatusLike =
      step.kind === "status" || (step.kind === "tool" && step.toolName === PHASE_TOOL);
    if (isStatusLike) {
      statusRun.push(step);
      continue;
    }
    flushStatus();
    out.push(step);
  }
  flushStatus();
  return out;
}

function groupToolSteps(steps: LiveStep[]): LiveDisplayItem[] {
  const items: LiveDisplayItem[] = [];
  let toolRun: ToolStep[] = [];

  const flushTools = () => {
    if (toolRun.length === 0) return;
    if (toolRun.length === 1) {
      items.push({ kind: "tool", step: toolRun[0]! });
    } else {
      items.push({
        kind: "tool-group",
        id: `group-${toolRun[0]!.id}-${toolRun.length}`,
        toolName: toolRun[0]!.toolName,
        steps: [...toolRun],
      });
    }
    toolRun = [];
  };

  for (const step of steps) {
    if (step.kind === "tool" && step.toolName !== PHASE_TOOL) {
      const last = toolRun[toolRun.length - 1];
      if (last && last.toolName === step.toolName) {
        toolRun.push(step);
      } else {
        flushTools();
        toolRun = [step];
      }
      continue;
    }
    flushTools();
    if (step.kind === "text") {
      items.push({ kind: "text", step });
    } else if (step.kind === "status") {
      items.push({ kind: "status", step });
    } else if (step.kind === "tool" && step.toolName === PHASE_TOOL) {
      items.push({
        kind: "status",
        step: { kind: "status", id: step.id, label: step.preview.trim() || "…" },
      });
    }
  }
  flushTools();
  return items;
}

export function buildLiveDisplayItems(steps: LiveStep[]): LiveDisplayItem[] {
  const visible = steps.filter(stepHasContent);
  const collapsedStatus = collapseStatusSteps(visible);
  return groupToolSteps(collapsedStatus);
}

export function summarizeToolPreview(toolName: string, preview: string, max = 72): string {
  if (toolName === "write_file") {
    const parsed = parseWriteFilePreview(preview);
    if (parsed) {
      const name = parsed.path.split(/[/\\]/).pop() || parsed.path;
      return name.length > max ? `${name.slice(0, max)}…` : name;
    }
  }
  if (toolName === "apply_patch") {
    const parsed = parseApplyPatchPreview(preview);
    if (parsed) return parsed.fileName;
  }

  const trimmed = preview.trim().replace(/\s+/g, " ");
  if (!trimmed) return toolName;
  if (toolName === "read_file" || toolName === "grep_project" || toolName === "glob_files") {
    const pathMatch = trimmed.match(/["']?([^\s"']+)["']?/);
    const hint = pathMatch?.[1] ?? trimmed;
    return hint.length > max ? `${hint.slice(0, max)}…` : hint;
  }
  return trimmed.length > max ? `${trimmed.slice(0, max)}…` : trimmed;
}

export function shouldExpandToolGroup(steps: ToolStep[]): boolean {
  return steps.some((step) => isActiveToolStatus(step.status));
}

export function shouldExpandSingleTool(step: ToolStep): boolean {
  return isActiveToolStatus(step.status);
}
