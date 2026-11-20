import { forwardRef, type HTMLAttributes } from "react";

interface DialogSurfaceProps extends HTMLAttributes<HTMLDivElement> {
  label?: string;
  labelledBy?: string;
}

// Only the visual shell is shared; approval and search retain their dismissal policies.
export const DialogSurface = forwardRef<HTMLDivElement, DialogSurfaceProps>(function DialogSurface(
  { label, labelledBy, className = "", children, ...rest },
  ref,
) {
  return (
    <div
      {...rest}
      ref={ref}
      role="dialog"
      aria-modal="true"
      aria-label={label}
      aria-labelledby={labelledBy}
      className={`ds-dialog-surface ${className}`.trim()}
    >
      {children}
    </div>
  );
});
