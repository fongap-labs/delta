/**
 * Run status — one visual language for "what is this run doing".
 *
 * Six states, each with its own icon silhouette AND word, so status never relies on colour alone
 * (the success green is almost the same lightness as the brand teal). "Running" deliberately uses
 * neutral ink, not teal: teal means "you can act on this", and a running run needs no action.
 *
 * The desktop only receives a boolean plus a label derived from stream events today, so the bar
 * renders `running`; the other states are defined here so the pause / resume / verified surfaces
 * can adopt them once the runtime's public contract reports them. It does not fabricate a
 * timeline or infer a state it was not given.
 */
import { useI18n } from "@delta/i18n/I18nContext";
import { Icon, type IconName } from "./Icon";

export type RunState = "running" | "awaiting" | "paused" | "cancelled" | "failed" | "done" | "verified";

interface RunStateStyle {
  icon: IconName;
  /** CSS colour for the icon + label. */
  color: string;
  /** Rotate the icon (disabled under prefers-reduced-motion in styles.css). */
  spin?: boolean;
  fallback: string;
}

export const RUN_STATES: Record<RunState, RunStateStyle> = {
  running: { icon: "spinner", color: "var(--ink)", spin: true, fallback: "Running" },
  awaiting: { icon: "hand", color: "var(--warn)", fallback: "Awaiting approval" },
  paused: { icon: "pauseCircle", color: "var(--muted)", fallback: "Paused" },
  cancelled: { icon: "cancelCircle", color: "var(--muted)", fallback: "Cancelled" },
  failed: { icon: "failCircle", color: "var(--danger)", fallback: "Failed" },
  done: { icon: "doneCircle", color: "var(--ink)", fallback: "Completed" },
  verified: { icon: "shield", color: "var(--ok)", fallback: "Completed and verified" },
};

export interface RunStatusProps {
  /** What the run is doing right now (derived from live events). */
  status: string;
  active: boolean;
  state?: RunState;
}

export function RunStatusBar({ status, active, state = "running" }: RunStatusProps) {
  const { t } = useI18n();
  if (!active) return null;
  const s = RUN_STATES[state];

  return (
    <div
      className="run-status-bar max-w-3xl mx-auto mb-2 flex items-center gap-2.5 h-11 px-3.5 rounded-[14px] border border-line bg-panel text-[13px]"
      role="status"
      aria-live="polite"
      data-testid="run-status-bar"
      data-run-state={state}
    >
      <span className={"flex shrink-0" + (s.spin ? " run-spin" : "")} style={{ color: s.color }}>
        <Icon name={s.icon} size={16} />
      </span>
      <span className="font-semibold shrink-0" style={{ color: s.color }}>
        {t(`run.state.${state}`, undefined, s.fallback)}
      </span>
      <span className="min-w-0 truncate text-muted">{status}</span>
    </div>
  );
}
