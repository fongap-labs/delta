// Approval dialog (design 2:604). Opens only for the consequential cases: an outbound action
// while the default egress level is "Ask each time", a connector write, and any approval the user
// asked to confirm in a dialog. Every other approval keeps its inline card. The decision and the
// copy are the card's own (ApprovalCard exports), so the two never disagree.
import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { getEgressSettings, type EgressLevel } from "../api";
import { humanizeApprovalTitle } from "../humanize";
import { useEscapeDismiss } from "../useEscapeDismiss";
import type { ApprovalDecision, Item } from "../types";
import {
  ConfirmCheck,
  PreviewBlock,
  SaveSkillPreview,
  TitleText,
  approvalActionLabels,
  scopeNote,
  shortArgs,
} from "./ApprovalCard";
import { Icon } from "./Icon";
import { useI18n } from "@delta/i18n/I18nContext";

type ApprovalItem = Extract<Item, { kind: "approval" }>;

// Outbound actions: the same set ApprovalCard treats as leaving the computer.
const OUTBOUND = new Set(["send_message", "send_file"]);
const FILE_WRITES = new Set(["write_file", "replace_in_file", "apply_patch", "apply_unified_diff"]);
// A scheduled-task approval carries §25 grant lines the dialog does not draw; it keeps its card.
const GRANT_CARD = "create_scheduled_task";

const PREF_KEY = "delta-approval-dialog";
const PREF_EVENT = "delta:approval-dialog-pref";

/** The "confirm important actions in a dialog" preference. Per device, like the theme. Off by default. */
export function readImportantOpsPref(): boolean {
  try {
    return localStorage.getItem(PREF_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeImportantOpsPref(isOn: boolean) {
  try {
    localStorage.setItem(PREF_KEY, isOn ? "1" : "0");
  } catch {
    /* best effort: the choice then lasts for this run only */
  }
  window.dispatchEvent(new Event(PREF_EVENT));
}

export function useImportantOpsPref(): [boolean, (isOn: boolean) => void] {
  const [isOn, setOn] = useState(readImportantOpsPref);
  useEffect(() => {
    const sync = () => setOn(readImportantOpsPref());
    window.addEventListener(PREF_EVENT, sync);
    return () => window.removeEventListener(PREF_EVENT, sync);
  }, []);
  return [isOn, writeImportantOpsPref];
}

/** The workspace default egress level, re-read whenever `refreshKey` changes (e.g. after Settings). */
export function useDefaultEgressLevel(refreshKey: unknown): EgressLevel | null {
  const [level, setLevel] = useState<EgressLevel | null>(null);
  useEffect(() => {
    getEgressSettings()
      .then((settings) => setLevel(settings.default_level))
      .catch(() => setLevel(null));
  }, [refreshKey]);
  return level;
}

/** Whether this approval gets the dialog rather than the inline card. */
export function usesApprovalModal(
  item: ApprovalItem,
  { egressLevel, importantOps }: { egressLevel: EgressLevel | null; importantOps: boolean },
): boolean {
  if (OUTBOUND.has(item.name) && egressLevel === "ask_each_time") return true;
  if (item.category === "connector") return true;
  return importantOps && item.name !== GRANT_CARD;
}

/** Mirrors the card's session-wide "always" rule: outbound, connector and shell/skill approvals never get one. */
export function canRememberChoice(item: ApprovalItem): boolean {
  return item.category !== "connector" && !OUTBOUND.has(item.name) && item.name !== "run_shell" && item.name !== "save_skill";
}

const FOCUSABLE = 'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])';

// Keeps Tab and Shift+Tab inside the dialog while it is open.
function keepFocusInside(event: KeyboardEvent<HTMLElement>) {
  if (event.key !== "Tab") return;
  const nodes = Array.from(event.currentTarget.querySelectorAll<HTMLElement>(FOCUSABLE));
  if (!nodes.length) return;
  const first = nodes[0];
  const last = nodes[nodes.length - 1];
  if (event.shiftKey && (document.activeElement === first || document.activeElement === event.currentTarget)) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

export function ApprovalModal({ item, onApprove }: { item: ApprovalItem; onApprove: (decision: ApprovalDecision) => void }) {
  const { t } = useI18n();
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const [isRemember, setRemember] = useState(false);
  const [isConfirmed, setConfirmed] = useState(false);
  const scope = scopeNote(t, item.name, item.args, item.category);
  const locked = scope.external && !isConfirmed;
  const canRemember = canRememberChoice(item);
  const labels = approvalActionLabels(t, item.name);
  const content = typeof item.args?.content === "string" ? item.args.content : "";
  const deny = () => onApprove("deny");

  // Escape and a click on the scrim both refuse; nothing is approved by accident.
  useEscapeDismiss(true, deny);

  // Focus moves into the dialog on open and goes back to where it came from on close.
  useEffect(() => {
    const returnTo = document.activeElement as HTMLElement | null;
    dialogRef.current?.focus();
    return () => returnTo?.focus?.();
  }, []);

  return (
    <div
      className="approval-modal-scrim"
      data-testid="approval-modal-scrim"
      onClick={(event) => {
        if (event.target === event.currentTarget) deny();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className="approval approval-modal"
        data-testid="approval-modal"
        onKeyDown={keepFocusInside}
      >
        <div className="approval-top">
          <div className="approval-heading">
            <span className="approval-ico" title={t("approval.toolTitle", { name: item.name })}>
              <Icon name="shield" size={15} />
            </span>
            <span id={titleId}>
              <TitleText line={humanizeApprovalTitle(item.name, item.args)} />
            </span>
          </div>
        </div>
        <span className={"approval-scope" + (scope.external ? " out" : "")}>{scope.text}</span>

        {item.name === "run_shell" && item.args?.command && <PreviewBlock text={String(item.args.command)} />}
        {FILE_WRITES.has(item.name) && content && <PreviewBlock text={content} />}
        {item.name === "send_message" && item.args?.text && <PreviewBlock text={String(item.args.text)} mono={false} />}
        {item.name === "save_skill" && <SaveSkillPreview args={item.args} />}
        {!FILE_WRITES.has(item.name) && !["run_shell", "send_message", "save_skill"].includes(item.name) && shortArgs(item.args) && (
          <div className="approval-rest">{shortArgs(item.args)}</div>
        )}

        {scope.external && <ConfirmCheck checked={isConfirmed} onChange={setConfirmed} />}
        {canRemember && (
          <label className="approval-confirm" data-testid="approval-remember">
            <input type="checkbox" checked={isRemember} onChange={(event) => setRemember(event.target.checked)} />
            {t("approval.modal.remember")}
          </label>
        )}

        <div className="approval-btns">
          <button className="btn approval-primary" disabled={locked} onClick={() => onApprove(isRemember && canRemember ? "always_tool" : "once")}>
            {labels.allow}
          </button>
          <span className="spacer" />
          <button className="btn quiet-deny" onClick={deny}>
            {labels.deny}
          </button>
        </div>
      </div>
    </div>
  );
}
