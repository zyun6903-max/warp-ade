import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { MessageView } from "../types";
import {
  flattenMessageParts,
  shouldRenderMarkdown,
} from "../utils/messageDisplay";

type MessageBodyProps = {
  message: MessageView;
};

export function MessageBody({ message }: MessageBodyProps) {
  const { text, tools } = flattenMessageParts(message.parts);

  if (!text.trim() && tools.length === 0) {
    return <div className="message-body muted">—</div>;
  }

  return (
    <div className="message-body-wrap">
      {tools.length > 0 && (
        <div className="message-tool-row" aria-label="工具调用">
          {tools.map((name) => (
            <span key={name} className="message-tool-chip">
              {name}
            </span>
          ))}
        </div>
      )}
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
