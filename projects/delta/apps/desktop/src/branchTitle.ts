// The title of a branch: the conversation's own title with a running number, "Plan (1)", "Plan (2)".
// A branch of a branch keeps the original title and takes the next free number, so the suffixes
// never pile up as "Plan (branch) (branch)".

const NUMBER_SUFFIX = /\s*[（(]\s*\d+\s*[）)]\s*$/;
// Branches made before the numbering existed ended in "(branch)", or in the same word in Chinese
// (\u5206\u652f) inside full-width brackets.
const OLD_SUFFIX = /\s*[（(]\s*(?:\u5206\u652f|branch)\s*[）)]\s*$/i;

/** The title without any branch suffix, however many were stacked on it. */
export function branchBase(title: string): string {
  let base = title.trim();
  for (;;) {
    const stripped = base.replace(NUMBER_SUFFIX, "").replace(OLD_SUFFIX, "").trim();
    if (stripped === base || stripped === "") return base;
    base = stripped;
  }
}

/**
 * The next branch title. `existing` are the titles already in use; the number is one above the
 * highest one a branch of the same base already carries. `format` joins the base and the number.
 */
export function nextBranchTitle(
  sourceTitle: string,
  existing: string[],
  format: (base: string, n: number) => string,
): string {
  const base = branchBase(sourceTitle);
  const escaped = base.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const numbered = new RegExp(`^${escaped}\\s*[（(]\\s*(\\d+)\\s*[）)]$`, "i");
  let highest = 0;
  for (const title of existing) {
    const match = numbered.exec(title.trim());
    if (match) highest = Math.max(highest, Number(match[1]));
  }
  return format(base, highest + 1);
}
