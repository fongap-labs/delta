import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createRef } from "react";
import { Button } from "./Button";
import { FieldInput } from "./FieldInput";
import { IconButton } from "./IconButton";

afterEach(cleanup);

describe("shared desktop UI control contracts", () => {
  it("keeps every button variant native, safe by default and keyboard accessible", () => {
    const onClick = vi.fn();
    const ref = createRef<HTMLButtonElement>();
    const { rerender } = render(
      <Button ref={ref} onClick={onClick} aria-label="Default action">Run</Button>,
    );
    const button = screen.getByRole("button", { name: "Default action" });
    expect(button).toBe(ref.current);
    expect(button.getAttribute("type")).toBe("button");
    expect(button.className).toContain("ds-button");
    expect(button.className).not.toContain("ds-button-primary");
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);

    for (const [variant, token] of [
      ["primary", "ds-button-primary"],
      ["ghost", "ds-button-ghost"],
      ["danger", "ds-button-danger"],
    ] as const) {
      rerender(<Button variant={variant} size="sm" aria-label="Default action">Run</Button>);
      expect(button.className).toContain(token);
      expect(button.className).toContain("ds-button-sm");
      expect(button.getAttribute("type")).toBe("button");
    }
    rerender(<Button type="submit" disabled aria-label="Default action">Run</Button>);
    expect(button.getAttribute("type")).toBe("submit");
    expect(button.hasAttribute("disabled")).toBe(true);
  });

  it("keeps icon-only actions named and non-submitting inside forms", () => {
    const onClick = vi.fn();
    render(
      <form>
        <IconButton icon="trash" label="Remove workspace" onClick={onClick} />
      </form>,
    );
    const button = screen.getByRole("button", { name: "Remove workspace" });
    expect(button.className).toContain("ds-icon-button");
    expect(button.getAttribute("type")).toBe("button");
    expect(button.getAttribute("title")).toBe("Remove workspace");
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("preserves native field constraints, accessible labels and input callbacks", () => {
    const onChange = vi.fn();
    const ref = createRef<HTMLInputElement>();
    render(
      <label>
        Visible sessions
        <FieldInput
          ref={ref}
          type="number"
          min={1}
          max={50}
          value={5}
          onChange={onChange}
          className="w-16"
        />
      </label>,
    );
    const field = screen.getByRole("spinbutton", { name: "Visible sessions" });
    expect(field).toBe(ref.current);
    expect(field.className).toContain("ds-field");
    expect(field.className).toContain("w-16");
    expect(field.getAttribute("min")).toBe("1");
    expect(field.getAttribute("max")).toBe("50");
    fireEvent.change(field, { target: { value: "10" } });
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});
