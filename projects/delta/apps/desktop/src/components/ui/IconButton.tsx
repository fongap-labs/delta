import type { ButtonHTMLAttributes } from "react";
import { Icon, type IconName } from "../Icon";

interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  icon: IconName;
  label: string;
  iconSize?: number;
}

// All icon-only buttons must have an accessible name.
export function IconButton({ icon, label, iconSize = 16, className = "", title, ...rest }: IconButtonProps) {
  return (
    <button
      type="button"
      {...rest}
      className={`ds-icon-button ${className}`.trim()}
      aria-label={label}
      title={title ?? label}
    >
      <Icon name={icon} size={iconSize} />
    </button>
  );
}
