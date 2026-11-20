import { useId, type ReactNode } from "react";
import { Icon } from "../Icon";

export function DisclosureSection({
  title,
  isOpen,
  onToggle,
  action,
  children,
}: {
  title: string;
  isOpen: boolean;
  onToggle: () => void;
  action?: ReactNode;
  children: ReactNode;
}) {
  const contentId = useId();
  return (
    <section className="rail-section ds-disclosure" data-state={isOpen ? "open" : "closed"}>
      <div className="rail-section-head">
        <button
          type="button"
          className="rail-section-toggle ds-disclosure-trigger"
          aria-expanded={isOpen}
          aria-controls={isOpen ? contentId : undefined}
          onClick={onToggle}
        >
          <Icon name={isOpen ? "chevronDown" : "chevronRight"} size={14} className="rail-chev" />
          <span>{title}</span>
        </button>
        {action}
      </div>
      {isOpen && (
        <div id={contentId} role="region" aria-label={title} className="rail-section-body">
          {children}
        </div>
      )}
    </section>
  );
}
