// Shared style grammar for the Connectors surfaces (UX-DECISIONS §21): macOS
// System-Settings-style grouped inset lists — quiet rows, hairline separators,
// pill buttons and small status tags.

/** Grouped inset list container; children separate with hairlines. */
export const GRP = "rounded-xl2 border border-line bg-panel divide-y divide-line overflow-hidden";

/** One list row: 44px minimum, comfortable gaps. */
export const ROW = "flex items-center gap-3 px-4 py-2.5 min-h-[44px]";

/** Sentence-case section header above a group. */
export const GRP_H = "text-[12px] font-semibold text-muted px-4 mt-6 mb-1.5";

/** Quiet footnote under a group. */
export const FOOT = "text-[12px] text-faint px-4 pt-1.5";

export const PILL_ACCENT =
  "text-[12.5px] font-medium px-3 py-1.5 rounded-full bg-primary text-on-primary shrink-0 disabled:opacity-50";
export const PILL_QUIET =
  "text-[12.5px] font-medium px-3 py-1.5 rounded-full bg-bg border border-line text-primary shrink-0 hover:border-line-strong";
export const PILL_LINE =
  "text-[12.5px] font-medium px-3 py-1.5 rounded-full border border-line-strong text-ink shrink-0 hover:bg-bg";

export const TAG_ACCENT =
  "text-[10.5px] font-semibold px-1.5 py-0.5 rounded bg-selected text-primary shrink-0";
export const TAG_WARN =
  "text-[10.5px] font-semibold px-1.5 py-0.5 rounded bg-warn-soft text-warn shrink-0";
export const TAG_QUIET =
  "text-[10.5px] font-semibold px-1.5 py-0.5 rounded bg-bg border border-line text-muted shrink-0";

/** Status chip variants for the Connected list. */
export const CHIP_OK =
  "text-[11px] font-medium px-2 py-0.5 rounded-full bg-ok-soft text-ok border border-ok-line shrink-0";
export const CHIP_WARN =
  "text-[11px] font-medium px-2 py-0.5 rounded-full bg-warn-soft text-warn border border-warn/20 shrink-0";
export const CHIP_OFF =
  "text-[11px] font-medium px-2 py-0.5 rounded-full bg-bg text-muted border border-line-strong shrink-0";

/** Small × affordance (danger on hover). */
export const XBTN = "text-faint hover:text-danger shrink-0 leading-none";
