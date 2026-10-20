import { Copy } from "lucide-react";
import { toast } from "../Toasts";
import { IconButton } from "./IconButton";

/** Monospace output with a copy button. */
export function CodeBlock({ children, label }: { children: string; label: string }) {
  const copy = () =>
    navigator.clipboard.writeText(children).then(
      () => toast.ok("Đã sao chép"),
      (e) => toast.error("Không sao chép được", e),
    );
  return (
    <div className="relative rounded-lg border border-border bg-surface-secondary">
      <pre aria-label={label} className="selectable max-h-72 overflow-auto p-3 pr-12 font-mono text-xs leading-5 break-all whitespace-pre-wrap">
        {children}
      </pre>
      <IconButton label="Sao chép kết quả" className="absolute top-1.5 right-1.5" onPress={copy}>
        <Copy className="size-4" />
      </IconButton>
    </div>
  );
}
