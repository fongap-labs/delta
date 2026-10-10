// Escape closes the top layer it belongs to — and only that. Each surface that registers this
// hook decides for itself whether Escape is its business; there is no global stack.
//
// Escape is deliberately NOT ours when:
//  - the key is part of an input-method composition (`isComposing` / keyCode 229): for Chinese,
//    Japanese and Korean input it cancels the candidate text, and swallowing it would eat the
//    user's composition;
//  - another handler already used it (`defaultPrevented`): the composer clears its slash menu and
//    cancels dictation on Escape, a rename field cancels its edit, and so on;
//  - (optional) the focus is in a text field, where Escape belongs to the field.
import { useEffect } from "react";

const FIELD_TAGS = new Set(["INPUT", "TEXTAREA", "SELECT"]);

export function shouldDismissOnEscape(
  event: Pick<KeyboardEvent, "key" | "defaultPrevented" | "isComposing" | "keyCode" | "target">,
  options: { shouldIgnoreFields?: boolean } = {},
): boolean {
  if (event.key !== "Escape") return false;
  if (event.defaultPrevented || event.isComposing || event.keyCode === 229) return false;
  if (options.shouldIgnoreFields) {
    const target = event.target as HTMLElement | null;
    if (target && (FIELD_TAGS.has(target.tagName) || target.isContentEditable)) return false;
  }
  return true;
}

/** While `isActive`, Escape calls `onDismiss` (and is marked handled so lower layers skip it). */
export function useEscapeDismiss(
  isActive: boolean,
  onDismiss: () => void,
  options: { shouldIgnoreFields?: boolean } = {},
) {
  const { shouldIgnoreFields } = options;
  useEffect(() => {
    if (!isActive) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (!shouldDismissOnEscape(event, { shouldIgnoreFields })) return;
      event.preventDefault();
      onDismiss();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isActive, onDismiss, shouldIgnoreFields]);
}
