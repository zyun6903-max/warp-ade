import type { ChatAttachment } from "../types";

export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result;
      if (typeof result !== "string") {
        reject(new Error("无法读取文件"));
        return;
      }
      const comma = result.indexOf(",");
      resolve(comma >= 0 ? result.slice(comma + 1) : result);
    };
    reader.onerror = () => reject(reader.error ?? new Error("无法读取文件"));
    reader.readAsDataURL(file);
  });
}

export function formatMessageWithAttachments(
  text: string,
  attachments: ChatAttachment[],
): string {
  if (attachments.length === 0) {
    return text;
  }
  const lines = attachments.map(
    (a) => `- ${a.path} (${a.kind}, ${a.fileName}, ${a.size} bytes)`,
  );
  const prefix = text.trim() ? `${text.trim()}\n\n` : "";
  return `${prefix}[附件路径]\n${lines.join("\n")}\n\n请使用 read_file 工具按路径读取附件（图片会返回 base64 信息）。`;
}

export type ParsedAttachmentMeta = {
  path: string;
  kind: string;
  fileName: string;
  size: number;
};

const ATTACHMENT_LINE_RE =
  /^- (.+) \(([^,]+), ([^,]+), (\d+) bytes\)$/;

/** 从存储的消息文本中剥离 Agent 用的附件块，供 UI 展示 */
export function parseMessageForDisplay(text: string): {
  text: string;
  attachments: ParsedAttachmentMeta[];
} {
  const marker = "[附件路径]";
  const idx = text.indexOf(marker);
  if (idx === -1) {
    return { text, attachments: [] };
  }

  const displayText = text.slice(0, idx).replace(/\n+$/, "");
  const block = text.slice(idx + marker.length);
  const attachments: ParsedAttachmentMeta[] = [];

  for (const line of block.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("- ")) continue;
    const match = ATTACHMENT_LINE_RE.exec(trimmed);
    if (!match) continue;
    attachments.push({
      path: match[1],
      kind: match[2],
      fileName: match[3],
      size: Number(match[4]),
    });
  }

  return { text: displayText, attachments };
}

export function attachmentLabel(attachment: ChatAttachment): string {
  return `${attachment.fileName} (${attachment.kind})`;
}

export function isImageAttachment(attachment: ChatAttachment): boolean {
  return attachment.kind === "image" || attachment.mime.startsWith("image/");
}

export function revokePreviewUrls(previews: Record<string, string>, ids?: string[]): void {
  if (ids) {
    for (const id of ids) {
      const url = previews[id];
      if (url) URL.revokeObjectURL(url);
    }
    return;
  }
  for (const url of Object.values(previews)) {
    if (url) URL.revokeObjectURL(url);
  }
}

export function pickPreviews(
  previews: Record<string, string>,
  attachments: ChatAttachment[],
): Record<string, string> {
  const picked: Record<string, string> = {};
  for (const a of attachments) {
    if (previews[a.id]) picked[a.id] = previews[a.id];
  }
  return picked;
}

export function omitPreviewIds(
  previews: Record<string, string>,
  ids: string[],
): Record<string, string> {
  const next = { ...previews };
  for (const id of ids) delete next[id];
  return next;
}
