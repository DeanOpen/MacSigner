import { describe, expect, it } from "vitest";
import { mock } from "./mock";
import type { PortStatus, Services, Settings } from "./api";

describe("VNPT preview contract", () => {
  it("does not turn protocol-only support into a running service", async () => {
    const services = await mock<Services>("services_status");
    expect(services.vnpt.available).toBe(false);
    expect(services.vnpt.running).toBe(false);
    expect(services.vnpt.address).toBeNull();
    await expect(mock("set_service", { name: "vnpt", running: true })).rejects.toThrow("VNPT");
    const stopped = await mock<Services>("set_service", { name: "vnpt", running: false });
    expect(stopped.vnpt.running).toBe(false);
  });

  it("refuses autostart and labels preview port diagnostics as unknown", async () => {
    const settings = await mock<Settings>("get_settings");
    await expect(mock("save_settings", { settings: { ...settings, vnpt: { ...settings.vnpt, enabled: true } } })).rejects.toThrow("VNPT");
    const results = await mock<PortStatus[]>("vnpt_diagnostics");
    expect(results.map((result) => result.port)).toEqual([4433, 4434, 4435, 9201, 9202]);
    expect(results.every((result) => result.state === "unknown")).toBe(true);
  });
});
