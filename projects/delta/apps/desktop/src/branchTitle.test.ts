import { describe, expect, it } from "vitest";
import { branchBase, nextBranchTitle } from "./branchTitle";

const zh = (base: string, n: number) => `${base}（${n}）`;

describe("branch titles", () => {
  it("numbers the first branch of a conversation (1)", () => {
    expect(nextBranchTitle("季度计划", ["季度计划", "别的对话"], zh)).toBe("季度计划（1）");
  });

  it("takes the next number after the branches that already exist", () => {
    expect(nextBranchTitle("季度计划", ["季度计划", "季度计划（1）", "季度计划（2）"], zh)).toBe("季度计划（3）");
  });

  it("branches a branch from the original title instead of stacking suffixes", () => {
    expect(nextBranchTitle("季度计划（1）", ["季度计划", "季度计划（1）"], zh)).toBe("季度计划（2）");
    expect(nextBranchTitle("Plan (2)", ["Plan", "Plan (1)", "Plan (2)"], (b, n) => `${b} (${n})`)).toBe("Plan (3)");
  });

  it("fills no gaps and ignores other conversations that only start the same way", () => {
    expect(nextBranchTitle("计划", ["计划（1）", "计划（5）", "计划书（9）", "计划 草稿（7）"], zh)).toBe("计划（6）");
  });

  it("drops the old (branch) suffixes, however many were stacked", () => {
    expect(branchBase("计划（分支）（分支）")).toBe("计划");
    expect(branchBase("Plan (branch) (branch)")).toBe("Plan");
    expect(nextBranchTitle("计划（分支）（分支）", ["计划（分支）（分支）"], zh)).toBe("计划（1）");
  });

  it("matches full-width and ASCII brackets alike, and keeps a title that is only a number", () => {
    expect(nextBranchTitle("计划(1)", ["计划", "计划(1)", "计划（2）"], zh)).toBe("计划（3）");
    expect(branchBase("（1）")).toBe("（1）");
  });

  it("treats the base as text, not a pattern", () => {
    expect(nextBranchTitle("a+b (v2)", ["a+b (v2) (1)"], (b, n) => `${b} (${n})`)).toBe("a+b (v2) (2)");
  });
});
