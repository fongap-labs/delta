import { Fragment, useEffect, useId, useRef, useState, type ReactNode } from "react";
import type { ApprovalDecision, Attachment, Item, UndoFilesResult } from "../types";
import { shortArgs } from "./ApprovalCard";
import { humanizeAsk, humanizeTool, resolveHumanLine, type HumanLine } from "../humanize";
import { Markdown, OPEN_ARTIFACT_EVENT } from "./Markdown";
import { ConnectorMessageCard } from "./ConnectorMessageCard";
import { MemorySuggestion } from "./MemorySuggestion";
import { Icon } from "./Icon";
import { IconButton } from "./ui/IconButton";
import { Button } from "./ui/Button";
import { DisclosureTrigger, DisclosureContent } from "./ui/DisclosureControl";
import { useI18n } from "@delta/i18n/I18nContext";
import { formatToolDuration, toolDurationMs } from "../toolTiming";

// Long user pastes swallow the transcript (owner ask 2026-07-30): clamp past a generous
// threshold with a more…/less… toggle. Normal typed messages never see the control; the
// full text still drives copy (BubbleMeta) and is what the model received.
const USER_CLAMP_CHARS = 1200;

function ClampedUserText({ text }: { text: string }) {
  const { t } = useI18n();
  const [isOpen, setOpen] = useState(false);
  if (text.length <= USER_CLAMP_CHARS) return <>{text}</>;
  return (
    <>
      {isOpen ? text : text.slice(0, USER_CLAMP_CHARS).trimEnd() + "…"}
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="block ml-auto mt-1.5 text-[12.5px] font-medium opacity-75 hover:opacity-100"
      >
        {isOpen ? t("transcript.less", undefined, "less…") : t("transcript.more", undefined, "more…")}
      </button>
    </>
  );
}

// Hover affordances for a message bubble (FB-005): copy the raw text + the message's time.
// Lives in a ZERO-HEIGHT strip under the bubble (absolute, inside the transcript's 20px gap)
// so revealing it on group-hover never shifts the layout. `ts` is unix seconds — canonical
// messages carry it, pre-stamp history doesn't, so the time simply omits itself when absent.
function BubbleMeta({
  text,
  ts,
  align,
  actions,
}: {
  text: string;
  ts?: number;
  align: "left" | "right";
  /** Extra inline actions (e.g. Edit) rendered after the time, before copy. */
  actions?: ReactNode;
}) {
  const { t } = useI18n();
  const [isCopied, setCopied] = useState(false);
  const when = typeof ts === "number" ? new Date(ts * 1000) : null;
  // One strip for every bubble, always visible at rest: the time and the actions (copy, edit,
  // regenerate…) sit at the same contrast on every message, so a reader never has to hover to
  // find out which controls a message has. Only the emphasis changes on hover. Pre-stamp history
  // carries no `ts`, so it simply has no time to show.
  const strip = "opacity-70 hover:opacity-100 focus-within:opacity-100 ";
  const copy = () => {
    // "Copied" only after the write actually lands — WebKit can reject outside a
    // trusted gesture, and claiming success on a silent no-op would gaslight the user.
    navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      })
      .catch(() => {});
  };
  return (
    <div className="relative h-0 select-none">
      <div
        className={
          "absolute top-1 flex items-center gap-1.5 text-[10.5px] leading-none text-faint whitespace-nowrap transition-opacity " +
          strip +
          (align === "right" ? "right-0" : "left-0")
        }
      >
        {when && (
          <span data-testid="bubble-ts" title={when.toLocaleString()}>
            {when.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}
          </span>
        )}
        {actions}
        <button
          className="flex items-center cursor-pointer hover:text-muted"
          data-testid="bubble-copy"
          title={t("transcript.copyMessage", undefined, "Copy message")}
          onClick={copy}
        >
          {isCopied ? t("transcript.copied", undefined, "Copied") : <Icon name="copy" size={11} />}
        </button>
      </div>
    </div>
  );
}

// Reasoning-model thinking text (model-layer roadmap item 4): a quiet disclosure —
// collapsed by default, the trace one click away. `live` = still streaming (pulsing label);
// App renders that variant above the transcript, this one rides a finalized assistant item.
// Server-side notices persist raw English markers ("Interrupted.", "Context compacted",
// "Stopped: max iterations reached.", "Error: …", "Model switched to …"). The wire and
// history format stays untouched; the known markers localize at render — the same
// pattern as localizeVerifyMsg in ProviderSetup. Unknown text passes through verbatim
// (audit information is never rewritten).
// The runtime's line when a model refuses tool definitions (rust: runtime/model_turn.rs). Must match it exactly.
export const TOOLS_REFUSED_TEXT =
  "This model does not accept tool calls, so it answers without tools. Web search still works: the model asks for a search in text and Delta runs it.";

export function localizeNoticeText(
  text: string,
  t: (key: string, vars?: Record<string, string | number>, fallback?: string) => string,
): string {
  if (text === "Interrupted.") return t("transcript.interrupted", undefined, "Interrupted.");
  if (text === "Stopped: max iterations reached.")
    return t("transcript.maxIterations", undefined, "Stopped: max iterations reached.");
  if (text.startsWith("Context compacted"))
    return t("transcript.contextCompacted", undefined, text);
  if (text === "That message was rejected.")
    return t("transcript.rejected", undefined, "That message was rejected.");
  if (text === "Model switched") return t("transcript.modelSwitched", undefined, "Model switched");
  if (text === TOOLS_REFUSED_TEXT) return t("transcript.toolsRefused", undefined, text);
  const switched = text.match(/^Model switched to (.+)$/);
  if (switched)
    return t("transcript.modelSwitchedTo", { model: switched[1] }, `Model switched to ${switched[1]}`);
  if (text.startsWith("Error: "))
    return (
      t("transcript.errorPrefix", undefined, "Error: ") +
      text.slice("Error: ".length)
    );
  return text;
}

/** Error/warn notice with view-then-collapse behavior: born expanded so the error is
    readable immediately, auto-collapses to a one-line summary after a few idle seconds
    (a long raw API dump used to sit in the transcript forever), and re-expands on click.
    Hovering pauses the collapse — actively reading it counts as "not yet viewed".
    Hierarchy: the reason is the message, Retry is the action, technical details the
    weak disclosure. `repeatCount` > 1 collapses N identical consecutive failures into
    this one notice with a "retried N-1 times" suffix (display merge only — every
    error stays in the item history/audit). */
function CollapsibleNotice({
  text,
  providerError,
  showRetry,
  retrying,
  onRetry,
  onSwitchModel,
  repeatCount = 1,
}: {
  text: string;
  providerError?: boolean;
  showRetry?: boolean;
  retrying?: boolean;
  onRetry?: () => void;
  onSwitchModel?: () => void;
  repeatCount?: number;
}) {
  const { t } = useI18n();
  const [isExpanded, setExpanded] = useState(false);
  const detailsId = useId();
  const [isCopied, setCopied] = useState(false);
  const hoverRef = useRef(false);
  // Auto-collapse once the user has had a chance to read it (and isn't mid-read).
  useEffect(() => {
    if (!isExpanded) return;
    const timer = setTimeout(() => {
      if (!hoverRef.current) setExpanded(false);
    }, 6000);
    return () => clearTimeout(timer);
  }, [isExpanded]);
  const firstLine = text.split("\n", 1)[0] || text;
  const summary = providerError
    ? t(
        "errors.provider.temporarilyUnavailable",
        undefined,
        "Provider temporarily unavailable. You can retry.",
      )
    : localizeNoticeText(firstLine, t);
  const hasDetails = providerError || text.includes("\n") || text.length > 140;
  const retried = repeatCount - 1;
  return (
    <div
      className={"notice notice-error" + (isExpanded ? " expanded" : "")}
      onMouseEnter={() => {
        hoverRef.current = true;
      }}
      onMouseLeave={() => {
        hoverRef.current = false;
      }}
    >
      <span className="notice-error-line">
        <span className="min-w-0">
          {summary}
          {retried > 0 && (
            <span className="text-faint">
              {" · "}
              {t("transcript.errorRetried", { n: retried }, `retried ${retried}×`)}
            </span>
          )}
        </span>
        {retrying && (
          <span className="text-faint text-[11px] shrink-0" data-testid="notice-retrying">
            {t("transcript.retrying", undefined, "Retrying…")}
          </span>
        )}
        <span className="ml-auto flex items-center gap-2 shrink-0">
          {hasDetails && (
            <DisclosureTrigger
              isOpen={isExpanded}
              contentId={detailsId}
              className="text-[11px] text-faint hover:text-muted"
              onClick={() => setExpanded((current) => !current)}
            >
              {isExpanded
                ? t("transcript.errorHide", undefined, "Hide details")
                : t("transcript.errorShow", undefined, "Show details")}
            </DisclosureTrigger>
          )}
          {showRetry && providerError && onSwitchModel && (
            <Button
              size="sm"
              data-testid="notice-switch-model"
              onClick={(e) => {
                e.stopPropagation();
                onSwitchModel();
              }}
            >
              {t("transcript.switchModel", undefined, "Switch model")}
            </Button>
          )}
          {showRetry && (
            <Button
              size="sm"
              data-testid="notice-retry"
              onClick={(e) => {
                e.stopPropagation();
                onRetry?.();
              }}
            >
              {t("transcript.retry", undefined, "Retry")}
            </Button>
          )}
        </span>
      </span>
      <DisclosureContent isOpen={isExpanded && hasDetails} id={detailsId}
        label={t("transcript.errorShow", undefined, "Show details")}
        className="mt-2 rounded-md border border-line bg-bg/70 p-2 text-left text-[11.5px] font-mono whitespace-pre-wrap break-all">
          <div>{text}</div>
          <button
            type="button"
            className="mt-1.5 font-sans text-faint hover:text-ink"
            onClick={() => {
              void navigator.clipboard?.writeText(text);
              setCopied(true);
            }}
          >
            {isCopied
              ? t("transcript.copied", undefined, "Copied")
              : t("transcript.copyDetails", undefined, "Copy details")}
          </button>
      </DisclosureContent>
    </div>
  );
}

/** One quiet line for the collapsed disclosure: the first non-empty line, flattened and clipped.
 *  Kept for the expanded body's preview and for callers that want a plain-language summary. */
export function thinkingSummary(text: string, max = 80): string {
  const first = text.split(/\r?\n/).map((l) => l.replace(/^[#>*\-\s]+/, "").replace(/\s+/g, " ").trim()).find(Boolean) ?? "";
  return first.length > max ? first.slice(0, max - 1).trimEnd() + "…" : first;
}

export function ThinkingBlock({ text, isLive }: { text: string; isLive?: boolean }) {
  const { t } = useI18n();
  const [isOpen, setOpen] = useState(false);
  const bodyId = `thinking-body-${useId()}`;
  // A collapsed disclosure announces that something is hidden; it must not preview what is
  // hidden. Rendering the first line of the reasoning next to the label told the reader the
  // block was open while it was shut — and leaked half a sentence that broke mid-clause.
  return (
    <div className="thinking">
      <DisclosureTrigger
        isOpen={isOpen}
        contentId={bodyId}
        className="thinking-head"
        onClick={() => setOpen((v) => !v)}
        data-testid="thinking-toggle"
      >
        <Icon name="chevronDown" size={12} className={"thinking-caret" + (isOpen ? " open" : "")} />
        <span>
          {isLive ? t("transcript.thinkingLive", undefined, "Thinking") : t("transcript.thoughtProcess", undefined, "Thought process")}
          {isLive && <span className="thinking-dots" aria-hidden="true"><span /><span /><span /></span>}
        </span>
      </DisclosureTrigger>
      <DisclosureContent isOpen={isOpen} id={bodyId}
        label={isLive ? t("transcript.thinkingLive", undefined, "Thinking") : t("transcript.thoughtProcess", undefined, "Thought process")}
        className="thinking-body" testId="thinking-body">
        {text}
      </DisclosureContent>
    </div>
  );
}

type ToolItem = Extract<Item, { kind: "tool" }>;
type ApprovalItem = Extract<Item, { kind: "approval" }>;
type AssistantItem = Extract<Item, { kind: "assistant" }>;
type TurnItem = ToolItem | ApprovalItem | AssistantItem;

// Presentation-only mode: runtime events, approvals, and model reasoning remain untouched.
type ActivityMode = "concise" | "process" | "detail";

// TurnGroup (§33, absorbs §7's StepGroup): the whole user-message → final-answer span collapses
// as ONE disclosure — "N steps" — with the agent's narration (assistant text followed by more
// activity in the same turn) and humanized one-line steps interleaved inside. The final assistant
// text renders as a normal bubble OUTSIDE the group (see the flush logic in Transcript below).
// Approvals fold into their tool's row as a chip; an approval with no executed call (typically
// declined) keeps its own "Wanted to …" row. Raw args+result stay one click away per row.

type TurnRow =
  | { type: "narr"; text: string }
  | { type: "step"; tool: ToolItem; approval?: ApprovalItem }
  | { type: "ask"; approval: ApprovalItem };

function buildRows(items: TurnItem[]): TurnRow[] {
  // First pass: tool rows in order; then pair each resolved approval with the nearest
  // same-name tool that doesn't have one yet (approvals may stream before or after their call).
  const rows: TurnRow[] = items
    .filter((it): it is ToolItem | AssistantItem => it.kind !== "approval")
    // Thinking-only assistant items (no text) carry nothing narratable — skip the row.
    .filter((it) => it.kind !== "assistant" || it.text)
    .map((it) =>
      it.kind === "assistant" ? { type: "narr" as const, text: it.text } : { type: "step" as const, tool: it },
    );
  const approvals = items.filter((it): it is ApprovalItem => it.kind === "approval");
  for (const ap of approvals) {
    const at = items.indexOf(ap);
    let bestRow: Extract<TurnRow, { type: "step" }> | null = null;
    let bestDist = Infinity;
    for (let i = 0; i < items.length; i++) {
      const it = items[i];
      if (it.kind !== "tool" || it.name !== ap.name) continue;
      const row = rows.find((r) => r.type === "step" && r.tool === it) as
        | Extract<TurnRow, { type: "step" }>
        | undefined;
      if (!row || row.approval) continue;
      const dist = Math.abs(i - at);
      if (dist < bestDist) {
        bestRow = row;
        bestDist = dist;
      }
    }
    if (bestRow && ap.resolved !== "deny") bestRow.approval = ap;
    else {
      // No executed call to attach to (or it was declined) — the ask keeps its own row,
      // placed where the approval sat in the stream.
      const after = items.slice(0, at).filter((it) => it.kind !== "approval").length;
      rows.splice(after, 0, { type: "ask", approval: ap });
    }
  }
  return rows;
}

function ApprovalChip({ resolved }: { resolved: ApprovalDecision | undefined }) {
  const { t } = useI18n();
  if (resolved === "deny")
    return <span className="text-[10.5px] px-1.5 rounded-full bg-danger-soft text-danger shrink-0">{t("approval.chipDeclined", undefined, "✕ declined")}</span>;
  return (
    <span
      className="text-[10.5px] px-1.5 rounded-full bg-ok-soft text-ok shrink-0"
      title={resolved ? t("approval.chipApprovedWith", { mode: resolved.replace(/_/g, " ") }, `approved · ${resolved.replace(/_/g, " ")}`) : t("approval.chipApprovedTitle", undefined, "approved")}
    >
      {t("approval.chipApproved", undefined, "✓ approved")}
    </span>
  );
}

function LineText({ line }: { line: HumanLine }) {
  const { t } = useI18n();
  const r = resolveHumanLine(line, t);
  return (
    <span className="min-w-0 text-[13px] leading-relaxed">
      <span className="text-muted">{r.pre}</span>
      {r.obj && <span className="text-ink">{r.obj}</span>}
      {r.post && <span className="text-muted">{r.post}</span>}
    </span>
  );
}

function StepRow({ tool, approval, mode }: { tool: ToolItem; approval?: ApprovalItem; mode: ActivityMode }) {
  const { t } = useI18n();
  // Detail mode explicitly opts in to showing tool inputs/results, never model reasoning.
  const [rawOverride, setRawOverride] = useState<{ mode: ActivityMode; open: boolean } | null>(null);
  const isRaw = rawOverride?.mode === mode ? rawOverride.open : mode === "detail";
  const rawId = useId();
  const running = tool.status === "…";
  const failed = tool.status === "error";
  const interrupted = tool.status === "interrupted";
  const succeeded = tool.status === "ok";
  // An absent result, unknown future status or explicitly interrupted call is not a success.
  const state = running ? "running" : failed ? "error" : interrupted ? "interrupted" : succeeded ? "finished" : "unknown";
  const statusLabel = state === "running"
    ? t("transcript.toolRunning", undefined, "Running")
    : state === "error"
      ? t("transcript.toolFailed", undefined, "Failed")
      : state === "interrupted"
        ? t("transcript.toolInterrupted", undefined, "Interrupted")
        : state === "finished"
          ? t("transcript.toolSucceeded", undefined, "Completed")
          : t("transcript.toolUnknown", undefined, "No recorded result");
  const statusTone = failed ? "text-danger" : interrupted || state === "unknown" ? "text-warn" : running ? "text-primary" : "text-ok";
  const duration = toolDurationMs(tool);
  return (
    <div data-state={state}>
      <div className="group flex items-center gap-2 px-2 py-1.5 rounded-lg hover:bg-bg" data-testid="turn-step">
        <span className={"w-3.5 text-center text-[10px] shrink-0 " + statusTone} aria-label={statusLabel}>
          {running ? <span className="spinner" data-testid="step-running" /> : failed ? "×" : interrupted ? "■" : succeeded ? "●" : "○"}
        </span>
        <LineText
          line={
            // A refused load must not read as a success — "Used skill:" is the trust line
            // (SKILLS-SPEC §4.1 #4), so a blocked attempt gets honest wording instead.
            tool.name === "load_skill" && tool.preview?.includes('"error"')
              ? { pre: t("transcript.triedSkill", undefined, "Tried skill: "), obj: String(tool.args?.name ?? ""), post: t("transcript.triedSkillUnavailable", undefined, " — not available") }
              : humanizeTool(tool.name, tool.args)
          }
        />
        {approval && <ApprovalChip resolved={approval.resolved} />}
        {!!tool.standingRule && (
          <span
            className="text-[10.5px] px-1.5 rounded-full bg-surface-2 text-ink border border-line-strong shrink-0"
            data-testid="tool-standing-rule"
            title={t("transcript.autoAllowedTitle", { rule: tool.standingRule }, `Auto-allowed by this automation's standing approval: ${tool.standingRule}. Revoke on its Automations page.`)}
          >
            {t("transcript.autoAllowed", undefined, "auto-allowed")}
          </span>
        )}
        {!!tool.hidden && (
          <span
            className="text-[11px] text-warn shrink-0"
            data-testid="tool-hidden-count"
            title={t("transcript.hiddenCountTitle", undefined, "Removed by your privacy filters before the agent saw the results — agents get no trace of these.")}
          >
            {t("transcript.hiddenCount", { n: tool.hidden }, `${tool.hidden} hidden`)}
          </span>
        )}
        <span data-testid="step-status" className={"text-[10.5px] shrink-0 " + statusTone}>{statusLabel}</span>
        {duration !== null && (
          <span
            data-testid="tool-duration"
            className="text-[10.5px] tabular-nums text-muted shrink-0"
            title={t("transcript.toolDurationSource", undefined, "Elapsed between runtime start and finish events")}
          >{formatToolDuration(duration)}</span>
        )}
        <DisclosureTrigger
          isOpen={isRaw}
          contentId={rawId}
          className="ml-auto shrink-0 inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] text-muted hover:bg-surface-2 hover:text-ink cursor-pointer"
          onClick={() => setRawOverride({ mode, open: !isRaw })}
          data-testid="tool-details-toggle"
        >
          {t("transcript.toolDetails", undefined, "Details")}
          <Icon name="chevronDown" size={12} className={isRaw ? "rotate-180" : ""} />
        </DisclosureTrigger>
      </div>
      <DisclosureContent
        isOpen={isRaw}
        id={rawId}
        label={t("transcript.toolDetails", undefined, "Details") + " — " + tool.name}
        className="ml-8 mr-2 my-1 px-3 py-2 rounded-lg border border-line bg-bg text-[11.5px] leading-relaxed"
        testId="tool-detail-panel"
      >
        <div className="text-faint font-medium mb-1">{t("transcript.toolInput", undefined, "Call and input")}</div>
        <pre className="font-mono text-muted whitespace-pre-wrap break-all max-h-48 overflow-auto">{tool.name}{"  "}{shortArgs(tool.args)}</pre>
        {running ? (
          <div className="mt-2 text-primary" data-testid="tool-detail-pending">
            {t("transcript.toolPending", undefined, "Waiting for tool result…")}
          </div>
        ) : tool.preview ? (
          <div className="mt-2" data-testid="tool-detail-output">
            <div className="text-faint font-medium mb-1">{t("transcript.toolOutput", undefined, "Result")}</div>
            <pre className="font-mono text-muted whitespace-pre-wrap break-all max-h-56 overflow-auto">{tool.preview.length > 1500 ? tool.preview.slice(0, 1500) + "\n…" : tool.preview}</pre>
          </div>
        ) : null}
      </DisclosureContent>
    </div>
  );
}

function TurnGroup({
  items,
  isLive,
  streamingText,
  stepOffset = 0,
  mode,
}: {
  items: TurnItem[];
  isLive?: boolean;
  mode: ActivityMode;
  // Sub-threshold streamed text belongs to THIS group (§33 ref #3): collapsed → it rides
  // the header as the live line; expanded → the small quiet line under the steps.
  streamingText?: string;
  // Steps already taken earlier in the same request (in groups split by a question).
  stepOffset?: number;
}) {
  const { t } = useI18n();
  // Turns start COLLAPSED, running or not (owner call 2026-07-14) — the header's live
  // line is the pulse; expanding is opt-in.
  const rows = buildRows(items);
  const tools = items.filter((it): it is ToolItem => it.kind === "tool");
  const running = isLive || tools.some((t) => t.status === "…");
  const [userToggle, setUserToggle] = useState<{ mode: ActivityMode; open: boolean } | null>(null);
  const isOpen = userToggle?.mode === mode ? userToggle.open : mode !== "concise";
  const lastNarr = [...items].reverse().find((it): it is AssistantItem => it.kind === "assistant");
  // A live call takes precedence over stale narration, but newer streamed text is authoritative.
  // Only use events that exist; never manufacture reasoning or a percent-complete estimate.
  const activeCall = [...tools].reverse().find((tool) => tool.status === "…");
  const activeLine = activeCall ? resolveHumanLine(humanizeTool(activeCall.name, activeCall.args), t) : null;
  const currentAction = activeLine ? [activeLine.pre, activeLine.obj, activeLine.post].filter(Boolean).join("").trim() : "";
  const liveLine = streamingText || currentAction || lastNarr?.text || "";

  const nSteps = stepOffset + rows.filter((r) => r.type !== "narr").length;
  const declined = items.filter((it) => it.kind === "approval" && it.resolved === "deny").length;
  const hiddenTotal = tools.reduce((n, t) => n + (t.hidden || 0), 0);
  const stepsLabel = t("transcript.stepCount", { n: nSteps, s: nSteps === 1 ? "" : "s" }, `${nSteps} step${nSteps === 1 ? "" : "s"}`);
  // Only real results count as finished; missing results and interruptions must not inflate this.
  const finishedCalls = tools.filter((tool) => tool.status === "ok" || tool.status === "error").length;
  const failures = tools.filter((tool) => tool.status === "error").length;
  const interruptedCalls = tools.filter((tool) => tool.status === "interrupted").length;
  const unknownCalls = tools.filter((tool) => !["…", "ok", "error", "interrupted"].includes(tool.status)).length;

  return (
    <details className="stepgroup" open={isOpen}>
      <summary
        className="stepgroup-head flex items-center gap-2 py-0.5 cursor-pointer select-none text-[12.5px] text-faint hover:text-muted"
        onClick={(e) => {
          e.preventDefault(); // drive open/closed from state, not the native toggle
          setUserToggle({ mode, open: !isOpen });
        }}
      >
        <span className={"chev inline-block transition-transform" + (isOpen ? " rotate-90" : "")}>›</span>
        <span>
          <span>{running ? t("transcript.runningSteps", { label: stepsLabel }, `Running ${stepsLabel}…`) : stepsLabel}</span>
          {running && tools.length > 1 && (
            <span className="text-faint" data-testid="turn-tool-progress">
              {" · "}{t("transcript.toolProgress", { done: finishedCalls, total: tools.length }, String(finishedCalls) + "/" + tools.length + " tools finished")}
            </span>
          )}
          {failures > 0 && (
            <span className="text-danger" data-testid="turn-failed-count">
              {" · "}{t("transcript.failedCount", { n: failures }, failures + " failed")}
            </span>
          )}
          {interruptedCalls > 0 && (
            <span className="text-warn" data-testid="turn-interrupted-count">
              {" · "}{t("transcript.interruptedCount", { n: interruptedCalls }, interruptedCalls + " interrupted")}
            </span>
          )}
          {unknownCalls > 0 && (
            <span className="text-warn" data-testid="turn-unknown-count">
              {" · "}{t("transcript.unknownCount", { n: unknownCalls }, unknownCalls + " without results")}
            </span>
          )}
          {declined > 0 && (
            <>
              {" · "}
              <span className="text-danger" data-testid="stepgroup-declined">
                {t("transcript.declinedCount", { n: declined }, `${declined} declined`)}
              </span>
            </>
          )}
          {hiddenTotal > 0 && (
            <>
              {" · "}
              <span className="text-warn" data-testid="stepgroup-hidden">
                {t("transcript.hiddenByFilters", { n: hiddenTotal }, `${hiddenTotal} hidden by your filters`)}
              </span>
            </>
          )}
        </span>
        {running && !isOpen && liveLine && (
          <span className="min-w-0 flex-1 truncate" data-testid="turn-live-line">
            · {liveLine}
          </span>
        )}
      </summary>
      {isOpen && (
        <ol
          className="ml-1.5 mt-1 pl-2 border-l-2 border-line flex flex-col gap-0.5 list-none"
          aria-label={t("transcript.executionTimeline", undefined, "Agent execution timeline")}
          data-testid="agent-execution-timeline"
        >
          {rows.map((row, i) =>
            row.type === "narr" ? (
              <li className="turn-narr px-2 py-1 text-[13px] text-muted max-w-[60ch]" key={`narr-${i}`} data-testid="turn-narration">
                <Markdown text={row.text} />
              </li>
            ) : row.type === "ask" ? (
              <li className="flex items-baseline gap-2 px-2 py-0.5" key={`ask-${i}`} data-testid="turn-ask">
                <span className={"w-3.5 text-center text-[10px] shrink-0 " + (row.approval.resolved === "deny" ? "text-danger" : "text-ok")}>●</span>
                <LineText line={humanizeAsk(row.approval.name, row.approval.args)} />
                <ApprovalChip resolved={row.approval.resolved} />
              </li>
            ) : (
              // The call ID survives streamed status changes and preserves an opened inspector.
              <li key={`tool-${row.tool.id}`} data-testid="timeline-tool" data-call-id={row.tool.id}>
                <StepRow tool={row.tool} approval={row.approval} mode={mode} />
              </li>
            ),
          )}
          {streamingText && (
            <li
              className="turn-narr px-2 py-1 text-[13px] text-muted max-w-[60ch]"
              data-testid="turn-live-stream"
            >
              <Markdown text={streamingText} />
              <span className="stream-cursor">▍</span>
            </li>
          )}
        </ol>
      )}
    </details>
  );
}

interface Props {
  items: Item[];
  onApprove: (decision: ApprovalDecision) => void;
  // The session's live flag. While true, the FINAL run's trailing assistant text is still
  // narration (status), not the answer — promoting it early made each line flash as a full
  // ASSISTANT bubble and then vanish into the group when the next tool call arrived
  // (owner report 2026-07-13). The answer bubble appears once, when the turn ends.
  running?: boolean;
  // Sub-threshold streamed text (streamGate mode "quiet") — handed to the live turn group.
  streamingText?: string;
  // The live turn's thinking (reasoning models). It shows above the live steps, not below them.
  liveReasoning?: string;
  // Re-run the failed turn (no new user message). Offered only on a retriable notice that
  // is the transcript tail of an idle session — anywhere else the error is history.
  onRetry?: () => void;
  // Continue an interrupted run. Offered only on the transcript's tail "Interrupted." notice of an
  // idle session, and only when the runtime said it can resume (`canResume`).
  canResume?: boolean;
  onResume?: () => void;
  // MEMORY-SPEC §5.1: undo a just-announced write. `previous` (set when the write was
  // an edit) is the text to restore; without it the memory is deleted.
  onUndoMemory?: (id: number, previous?: string) => void;
  // opencode-style revert: truncate from this user message onward + prefill the composer.
  onEditMessage?: (index: number) => void;
  // A provider failure offers a way out besides Retry: pick another model.
  onSwitchModel?: () => void;
  // Ask the agent to turn a finished, multi-step task into a reusable skill (the user reviews and
  // approves the text before anything is saved).
  onSaveAsSkill?: () => void;
  // Put back the earlier version the runtime kept when a file was replaced. Resolves to an error
  // message, or null when it worked.
  onRestoreVersion?: (backup: string, target: string) => Promise<string | null>;
  // Regenerate the last reply: revert to before the last user message and send it again.
  onRegenerate?: (index: number, attachments?: Attachment[]) => void;
  // Carry a message OUT of this conversation into the one the user is actually working in
  // (ChatGPT's "continue in the main thread"). A side discussion is where a plan gets
  // refined; the work still happens in the task conversation. The receiver decides how to
  // present the carried text (quoted, editable) — the transcript only hands it over.
  onSendToMain?: (text: string) => void;
  // Start a new conversation from the end of this answer (`index` is its stored message index).
  onBranch?: (index: number) => void;
  // Undo the files the last request wrote, as one (`runId` is its run). Resolves to what happened.
  onUndoFiles?: (runId: string) => Promise<UndoFilesResult>;
}

// The transcript index whose notice gets the Retry button: the tail error notice, looking
// through info notices after it (model switches must not consume the retry — switching
// models and THEN retrying is the intended recovery path). -1 when the tail is anything else.
export function retryAnchor(items: Item[]): number {
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i];
    if (it.kind !== "notice") return -1;
    if (it.retriable) return i;
    if (it.tone !== "info") return -1;
  }
  return -1;
}

// The transcript index of the tail "Interrupted." notice (looking through info notices after it), or
// -1 when the transcript ends in anything else. Only that notice can offer Resume.
export function interruptedAnchor(items: Item[]): number {
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i];
    if (it.kind !== "notice") return -1;
    if (it.tone === "warn" && !it.retriable && it.text === "Interrupted.") return i;
    if (it.tone !== "info") return -1;
  }
  return -1;
}

export function Transcript({ items, running, streamingText, liveReasoning, onRetry,onSwitchModel, onSaveAsSkill, onRestoreVersion, canResume, onResume, onUndoMemory, onEditMessage, onRegenerate, onSendToMain, onBranch, onUndoFiles }: Props) {
  const { t } = useI18n();
  const [activityMode, setActivityMode] = useState<ActivityMode>("concise");
  // §33 grouping: a turn = the maximal run of assistant/tool/resolved-approval items between
  // breakers (user, connector, notices, plan/dir requests…). Trailing assistant texts are the
  // ANSWER and render as bubbles after the group; interior assistant texts are narration and
  // stay inside. A run with no activity at all is just bubbles (unchanged chat behavior).
  const blocks: Array<{ turn: TurnItem[]; isLive?: boolean } | { item: Item; i: number }> = [];
  let run: TurnItem[] = [];
  const flush = (isLive = false) => {
    if (!run.length) return;
    const turn = [...run];
    run = [];
    const answers: AssistantItem[] = [];
    // A live run with tool activity keeps its trailing text inside as the status line;
    // a live run with NO activity is a plain streaming reply — bubbles, as ever.
    const keepTrailing = isLive && turn.some((it) => it.kind !== "assistant");
    if (!keepTrailing)
      while (turn.length && turn[turn.length - 1].kind === "assistant")
        answers.unshift(turn.pop() as AssistantItem);
    if (turn.some((it) => it.kind !== "assistant")) {
      blocks.push({ turn, isLive });
      answers.forEach((a) => blocks.push({ item: a, i: -1 }));
    } else {
      turn.forEach((t) => blocks.push({ item: t, i: -1 }));
      answers.forEach((a) => blocks.push({ item: a, i: -1 }));
    }
  };
  items.forEach((item, i) => {
    if (item.kind === "tool" || item.kind === "assistant" || (item.kind === "approval" && item.resolved))
      run.push(item);
    else if (
      // PENDING interactive items render elsewhere (approval/question → composer head) and
      // nothing here — if they broke the run, the trailing narration would flash into an
      // answer bubble exactly while the user is being asked to decide.
      (item.kind === "approval" || item.kind === "dirreq" || item.kind === "planreq" || item.kind === "question") &&
      !item.resolved
    ) {
      return;
    } else {
      flush();
      blocks.push({ item, i });
    }
  });
  flush(!!running);

  // One thinking disclosure per user message. A message's thinking can span several model calls
  // (tool steps, a retry after a dropped connection), and all of it belongs to that message, so it
  // is collected here and rendered once, under the user message that opened the span. Owner -1 is
  // a span before any user message.
  const spanThinking = new Map<number, string[]>();
  let spanOwner = -1;
  items.forEach((item, i) => {
    if (item.kind === "user") spanOwner = i;
    else if (item.kind === "assistant" && item.reasoning)
      spanThinking.set(spanOwner, [...(spanThinking.get(spanOwner) ?? []), item.reasoning]);
  });
  const liveOwner = spanOwner;
  const thinkingFor = (owner: number): { text: string; isLive: boolean } | null => {
    const parts = [...(spanThinking.get(owner) ?? [])];
    const isLive = !!running && owner === liveOwner && !!liveReasoning;
    if (isLive && liveReasoning) parts.push(liveReasoning);
    return parts.length ? { text: parts.join("\n\n"), isLive } : null;
  };

  const deliveredFiles = deliveredSinceUser(items);
  const stepsInLastRequest = stepsSinceUser(items);
  const lastTurnIndex = blocks.reduce((acc, b, i) => ("turn" in b ? i : acc), -1);
  // Regenerate is offered only on the final answer of an idle thread whose last user message is
  // a plain (non-skill) message with a raw index; anywhere else the reply is history.
  let lastUserPos = -1;
  let lastAnswerPos = -1;
  items.forEach((it, pos) => {
    if (it.kind === "user") lastUserPos = pos;
    else if (it.kind === "assistant" && it.text) lastAnswerPos = pos;
  });
  const lastUser = lastUserPos >= 0 ? (items[lastUserPos] as Extract<Item, { kind: "user" }>) : null;
  const canRegenerate =
    !running &&
    !!onRegenerate &&
    !!lastUser &&
    lastUser.index !== undefined &&
    !lastUser.text.startsWith("/") &&
    lastAnswerPos > lastUserPos;
  // A block holding one retriable warn notice (a failed provider attempt).
  const isErrNotice = (b: (typeof blocks)[number]): b is { item: Extract<Item, { kind: "notice" }>; i: number } =>
    "item" in b && b.item.kind === "notice" && b.item.tone === "warn" && !!b.item.retriable;
  // Day dividers (ChatGPT/Claude parity). A per-message clock time alone cannot tell a
  // reader whether a reply came this morning or three days ago, so the day is announced
  // once when it changes and left implicit inside the run.
  //
  // The day of an exchange is the day of the USER message that opened it — never the day
  // its answer happens to land on. A long turn that runs past midnight is still one
  // exchange, and splitting it with a divider would tell the reader two different stories
  // about one answer. Everything between two user messages therefore inherits the first
  // one's day.
  //
  // Days are compared as CALENDAR STRINGS, not as epoch-day integers: local midnight in a
  // non-UTC zone is not a multiple of 86.4M ms, so flooring loses the zone's offset and the
  // round trip lands on the previous date (UTC+8 showed "Oct 12" for Oct 13).
  const dayKey = (ts: number): string => {
    const d = new Date(ts * 1000);
    return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
  };
  // Item variants that never carry a clock (tool rows, notices, connector cards) are
  // read through `unknown` — asking for a `ts` on a type that has no such field is a
  // weak-type error, not a bug.
  const itemDay = (item: unknown): string | null => {
    const ts = (item as { ts?: unknown } | null | undefined)?.ts;
    return typeof ts === "number" ? dayKey(ts) : null;
  };
  const blockDays = ((): (string | null)[] => {
    let openedBy: string | null = null;
    return blocks.map((b) => {
      if ("turn" in b) return openedBy ?? itemDay(b.turn[0]);
      if (b.item.kind === "user") {
        const own = itemDay(b.item);
        if (own !== null) openedBy = own;
        return own;
      }
      return openedBy ?? itemDay(b.item);
    });
  })();
  // Steps taken before each group in the same request, so a group split by a question keeps counting.
  const stepBases = ((): number[] => {
    let base = 0;
    return blocks.map((b) => {
      if ("turn" in b) {
        const at = base;
        base += buildRows(b.turn).filter((r) => r.type !== "narr").length;
        return at;
      }
      if (b.item.kind === "user") base = 0;
      return 0;
    });
  })();
  const dayLabel = (key: string): string => {
    const [y, m, d] = key.split("-").map(Number);
    const day = new Date(y, m, d);
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const gap = Math.round((today.getTime() - day.getTime()) / 86_400_000);
    if (gap <= 0) return t("transcript.dayToday", undefined, "Today");
    if (gap === 1) return t("transcript.dayYesterday", undefined, "Yesterday");
    return day.toLocaleDateString([], { year: "numeric", month: "short", day: "numeric" });
  };
  // Today is the default context of the screen, so it is never announced; only earlier days get a divider.
  const todayKey = dayKey(Date.now() / 1000);
  let lastDay: string | null = null;
  return (
    <div className="transcript">
      {items.some((item) => item.kind === "tool") && (
        <div className="flex justify-end pb-2" data-testid="activity-modes">
          <div
            role="group"
            aria-label={t("transcript.activityModes", undefined, "Activity display")}
            className="inline-flex items-center gap-0.5 rounded-xl border border-line bg-surface-2 p-1"
          >
            {(["concise", "process", "detail"] as const).map((mode) => (
              <button
                key={mode}
                type="button"
                data-testid={`activity-mode-${mode}`}
                aria-pressed={activityMode === mode}
                className={
                  "rounded-lg px-2.5 py-1 text-[11.5px] transition-colors cursor-pointer " +
                  (activityMode === mode ? "bg-bg text-ink font-medium" : "text-muted hover:text-ink")
                }
                onClick={() => setActivityMode(mode)}
              >
                {t(`transcript.mode.${mode}`, undefined, mode)}
              </button>
            ))}
          </div>
        </div>
      )}
      {thinkingFor(-1) && <ThinkingBlock key={thinkingFor(-1)!.isLive ? "live" : "done"} text={thinkingFor(-1)!.text} isLive={thinkingFor(-1)!.isLive} />}
      {blocks.map((block, bi) => {
        const day = blockDays[bi];
        const showDivider = day !== null && day !== lastDay && day !== todayKey;
        if (day !== null) lastDay = day;
        const node = (() => {
        if ("turn" in block) {
          return (
            <Fragment key={`${bi}-${activityMode}`}>
              <TurnGroup
                items={block.turn}
                isLive={block.isLive}
                streamingText={block.isLive && bi === lastTurnIndex ? streamingText : undefined}
                stepOffset={stepBases[bi]}
                mode={activityMode}
              />
            </Fragment>
          );
        }
        const { item } = block;
        switch (item.kind) {
          case "connector":
            return <ConnectorMessageCard source={item.source} key={bi} />;
          case "user": {
            const thinking = thinkingFor(block.i);
            return (
              <Fragment key={bi}>
              <div className="group self-end max-w-[78%] flex flex-col items-end">
                  <div className="bubble-user px-3.5 py-2.5 rounded-[14px_14px_4px_14px] bg-surface-2 text-ink text-[14.5px] leading-relaxed whitespace-pre-wrap">
                    {item.attachments && item.attachments.length > 0 && (
                      <div className="bubble-attachments">
                        {item.attachments.map((a, i) =>
                          a.kind === "image" ? (
                            <img key={i} className="msg-img" src={a.data_url} alt={a.name} />
                          ) : (
                            <span key={i} className="msg-file inline-flex items-center gap-1.5"><Icon name="file" size={12} />{a.name}</span>
                          ),
                        )}
                      </div>
                    )}
                    <ClampedUserText text={item.text} />
                  </div>
                  {item.steer && (
                    <div className="mt-1 text-[11px] text-faint" data-testid="steer-receipt">
                      {t("transcript.steerAdded", undefined, "Added to the current task")}
                    </div>
                  )}
                  <BubbleMeta
                    text={item.text}
                    ts={item.ts}
                    align="right"
                    actions={
                      onEditMessage && item.index !== undefined ? (
                        /* Edit = revert + prefill: the thread truncates back to before this
                            message and the original text returns to the composer; re-send
                            produces a fresh answer to the edited content. */
                        <IconButton
                          className="flex items-center cursor-pointer hover:text-muted"
                          icon="pencil"
                          iconSize={11}
                          label={t("transcript.editMessage", undefined, "编辑消息")}
                          onClick={() => onEditMessage(item.index!)}
                        />
                      ) : undefined
                    }
                  />
              </div>
              {thinking && <ThinkingBlock key={thinking.isLive ? "live" : "done"} text={thinking.text} isLive={thinking.isLive} />}
              </Fragment>
            );
          }
          case "assistant":
            // Thinking-only item (stopped mid-reasoning): no bubble; its thinking shows under the message.
            if (!item.text && item.reasoning) return null;
            return (
              <div className="group bubble-assistant" key={bi}>
                {/* No "assistant" speaker label — mainstream chat UIs distinguish by
                    alignment alone, and the label read as a stray "Agent" tag. */}
                <Markdown text={item.text} />
                <BubbleMeta
                  text={item.text}
                  ts={item.ts}
                  align="left"
                  actions={
                    <>
                      {/* Carry the conclusion out of this side discussion into the task
                          conversation. Placed first: it is the action that starts real work,
                          while regenerate/copy are refinements of this reply. */}
                      {onSendToMain && (
                        <IconButton
                          className="flex items-center cursor-pointer hover:text-muted"
                          data-testid="bubble-send-to-main"
                          icon="sparkle"
                          iconSize={11}
                          label={t("transcript.sendToMain", undefined, "Bring to current task")}
                          onClick={() => onSendToMain(item.text)}
                        />
                      )}
                      {/* Only a stored answer can end a branch, and only while nothing is running:
                          the copy is made from what the runtime has saved. */}
                      {onBranch && !running && item.index !== undefined && (
                        <IconButton
                          className="flex items-center cursor-pointer hover:text-muted"
                          data-testid="bubble-branch"
                          icon="branch"
                          iconSize={11}
                          label={t("transcript.branch", undefined, "Branch in new conversation")}
                          onClick={() => onBranch(item.index!)}
                        />
                      )}
                      {canRegenerate && item === items[lastAnswerPos] ? (
                        <IconButton
                          className="flex items-center cursor-pointer hover:text-muted"
                          data-testid="bubble-regenerate"
                          icon="refresh"
                          iconSize={11}
                          label={t("transcript.regenerate", undefined, "Regenerate")}
                          onClick={() => onRegenerate!(lastUser!.index!, lastUser!.attachments)}
                        />
                      ) : null}
                    </>
                  }
                />
              </div>
            );
          case "dirreq":
            if (!item.resolved) return null;
            return (
              <div className="approval-inline" key={bi}>
                <span className={"status " + (item.resolved === "granted" ? "ok" : "denied")}>
                  {item.resolved === "granted" ? "✓" : "✕"}
                </span>
                <span>{item.resolved === "granted" ? t("transcript.grantedAccess", undefined, "Granted folder access") : t("transcript.declinedAccess", undefined, "Declined folder access")}</span>
                {item.path && <span className="dim">{item.path}</span>}
              </div>
            );
          case "planreq":
            if (!item.resolved) return null; // pending plan renders in the composer head
            return (
              <div className="bubble-assistant" key={bi}>
                <div className="who">{t("transcript.whoProposedPlan", undefined, "proposed plan")}</div>
                <Markdown text={item.plan} />
                <div className="approval-inline">
                  <span className={"status " + (item.resolved === "approved" ? "ok" : "denied")}>
                    {item.resolved === "approved" ? "✓" : "✕"}
                  </span>
                  <span>{item.resolved === "approved" ? t("transcript.planApproved", undefined, "Plan approved") : t("transcript.planFeedback", undefined, "Sent back with feedback")}</span>
                </div>
              </div>
            );
          case "notice": {
            // Provider errors MERGE (display-level only): N consecutive retriable warn
            // notices are N failed attempts (auto-retry or manual) and used to slice the
            // transcript into repeated large blocks. One notice + a retried-N suffix now;
            // every raw error stays in the item history/audit and the latest stays
            // expandable here. Merge runs break on any other block (user/turn/info…).
            if (item.tone === "warn" && item.retriable) {
              if (bi > 0 && isErrNotice(blocks[bi - 1])) return null; // absorbed above
              let end = bi;
              while (end + 1 < blocks.length && isErrNotice(blocks[end + 1])) end++;
              const last = blocks[end] as { item: Extract<Item, { kind: "notice" }>; i: number };
              const anchor = retryAnchor(items);
              const anchorInRun = anchor >= block.i && anchor <= last.i;
              return (
                <CollapsibleNotice
                  key={bi}
                  text={last.item.text}
                  providerError
                  repeatCount={end - bi + 1}
                  showRetry={anchorInRun && !running && !!onRetry}
                  retrying={anchorInRun && running}
                  onRetry={onRetry}
                  onSwitchModel={onSwitchModel}
                />
              );
            }
            // Errors/warnings get the view-then-collapse treatment (a raw API dump used
            // to sit expanded in the transcript forever).
            if (item.tone === "warn" && canResume && !running && onResume && block.i === interruptedAnchor(items)) {
              return (
                <div className="notice notice-event" key={bi}>
                  {localizeNoticeText(item.text, t)}
                  <button className="btn ml-2" data-testid="notice-resume" onClick={onResume}>
                    {t("transcript.resume", undefined, "Resume")}
                  </button>
                </div>
              );
            }
            if (item.tone === "warn") return <CollapsibleNotice key={bi} text={item.text} />;
            // Info notices are runtime events (model switch, compaction divider): quiet,
            // centered, compact — never chat-content weight.
            return (
              <div className="notice notice-event" key={bi}>
                {item.modelSwitchModel
                  ? t("transcript.modelSwitchedTo", { model: item.modelSwitchModel }, `Model switched to ${item.modelSwitchModel}`) +
                    (item.modelSwitchImageWarning
                      ? " — " + t("transcript.modelSwitchImageWarning", undefined, "earlier images can't be read by this model")
                      : "")
                  : localizeNoticeText(item.text, t)}
                {item.retriable && !running && onRetry && block.i === retryAnchor(items) && (
                  <button className="btn ml-2" data-testid="notice-retry" onClick={onRetry}>
                    {t("transcript.retry", undefined, "Retry")}
                  </button>
                )}
              </div>
            );
          }
          // §5.1 save notice: quiet, inline, and it STAYS — the user reads it in place
          // and can undo whenever they get to it.
          case "memory":
            return (
              <div
                className="notice flex items-center gap-2 text-left"
                data-testid="memory-notice"
                key={bi}
              >
                {item.undone ? (
                  <span data-testid="memory-notice-undone">
                    {item.previous ? t("transcript.memoryUndonePrevious", undefined, "Okay — put back the way it was.") : t("transcript.memoryUndoneNew", undefined, "Okay — forgotten.")}
                  </span>
                ) : (
                  <>
                    <span className="min-w-0">
                      <span className="font-medium">
                        {item.previous ? t("transcript.memoryUpdated", undefined, "I've updated what I remember") : t("transcript.memoryRemember", undefined, "I'll remember that")}
                      </span>
                      {item.text ? <span className="text-muted"> — {item.text}</span> : null}
                    </span>
                    {onUndoMemory && (
                      <button
                        className="btn ml-auto shrink-0"
                        data-testid="memory-notice-undo"
                        onClick={() => onUndoMemory(item.id, item.previous)}
                      >
                        {t("transcript.undo", undefined, "Undo")}
                      </button>
                    )}
                  </>
                )}
              </div>
            );
          case "memory_suggestion":
            return (
              <MemorySuggestion key={bi} candidateId={item.candidateId} text={item.text} quote={item.quote} />
            );
          default:
            return null;
        }
        })();
        if (!showDivider || !node) return node;
        return (
          <div key={bi} className="day-divider-wrap">
            <div className="day-divider" data-testid="day-divider">
              <span>{dayLabel(day!)}</span>
            </div>
            {node}
          </div>
        );
      })}
      {!running && (deliveredFiles.length > 0 || (onSaveAsSkill && stepsInLastRequest >= SKILL_MIN_STEPS)) && (
        <DeliveryCard
          files={deliveredFiles}
          onRestoreVersion={onRestoreVersion}
          onUndoAll={onUndoFiles && lastUser?.runId ? () => onUndoFiles(lastUser.runId!) : undefined}
          onSaveAsSkill={stepsInLastRequest >= SKILL_MIN_STEPS ? onSaveAsSkill : undefined}
        />
      )}
    </div>
  );
}

export interface DeliveredFile {
  path: string;
  /** Where the runtime kept the file this one replaced, when it replaced one. */
  previous?: string;
}

/** The earlier version the runtime kept for `path`, read from the tool's stored result. */
function previousVersionOf(preview: string | undefined, path: string): string | undefined {
  if (!preview) return undefined;
  try {
    const artifacts = JSON.parse(preview)?.artifacts;
    if (!Array.isArray(artifacts)) return undefined;
    const match = artifacts.find((a) => a && typeof a.previous_version === "string" && (a.path === path || !a.path));
    return match?.previous_version;
  } catch {
    return undefined; // a live preview can be cut short; the stored result is complete
  }
}

/** Files the last request wrote or edited, newest last, one row per path. */
export function deliveredSinceUser(items: Item[]): DeliveredFile[] {
  let from = 0;
  items.forEach((it, i) => {
    if (it.kind === "user") from = i + 1;
  });
  const files: DeliveredFile[] = [];
  for (const it of items.slice(from)) {
    if (it.kind !== "tool" || !DELIVERY_TOOLS.has(it.name)) continue;
    if (it.status === "…" || /^(error|denied|failed)/i.test(it.status)) continue;
    const path = typeof it.args?.path === "string" ? it.args.path : "";
    if (!path) continue;
    const previous = previousVersionOf(it.preview, path);
    const seen = files.find((f) => f.path === path);
    if (seen) seen.previous = seen.previous ?? previous;
    else files.push({ path, ...(previous ? { previous } : {}) });
  }
  return files;
}

export const filesSinceUser = (items: Item[]): string[] =>
  deliveredSinceUser(items).map((f) => f.path);

const DELIVERY_TOOLS = new Set([
  "write_file",
  "create_document",
  "edit_document",
  "apply_patch",
  "apply_unified_diff",
  "replace_in_file",
]);

/** Tool calls the last request made: a request with a real workflow behind it is worth keeping. */
export function stepsSinceUser(items: Item[]): number {
  let from = 0;
  items.forEach((it, i) => {
    if (it.kind === "user") from = i + 1;
  });
  return items.slice(from).filter((it) => it.kind === "tool").length;
}

/** Fewest tool calls before "Save as skill" is offered. */
const SKILL_MIN_STEPS = 3;

/** What the finished request produced, with a way to open each file and to keep the method. */
function DeliveryCard({
  files,
  onSaveAsSkill,
  onRestoreVersion,
  onUndoAll,
}: {
  files: DeliveredFile[];
  onSaveAsSkill?: () => void;
  onRestoreVersion?: (backup: string, target: string) => Promise<string | null>;
  onUndoAll?: () => Promise<UndoFilesResult>;
}) {
  const { t } = useI18n();
  // Undoing every file is two steps (it removes files the request made), then says what it did.
  const [undoStep, setUndoStep] = useState<"idle" | "confirm" | "working" | "done">("idle");
  const [undoResult, setUndoResult] = useState<UndoFilesResult | null>(null);
  // path -> "restored" or an error message, once the user has tried to put the old version back.
  const [restored, setRestored] = useState<Record<string, string>>({});
  const baseName = (p: string) => p.replace(/\\/g, "/").split("/").pop() || p;
  return (
    <div className="delivery-card" data-testid="delivery-card">
      {files.length > 0 && (
        <div className="delivery-title">
          {t("transcript.deliveredTitle", { n: files.length }, `Files from this request (${files.length})`)}
        </div>
      )}
      {files.map(({ path, previous }) => (
        <div key={path} className="delivery-line">
          <button
            className="delivery-row"
            title={path}
            data-testid="delivery-open"
            onClick={() => window.dispatchEvent(new CustomEvent(OPEN_ARTIFACT_EVENT, { detail: { path } }))}
          >
            <Icon name="file" size={13} />
            <span className="min-w-0 truncate">{baseName(path)}</span>
            <span className="delivery-open">{t("transcript.deliveredOpen", undefined, "Preview")}</span>
          </button>
          {previous && onRestoreVersion && (
            <div className="delivery-replaced">
              <span>{t("transcript.replacedExisting", undefined, "This replaced an existing file.")}</span>
              {restored[path] === "ok" ? (
                <span data-testid="restore-done">{t("transcript.restored", undefined, "Earlier version restored")}</span>
              ) : (
                <button
                  className="delivery-restore"
                  data-testid="restore-version"
                  onClick={async () => {
                    const error = await onRestoreVersion(previous, path);
                    setRestored((cur) => ({ ...cur, [path]: error ?? "ok" }));
                  }}
                >
                  {t("transcript.restoreEarlier", undefined, "Restore the earlier version")}
                </button>
              )}
              {restored[path] && restored[path] !== "ok" && (
                <span className="text-danger" data-testid="restore-error">{restored[path]}</span>
              )}
            </div>
          )}
        </div>
      ))}
      {onUndoAll && files.length > 0 && (
        <div className="delivery-undo" data-testid="delivery-undo">
          {undoStep === "idle" && (
            <button
              className="delivery-skill"
              data-testid="undo-run-files"
              title={t("transcript.undoFilesHint", undefined, "Put back the files this request replaced and take out the ones it made. Nothing is deleted: Delta keeps them in the task folder's .delta/versions.")}
              onClick={() => setUndoStep("confirm")}
            >
              {t("transcript.undoFiles", undefined, "Undo these file changes")}
            </button>
          )}
          {undoStep === "confirm" && (
            <span className="delivery-undo-confirm">
              <span>{t("transcript.undoFilesConfirm", { n: files.length }, `Undo the changes to ${files.length} file(s)?`)}</span>
              <button
                className="delivery-skill"
                data-testid="undo-run-files-confirm"
                onClick={async () => {
                  setUndoStep("working");
                  const result = await onUndoAll().catch(
                    (): UndoFilesResult => ({ ok: false, error: "unreachable" }),
                  );
                  setUndoResult(result);
                  setUndoStep("done");
                }}
              >
                {t("transcript.undoFilesYes", undefined, "Undo")}
              </button>
              <button className="delivery-skill" data-testid="undo-run-files-cancel" onClick={() => setUndoStep("idle")}>
                {t("transcript.undoFilesNo", undefined, "Keep")}
              </button>
            </span>
          )}
          {undoStep === "working" && <span className="rail-muted">{t("common.loading")}</span>}
          {undoStep === "done" && undoResult && (
            <span data-testid="undo-run-files-result" className={undoResult.ok ? "delivery-undo-ok" : "text-danger"}>
              {undoResult.ok
                ? t(
                    "transcript.undoFilesDone",
                    {
                      restored: undoResult.restored?.length ?? 0,
                      removed: undoResult.removed?.length ?? 0,
                      skipped: undoResult.skipped?.length ?? 0,
                    },
                    "Restored {restored}, removed {removed}; {skipped} left alone because they changed since.",
                  )
                : undoResult.error || t("transcript.restoreFailed")}
            </span>
          )}
        </div>
      )}
      {onSaveAsSkill && (
        <button
          className="delivery-skill"
          data-testid="save-as-skill"
          title={t("transcript.saveAsSkillHint", undefined, "Delta writes up how it did this as a reusable skill. Nothing is saved until you approve it.")}
          onClick={onSaveAsSkill}
        >
          {t("transcript.saveAsSkill", undefined, "Save as skill")}
        </button>
      )}
    </div>
  );
}
