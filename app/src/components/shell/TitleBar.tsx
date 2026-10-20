import { cn, Kbd, Tooltip } from "@heroui/react";
import { Minus, Moon, Search, Square, Sun, Usb, X } from "lucide-react";
import { Button } from "react-aria-components";
import { isTauri } from "../../ipc/api";
import { shortName, useUsb } from "../../lib/usb";
import { resolvedTheme, useApp } from "../../stores/app";

/** USB token presence, live from the PC/SC watcher. */
function TokenChip() {
  const { readers, ready } = useUsb();
  const setRoute = useApp((s) => s.setRoute);
  const present = readers.filter((r) => r.present);
  const label = !ready
    ? "Đang kiểm tra token…"
    : present.length === 0
      ? "Chưa cắm USB token"
      : present.length === 1
        ? shortName(present[0].name)
        : `${present.length} USB token`;
  return (
    <Button
      onPress={() => setRoute("certificates")}
      aria-label={present.map((r) => shortName(r.name)).join(", ") || "Cắm USB token để ký số"}
      className={cn(
        "flex h-7 max-w-[16rem] cursor-pointer items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-focus",
        present.length
          ? "border-success/30 bg-success-soft text-success-soft-foreground"
          : "border-border bg-surface/70 text-muted hover:bg-surface",
      )}
    >
      <Usb className="size-3.5 shrink-0" />
      <span className="truncate">{label}</span>
    </Button>
  );
}

function ChromeButton({ label, onPress, children, className }: { label: string; onPress: () => void; children: React.ReactNode; className?: string }) {
  return (
    <Tooltip delay={500} closeDelay={0}>
      <Button
        aria-label={label}
        onPress={onPress}
        className={cn(
          "grid size-8 cursor-pointer place-items-center rounded-lg text-muted outline-none transition-colors hover:bg-foreground/[0.06] hover:text-foreground focus-visible:ring-2 focus-visible:ring-focus",
          className,
        )}
      >
        {children}
      </Button>
      <Tooltip.Content placement="bottom">{label}</Tooltip.Content>
    </Tooltip>
  );
}

function WindowControls() {
  if (!isTauri()) return null;
  const win = () => import("@tauri-apps/api/window").then((m) => m.getCurrentWindow());
  const cls = "grid h-11 w-12 place-items-center text-muted transition-colors hover:bg-foreground/[0.06] hover:text-foreground";
  return (
    <div className="flex self-stretch">
      <button type="button" aria-label="Thu nhỏ" className={cls} onClick={() => win().then((w) => w.minimize())}>
        <Minus className="size-4" />
      </button>
      <button type="button" aria-label="Phóng to" className={cls} onClick={() => win().then((w) => w.toggleMaximize())}>
        <Square className="size-3.5" />
      </button>
      <button
        type="button"
        aria-label="Đóng"
        className={cn(cls, "hover:bg-[#e81123] hover:text-white")}
        onClick={() => win().then((w) => w.close())}
      >
        <X className="size-4" />
      </button>
    </div>
  );
}

/** One title bar: it doubles as the frameless window's drag region. */
export function TitleBar({ rail }: { rail: boolean }) {
  const { theme, setTheme, setPaletteOpen, setRoute } = useApp();
  const dark = resolvedTheme(theme) === "dark";
  return (
    <header data-tauri-drag-region className="flex h-11 shrink-0 items-center gap-3 pl-3">
      <div data-tauri-drag-region className={cn("flex shrink-0 items-center gap-2", rail ? "w-8" : "w-[208px] pl-1")}>
        <button type="button" onClick={() => setRoute("sign")} className="flex cursor-pointer items-center gap-2" aria-label="MacSigner">
          <img src="/logo.svg" alt="" className="size-5" draggable={false} />
          {!rail && <span className="text-[13px] font-semibold tracking-tight">MacSigner</span>}
        </button>
      </div>
      <Button
        aria-label="Tìm lệnh (Ctrl+K)"
        onPress={() => setPaletteOpen(true)}
        className="flex h-8 w-full max-w-[420px] min-w-0 cursor-pointer items-center gap-2 rounded-lg border border-border bg-surface/70 px-2.5 text-[13px] text-muted outline-none transition-colors hover:bg-surface focus-visible:ring-2 focus-visible:ring-focus"
      >
        <Search className="size-4 shrink-0" />
        <span className="truncate">Tìm lệnh hoặc chức năng…</span>
        <Kbd className="ml-auto hidden sm:inline-flex">
          <Kbd.Content>Ctrl+K</Kbd.Content>
        </Kbd>
      </Button>
      <div data-tauri-drag-region className="h-full min-w-4 flex-1" />
      <TokenChip />
      <ChromeButton label={dark ? "Giao diện sáng" : "Giao diện tối"} onPress={() => setTheme(dark ? "light" : "dark")}>
        {dark ? <Sun className="size-4" /> : <Moon className="size-4" />}
      </ChromeButton>
      <WindowControls />
      {!isTauri() && <span className="w-1" />}
    </header>
  );
}
