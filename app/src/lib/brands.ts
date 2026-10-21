// Recognise certificate authorities and token makers so the UI can show their mark.
// Official logos go in src/assets/brands/<key>.svg|png|jpg (see the README there); until a
// file exists, BrandMark draws a monogram in the brand colour below.
import type { Certificate, TokenDevice } from "../ipc/api";

/**
 * How far MacSigner supports a CA's USB tokens, from ETOKEN-SUPPORT.md:
 * "verified" = certificates read on a real token, "driver" = the CA's token
 * driver is collected but untested, "none" = no driver yet.
 */
export type Support = "verified" | "driver" | "none";

export interface Brand {
  key: string;
  name: string;
  /** Monogram, at most 3 characters. */
  short: string;
  color: string;
  match: RegExp;
  support?: Support;
}

// The 26 public CAs licensed by NEAC; keys are the provider_id in assets/ca-logos/manifest.json.
// Order matters: the first match wins, so names that contain another (IntrustCA ⊃ TrustCA) come first.
export const caBrands: Brand[] = [
  { key: "ca2", name: "CA2 (Nacencomm)", short: "CA2", color: "#0b5cad", match: /nacencomm|\bca2\b/i, support: "verified" },
  { key: "e-ca", name: "E-CA (Thái Sơn)", short: "E", color: "#1b5e20", match: /\be-ca\b|thaisonsoft|thai son/i, support: "verified" },
  { key: "vnpt-ca", name: "VNPT-CA", short: "VN", color: "#0067b3", match: /vnpt/i, support: "driver" },
  { key: "viettel-ca", name: "Viettel-CA", short: "VT", color: "#e4002b", match: /viettel/i, support: "driver" },
  { key: "fpt-ca", name: "FPT-CA", short: "FPT", color: "#f26f21", match: /\bfpt\b/i, support: "driver" },
  { key: "bkavca", name: "BkavCA", short: "BK", color: "#c8102e", match: /bkav/i, support: "driver" },
  { key: "misa-ca", name: "MISA-CA", short: "MI", color: "#1565c0", match: /misa/i, support: "driver" },
  { key: "efy-ca", name: "EFY-CA", short: "EFY", color: "#00897b", match: /\befy\b/i, support: "driver" },
  { key: "safe-ca", name: "Safe-CA", short: "SF", color: "#2e7d32", match: /safe-?ca/i, support: "driver" },
  { key: "newtel-ca", name: "Newtel-CA", short: "NT", color: "#6a1b9a", match: /newtel|new-?ca/i, support: "driver" },
  { key: "smartsign", name: "SmartSign (Vina-CA)", short: "SS", color: "#0097a7", match: /smart-?sign|vina-?ca/i, support: "driver" },
  { key: "intrustca", name: "IntrustCA", short: "IT", color: "#283593", match: /intrust/i, support: "none" },
  { key: "trustca", name: "TrustCA (CyberLotus)", short: "TR", color: "#ad1457", match: /\btrust-?ca|cyberlotus/i, support: "driver" },
  { key: "easyca", name: "EASYCA", short: "EZ", color: "#ef6c00", match: /easy-?ca/i, support: "driver" },
  { key: "nc-ca", name: "NC-CA", short: "NC", color: "#00695c", match: /\bnc-?ca\b/i, support: "driver" },
  { key: "lcs-ca", name: "LCS-CA", short: "LCS", color: "#4527a0", match: /\blcs\b/i, support: "driver" },
  { key: "cmc-ca", name: "CMC-CA", short: "CMC", color: "#0d47a1", match: /\bcmc\b/i, support: "driver" },
  { key: "fastca", name: "FASTCA", short: "FA", color: "#e65100", match: /fast-?ca/i, support: "driver" },
  { key: "i-ca", name: "I-CA", short: "I", color: "#1976d2", match: /\bi-?ca\b/i, support: "driver" },
  { key: "hilo-ca", name: "Hilo-CA", short: "HL", color: "#00838f", match: /hilo/i, support: "driver" },
  { key: "one-ca", name: "ONE-CA", short: "ONE", color: "#c62828", match: /\bone-?ca\b/i, support: "driver" },
  { key: "winca", name: "WINCA", short: "WIN", color: "#2e7d32", match: /\bwin-?ca\b/i, support: "driver" },
  { key: "matbao-ca", name: "MATBAO-CA", short: "MB", color: "#d84315", match: /mat\s?bao/i, support: "none" },
  { key: "mobifone-ca", name: "MOBIFONE CA", short: "MF", color: "#0050a0", match: /mobifone/i, support: "none" },
  { key: "vnpay-ca", name: "VNPAY-CA", short: "VP", color: "#005baa", match: /vnpay/i, support: "none" },
  { key: "mk-ca", name: "MK CA", short: "MK", color: "#455a64", match: /\bmk[- ]?ca\b|mk hi-?tek/i, support: "none" },
];

export const supportLabel: Record<Support, string> = {
  verified: "Đã đọc (Windows)",
  driver: "Chưa test",
  none: "Chưa hỗ trợ",
};

export const tokenBrands: Brand[] = [
  { key: "securemetric", name: "SecureMetric", short: "SM", color: "#d32f2f", match: /securemetric/i },
  { key: "feitian", name: "Feitian", short: "FT", color: "#1e63b6", match: /feitian|\bft\b|epass/i },
  { key: "safenet", name: "Thales SafeNet", short: "SN", color: "#242a75", match: /safenet|thales|gemalto/i },
  { key: "watchdata", name: "Watchdata", short: "WD", color: "#00838f", match: /watchdata/i },
  { key: "longmai", name: "Longmai", short: "LM", color: "#5e35b1", match: /longmai|mtoken/i },
  { key: "bit4id", name: "Bit4id", short: "B4", color: "#00796b", match: /bit4id/i },
];

const find = (list: Brand[], ...texts: (string | undefined)[]) => {
  const text = texts.filter(Boolean).join(" ");
  return text ? list.find((b) => b.match.test(text)) : undefined;
};

/** Value of one attribute in an issuer DN such as "CN=CA2, O=NACENCOMM SCT, C=VN". */
export function dnPart(dn: string, key: string) {
  const m = dn.match(new RegExp(`(?:^|[,/]\\s*)${key}=([^,/]+)`, "i"));
  return m ? m[1].trim() : "";
}

/** "CA2 · NACENCOMM SCT" for an issuer. */
export function issuerLabel(c: Certificate) {
  const cn = dnPart(c.issuer, "CN") || c.issuerName;
  const o = dnPart(c.issuer, "O");
  return o && o !== cn ? `${cn} · ${o}` : cn;
}

export const caBrand = (c: Certificate) => find(caBrands, dnPart(c.issuer, "O"), c.issuer, c.issuerName);

export const tokenBrand = (device?: TokenDevice, readerName?: string) =>
  find(tokenBrands, device?.manufacturer, device?.model, readerName);
