import { useRef, useState } from "react";
import { setWorkspaceTrusted, type WorkspaceCommandTrust } from "../api";
import { useI18n } from "@delta/i18n/I18nContext";
import { Button } from "./ui/Button";

export function WorkspaceTrustPrompt({
  request,
  onClose,
}: {
  request: WorkspaceCommandTrust;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [isSaving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const pending = useRef(false);

  const trust = async () => {
    if (pending.current) return;
    pending.current = true;
    setSaving(true);
    setError("");
    try {
      const result = await setWorkspaceTrusted(request.workspace, true).catch(() => null);
      if (!result?.ok) {
        setError(result?.error || t("workspace.errors.saveTrust", undefined, "Could not save workspace trust."));
        return;
      }
      onClose();
    } finally {
      pending.current = false;
      setSaving(false);
    }
  };

  return (
    <div className="gate-overlay" role="dialog" aria-modal="true" aria-labelledby="workspace-trust-title" aria-describedby="workspace-trust-description">
      <div className="gate max-w-[560px]">
        <div className="gate-mark">✦</div>
        <h2 id="workspace-trust-title">{t("workspace.trust.title", undefined, "Trust this workspace’s commands?")}</h2>
        <p id="workspace-trust-description" className="gate-sub">
          {t("workspace.trust.sub", undefined, "This project asks Delta to run the commands below without individual approval. Trust applies to future configuration changes at this exact folder until you revoke it in Settings.")}
        </p>
        <div className="rounded-lg border border-line bg-bg px-3 py-2.5 max-h-48 overflow-y-auto">
          {request.requested_commands.map((command) => (
            <code key={command} className="block text-[12.5px] py-1 text-ink">
              {command}
            </code>
          ))}
        </div>
        <div className="text-[11.5px] text-muted mt-2 break-all">{request.workspace}</div>
        {error && <div className="gate-error" role="alert">{error}</div>}
        <div className="gate-foot justify-end gap-2">
          <Button size="sm" onClick={onClose} disabled={isSaving}>
            {t("workspace.trust.keepAsking", undefined, "Keep asking")}
          </Button>
          <Button variant="primary" size="sm" onClick={() => void trust()} disabled={isSaving}
            aria-busy={isSaving}
            aria-label={t("workspace.trust.trustWorkspace", undefined, "Trust workspace")}>
            {isSaving ? t("workspace.trust.saving", undefined, "Saving…") : t("workspace.trust.trustWorkspace", undefined, "Trust workspace")}
          </Button>
        </div>
      </div>
    </div>
  );
}
