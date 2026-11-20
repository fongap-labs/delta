import { useEffect, useRef, useState, type KeyboardEvent } from "react";

// Shared roving focus for the option menus used by model and settings controls.
export function useOptionList(count: number, selected: number) {
  const [isOpen, setOpen] = useState(false);
  const [focused, setFocused] = useState(0);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const show = (index = selected) => {
    setFocused(Math.max(0, Math.min(index, count - 1)));
    setOpen(true);
  };
  const close = (shouldRestoreFocus = false) => {
    setOpen(false);
    if (shouldRestoreFocus) triggerRef.current?.focus();
  };
  const toggle = () => (isOpen ? close() : show());

  useEffect(() => {
    if (!isOpen) return;
    listRef.current?.querySelectorAll<HTMLElement>("[data-ds-option]")[focused]?.focus();
  }, [focused, isOpen]);

  const onTriggerKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      show(event.key === "ArrowDown" ? 0 : count - 1);
    }
  };
  const onListKey = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close(true);
      return;
    }
    if (event.key === "Tab") {
      close();
      return;
    }
    let next: number;
    if (event.key === "ArrowDown") next = (focused + 1) % count;
    else if (event.key === "ArrowUp") next = (focused - 1 + count) % count;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = count - 1;
    else return;
    if (count === 0) return;
    event.preventDefault();
    setFocused(next);
  };
  return { isOpen, focused, triggerRef, listRef, show, close, toggle, onTriggerKey, onListKey, setFocused };
}
