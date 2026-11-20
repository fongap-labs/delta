import { forwardRef, type InputHTMLAttributes } from "react";

type FieldInputProps = InputHTMLAttributes<HTMLInputElement> & {
  /** Use inside a shared ds-focus-surface (search, composer) to avoid nested focus borders. */
  appearance?: "default" | "embedded";
};

// Shared native input surface: validation, labels and change handling remain with the caller.
export const FieldInput = forwardRef<HTMLInputElement, FieldInputProps>(function FieldInput(
  { className = "", appearance = "default", ...props },
  ref,
) {
  const surfaceClass = appearance === "embedded" ? "ds-field-embedded" : "ds-field";
  return <input {...props} ref={ref} className={`${surfaceClass} ${className}`.trim()} />;
});
