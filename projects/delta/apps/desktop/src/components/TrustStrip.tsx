/**
 * Trust strip under an artifact's header: what the runtime recorded about how the file was made.
 * Fixed order — validation verdict, producer, checksum — and a fact that was not recorded is
 * left out rather than shown as "unknown". Status is always icon + words, never colour alone.
 */
import { useState } from "react";
import { useI18n } from "@delta/i18n/I18nContext";
import type { RunSource } from "../api";
import { displaySourceLocation, summarizeChecks, type ArtifactTrust } from "../artifactTrust";
import { Icon } from "./Icon";

const CHIP = "inline-flex items-center gap-1.5 h-7 px-2.5 rounded-lg text-[12px] font-medium border";

export function TrustStrip({ trust, sources = [] }: { trust: ArtifactTrust; sources?: RunSource[] }) {
  const { t } = useI18n();
  const [isOpen, setOpen] = useState(false);
  const [isSourceListOpen, setSourceListOpen] = useState(false);
  const summary = summarizeChecks(trust);
  if (!summary && !trust.producedBy && !trust.sha256 && sources.length === 0) return null;

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
        {sources.length > 0 && (
          <button
            type="button"
            role="listitem"
            aria-expanded={isSourceListOpen}
            data-testid="trust-sources"
            className={CHIP + " cursor-pointer bg-surface-2 text-ink border-transparent"}
            onClick={() => setSourceListOpen((value) => !value)}
          >
            <Icon name="book" size={14} />
            {sources.length === 1
              ? t("trust.sourcesReadOne", undefined, "Read 1 source in this run")
              : t("trust.sourcesRead", { count: sources.length }, `Read ${sources.length} sources in this run`)}
            <Icon name={isSourceListOpen ? "chevronDown" : "chevronRight"} size={12} />
          </button>
        )}
      </div>
      {isSourceListOpen && sources.length > 0 && (
        <ul className="mt-2.5 flex flex-col gap-1.5" data-testid="trust-source-list">
          {sources.map((source) => (
            <li key={source.source_id} className="flex items-start gap-2 text-[12.5px]">
              <span className="mt-0.5 shrink-0 text-muted">
                <Icon name="file" size={14} />
              </span>
              <span className="min-w-0 break-all">
                <span className="font-mono" title={source.location}>{displaySourceLocation(source.location)}</span>
                <span className="text-muted font-mono"> · {source.fingerprint.slice(0, 8)}</span>
                {source.cited && <span className="text-muted"> · {t("trust.sourceCited", undefined, "Cited")}</span>}
              </span>
            </li>
          ))}
        </ul>
      )}
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
