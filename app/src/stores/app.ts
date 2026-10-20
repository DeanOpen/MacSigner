import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Route = "sign" | "certificates" | "verify" | "history" | "services" | "tests" | "settings";
/** "system" follows the OS. */
export type Theme = "system" | "light" | "dark";

/** Where the stamp goes: page (1-based) and top-left corner as page fractions. */
export interface Placement {
  page: number;
  x: number;
  y: number;
}

interface AppState {
  route: Route;
  theme: Theme;
  file: string | null;
  recent: string[];
  zoom: number;
  placement: Placement | null;
  certId: string | null;
  mode: number;
  note: string;
  /** null follows the default template from preferences. */
  templateIndex: number | null;
  output: string;
  paletteOpen: boolean;
  setRoute: (route: Route) => void;
  setTheme: (theme: Theme) => void;
  openFile: (path: string | null) => void;
  setZoom: (zoom: number) => void;
  setPlacement: (placement: Placement | null) => void;
  setCert: (id: string | null) => void;
  setMode: (mode: number) => void;
  setNote: (note: string) => void;
  setTemplateIndex: (index: number | null) => void;
  setOutput: (output: string) => void;
  setPaletteOpen: (open: boolean) => void;
}

export const useApp = create<AppState>()(
  persist(
    (set) => ({
      route: "sign",
      theme: "system",
      file: null,
      recent: [],
      zoom: 1,
      placement: null,
      certId: null,
      mode: 0,
      note: "",
      templateIndex: null,
      output: "",
      paletteOpen: false,
      setRoute: (route) => set({ route }),
      setTheme: (theme) => set({ theme }),
      openFile: (file) =>
        set((s) => ({
          file,
          placement: null,
          route: file ? "sign" : s.route,
          recent: file ? [file, ...s.recent.filter((r) => r !== file)].slice(0, 6) : s.recent,
        })),
      setZoom: (zoom) => set({ zoom: Math.min(3, Math.max(0.4, Math.round(zoom * 100) / 100)) }),
      setPlacement: (placement) => set({ placement }),
      setCert: (certId) => set({ certId }),
      setMode: (mode) => set({ mode }),
      setNote: (note) => set({ note }),
      setTemplateIndex: (templateIndex) => set({ templateIndex }),
      setOutput: (output) => set({ output }),
      setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
    }),
    {
      name: "macsigner-ui",
      version: 3,
      // Only preferences survive restarts; the open document does not.
      partialize: (s) => ({ theme: s.theme, recent: s.recent, zoom: s.zoom, mode: s.mode, certId: s.certId }),
      // Version 2 stored Carbon theme names (white, g10, g90, g100).
      migrate: (state, version) => {
        const s = (state ?? {}) as { theme?: string };
        const theme: Theme =
          version < 2 ? "system" : s.theme === "g90" || s.theme === "g100" ? "dark" : s.theme === "white" || s.theme === "g10" ? "light" : "system";
        return { ...s, theme } as AppState;
      },
    },
  ),
);

export function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

export function resolvedTheme(theme: Theme): Exclude<Theme, "system"> {
  if (theme !== "system") return theme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}
