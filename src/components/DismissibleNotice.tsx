import type { ReactNode } from "react";
import { zh } from "../i18n/zh";

type NoticeVariant = "error" | "info" | "banner" | "plan" | "success";

type DismissibleNoticeProps = {
  variant?: NoticeVariant;
  className?: string;
  onDismiss: () => void;
  children: ReactNode;
};

function variantClass(variant: NoticeVariant): string {
  switch (variant) {
    case "error":
      return "error-toast";
    case "plan":
      return "plan-mode-banner";
    case "banner":
      return "info-banner";
    case "success":
      return "form-message";
    default:
      return "info-toast";
  }
}

export function DismissibleNotice({
  variant = "info",
  className = "",
  onDismiss,
  children,
}: DismissibleNoticeProps) {
  return (
    <div className={`dismissible-notice ${variantClass(variant)} ${className}`.trim()}>
      <div className="dismissible-notice-body">{children}</div>
      <button
        type="button"
        className="dismissible-notice-close"
        onClick={onDismiss}
        aria-label={zh.common.dismiss}
      >
        ×
      </button>
    </div>
  );
}
