// Live USB token state pushed by src-tauri/src/usb.rs ("token-status" events).
import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { create } from "zustand";
import { toast } from "../components/Toasts";
import { api, invoke, isTauri } from "../ipc/api";
import { keys } from "../queries";
import { scanning } from "./scan";

export interface Reader {
  name: string;
  present: boolean;
}

export const useUsb = create<{ readers: Reader[]; ready: boolean; set: (r: Reader[]) => void }>((set) => ({
  readers: [],
  ready: false,
  set: (readers) => set({ readers, ready: true }),
}));

/** Reader names are verbose ("SecureMetric ST3Ace 0"); keep the device part. */
export const shortName = (name: string) => name.replace(/\s+\d+$/, "").trim();

export function useUsbWatcher() {
  const qc = useQueryClient();
  useEffect(() => {
    if (!isTauri()) {
      useUsb.getState().set([
        { name: "SecureMetric ST3Ace 0", present: true },
        { name: "FT ePass2003Auto 0", present: true },
      ]);
      return;
    }
    // listen() resolves later; if the effect is torn down first (StrictMode, hot
    // reload) the listener must still be removed, or every event fires twice.
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let previous: Reader[] | null = null;

    const refresh = () => {
      // The Windows store learns new token certificates a moment after insertion
      // (certificate propagation), so refresh now and again shortly after.
      api.listCertificates(false).then((d) => qc.setQueryData(keys.certs, d));
      setTimeout(
        () =>
          scanning(api.listCertificates(true)).then((d) => {
            qc.setQueryData(keys.certs, d);
            qc.invalidateQueries({ queryKey: keys.tokens });
          }),
        2500,
      );
    };

    const apply = (readers: Reader[]) => {
      if (disposed) return;
      if (previous) {
        for (const r of readers) {
          const before = previous.find((p) => p.name === r.name);
          if (r.present && !before?.present) toast.ok("Đã cắm USB token", shortName(r.name));
          if (!r.present && before?.present) toast.info("Đã rút USB token", shortName(r.name));
        }
        for (const p of previous) {
          if (p.present && !readers.some((r) => r.name === p.name)) toast.info("Đã rút USB token", shortName(p.name));
        }
        refresh();
      }
      previous = readers;
      useUsb.getState().set(readers);
    };

    invoke<Reader[]>("token_status")
      .then((r) => {
        if (!disposed && previous === null) {
          previous = r;
          useUsb.getState().set(r);
        }
      })
      .catch(() => !disposed && useUsb.getState().set([]));
    import("@tauri-apps/api/event").then(({ listen }) =>
      listen<Reader[]>("token-status", (e) => apply(e.payload)).then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      }),
    );
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [qc]);
}
