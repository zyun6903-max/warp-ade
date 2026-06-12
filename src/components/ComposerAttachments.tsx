import { isImageAttachment } from "../utils/attachments";
import type { ChatAttachment } from "../types";

type ComposerAttachmentsProps = {
  attachments: ChatAttachment[];
  previews: Record<string, string>;
  onRemove: (id: string) => void;
};

export function ComposerAttachments({
  attachments,
  previews,
  onRemove,
}: ComposerAttachmentsProps) {
  if (attachments.length === 0) return null;

  return (
    <ul className="composer-attachments">
      {attachments.map((item) => {
        const previewUrl = previews[item.id];
        const isImage = isImageAttachment(item);

        if (isImage && previewUrl) {
          return (
            <li key={item.id} className="composer-attachment-item">
              <div className="composer-attachment-thumb" title={item.fileName}>
                <img src={previewUrl} alt={item.fileName} />
                <button
                  type="button"
                  className="composer-attachment-remove"
                  onClick={() => onRemove(item.id)}
                  aria-label="移除附件"
                >
                  ×
                </button>
              </div>
            </li>
          );
        }

        return (
          <li key={item.id} className="composer-attachment-item">
            <div className="composer-attachment-file" title={item.fileName}>
              <span className="composer-attachment-file-icon" aria-hidden="true">
                {isImage ? "🖼" : "📄"}
              </span>
              <span className="composer-attachment-file-name">{item.fileName}</span>
              <button
                type="button"
                className="composer-attachment-remove composer-attachment-remove-inline"
                onClick={() => onRemove(item.id)}
                aria-label="移除附件"
              >
                ×
              </button>
            </div>
          </li>
        );
      })}
    </ul>
  );
}
