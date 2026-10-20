import { cn } from "@heroui/react";
import type { ComponentType, ReactNode } from "react";

/** Dashed drop area that also opens the file picker on click or Enter. */
export function Dropzone({
  icon: Icon,
  title,
  hint,
  action,
  onActivate,
  className,
}: {
  icon: ComponentType<{ className?: string }>;
  title: ReactNode;
  hint?: ReactNode;
  action?: ReactNode;
  onActivate: () => void;
  className?: string;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onActivate}
      onKeyDown={(e) => e.key === "Enter" && onActivate()}
      className={cn(
        "group flex cursor-pointer flex-col items-center justify-center gap-3 rounded-xl border-[1.5px] border-dashed border-border-secondary bg-surface-secondary/50 px-8 py-12 text-center transition-colors",
        "outline-none hover:border-accent/50 hover:bg-accent-soft/40 focus-visible:ring-2 focus-visible:ring-focus",
        className,
      )}
    >
      <span className="grid size-12 place-items-center rounded-xl border border-border bg-surface text-muted shadow-[var(--card-shadow)] transition-colors group-hover:text-accent">
        <Icon className="size-5" />
      </span>
      <div>
        <div className="text-[15px] font-semibold">{title}</div>
        {hint && <div className="mt-0.5 text-[13px] text-muted">{hint}</div>}
      </div>
      {action && (
        <div className="mt-1" onClick={(e) => e.stopPropagation()}>
          {action}
        </div>
      )}
    </div>
  );
}
