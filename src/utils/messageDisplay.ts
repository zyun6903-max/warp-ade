import type { MessagePart } from "../types";

const XML_BLOCK_PATTERNS = [
  /<function_calls>[\s\S]*?<\/function_calls>/gi,
  /<invoke[\s\S]*?<\/invoke>/gi,
  /<tool_call[\s\S]*?<\/tool_call>/gi,
  /<antml:[\w_]+>[\s\S]*?<\/antml:[\w_]+>/gi,
  /<parameter[\s\S]*?<\/parameter>/gi,
];

/** 去掉 Cursor / Claude 等 Agent 导出里常见的 XML 工具块，避免聊天里露出裸标签 */
export function sanitizeAgentMarkup(text: string): string {
  let out = text;
  for (const re of XML_BLOCK_PATTERNS) {
    out = out.replace(re, "");
  }
  out = out.replace(/^\s*<\/?[a-zA-Z][\w:-]*(?:\s[^>]*)?\/?>\s*$/gm, "");
  out = out.replace(/\n{3,}/g, "\n\n");
  return out.trim();
}

export function flattenMessageParts(parts: MessagePart[]): {
  text: string;
  tools: string[];
  toolCounts: Map<string, number>;
} {
  const textParts: string[] = [];
  const toolCounts = new Map<string, number>();

  for (const part of parts) {
    if (part.partType === "tool_call") {
      const label = part.name?.trim() || "工具";
      toolCounts.set(label, (toolCounts.get(label) ?? 0) + 1);
      continue;
    }
    const raw = part.text?.trim();
    if (!raw) continue;
    const cleaned = sanitizeAgentMarkup(raw);
    if (cleaned) textParts.push(cleaned);
  }

  return {
    text: textParts.join("\n\n"),
    tools: [...toolCounts.keys()],
    toolCounts,
  };
}

export function shouldRenderMarkdown(text: string, role: string): boolean {
  if (role !== "assistant") return false;
  const t = text.trim();
  if (!t) return false;
  if (
    /<[a-zA-Z][\w:-]*[\s/>]/.test(t) &&
    (t.includes("</") || t.includes("<function") || t.includes("<invoke"))
  ) {
    return false;
  }
  return true;
}
