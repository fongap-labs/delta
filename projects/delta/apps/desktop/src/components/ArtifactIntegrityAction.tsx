import { useRef, useState } from "react";
import { useI18n } from "@delta/i18n/I18nContext";
import { verifyArtifactIntegrity, type ArtifactIntegrityStatus } from "../api";
import { Button } from "./ui/Button";

type CheckState = "idle" | "checking" | ArtifactIntegrityStatus | "unavailable";

/**
 * An explicit, read-only point-in-time comparison. The check never authorizes delivery.
 * Remount per session and artifact path so earlier results cannot leak between files.
 */
export function ArtifactIntegrityAction({ sessionId, path }: { sessionId: string; path: string }) {
  const { t } = useI18n();
  const [state, setState] = useState<CheckState>("idle");
  const run = useRef(0);
  const check = async () => {
    const id = ++run.current;
    setState("checking");
    try {
      const result = await verifyArtifactIntegrity(sessionId, path);
      if (id !== run.current) return;
      const valid = ["matched", "changed", "unrecorded", "unsupported"];
      const next = result.ok && result.path === path && valid.includes(result.status || "")
        ? result.status as ArtifactIntegrityStatus : "unavailable";
      setState(next);
    } catch {
      if (id === run.current) setState("unavailable");
    }
  };
  const message: Record<CheckState, string> = {
    idle: t("artifacts.integrityIdle", undefined, "Not checked"),
    checking: t("artifacts.reviewWaiting", undefined, "Checking…"),
    matched: t("artifacts.integrityMatched", undefined, "Matches recorded SHA-256 at check time"),
    changed: t("artifacts.integrityChanged", undefined, "File differs from recorded SHA-256"),
    unrecorded: t("artifacts.integrityUnrecorded", undefined, "No usable recorded SHA-256"),
    unsupported: t("artifacts.integrityUnsupported", undefined, "This file type cannot be compared"),
    unavailable: t("artifacts.reviewUnavailable", undefined, "Unavailable"),
  };
  return (
    <div className="flex flex-wrap justify-between items-center gap-2" data-testid="artifact-integrity-check">
      <div className="min-w-0 flex-1">
        <div className="text-muted text-[11px]">
          {t("artifacts.integrityLabel", undefined, "Current-file integrity")}
        </div>
        <div data-testid="artifact-integrity-status" data-state={state}
          className={"text-[11.5px] " + (state === "changed" ? "text-danger" : "text-muted")}>
          {message[state]}
        </div>
      </div>
      <Button size="sm" variant="ghost" disabled={state === "checking"}
        onClick={() => { void check(); }}>
        {t("artifacts.integrityVerify", undefined, "Check now")}
      </Button>
    </div>
  );
}
