import { useEffect } from "react";
import { BrowserSignDialog } from "./components/BrowserSignDialog";
import { CommandPalette } from "./components/CommandPalette";
import { Sidebar } from "./components/shell/Sidebar";
import { TitleBar } from "./components/shell/TitleBar";
import { useRail } from "./components/shell/useRail";
import { Toasts, toast } from "./components/Toasts";
import { CertificatesView } from "./components/views/CertificatesView";
import { HistoryView } from "./components/views/HistoryView";
import { ServicesView } from "./components/views/ServicesView";
import { SettingsView } from "./components/views/SettingsView";
import { SignView } from "./components/views/SignView";
import { TestsView } from "./components/views/TestsView";
import { VerifyView } from "./components/views/VerifyView";
import { api, isTauri } from "./ipc/api";
import { useUsbWatcher } from "./lib/usb";
import { resolvedTheme, useApp } from "./stores/app";

function useThemeClass() {
  const theme = useApp((s) => s.theme);
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const t = resolvedTheme(theme);
      const root = document.documentElement;
      root.classList.toggle("dark", t === "dark");
      root.classList.toggle("light", t === "light");
      root.dataset.theme = t;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}

/** PDFs dropped on the window, or passed on the command line, open for signing. */
function useFileInputs() {
  const openFile = useApp((s) => s.openFile);
  useEffect(() => {
    if (!isTauri()) return;
    api.takeInitialFile().then((f) => f && openFile(f));
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((event) => {
          if (event.payload.type !== "drop") return;
          const pdf = event.payload.paths.find((p) => p.toLowerCase().endsWith(".pdf"));
          if (pdf) openFile(pdf);
          else toast.info("Chỉ hỗ trợ tệp PDF");
        })
        .then((fn) => (unlisten = fn)),
    );
    return () => unlisten?.();
  }, [openFile]);
}

const views = {
  sign: SignView,
  certificates: CertificatesView,
  verify: VerifyView,
  history: HistoryView,
  services: ServicesView,
  tests: TestsView,
  settings: SettingsView,
};

export default function App() {
  useThemeClass();
  useFileInputs();
  useUsbWatcher();
  const route = useApp((s) => s.route);
  const rail = useRail();
  const View = views[route];
  return (
    <div className="flex h-full flex-col bg-background">
      <TitleBar rail={rail} />
      <div className="flex min-h-0 flex-1">
        <Sidebar rail={rail} />
        <main className="mr-2 mb-2 min-w-0 flex-1 overflow-hidden rounded-xl border border-border bg-surface shadow-[var(--card-shadow)]">
          <View />
        </main>
      </div>
      <CommandPalette />
      <BrowserSignDialog />
      <Toasts />
    </div>
  );
}
