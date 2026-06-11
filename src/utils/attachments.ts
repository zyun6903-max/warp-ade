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

export function attachmentLabel(attachment: ChatAttachment): string {
  return `${attachment.fileName} (${attachment.kind})`;
}
