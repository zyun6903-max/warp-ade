import { useState } from "react";
import { MessageBody } from "./MessageBody";
import type { MessageView } from "../types";

type MessageBranchProps = {
  message: MessageView;
};

export function MessageBranch({ message }: MessageBranchProps) {
  const [open, setOpen] = useState(false);
  const isSubagent = Boolean(message.metadata?.subagent);

  if (!isSubagent) {
    return <MessageBody message={message} />;
  }

  return (
    <details
      className="message-branch"
      open={open}
      onToggle={(e) => setOpen((e.target as HTMLDetailsElement).open)}
    >
      <summary className="message-branch-summary">子 Agent 分支</summary>
      <div className="message-branch-body">
        <MessageBody message={message} />
      </div>
    </details>
  );
}
