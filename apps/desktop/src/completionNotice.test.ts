import { describe, expect, it, vi } from "vitest";
import { buildCompletionNotice, plainSummary, showCompletionNotice } from "./completionNotice";

const t = (key: string, vars?: Record<string, string | number>) =>
  ({
    "notify.taskDone": "Your task is done.",
    "notify.files": "Files: {names}",
    "notify.filesMore": "Files: {names} and {more} more",
  })[key]!.replace(/\{(\w+)\}/g, (_, name) => String(vars?.[name]));

describe("plainSummary", () => {
  it("drops Markdown markers, link addresses and code, and collapses whitespace", () => {
    expect(plainSummary("## Done\n\n**The report** is [ready](artifact:out/q3.docx).\n\n```py\nprint(1)\n```\n- next step")).toBe(
      "Done The report is ready. next step",
    );
  });

  it("cuts on a character boundary and marks the cut", () => {
    expect(plainSummary("完成了报告和图表的制作", 4)).toBe("完成了报…");
    expect(plainSummary("short", 40)).toBe("short");
    expect(plainSummary("")).toBe("");
  });
});

describe("buildCompletionNotice", () => {
  it("says the task is done when there is no answer text, and offers no file", () => {
    expect(buildCompletionNotice({ answer: "", files: [], t })).toEqual({ title: "Delta", body: "Your task is done." });
  });

  it("names the files and opens the first one", () => {
    const notice = buildCompletionNotice({
      answer: "The report is ready.",
      files: [
        { path: "out/q3.docx", name: "q3.docx" },
        { path: "out/q3.xlsx", name: "q3.xlsx" },
      ],
      t,
    });
    expect(notice.body).toBe("The report is ready.\nFiles: q3.docx, q3.xlsx");
    expect(notice.openPath).toBe("out/q3.docx");
  });

  it("lists three files and counts the rest", () => {
    const files = [1, 2, 3, 4, 5].map((n) => ({ path: `f${n}.md`, name: `f${n}.md` }));
    expect(buildCompletionNotice({ answer: "ok", files, t }).body).toBe("ok\nFiles: f1.md, f2.md, f3.md and 2 more");
  });
});

describe("showCompletionNotice", () => {
  function fakeNotification(permission: NotificationPermission) {
    const shown: { title: string; options: NotificationOptions; onclick?: () => void }[] = [];
    class Fake {
      static permission = permission;
      static requestPermission = vi.fn(async () => "granted" as NotificationPermission);
      onclick?: () => void;
      constructor(title: string, options: NotificationOptions) {
        shown.push(Object.assign(this, { title, options }));
      }
    }
    return { Fake: Fake as unknown as typeof Notification, shown };
  }

  it("a click focuses the window and opens the file", () => {
    const { Fake, shown } = fakeNotification("granted");
    const onOpen = vi.fn();
    const focus = vi.fn();
    showCompletionNotice({ title: "Delta", body: "x", openPath: "out/q3.docx" }, onOpen, Fake, focus);
    expect(shown).toHaveLength(1);
    expect(shown[0].options.body).toBe("x");
    shown[0].onclick!();
    expect(focus).toHaveBeenCalled();
    expect(onOpen).toHaveBeenCalledWith("out/q3.docx");
  });

  it("a click on a notice with no file only brings the window forward", () => {
    const { Fake, shown } = fakeNotification("granted");
    const onOpen = vi.fn();
    showCompletionNotice({ title: "Delta", body: "x" }, onOpen, Fake, vi.fn());
    shown[0].onclick!();
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("asks for permission once and shows after it is granted; shows nothing when denied", async () => {
    const asked = fakeNotification("default");
    showCompletionNotice({ title: "Delta", body: "x" }, vi.fn(), asked.Fake, vi.fn());
    await Promise.resolve();
    await Promise.resolve();
    expect(asked.shown).toHaveLength(1);
    const denied = fakeNotification("denied");
    showCompletionNotice({ title: "Delta", body: "x" }, vi.fn(), denied.Fake, vi.fn());
    expect(denied.shown).toHaveLength(0);
    expect(() => showCompletionNotice({ title: "Delta", body: "x" }, vi.fn(), undefined, vi.fn())).not.toThrow();
  });
});
