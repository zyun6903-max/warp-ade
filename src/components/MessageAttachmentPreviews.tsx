import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { ParsedAttachmentMeta } from "../utils/attachments";

type MessageAttachmentPreviewsProps = {
  attachments: ParsedAttachmentMeta[];
};

export function MessageAttachmentPreviews({ attachments }: MessageAttachmentPreviewsProps) {
  const [urls, setUrls] = useState<Record<string, string>>({});

  useEffect(() => {
    let cancelled = false;
    const imageAttachments = attachments.filter((a) => a.kind === "image");
    if (imageAttachments.length === 0) return;

    void (async () => {
      const next: Record<string, string> = {};
      for (const item of imageAttachments) {
        try {
          const dataUrl = await invoke<string | null>("get_attachment_data_url", {
            path: item.path,
          });
          if (dataUrl) next[item.path] = dataUrl;
        } catch {
          // ignore missing preview
        }
      }
      if (!cancelled) setUrls(next);
    })();

    return () => {
      cancelled = true;
    };
  }, [attachments]);

  if (attachments.length === 0) return null;

  return (
    <ul className="message-attachments">
      {attachments.map((item) => {
        const previewUrl = urls[item.path];
        if (item.kind === "image" && previewUrl) {
          return (
            <li key={item.path} className="message-attachment-item">
              <div className="message-attachment-thumb" title={item.fileName}>
                <img src={previewUrl} alt={item.fileName} />
              </div>
            </li>
          );
        }
        return (
          <li key={item.path} className="message-attachment-item">
            <span className="message-attachment-file" title={item.path}>
              📄 {item.fileName}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
