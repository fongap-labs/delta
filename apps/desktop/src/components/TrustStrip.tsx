/**
 * Trust strip under an artifact's header: what the runtime recorded about how the file was made.
 * Fixed order — validation verdict, producer, checksum — and a fact that was not recorded is
 * left out rather than shown as "unknown". Status is always icon + words, never colour alone.
 */
import { useState } from "react";
import { useI18n } from "@delta/i18n/I18nContext";
import { summarizeChecks, type ArtifactTrust } from "../artifactTrust";
import { Icon } from "./Icon";

const CHIP = "inline-flex items-center gap-1.5 h-7 px-2.5 rounded-lg text-[12px] font-medium border";

export function TrustStrip({ trust }: { trust: ArtifactTrust }) {
  const { t } = useI18n();
  const [isOpen, setOpen] = useState(false);
  const summary = summarizeChecks(trust);
  if (!summary && !trust.producedBy && !trust.sha256) return null;

  const isOk = !!trust.isValidationOk;
  const shortHash = trust.sha256 ? trust.sha256.slice(0, 8) : "";

  return (
    <div className="px-[18px] py-2.5 border-b border-line" data-testid="trust-strip">
      <div className="flex flex-wrap items-center gap-2" role="list" aria-label={t("trust.list", undefined, "Trust details")}>
        {summary && (
          <button
            type="button"
            role="listitem"
            aria-expanded={isOpen}
            data-testid="trust-verdict"
            data-verdict={isOk ? "ok" : "failed"}
            className={
              CHIP + " cursor-pointer " +
              (isOk ? "bg-ok-soft text-ok border-ok-line" : "bg-danger-soft text-danger border-danger-line")
            }
            onClick={() => setOpen((value) => !value)}
          >
            <Icon name={isOk ? "shield" : "failCircle"} size={14} />
            {isOk
              ? t("trust.checksPassed", { passed: summary.passed, total: summary.total }, `${summary.passed} of ${summary.total} checks passed`)
              : t("trust.checksFailed", { failed: summary.total - summary.passed, total: summary.total }, `${summary.total - summary.passed} of ${summary.total} checks failed`)}
            <Icon name={isOpen ? "chevronDown" : "chevronRight"} size={12} />
          </button>
        )}
        {trust.producedBy && (
          <span role="listitem" className={CHIP + " bg-surface-2 text-ink border-transparent"} data-testid="trust-producer">
            {t("trust.madeBy", { tool: trust.producedBy }, `Made by ${trust.producedBy}`)}
          </span>
        )}
        {trust.sha256 && (
          <span
            role="listitem"
            className={CHIP + " bg-surface-2 text-muted border-transparent font-mono"}
            data-testid="trust-checksum"
            title={t("trust.checksumTitle", { hash: trust.sha256 }, `SHA-256: ${trust.sha256}`)}
          >
            {t("trust.checksum", { hash: shortHash }, `SHA-256 ${shortHash}`)}
          </span>
        )}
      </div>
      {isOpen && trust.checks && (
        <ul className="mt-2.5 flex flex-col gap-1.5" data-testid="trust-checks">
          {trust.checks.map((check) => (
            <li key={check.name} className="flex items-start gap-2 text-[12.5px]">
              <span
                className={"mt-0.5 shrink-0 " + (check.isOk ? "text-ok" : "text-danger")}
                role="img"
                aria-label={check.isOk ? t("trust.checkPassed", undefined, "Passed") : t("trust.checkFailed", undefined, "Failed")}
              >
                <Icon name={check.isOk ? "check" : "failCircle"} size={14} />
              </span>
              <span className="min-w-0">
                <span className="font-medium">{check.name}</span>
                {check.detail && <span className="text-muted"> · {check.detail}</span>}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
