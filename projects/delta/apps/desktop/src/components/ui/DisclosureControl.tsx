import type { ButtonHTMLAttributes, ReactNode } from "react";

interface DisclosureTriggerProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-expanded" | "aria-controls" | "type"> {
  isOpen: boolean;
  contentId: string;
}

// Disclosure state remains with callers so error auto-collapse and tool timing stay unchanged.
export function DisclosureTrigger({ isOpen, contentId, className = "", ...props }: DisclosureTriggerProps) {
  return (
    <button
      {...props}
      type="button"
      className={`ds-transcript-disclosure-trigger ${className}`.trim()}
      aria-expanded={isOpen}
      aria-controls={contentId}
    />
  );
}

export function DisclosureContent({
  isOpen, id, label, children, className = "", testId, as = "div",
}: {
  isOpen: boolean;
  id: string;
  label: string;
  children: ReactNode;
  className?: string;
  testId?: string;
  as?: "div" | "pre";
}) {
  if (!isOpen) return null;
  const attributes = {
    id, role: "region" as const, "aria-label": label,
    className: `ds-transcript-disclosure-content ${className}`.trim(),
    "data-testid": testId,
  };
  return as === "pre" ? <pre {...attributes}>{children}</pre> : <div {...attributes}>{children}</div>;
}
