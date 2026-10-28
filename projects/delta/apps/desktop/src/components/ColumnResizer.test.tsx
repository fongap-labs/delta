import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ColumnResizer } from "./ColumnResizer";

// The handle measures its parent column, so the test gives the parent a fixed width.
function renderResizer(opts: { edge: "left" | "right"; width: number; min?: number; max?: number; onResize: (w: number) => void }) {
  const { container } = render(
    <div>
      <ColumnResizer
        edge={opts.edge}
        label="Resize column"
        minWidth={opts.min ?? 100}
        maxWidth={() => opts.max ?? 1000}
        onResize={opts.onResize}
      />
    </div>,
  );
  const column = container.firstElementChild as HTMLElement;
  column.getBoundingClientRect = () => ({ width: opts.width }) as DOMRect;
  const handle = screen.getByRole("separator", { name: "Resize column" });
  // jsdom has no pointer capture; the drag only needs the call to exist.
  handle.setPointerCapture = vi.fn();
  return handle;
}

describe("ColumnResizer", () => {
  afterEach(cleanup);

  it("widens a left sidebar when the arrow key points toward the centre", () => {
    const onResize = vi.fn();
    const handle = renderResizer({ edge: "right", width: 300, onResize });
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(onResize).toHaveBeenLastCalledWith(316);
    fireEvent.keyDown(handle, { key: "ArrowLeft" });
    expect(onResize).toHaveBeenLastCalledWith(284);
  });

  it("widens a right rail when the arrow key points toward the centre", () => {
    const onResize = vi.fn();
    const handle = renderResizer({ edge: "left", width: 300, onResize });
    fireEvent.keyDown(handle, { key: "ArrowLeft" });
    expect(onResize).toHaveBeenLastCalledWith(316);
  });

  it("clamps to the minimum and the current maximum", () => {
    const onResize = vi.fn();
    renderResizer({ edge: "right", width: 300, min: 290, max: 310, onResize });
    const handle = screen.getByRole("separator");
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(onResize).toHaveBeenLastCalledWith(310);
    fireEvent.keyDown(handle, { key: "ArrowLeft" });
    expect(onResize).toHaveBeenLastCalledWith(290);
  });

  it("follows the pointer from where the drag began", () => {
    const onResize = vi.fn();
    const handle = renderResizer({ edge: "left", width: 300, onResize });
    fireEvent.pointerDown(handle, { button: 0, clientX: 500, pointerId: 1 });
    fireEvent.pointerMove(handle, { clientX: 460, pointerId: 1 });
    // The rail's edge sits on its left, so dragging left by 40px widens it by 40px.
    expect(onResize).toHaveBeenLastCalledWith(340);
    fireEvent.pointerUp(handle, { pointerId: 1 });
    fireEvent.pointerMove(handle, { clientX: 400, pointerId: 1 });
    expect(onResize).toHaveBeenCalledTimes(1);
  });

  it("ignores keys it does not use", () => {
    const onResize = vi.fn();
    const handle = renderResizer({ edge: "right", width: 300, onResize });
    fireEvent.keyDown(handle, { key: "Enter" });
    expect(onResize).not.toHaveBeenCalled();
  });
});
