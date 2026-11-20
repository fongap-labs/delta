import { forwardRef, type ButtonHTMLAttributes } from "react";

type ButtonVariant = "default" | "primary" | "ghost" | "danger";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: "default" | "sm";
}

// Only the visual contract is shared; callers retain approval and navigation behavior.
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "default", size = "default", className = "", type = "button", ...props },
  ref,
) {
  const variants: Record<ButtonVariant, string> = {
    default: "",
    primary: "ds-button-primary",
    ghost: "ds-button-ghost",
    danger: "ds-button-danger",
  };
  return (
    <button
      {...props}
      ref={ref}
      type={type}
      className={["ds-button", variants[variant], size === "sm" ? "ds-button-sm" : "", className].filter(Boolean).join(" ")}
    />
  );
});
