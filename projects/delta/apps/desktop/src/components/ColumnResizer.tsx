// Drag handle on a column's inner edge. The parent owns the width (it is applied as a CSS
// variable on .app), so the handle only reports the width the user asks for, clamped to the
// column's minimum and the caller's current maximum. The maximum is derived from the centre
// column's minimum, so no side column can be dragged until the chat is squeezed out.
import { useRef, type KeyboardEvent, type PointerEvent } from "react";

export const SIDEBAR_MIN_WIDTH = 256;
export const RAIL_MIN_WIDTH = 280;
export const CENTER_MIN_WIDTH = 360;

const KEY_STEP = 16;

export function ColumnResizer({
  edge,
  label,
  minWidth,
  maxWidth,
  onResize,
}: {
  // The parent edge the handle sits on: "right" for the left sidebar, "left" for the right rail.
  edge: "left" | "right";
  label: string;
  minWidth: number;
  maxWidth: () => number;
  onResize: (width: number) => void;
}) {
  const handle = useRef<HTMLDivElement>(null);
  // The column's width and the pointer position when a drag began.
  const drag = useRef<{ startX: number; startWidth: number } | null>(null);
  // Moving the pointer away from the centre widens a left sidebar and a right rail alike only
  // in opposite directions, so the sign depends on which edge the handle is on.
  const sign = edge === "right" ? 1 : -1;

  const clamp = (width: number) =>
    Math.round(Math.min(Math.max(width, minWidth), Math.max(minWidth, maxWidth())));
  const currentWidth = () => handle.current?.parentElement?.getBoundingClientRect().width ?? minWidth;

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    drag.current = { startX: event.clientX, startWidth: currentWidth() };
    event.currentTarget.setPointerCapture(event.pointerId);
    document.body.style.cursor = "col-resize";
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const start = drag.current;
    if (!start) return;
    onResize(clamp(start.startWidth + sign * (event.clientX - start.startX)));
  };
  const endDrag = () => {
    drag.current = null;
    document.body.style.cursor = "";
  };
  // Arrow keys nudge the edge by one step, so the width is adjustable without a pointer.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const direction = event.key === "ArrowRight" ? 1 : -1;
    onResize(clamp(currentWidth() + sign * direction * KEY_STEP));
  };

  return (
    <div
      ref={handle}
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      tabIndex={0}
      className={"column-resizer edge-" + edge}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onKeyDown={onKeyDown}
    />
  );
}
