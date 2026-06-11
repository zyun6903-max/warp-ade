export type WriteFilePreview = {
  kind: "write_file";
  path: string;
  bytes: number;
  label: string;
};

export type ApplyPatchPreview = {
  kind: "apply_patch";
  path: string;
  oldText: string;
  newText: string;
  fileName: string;
  removedLines: number;
  addedLines: number;
};

const WRITE_DONE_RE = /^已写入\s+(.+?)（(\d+)\s*字节）$/;
const WRITE_UPDATE_RE = /^已更新\s+(.+?)（替换\s*(\d+)\s*处）$/;

function tryParseJson(preview: string): unknown {
  const trimmed = preview.trim();
  if (!trimmed.startsWith("{")) return null;
  try {
    return JSON.parse(trimmed) as unknown;
  } catch {
    return null;
  }
}

function basename(path: string): string {
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

export function parseWriteFilePreview(preview: string): WriteFilePreview | null {
  const trimmed = preview.trim();
  if (!trimmed) return null;

  const done = WRITE_DONE_RE.exec(trimmed);
  if (done) {
    return {
      kind: "write_file",
      path: done[1]!.trim(),
      bytes: Number.parseInt(done[2]!, 10) || 0,
      label: "已写入",
    };
  }

  const parsed = tryParseJson(trimmed) as Record<string, unknown> | null;
  if (parsed?.kind === "write_file" && typeof parsed.path === "string") {
    return {
      kind: "write_file",
      path: parsed.path,
      bytes: typeof parsed.bytes === "number" ? parsed.bytes : 0,
      label: "写入",
    };
  }

  const loosePath = trimmed.match(/"path"\s*:\s*"([^"]+)"/);
  if (loosePath) {
    return {
      kind: "write_file",
      path: loosePath[1]!,
      bytes: 0,
      label: "写入",
    };
  }

  return null;
}

export function parseApplyPatchPreview(preview: string): ApplyPatchPreview | null {
  const trimmed = preview.trim();
  if (!trimmed) return null;

  const parsed = tryParseJson(trimmed) as Record<string, unknown> | null;
  if (parsed?.kind === "apply_patch" && typeof parsed.path === "string") {
    const oldText = typeof parsed.old === "string" ? parsed.old : "";
    const newText = typeof parsed.new === "string" ? parsed.new : "";
    const oldLines = oldText ? oldText.split("\n") : [];
    const newLines = newText ? newText.split("\n") : [];
    return {
      kind: "apply_patch",
      path: parsed.path,
      oldText,
      newText,
      fileName: basename(parsed.path),
      removedLines: oldLines.length,
      addedLines: newLines.length,
    };
  }

  const updated = WRITE_UPDATE_RE.exec(trimmed);
  if (updated) {
    return {
      kind: "apply_patch",
      path: updated[1]!.trim(),
      oldText: "",
      newText: "",
      fileName: basename(updated[1]!.trim()),
      removedLines: 0,
      addedLines: 0,
    };
  }

  return null;
}

export function isFileMutationTool(toolName: string): boolean {
  return toolName === "write_file" || toolName === "apply_patch";
}

export function formatWriteFileLine(preview: WriteFilePreview): string {
  if (preview.bytes > 0) {
    return `${preview.label} ${preview.path}（${preview.bytes} 字节）`;
  }
  return `${preview.label} ${preview.path}`;
}

export function patchDiffLines(
  preview: ApplyPatchPreview,
): Array<{ type: "del" | "add"; text: string }> {
  const lines: Array<{ type: "del" | "add"; text: string }> = [];
  for (const line of preview.oldText.split("\n")) {
    lines.push({ type: "del", text: line });
  }
  for (const line of preview.newText.split("\n")) {
    lines.push({ type: "add", text: line });
  }
  return lines.slice(0, 24);
}
