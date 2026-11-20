import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { AddFolderForm } from "./AddFolderForm";
import { chooseFolder } from "../tauri";

vi.mock("../tauri", () => ({ chooseFolder: vi.fn() }));

const onAdd = vi.fn();
const onDismiss = vi.fn();
const renderForm = (props: { busy?: boolean; startOpen?: boolean } = {}) =>
  render(
    <I18nProvider locale="en-US">
      <AddFolderForm onAdd={onAdd} onDismiss={onDismiss} {...props} />
    </I18nProvider>,
  );

beforeEach(() => {
  vi.clearAllMocks();
  onAdd.mockResolvedValue(true);
  vi.mocked(chooseFolder).mockResolvedValue(null);
});
afterEach(cleanup);

describe("folder access form shared controls", () => {
  it("starts collapsed and grants read-only folder access only after an explicit submission", async () => {
    renderForm();
    const trigger = screen.getByRole("button", { name: "Give access to a folder" });
    expect(trigger.className).toContain("ds-button-ghost");
    expect(onAdd).not.toHaveBeenCalled();
    fireEvent.click(trigger);
    const form = screen.getByTestId("folder-access-form");
    const path = screen.getByRole("textbox", { name: "Choose or paste a folder path…" }) as HTMLInputElement;
    const add = screen.getByRole("button", { name: "Add" }) as HTMLButtonElement;
    expect(path.className).toContain("ds-field");
    expect(add.className).toContain("ds-button-primary");
    expect(add.disabled).toBe(true);
    expect(add.getAttribute("type")).toBe("submit");
    const writable = screen.getByRole("checkbox", { name: "Allow writes" }) as HTMLInputElement;
    expect(writable.checked).toBe(false);
    fireEvent.change(path, { target: { value: "  /work/research  " } });
    expect(onAdd).not.toHaveBeenCalled();
    fireEvent.submit(form);
    await waitFor(() => expect(onAdd).toHaveBeenCalledWith("/work/research", false));
    expect(onDismiss).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Give access to a folder" })).toBeTruthy();
  });

  it("keeps write access opt-in and preserves the selection when adding is declined", async () => {
    onAdd.mockResolvedValue(false);
    renderForm({ startOpen: true });
    const path = screen.getByRole("textbox", { name: "Choose or paste a folder path…" });
    const writable = screen.getByRole("checkbox", { name: "Allow writes" }) as HTMLInputElement;
    fireEvent.change(path, { target: { value: "/work/notes" } });
    fireEvent.click(writable);
    expect(writable.checked).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(onAdd).toHaveBeenCalledWith("/work/notes", true));
    expect(onDismiss).not.toHaveBeenCalled();
    expect((screen.getByRole("checkbox", { name: "Allow writes" }) as HTMLInputElement).checked).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("respects busy state for both mouse and native form submissions", () => {
    renderForm({ busy: true, startOpen: true });
    const path = screen.getByRole("textbox", { name: "Choose or paste a folder path…" });
    fireEvent.change(path, { target: { value: "/work/docs" } });
    const add = screen.getByRole("button", { name: "Add" }) as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    expect(add.getAttribute("aria-busy")).toBe("true");
    fireEvent.submit(screen.getByTestId("folder-access-form"));
    expect(onAdd).not.toHaveBeenCalled();
  });

  it("does not change path on canceled browsing and uses the selected folder when provided", async () => {
    vi.mocked(chooseFolder).mockResolvedValueOnce(null).mockResolvedValueOnce("/work/picked");
    renderForm({ startOpen: true });
    const path = screen.getByRole("textbox", { name: "Choose or paste a folder path…" }) as HTMLInputElement;
    const browse = screen.getByRole("button", { name: "Choose location" });
    expect(browse.className).toContain("ds-icon-button");
    fireEvent.click(browse);
    await waitFor(() => expect(chooseFolder).toHaveBeenCalledTimes(1));
    expect(path.value).toBe("");
    fireEvent.click(browse);
    await waitFor(() => expect(path.value).toBe("/work/picked"));
    expect(onAdd).not.toHaveBeenCalled();
  });
});
