import type { KeyboardEvent } from "react";

export interface SegmentOption<Value extends string> {
  value: Value;
  label: string;
  testId?: string;
}

export function SegmentedControl<Value extends string>({
  label,
  value,
  options,
  onChange,
  className = "",
  testId,
}: {
  label: string;
  value: Value;
  options: readonly SegmentOption<Value>[];
  onChange: (value: Value) => void;
  className?: string;
  testId?: string;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const current = options.findIndex((option) => option.value === value);
    if (current === -1 || options.length < 2) return;

    let next = current;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (current + 1) % options.length;
    else if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = (current - 1 + options.length) % options.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = options.length - 1;
    else return;

    event.preventDefault();
    const target = options[next];
    onChange(target.value);
    event.currentTarget.querySelectorAll<HTMLButtonElement>("button")[next]?.focus();
  };

  return (
    <div
      className={`seg ds-segmented ${className}`.trim()}
      role="group"
      aria-label={label}
      data-testid={testId}
      onKeyDown={onKeyDown}
    >
      {options.map((option) => {
        const selected = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            data-testid={option.testId}
            className={selected ? "active" : ""}
            aria-pressed={selected}
            tabIndex={selected ? 0 : -1}
            onClick={() => onChange(option.value)}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
