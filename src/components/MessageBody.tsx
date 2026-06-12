import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { zh } from "../i18n/zh";
import type { MessageView } from "../types";
import { parseMessageForDisplay } from "../utils/attachments";
import {
  flattenMessageParts,
  shouldRenderMarkdown,
} from "../utils/messageDisplay";
import { MessageAttachmentPreviews } from "./MessageAttachmentPreviews";

type MessageBodyProps = {
  message: MessageView;
};

export function MessageBody({ message }: MessageBodyProps) {
  const { text: rawText, toolCounts } = flattenMessageParts(message.parts);
  const parsed =
    message.role === "user" ? parseMessageForDisplay(rawText) : { text: rawText, attachments: [] };
  const text = parsed.text;
  const displayAttachments = parsed.attachments;
  const toolEntries = [...toolCounts.entries()];
  const totalTools = toolEntries.reduce((sum, [, count]) => sum + count, 0);
  const [toolsExpanded, setToolsExpanded] = useState(false);

  if (!text.trim() && toolEntries.length === 0 && displayAttachments.length === 0) {
    return <div className="message-body muted">—</div>;
  }

  const showCollapsedTools = totalTools > 1 && !toolsExpanded;

  return (
    <div className="message-body-wrap">
      {toolEntries.length > 0 && (
        <div className="message-tool-row" aria-label="工具调用">
          {showCollapsedTools ? (
            <button
              type="button"
              className="message-tool-collapse"
              onClick={() => setToolsExpanded(true)}
            >
              {toolEntries
                .map(([name, count]) =>
                  count > 1 ? `${name} ×${count}` : name,
                )
                .join(" · ")}
              <span className="message-tool-ellipsis"> …</span>
            </button>
          ) : (
            <>
              {toolEntries.map(([name, count]) => (
                <span key={name} className="message-tool-chip">
                  {count > 1 ? `${name} ×${count}` : name}
                </span>
              ))}
              {totalTools > 1 ? (
                <button
                  type="button"
                  className="message-tool-collapse-btn"
                  onClick={() => setToolsExpanded(false)}
                >
                  {zh.chat.toolGroupFold}
                </button>
              ) : null}
            </>
          )}
        </div>
      )}
      {message.role === "user" && displayAttachments.length > 0 ? (
        <MessageAttachmentPreviews attachments={displayAttachments} />
      ) : null}
      {text.trim() ? (
        message.role === "user" ? (
          <div className="message-body message-body-plain">{text}</div>
        ) : shouldRenderMarkdown(text, message.role) ? (
          <div className="message-body message-body-markdown">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
          </div>
        ) : (
          <div className="message-body message-body-plain">{text}</div>
        )
      ) : null}
    </div>
  );
}
