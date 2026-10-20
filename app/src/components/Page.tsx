import { cn } from "@heroui/react";
import type { ReactNode } from "react";

/** A scrolling page inside the content card: title, optional subtitle and actions, then the body. */
export function Page({
  title,
  subtitle,
  actions,
  width = "default",
  children,
}: {
  title: string;
  subtitle?: ReactNode;
  actions?: ReactNode;
  width?: "default" | "narrow";
  children: ReactNode;
}) {
  return (
    <div className="h-full overflow-y-auto">
      <div className={cn("mx-auto flex flex-col gap-6 px-8 pt-7 pb-10", width === "narrow" ? "max-w-3xl" : "max-w-5xl")}>
        {/* Actions stay top-right; a long subtitle wraps instead of pushing them below. */}
        <header className="flex items-start justify-between gap-6">
          <div className="min-w-0 flex-1">
            <h1 className="text-xl leading-8 font-semibold tracking-tight">{title}</h1>
            {subtitle && <p className="mt-0.5 text-[13px] text-muted">{subtitle}</p>}
          </div>
          {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
        </header>
        {children}
      </div>
    </div>
  );
}
