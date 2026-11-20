import { useId } from "react";
import { Icon } from "./Icon";
import { useOptionList } from "./ui/useOptionList";

export interface SelectMenuOption {
  value: string;
  label: string;
  sub?: string;
  dot?: boolean;
  group?: string;
}

export function SelectMenu({
  value,
  options,
  onChange,
  ariaLabel,
}: {
  value: string;
  options: SelectMenuOption[];
  onChange: (value: string) => void;
  ariaLabel: string;
}) {
  const menu = useOptionList(options.length, options.findIndex((option) => option.value === value));
  const id = useId();
  const current = options.find((option) => option.value === value);

  return (
    <div className="relative">
      <button
        ref={menu.triggerRef}
        type="button"
        className="ds-field w-full flex items-center gap-2"
        aria-haspopup="listbox"
        aria-controls={menu.isOpen ? id : undefined}
        aria-expanded={menu.isOpen}
        aria-label={ariaLabel}
        onClick={menu.toggle}
        onKeyDown={menu.onTriggerKey}
      >
        <span className="flex-1 min-w-0 text-left truncate">{current?.label || value}</span>
        {current?.dot && <span className="w-1.5 h-1.5 rounded-full bg-ok shrink-0" />}
        <Icon name="chevronDown" size={13} className="text-faint shrink-0" />
      </button>
      {menu.isOpen && (
        <>
          <div className="fixed inset-0 z-[190]" onClick={() => menu.close()} />
          <div
            ref={menu.listRef}
            id={id}
            role="listbox"
            aria-label={ariaLabel}
            className="ds-menu absolute z-[200] left-0 right-0 mt-1"
            onKeyDown={menu.onListKey}
          >
            {options.map((option, index) => {
              const selected = option.value === value;
              return (
                <div key={option.value}>
                  {option.group && option.group !== options[index - 1]?.group && (
                    <div role="presentation" className="px-2.5 pt-2.5 pb-1 text-[11px] font-semibold text-faint">
                      {option.group}
                    </div>
                  )}
                  <button
                    type="button"
                    data-ds-option
                    role="option"
                    aria-selected={selected}
                    tabIndex={menu.focused === index ? 0 : -1}
                    className="ds-menu-item"
                    onFocus={() => menu.setFocused(index)}
                    onClick={() => {
                      onChange(option.value);
                      menu.close(true);
                    }}
                  >
                    <span className="w-full flex items-center gap-2">
                      <span className="flex-1 text-[13px] font-medium truncate">{option.label}</span>
                      {selected && <span className="text-primary shrink-0" aria-hidden="true">✓</span>}
                      {option.dot && <span className="w-1.5 h-1.5 rounded-full bg-ok shrink-0" title="Key set" />}
                    </span>
                    {option.sub && <span className="text-[11.5px] text-muted truncate">{option.sub}</span>}
                  </button>
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
