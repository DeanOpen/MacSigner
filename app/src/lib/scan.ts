// Whether a full token scan (PKCS#11 probes) is running; probes take seconds.
import { create } from "zustand";

export const useScan = create<{ scanning: boolean; set: (v: boolean) => void }>((set) => ({
  scanning: false,
  set: (scanning) => set({ scanning }),
}));

/** Run a full scan, flagging it so the UI can tell "reading" from "unreadable". */
export async function scanning<T>(work: Promise<T>): Promise<T> {
  useScan.getState().set(true);
  try {
    return await work;
  } finally {
    useScan.getState().set(false);
  }
}