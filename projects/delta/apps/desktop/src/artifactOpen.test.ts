import { beforeEach, describe, expect, it, vi } from "vitest";

let reply: Record<string, unknown> = { ok: true };
const openArtifact = vi.fn(async (_session: string, _path: string, _mode: string) => reply);

vi.mock("./runtimeTransport", () => ({
  openArtifact: (session: string, path: string, mode: string) => openArtifact(session, path, mode),
}));
vi.mock("pdfjs-dist/build/pdf.worker.min.mjs?url", () => ({ default: "pdf.worker.js" }));

import { revealArtifact } from "./api";

// What may be launched is decided in Rust (control_plane::can_open_externally); the front end only
// names a session and a registered artifact path, never a file the shell will run.
describe("revealArtifact", () => {
  beforeEach(() => {
    openArtifact.mockClear();
    reply = { ok: true };
  });

  it("asks the runtime to open the artifact by session and registered path", async () => {
    await expect(revealArtifact("s", "out/a.pdf", "open")).resolves.toEqual({ ok: true });
    expect(openArtifact).toHaveBeenCalledWith("s", "out/a.pdf", "open");
  });

  it("reveals by default", async () => {
    await revealArtifact("s", "out/a.pdf");
    expect(openArtifact).toHaveBeenCalledWith("s", "out/a.pdf", "reveal");
  });

  it("passes on that a file was only revealed", async () => {
    reply = { ok: true, downgraded: true };
    await expect(revealArtifact("s", "x.bat", "open")).resolves.toEqual({ ok: true, downgraded: true });
  });

  it("reports why an artifact is unavailable", async () => {
    reply = { ok: false, error: "artifact is not registered for this session" };
    await expect(revealArtifact("s", "../x.pdf", "open")).resolves.toEqual({
      ok: false,
      error: "artifact is not registered for this session",
    });
  });
});
