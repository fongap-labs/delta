import { Icon } from "./Icon";

// A slim tab on the edge of a side panel, the way PDF-XChange Editor marks its panes: the chevron
// points where the panel will go, so the same spot collapses it when open and brings it back when
// closed. `side` is the side of the window the panel lives on.
export function EdgeTab({
  side,
  isExpanded,
  label,
  onClick,
  onMouseEnter,
  testId,
}: {
  side: "left" | "right";
  isExpanded: boolean;
  label: string;
  onClick: () => void;
  onMouseEnter?: () => void;
  testId: string;
}) {
  // A left panel collapses toward the left (chevron "‹") and a right panel toward the right ("›").
  const pointsRight = side === "left" ? !isExpanded : isExpanded;
  return (
    <button
      type="button"
      className={"edge-tab edge-tab-" + side + (isExpanded ? " is-expanded" : "")}
      aria-label={label}
      aria-expanded={isExpanded}
      title={label}
      data-testid={testId}
      onClick={onClick}
      onMouseEnter={onMouseEnter}
    >
      <Icon name="chevronRight" size={12} className={pointsRight ? "" : "edge-tab-flip"} />
    </button>
  );
}
