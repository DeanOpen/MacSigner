import { cn } from "@heroui/react";
import type { ComponentType, ReactNode } from "react";

/** A titled block on a page: heading and description on the left, actions on the right. */
export function Section({
  title,
  description,
  actions,
  children,
  className,
}: {
  title?: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("flex flex-col gap-3", className)}>
      {(title || actions) && (
        <div className="flex items-end justify-between gap-4">
          <div className="min-w-0">
            {title && <h2 className="text-[15px] leading-6 font-semibold">{title}</h2>}
            {description && <p className="text-[13px] text-muted">{description}</p>}
          </div>
          {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
        </div>
      )}
      {children}
    </section>
  );
}

/** Bordered card that stacks rows with hairline dividers. */
export function Panel({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={cn(
        "divide-y divide-separator overflow-hidden rounded-xl border border-border bg-surface shadow-[var(--card-shadow)]",
        className,
      )}
    >
      {children}
    </div>
  );
}

/** One setting: icon, title and description on the left, the control on the right. */
export function SettingRow({
  icon: Icon,
  title,
  description,
  children,
  className,
}: {
  icon?: ComponentType<{ className?: string }>;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex items-center gap-4 px-4 py-3.5", className)}>
      {Icon && (
        <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-border bg-surface-secondary text-muted">
          <Icon className="size-4" />
        </span>
      )}
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium">{title}</div>
        {description && <div className="text-[13px] text-muted">{description}</div>}
      </div>
      {children}
    </div>
  );
}

/** Muted single-line message inside a Panel (loading, empty list). */
export function PanelNote({ children }: { children: ReactNode }) {
  return <div className="flex items-center gap-2 px-4 py-6 text-[13px] text-muted">{children}</div>;
}
