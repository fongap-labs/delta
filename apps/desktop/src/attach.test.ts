import { afterEach, describe, expect, it, vi } from "vitest";
import {
  MAX_IMAGE_FILE_BYTES,
  MAX_OFFICE_FILE_BYTES,
  OFFICE_FILE_ACCEPT,
  TEXT_FILE_ACCEPT,
  attachmentFileKind,
  mergeAttachments,
  readFile,
  validateAttachmentSet,
} from "./attach";
import type { Attachment } from "./types";

afterEach(() => vi.unstubAllGlobals());

const OOXML = {
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  xlsx: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  pptx: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
};

describe("Office attachments", () => {
  it("accepts Word, Excel and PowerPoint files by extension or OOXML type", () => {
    for (const [ext, type] of Object.entries(OOXML)) {
      expect(attachmentFileKind(new File(["x"], `report.${ext}`))).toBe("office");
      expect(attachmentFileKind(new File(["x"], "report.bin", { type }))).toBe("office");
      expect(OFFICE_FILE_ACCEPT).toContain(`.${ext}`);
    }
    expect(attachmentFileKind(new File(["x"], "legacy.doc"))).toBeNull();
    expect(attachmentFileKind(new File(["x"], "legacy.xls"))).toBeNull();
    expect(attachmentFileKind(new File(["x"], "archive.zip"))).toBeNull();
  });

  it("reads an Office file as a data URL with the OOXML MIME type, even when the browser reports none", async () => {
    const result = await readFile(new File(["PK-bytes"], "report.docx", { type: "" }));
    expect(result).toMatchObject({ ok: true, attachment: { kind: "office", name: "report.docx", mime: OOXML.docx } });
    if (result.ok) {
      expect(result.attachment.data_url?.startsWith(`data:${OOXML.docx};base64,`)).toBe(true);
    }
  });

  it("rejects an Office file over the 10 MB limit with the explicit code", async () => {
    await expect(
      readFile(new File([new Uint8Array(MAX_OFFICE_FILE_BYTES + 1)], "big.xlsx", { type: OOXML.xlsx })),
    ).resolves.toMatchObject({ ok: false, code: "file_too_large", name: "big.xlsx", maxBytes: MAX_OFFICE_FILE_BYTES });
  });

  it("rejects an Office attachment whose data URL is not an OOXML payload", () => {
    const bad: Attachment = { kind: "office", name: "x.docx", data_url: "data:application/octet-stream;base64,AAAA" };
    expect(validateAttachmentSet([bad])).toMatchObject({ ok: false, code: "invalid_payload" });
  });
});

describe("attachment policy", () => {
  it("keeps picker extensions and validation in one source", () => {
    for (const name of ["notes.markdown", "table.tsv", "config.ini", "Main.java", "a.cpp", "run.sh", "page.html", "query.sql", "doc.xml"]) {
      expect(attachmentFileKind(new File(["x"], name))).toBe("text");
      expect(TEXT_FILE_ACCEPT).toContain(`.${name.split(".").pop()}`);
    }
    expect(attachmentFileKind(new File(["x"], "archive.zip", { type: "application/zip" }))).toBeNull();
  });

  it("rejects unknown binary files and oversized images with explicit codes", async () => {
    await expect(readFile(new File(["x"], "archive.zip", { type: "application/zip" }))).resolves.toMatchObject({
      ok: false,
      code: "unsupported",
      name: "archive.zip",
    });
    await expect(
      readFile(new File([new Uint8Array(MAX_IMAGE_FILE_BYTES + 1)], "large.png", { type: "image/png" })),
    ).resolves.toMatchObject({ ok: false, code: "file_too_large", name: "large.png" });
  });

  it("reports the ninth attachment instead of silently truncating it", () => {
    const current: Attachment[] = Array.from({ length: 8 }, (_, index) => ({
      kind: "text",
      name: `${index}.txt`,
      text: "ok",
    }));
    const merged = mergeAttachments(current, [{ kind: "text", name: "ninth.txt", text: "no" }]);
    expect(merged.attachments).toHaveLength(8);
    expect(merged.rejected).toEqual([
      expect.objectContaining({ code: "too_many", name: "ninth.txt" }),
    ]);
  });

  it("rejects a total encoded payload over the backend message cap", () => {
    const attachments: Attachment[] = ["a", "b"].map((name) => ({
      kind: "image",
      name: `${name}.png`,
      data_url: `data:image/png;base64,${"A".repeat(7_600_000)}`,
    }));
    expect(validateAttachmentSet(attachments)).toMatchObject({ code: "total_too_large" });
  });

  it("rejects empty text and empty data URLs before they become visible chips", () => {
    expect(validateAttachmentSet([{ kind: "text", name: "empty.txt", text: "" }])).toMatchObject({
      code: "invalid_payload",
      name: "empty.txt",
    });
    expect(
      validateAttachmentSet([{ kind: "image", name: "empty.png", data_url: "data:image/png;base64," }]),
    ).toMatchObject({ code: "invalid_payload", name: "empty.png" });
    expect(
      validateAttachmentSet([{ kind: "pdf", name: "empty.pdf", data_url: "data:application/pdf;base64," }]),
    ).toMatchObject({ code: "invalid_payload", name: "empty.pdf" });
  });

  it("surfaces FileReader failures", async () => {
    class BrokenReader {
      result: string | ArrayBuffer | null = null;
      onerror: (() => void) | null = null;
      onload: (() => void) | null = null;
      readAsText() {
        this.onerror?.();
      }
      readAsDataURL() {
        this.onerror?.();
      }
    }
    vi.stubGlobal("FileReader", BrokenReader);
    await expect(readFile(new File(["x"], "notes.txt", { type: "text/plain" }))).resolves.toMatchObject({
      ok: false,
      code: "read_failed",
      name: "notes.txt",
    });
  });
});
