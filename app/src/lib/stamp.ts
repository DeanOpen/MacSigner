// Mirrors stamp::text in src-tauri so the on-screen preview matches the PDF.
import type { CertFields, SignatureTemplate } from "../ipc/api";

export function stampLines(template: SignatureTemplate, fields: CertFields | undefined, mode: number, note: string) {
  const f = fields ?? { commonName: "[Người ký]", organization: "", unit: "", email: "", taxCode: "" };
  const lines: string[] = [];
  if (template.label || mode === 1) lines.push(`Người ký: ${f.commonName}`);
  if (template.organization && f.organization) lines.push(`Tổ chức: ${f.organization}`);
  if (template.unit && f.unit) lines.push(`Đơn vị: ${f.unit}`);
  if (template.email && f.email) lines.push(`Email: ${f.email}`);
  if (template.position) lines.push(`Chức vụ: ${template.position}`);
  if (template.signing_time || mode === 1) {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    lines.push(`Ngày ký: ${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}`);
  }
  if (mode === 1 && note.trim()) lines.push(note.trim());
  return lines;
}
