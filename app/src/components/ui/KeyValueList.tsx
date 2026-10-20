import { cn } from "@heroui/react";
import type { ReactNode } from "react";

export interface KeyValue {
  label: ReactNode;
  value: ReactNode;
  icon?: ReactNode;
  mono?: boolean;
}

/** Label / value rows, e.g. signature checks or certificate details. */
export function KeyValueList({ items, className, labelWidth = "11rem" }: { items: KeyValue[]; className?: string; labelWidth?: string }) {
  return (
    <dl className={cn("divide-y divide-separator text-[13px]", className)}>
      {items.map((it, i) => (
        <div key={i} className="grid items-start gap-3 px-4 py-2.5" style={{ gridTemplateColumns: `${it.icon ? "1rem " : ""}${labelWidth} minmax(0, 1fr)` }}>
          {it.icon && <span className="mt-0.5 flex">{it.icon}</span>}
          <dt className="text-muted">{it.label}</dt>
          <dd className={cn("selectable min-w-0 break-words", it.mono && "font-mono text-xs leading-5")}>{it.value}</dd>
        </div>
      ))}
    </dl>
  );
}
