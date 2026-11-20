import { useId } from "react";
import { Icon } from "./Icon";
import { useOptionList } from "./ui/useOptionList";

export interface Option {
  value: string;
  label: string;
  description?: string;
}

interface Props {
  prefix?: string;
  value: string;
  options: Option[];
  onChange: (value: string) => void;
  align?: "left" | "right";
  className?: string;
}

export function Dropdown({ prefix, value, options, onChange, align = "left", className }: Props) {
  const selected = options.findIndex((option) => option.value === value);
  const menu = useOptionList(options.length, selected);
  const menuId = useId();
  const current = options[selected];
  const label = (prefix ? `${prefix}: ` : "") + (current?.label || value);

  return (
    <div className="dd">
      <button
        ref={menu.triggerRef}
        type="button"
        className={"pill" + (className ? " " + className : "")}
        aria-haspopup="menu"
        aria-controls={menu.isOpen ? menuId : undefined}
        aria-expanded={menu.isOpen}
        aria-label={label}
        title={label}
        onClick={menu.toggle}
        onKeyDown={menu.onTriggerKey}
      >
        <span className="pill-label">{label}</span>
        <Icon name="chevronDown" size={13} className="caret" />
      </button>
      {menu.isOpen && (
        <>
          <div className="dd-backdrop" onClick={() => menu.close()} />
          <div
            ref={menu.listRef}
            id={menuId}
            role="menu"
            className={"dd-menu ds-menu " + align}
            onKeyDown={menu.onListKey}
          >
            {options.map((option, index) => {
              const active = option.value === value;
              return (
                <button
                  key={option.value}
                  type="button"
                  data-ds-option
                  role="menuitemradio"
                  aria-checked={active}
                  tabIndex={menu.focused === index ? 0 : -1}
                  className={"dd-item ds-menu-item" + (active ? " sel" : "")}
                  onFocus={() => menu.setFocused(index)}
                  onClick={() => {
                    onChange(option.value);
                    menu.close(true);
                  }}
                >
                  <span className="dd-label">
                    {option.label}
                    {active && <span className="chk" aria-hidden="true">✓</span>}
                  </span>
                  {option.description && <span className="dd-desc">{option.description}</span>}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
