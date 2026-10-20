import type { Certificate } from "../ipc/api";
import type { Tone } from "./ui/StatusChip";

export function expiresSoon(validTo: string) {
  const t = Date.parse(validTo.replace(" ", "T"));
  return !Number.isNaN(t) && t - Date.now() < 30 * 24 * 3600 * 1000;
}

export function formatDate(value: string) {
  const t = Date.parse(value.replace(" ", "T"));
  return Number.isNaN(t) ? value : new Date(t).toLocaleDateString("vi-VN");
}

export function daysLeft(validTo: string) {
  const t = Date.parse(validTo.replace(" ", "T"));
  return Number.isNaN(t) ? null : Math.ceil((t - Date.now()) / (24 * 3600 * 1000));
}

/** Validity of a certificate as a status chip tone and label. */
export function validity(c: Certificate): { tone: Tone; text: string } {
  if (!c.validNow) return { tone: "fail", text: "Hết hiệu lực" };
  if (expiresSoon(c.validTo)) return { tone: "warn", text: "Sắp hết hạn" };
  return { tone: "ok", text: "Còn hiệu lực" };
}
