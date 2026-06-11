import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { zh } from "../i18n/zh";
import type { LiveStep, LiveToolStatus } from "../types";
import {
  buildLiveDisplayItems,
  shouldExpandSingleTool,
  shouldExpandToolGroup,
  summarizeToolPreview,
  type ToolStep,
} from "../utils/liveStepsDisplay";
import {
  formatWriteFileLine,
  isFileMutationTool,
  parseApplyPatchPreview,
  parseWriteFilePreview,
  patchDiffLines,
} from "../utils/toolPreviewFormat";

type LiveGenerationViewProps = {
  steps: LiveStep[];
};

function toolStatusLabel(status: LiveToolStatus): string {
  switch (status) {
    case "streaming":
      return zh.chat.toolStatusStreaming;
    case "running":
      return zh.chat.toolStatusRunning;
    case "done":
      return zh.chat.toolStatusDone;
    case "error":
      return zh.chat.toolStatusError;
    case "approval":
      return zh.chat.toolStatusApproval;
    default:
      return status;
  }
}

function WriteFileRow({ preview, status }: { preview: string; status: LiveToolStatus }) {
  const parsed = parseWriteFilePreview(preview);
  return (
    <li className={`live-tool-group-item live-tool-group-item-${status}`}>
      <span className={`live-tool-status live-tool-status-${status}`} />
      <span className="live-tool-group-text live-write-file-line">
        {parsed ? formatWriteFileLine(parsed) : preview || zh.chat.toolPreparing}
      </span>
    </li>
  );
}

function PatchDiffView({
  preview,
  expanded,
}: {
  preview: ReturnType<typeof parseApplyPatchPreview>;
  expanded: boolean;
}) {
  if (!preview) return null;
  const lines = patchDiffLines(preview);
  const hasDiff = lines.length > 0;

  return (
    <div className="live-patch-diff">
      <div className="live-patch-file">
        <span className="live-patch-name">{preview.fileName}</span>
        {(preview.addedLines > 0 || preview.removedLines > 0) && (
          <span className="live-patch-stats">
            {preview.addedLines > 0 ? `+${preview.addedLines}` : null}
            {preview.removedLines > 0 ? ` -${preview.removedLines}` : null}
          </span>
        )}
      </div>
      {expanded && hasDiff ? (
        <div className="live-patch-lines">
          {lines.map((line, index) => (
            <div
              key={`${line.type}-${index}`}
              className={`live-patch-line live-patch-${line.type}`}
            >
              <span className="live-patch-gutter">{line.type === "del" ? "−" : "+"}</span>
              <code>{line.text || " "}</code>
            </div>
          ))}
        </div>
      ) : null}
      {expanded && !hasDiff && preview.path ? (
        <p className="live-tool-preview muted">{preview.path}</p>
      ) : null}
    </div>
  );
}

function renderMutationBody(step: ToolStep, expanded: boolean) {
  if (step.toolName === "write_file") {
    return (
      <ul className="live-tool-group-list live-tool-group-list-single">
        <WriteFileRow preview={step.preview} status={step.status} />
      </ul>
    );
  }

  if (step.toolName === "apply_patch") {
    const parsed = parseApplyPatchPreview(step.preview);
    if (parsed) {
      return <PatchDiffView preview={parsed} expanded={expanded} />;
    }
  }

  return null;
}

function ToolStepCard({
  step,
  defaultExpanded,
}: {
  step: ToolStep;
  defaultExpanded: boolean;
}) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const mutation = isFileMutationTool(step.toolName);
  const writeParsed = step.toolName === "write_file" ? parseWriteFilePreview(step.preview) : null;
  const patchParsed =
    step.toolName === "apply_patch" ? parseApplyPatchPreview(step.preview) : null;
  const summary = mutation
    ? writeParsed?.path || patchParsed?.fileName || summarizeToolPreview(step.toolName, step.preview)
    : summarizeToolPreview(step.toolName, step.preview);

  useEffect(() => {
    if (defaultExpanded) setExpanded(true);
  }, [defaultExpanded]);

  const mutationBody = mutation ? renderMutationBody(step, expanded) : null;

  return (
    <div className={`live-step live-step-tool live-step-tool-${step.status}`}>
      <button
        type="button"
        className="live-tool-header live-tool-header-toggle"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
      >
        <span className={`live-tool-status live-tool-status-${step.status}`} />
        <span className="live-tool-name">{step.toolName}</span>
        {!expanded && summary ? <span className="live-tool-summary muted">{summary}</span> : null}
        {patchParsed && (patchParsed.addedLines > 0 || patchParsed.removedLines > 0) ? (
          <span className="live-patch-stats live-patch-stats-inline">
            {patchParsed.addedLines > 0 ? `+${patchParsed.addedLines}` : null}
            {patchParsed.removedLines > 0 ? ` -${patchParsed.removedLines}` : null}
          </span>
        ) : null}
        <span className="live-tool-state muted">{toolStatusLabel(step.status)}</span>
        <span className="live-tool-chevron" aria-hidden="true">
          {expanded ? "▾" : "▸"}
        </span>
      </button>
      {expanded ? (
        mutationBody ?? (
          <p className="live-tool-preview live-tool-preview-text muted">
            {step.preview.trim() || zh.chat.toolPreparing}
          </p>
        )
      ) : null}
    </div>
  );
}

function ToolGroupCard({
  toolName,
  steps,
  defaultExpanded,
}: {
  toolName: string;
  steps: ToolStep[];
  defaultExpanded: boolean;
}) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const active = shouldExpandToolGroup(steps);
  const isWriteGroup = toolName === "write_file";

  useEffect(() => {
    if (defaultExpanded || active) setExpanded(true);
  }, [defaultExpanded, active]);

  const firstSummary = summarizeToolPreview(toolName, steps[0]?.preview ?? "");
  const lastSummary = summarizeToolPreview(toolName, steps[steps.length - 1]?.preview ?? "");

  return (
    <div
      className={`live-step live-step-tool live-step-tool-group ${active ? "live-step-tool-active" : ""}`}
    >
      <button
        type="button"
        className="live-tool-header live-tool-header-toggle"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
      >
        <span className="live-tool-status live-tool-status-done" />
        <span className="live-tool-name">{toolName}</span>
        <span className="live-tool-count">×{steps.length}</span>
        {!expanded ? (
          <span className="live-tool-summary muted">
            {firstSummary === lastSummary || steps.length === 1
              ? firstSummary
              : `${firstSummary} … ${lastSummary}`}
          </span>
        ) : null}
        <span className="live-tool-state muted">{zh.chat.toolGroupCollapsed(steps.length)}</span>
        <span className="live-tool-chevron" aria-hidden="true">
          {expanded ? "▾" : "▸"}
        </span>
      </button>
      {expanded ? (
        <ul className="live-tool-group-list">
          {steps.map((step) =>
            isWriteGroup ? (
              <WriteFileRow key={step.id} preview={step.preview} status={step.status} />
            ) : (
              <li key={step.id} className={`live-tool-group-item live-tool-group-item-${step.status}`}>
                <span className={`live-tool-status live-tool-status-${step.status}`} />
                <span className="live-tool-group-text">
                  {summarizeToolPreview(step.toolName, step.preview, 120) || zh.chat.toolPreparing}
                </span>
              </li>
            ),
          )}
        </ul>
      ) : null}
    </div>
  );
}

export function LiveGenerationView({ steps }: LiveGenerationViewProps) {
  const displayItems = buildLiveDisplayItems(steps);
  const hasSteps = displayItems.length > 0;

  if (!hasSteps) {
    return (
      <div className="live-generation-empty">
        <span className="typing-dots">
          <span />
          <span />
          <span />
        </span>
        {zh.chat.thinking}
      </div>
    );
  }

  return (
    <div className="live-generation">
      {displayItems.map((item) => {
        if (item.kind === "status") {
          return (
            <div key={item.step.id} className="live-step live-step-status">
              <span className="typing-dots">
                <span />
                <span />
                <span />
              </span>
              <span>{item.step.label}</span>
            </div>
          );
        }

        if (item.kind === "text") {
          if (!item.step.content.trim()) return null;
          return (
            <div key={item.step.id} className="live-step live-step-text">
              <div className="message-body message-body-markdown streaming-body">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{item.step.content}</ReactMarkdown>
              </div>
            </div>
          );
        }

        if (item.kind === "tool-group") {
          return (
            <ToolGroupCard
              key={item.id}
              toolName={item.toolName}
              steps={item.steps}
              defaultExpanded={shouldExpandToolGroup(item.steps)}
            />
          );
        }

        return (
          <ToolStepCard
            key={item.step.id}
            step={item.step}
            defaultExpanded={shouldExpandSingleTool(item.step)}
          />
        );
      })}
    </div>
  );
}
