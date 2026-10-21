import { describe, expect, it, vi } from "vitest";
import { coalesceScans, scanning, useScan } from "./scan";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((accept, fail) => {
    resolve = accept;
    reject = fail;
  });
  return { promise, resolve, reject };
}

describe("automatic token scans", () => {
  it("keeps reading active until every concurrent scan has finished", async () => {
    const first = deferred<number>();
    const second = deferred<number>();
    const firstScan = scanning(first.promise);
    const secondScan = scanning(second.promise);
    expect(useScan.getState().scanning).toBe(true);
    first.resolve(1);
    await firstScan;
    expect(useScan.getState().scanning).toBe(true);
    second.resolve(2);
    await secondScan;
    expect(useScan.getState().scanning).toBe(false);
  });

  it("coalesces callers and refreshes once after a USB change during a scan", async () => {
    const first = deferred<number>();
    const second = deferred<number>();
    const work = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const scan = coalesceScans<number>(work);
    const result = scan();
    expect(scan()).toBe(result);
    expect(scan(true)).toBe(result);
    expect(scan(true)).toBe(result);
    expect(work).toHaveBeenCalledTimes(1);
    first.resolve(1);
    await Promise.resolve();
    expect(work).toHaveBeenCalledTimes(2);
    second.resolve(2);
    await expect(result).resolves.toBe(2);
  });

  it("releases failed scans and allows a later retry", async () => {
    const work = vi.fn().mockRejectedValueOnce(new Error("removed")).mockResolvedValueOnce(2);
    const scan = coalesceScans<number>(() => scanning(work()));
    await expect(scan()).rejects.toThrow("removed");
    expect(useScan.getState().scanning).toBe(false);
    await expect(scan()).resolves.toBe(2);
    expect(work).toHaveBeenCalledTimes(2);
  });
});
