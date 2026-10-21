// Whether a full token scan (PKCS#11 probes) is running; probes take seconds.
import { create } from "zustand";

export const useScan = create<{ scanning: boolean; set: (v: boolean) => void }>((set) => ({
  scanning: false,
  set: (scanning) => set({ scanning }),
}));

let activeScans = 0;

/** Run a full scan, flagging it so the UI can tell "reading" from "unreadable". */
export async function scanning<T>(work: Promise<T>): Promise<T> {
  activeScans += 1;
  useScan.getState().set(true);
  try {
    return await work;
  } finally {
    activeScans -= 1;
    useScan.getState().set(activeScans > 0);
  }
}

export function coalesceScans<T>(scan: () => Promise<T>) {
  let pending: Promise<T> | undefined;
  let repeat = false;
  return (refresh = false): Promise<T> => {
    if (pending) {
      repeat ||= refresh;
      return pending;
    }
    pending = (async () => {
      let result: T;
      do {
        repeat = false;
        result = await scan();
      } while (repeat);
      return result;
    })().finally(() => {
      pending = undefined;
      repeat = false;
    });
    return pending;
  };
}
