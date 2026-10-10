import { describe, expect, it } from "vitest";
import { dateBucket, groupByDate, parseProjectStore, projectOps, type ProjectStore } from "./sessionGroups";

// A fixed "now": Wednesday 2026-10-14 15:00 local.
const NOW = new Date(2026, 9, 14, 15, 0, 0).getTime();

describe("dateBucket", () => {
  it("buckets by local calendar day, not by 24h windows", () => {
    expect(dateBucket("2026-10-14 00:05:00", NOW)).toBe("today");
    expect(dateBucket("2026-10-13 23:59:00", NOW)).toBe("yesterday");
    expect(dateBucket("2026-10-08 12:00:00", NOW)).toBe("last7");
    expect(dateBucket("2026-10-07 12:00:00", NOW)).toBe("last7");
    expect(dateBucket("2026-10-06 23:59:00", NOW)).toBe("earlier");
  });
  it("accepts ISO stamps and treats missing / invalid ones as earlier", () => {
    expect(dateBucket("2026-10-14T09:00:00", NOW)).toBe("today");
    expect(dateBucket(null, NOW)).toBe("earlier");
    expect(dateBucket("not a date", NOW)).toBe("earlier");
  });
});

describe("groupByDate", () => {
  it("keeps the incoming order inside a bucket and drops empty buckets", () => {
    const items = [
      { id: "a", updated_at: "2026-10-14 10:00:00" },
      { id: "b", updated_at: "2026-10-14 09:00:00" },
      { id: "c", updated_at: "2026-09-01 09:00:00" },
    ];
    const groups = groupByDate(items, NOW);
    expect(groups.map((g) => g.bucket)).toEqual(["today", "earlier"]);
    expect(groups[0].items.map((i) => i.id)).toEqual(["a", "b"]);
  });
});

describe("projectOps", () => {
  const base: ProjectStore = { projects: [], assign: {} };

  it("creates, trims and ignores empty names", () => {
    const s1 = projectOps.create(base, "p1", "  Q3 planning ");
    expect(s1.projects).toEqual([{ id: "p1", name: "Q3 planning" }]);
    expect(projectOps.create(s1, "p2", "   ")).toBe(s1);
  });

  it("moves a session into one project at a time, and out again", () => {
    let s = projectOps.create(projectOps.create(base, "p1", "A"), "p2", "B");
    s = projectOps.move(s, "s1", "p1");
    s = projectOps.move(s, "s1", "p2");
    expect(s.assign).toEqual({ s1: "p2" });
    s = projectOps.move(s, "s1", null);
    expect(s.assign).toEqual({});
    // Moving into a project that does not exist ungroups instead of dangling.
    expect(projectOps.move(s, "s1", "ghost").assign).toEqual({});
  });

  it("removing a project ungroups its sessions and keeps the others", () => {
    let s = projectOps.create(projectOps.create(base, "p1", "A"), "p2", "B");
    s = projectOps.move(projectOps.move(s, "s1", "p1"), "s2", "p2");
    s = projectOps.remove(s, "p1");
    expect(s.projects.map((p) => p.id)).toEqual(["p2"]);
    expect(s.assign).toEqual({ s2: "p2" });
  });

  it("renames and toggles collapse", () => {
    let s = projectOps.create(base, "p1", "A");
    s = projectOps.rename(s, "p1", "Alpha");
    s = projectOps.toggle(s, "p1");
    expect(s.projects[0]).toEqual({ id: "p1", name: "Alpha", collapsed: true });
    expect(projectOps.rename(s, "p1", " ")).toBe(s);
  });
});

describe("parseProjectStore", () => {
  it("survives garbage and drops assignments to unknown projects", () => {
    expect(parseProjectStore("{oops")).toEqual({ projects: [], assign: {} });
    expect(parseProjectStore(null)).toEqual({ projects: [], assign: {} });
    const raw = JSON.stringify({
      projects: [{ id: "p1", name: "A" }, { id: 7, name: "bad" }, { id: "p3", name: " " }],
      assign: { s1: "p1", s2: "p9", s3: 4 },
    });
    expect(parseProjectStore(raw)).toEqual({ projects: [{ id: "p1", name: "A" }], assign: { s1: "p1" } });
  });
});
