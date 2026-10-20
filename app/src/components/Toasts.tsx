// Compact toasts, bottom centre, so they never cover the side panels.
import { Toast, toast as heroToast } from "@heroui/react";

type Kind = "success" | "danger" | "default";

// The same message twice in a row (e.g. two USB events) shows once.
const open = new Map<string, string>();

function push(kind: Kind, title: string, description?: string) {
  const key = `${kind}|${title}|${description ?? ""}`;
  if (open.has(key)) return;
  const options = {
    description,
    timeout: kind === "danger" ? 8000 : 3500,
    onClose: () => open.delete(key),
  };
  const id =
    kind === "success" ? heroToast.success(title, options) : kind === "danger" ? heroToast.danger(title, options) : heroToast(title, options);
  open.set(key, id);
}

export const toast = {
  ok: (title: string, subtitle?: string) => push("success", title, subtitle),
  error: (title: string, detail?: unknown) => push("danger", title, detail == null ? undefined : String(detail)),
  info: (title: string, subtitle?: string) => push("default", title, subtitle),
};

export function Toasts() {
  return <Toast.Provider placement="bottom" maxVisibleToasts={3} />;
}
