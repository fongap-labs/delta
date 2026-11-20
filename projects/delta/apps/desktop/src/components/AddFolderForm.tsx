import { useState } from "react";
import { chooseFolder } from "../tauri";
import { Icon } from "./Icon";
import { useI18n } from "@delta/i18n/I18nContext";
import { Button } from "./ui/Button";
import { FieldInput } from "./ui/FieldInput";
import { IconButton } from "./ui/IconButton";

// A single "Give access to a folder" affordance. Collapsed it's one button; expanded it's a path
// field (Browse on desktop, paste anywhere) + an "Allow writing" checkbox that's OFF by default —
// so access is read-only unless explicitly granted. Used by the composer chip and the start panel.
export function AddFolderForm({
  onAdd,
  busy,
  compact,
  startOpen,
  onDismiss,
}: {
  onAdd: (path: string, isWritable: boolean) => Promise<boolean> | boolean | void;
  busy?: boolean;
  compact?: boolean;
  // Render the form expanded immediately (the caller owns the trigger); Cancel/success then
  // notify via onDismiss so the caller can collapse it.
  startOpen?: boolean;
  onDismiss?: () => void;
}) {
  const { t } = useI18n();
  const [open, setOpen] = useState(!!startOpen);
  const [path, setPath] = useState("");
  const [isWritable, setWritable] = useState(false);

  const reset = () => {
    setOpen(false);
    setPath("");
    setWritable(false);
    onDismiss?.();
  };

  const browse = async () => {
    const p = await chooseFolder();
    if (p) setPath(p);
  };

  const submit = async () => {
    // The submit handler must enforce the disabled state even for Enter key submissions.
    if (busy || !path.trim()) return;
    const ok = await onAdd(path.trim(), isWritable);
    if (ok !== false) reset();
  };

  if (!open) {
    return (
      <Button variant="ghost" className={"addfolder-trigger" + (compact ? " compact" : "")} onClick={() => setOpen(true)}
        aria-expanded={false}>
        <Icon name="folderPlus" size={15} /> {t("folder.giveAccess", undefined, "Give access to a folder")}
      </Button>
    );
  }

  return (
    <form className="addfolder-form" data-testid="folder-access-form" onSubmit={(event) => {
      event.preventDefault();
      void submit();
    }}>
      <div className="addfolder-row">
        <FieldInput
          className="addfolder-path"
          aria-label={t("folder.pathPlaceholder", undefined, "Choose or paste a folder path…")}
          autoFocus
          placeholder={t("folder.pathPlaceholder", undefined, "Choose or paste a folder path…")}
          value={path}
          spellCheck={false}
          onChange={(e) => setPath(e.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") reset();
          }}
        />
        <IconButton icon="folder" iconSize={15} className="ds-button"
          onClick={() => void browse()}
          label={t("folder.chooseLocation", undefined, "Choose location")}
        />
      </div>
      <div className="addfolder-actions">
        <label className="addfolder-write" title={t("folder.writeHint", undefined, "Off = read-only. Tick to let the agent write here.")}>
          <input type="checkbox" checked={isWritable} onChange={(e) => setWritable(e.target.checked)} />
          {t("folder.allowWrites", undefined, "Allow writes")}
        </label>
        <span className="spacer" />
        <Button size="sm" onClick={reset}>
          {t("common.cancel", undefined, "Cancel")}
        </Button>
        <Button variant="primary" size="sm" type="submit" disabled={busy || !path.trim()} aria-busy={!!busy}>
          {t("common.add", undefined, "Add")}
        </Button>
      </div>
    </form>
  );
}
