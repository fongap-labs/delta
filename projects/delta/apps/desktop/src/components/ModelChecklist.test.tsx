// Manual model ids are prefixed with the selected provider alias.
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ModelChecklist } from "./ModelChecklist";
import { I18nProvider } from "@delta/i18n/I18nContext";

vi.mock("../api", () => ({
  addModel: vi.fn(async (id: string) => ({ ok: true, models: [id], model: id })),
  removeModel: vi.fn(async () => ({ ok: true, models: [], model: "" })),
  setDefaultModel: vi.fn(async () => ({ ok: true })),
  getSettings: vi.fn(async () => ({ models: [], model: "" })),
}));

import { addModel } from "../api";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const KNOWN = ["openai", "anthropic", "openrouter"];

function renderList(provider: string) {
  return render(
    <I18nProvider locale="en-US">
      <ModelChecklist
        provider={provider}
        knownProviders={KNOWN}
        suggested={[]}
        curated={[]}
        defaultModel=""
        onChanged={() => {}}
      />
    </I18nProvider>,
  );
}

async function addTyped(id: string) {
  // The manual-add row is collapsed by default (spec: the manual model-add
  // form uses a collapsed state) —
  // open it, then type into the revealed Model ID field. Add() collapses the form
  // again asynchronously, so wait for the toggle to come back between adds.
  fireEvent.click(screen.getByTestId("mlist-add-toggle"));
  fireEvent.change(screen.getByPlaceholderText("Model ID"), {
    target: { value: id },
  });
  fireEvent.click(screen.getByText("Add"));
  await waitFor(() =>
    expect(screen.queryByTestId("mlist-add-form")).toBeNull(),
  );
}

describe("ModelChecklist manual model ids", () => {

  it("keeps manual model entry labelled and its primary action disabled until valid", () => {
    renderList("openrouter");
    const toggle = screen.getByTestId("mlist-add-toggle");
    expect(toggle.className).toContain("ds-button-ghost");
    expect(toggle.getAttribute("type")).toBe("button");
    fireEvent.click(toggle);
    const input = screen.getByRole("textbox", { name: "Model ID" }) as HTMLInputElement;
    const add = screen.getByRole("button", { name: "Add" }) as HTMLButtonElement;
    expect(input.className).toContain("ds-field");
    expect(input.autocomplete).toBe("off");
    expect(add.className).toContain("ds-button-primary");
    expect(add.disabled).toBe(true);
    fireEvent.change(input, { target: { value: "  " } });
    expect(add.disabled).toBe(true);
    fireEvent.change(input, { target: { value: "z-ai/glm-5.2" } });
    expect(add.disabled).toBe(false);
    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByTestId("mlist-add-form")).toBeNull();
    expect(addModel).not.toHaveBeenCalled();
  });

  it("preserves a stable accessible refresh name when the list is busy", () => {
    const refresh = vi.fn();
    render(
      <I18nProvider locale="en-US">
        <ModelChecklist
          provider="openrouter"
          knownProviders={KNOWN}
          suggested={[]}
          curated={[]}
          defaultModel=""
          onChanged={() => {}}
          onRefresh={refresh}
          refreshing
        />
      </I18nProvider>,
    );
    const button = screen.getByRole("button", { name: "Fetch models" }) as HTMLButtonElement;
    expect(button.className).toContain("ds-button-sm");
    expect(button.getAttribute("aria-busy")).toBe("true");
    expect(button.disabled).toBe(true);
    fireEvent.click(button);
    expect(refresh).not.toHaveBeenCalled();
  });

  it("distinguishes each default-model radio by its model ID", () => {
    const curated = ["openrouter:z-ai/glm-5.2", "openrouter:qwen3"];
    render(
      <I18nProvider locale="en-US">
        <ModelChecklist
          provider="openrouter"
          knownProviders={KNOWN}
          suggested={[]}
          curated={curated}
          defaultModel={curated[0]}
          onChanged={() => {}}
        />
      </I18nProvider>,
    );
    const first = screen.getByRole("radio", { name: /z-ai\/glm-5\.2/ }) as HTMLInputElement;
    const second = screen.getByRole("radio", { name: /qwen3/ }) as HTMLInputElement;
    expect(first.checked).toBe(true);
    expect(second.checked).toBe(false);
  });

  it("prefixes the model with the provider alias", async () => {
    renderList("openrouter");
    expect(screen.queryByTestId("mlist-family")).toBeNull();
    await addTyped("z-ai/glm-5.2");
    expect(addModel).toHaveBeenCalledWith("openrouter:z-ai/glm-5.2");
  });
});
