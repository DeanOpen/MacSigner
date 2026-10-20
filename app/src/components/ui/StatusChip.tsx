import { Chip, cn } from "@heroui/react";
import type { ReactNode } from "react";

export type Tone = "ok" | "warn" | "fail" | "info" | "neutral";

const color = { ok: "success", warn: "warning", fail: "danger", info: "accent", neutral: "default" } as const;

/** Soft status pill: "Còn hiệu lực", "Lỗi", "Đang chạy"… */
export function StatusChip({ tone, children, className, title }: { tone: Tone; children: ReactNode; className?: string; title?: string }) {
  return (
    <Chip color={color[tone]} variant="soft" size="sm" className={cn("shrink-0", className)} title={title}>
      <Chip.Label>{children}</Chip.Label>
    </Chip>
  );
}

/** Small status dot for dense places (sidebar service lines). */
export function StatusDot({ tone }: { tone: Tone }) {
  const bg = { ok: "bg-success", warn: "bg-warning", fail: "bg-danger", info: "bg-accent", neutral: "bg-muted/50" }[tone];
  return <span className={cn("inline-block size-2 shrink-0 rounded-full", bg)} aria-hidden="true" />;
}
