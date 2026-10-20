import { cn, Kbd, Modal } from "@heroui/react";
import { CornerDownLeft, FolderOpen, Moon, Search, Sun, type LucideIcon } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useApp, type Route } from "../stores/app";
import { navGroups, settingsNav } from "./shell/nav";
import { pickPdf } from "./views/SignView";

interface Command {
  label: string;
  group: string;
  icon: LucideIcon;
  hint?: string;
  run: () => void;
}

const normalize = (s: string) =>
  s.normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/đ/g, "d").toLowerCase();

export function CommandPalette() {
  const app = useApp();
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const go = (route: Route) => () => app.setRoute(route);
  const commands: Command[] = useMemo(
    () => [
      { group: "Hành động", label: "Mở tệp PDF để ký", icon: FolderOpen, hint: "Ctrl O", run: () => pickPdf().then((p) => p && app.openFile(p)) },
      ...[...navGroups.flatMap((g) => g.items), settingsNav].map((n) => ({
        group: "Đi tới",
        label: n.label,
        icon: n.icon,
        hint: n.route === "settings" ? "Ctrl ," : undefined,
        run: go(n.route),
      })),
      { group: "Giao diện", label: "Giao diện sáng", icon: Sun, run: () => app.setTheme("light") },
      { group: "Giao diện", label: "Giao diện tối", icon: Moon, run: () => app.setTheme("dark") },
    ],
    [], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const filtered = commands.filter((c) => normalize(`${c.group} ${c.label}`).includes(normalize(query)));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "k") {
        e.preventDefault();
        app.setPaletteOpen(!useApp.getState().paletteOpen);
      } else if (mod && e.key.toLowerCase() === "o") {
        e.preventDefault();
        pickPdf().then((p) => p && app.openFile(p));
      } else if (mod && e.key === ",") {
        e.preventDefault();
        app.setRoute("settings");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    setQuery("");
    setIndex(0);
  }, [app.paletteOpen]);

  const run = (c?: Command) => {
    if (!c) return;
    app.setPaletteOpen(false);
    c.run();
  };

  return (
    <>
      <Modal.Backdrop isOpen={app.paletteOpen} onOpenChange={app.setPaletteOpen} className="bg-black/30">
        <Modal.Container placement="top" size="lg" className="pt-[12vh]">
          <Modal.Dialog aria-label="Tìm lệnh" className="gap-0 overflow-hidden p-0 sm:max-w-[560px]">
            <div className="flex items-center gap-2.5 border-b border-separator px-4">
              <Search className="size-4 shrink-0 text-muted" />
              <input
                autoFocus
                aria-label="Tìm lệnh"
                placeholder="Gõ lệnh hoặc tìm chức năng…"
                className="h-12 w-full bg-transparent text-sm outline-none placeholder:text-muted"
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setIndex(0);
                }}
                onKeyDown={(e) => {
                  if (e.key === "ArrowDown") (e.preventDefault(), setIndex((i) => Math.min(i + 1, filtered.length - 1)));
                  if (e.key === "ArrowUp") (e.preventDefault(), setIndex((i) => Math.max(i - 1, 0)));
                  if (e.key === "Enter") run(filtered[index]);
                }}
              />
            </div>
            <div role="listbox" aria-label="Lệnh" className="max-h-[22rem] overflow-y-auto p-1.5">
              {filtered.map((c, i) => {
                const Ico = c.icon;
                const first = i === 0 || filtered[i - 1].group !== c.group;
                return (
                  <div key={c.label}>
                    {first && <div className="px-2.5 pt-2 pb-1 text-[11px] font-medium tracking-wide text-muted uppercase">{c.group}</div>}
                    <div
                      role="option"
                      aria-selected={i === index}
                      ref={i === index ? (el) => el?.scrollIntoView({ block: "nearest" }) : undefined}
                      className={cn(
                        "flex h-9 cursor-pointer items-center gap-2.5 rounded-lg px-2.5 text-[13px]",
                        i === index ? "bg-default text-foreground" : "text-foreground/80",
                      )}
                      onMouseMove={() => setIndex(i)}
                      onClick={() => run(c)}
                    >
                      <Ico className={cn("size-4 shrink-0", i === index ? "text-accent" : "text-muted")} />
                      <span className="truncate">{c.label}</span>
                      {c.hint && <span className="ml-auto font-mono text-[11px] text-muted">{c.hint}</span>}
                    </div>
                  </div>
                );
              })}
              {filtered.length === 0 && <div className="px-3 py-8 text-center text-[13px] text-muted">Không có lệnh phù hợp</div>}
            </div>
            <div className="flex items-center gap-4 border-t border-separator bg-surface-secondary/60 px-4 py-2 text-[11px] text-muted">
              <span className="flex items-center gap-1.5">
                <Kbd>↑</Kbd>
                <Kbd>↓</Kbd>
                chọn
              </span>
              <span className="flex items-center gap-1.5">
                <Kbd>
                  <CornerDownLeft className="size-3" />
                </Kbd>
                mở
              </span>
              <span className="flex items-center gap-1.5">
                <Kbd>Esc</Kbd>
                đóng
              </span>
            </div>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </>
  );
}
