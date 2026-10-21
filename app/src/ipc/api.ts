// Typed wrappers around the Tauri commands in src-tauri/src/commands.rs.
// Outside Tauri (plain `pnpm dev` in a browser) a small mock answers instead,
// so the UI can be developed and previewed without the native shell.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { mock } from "./mock";

export const isTauri = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) return mock<T>(cmd, args);
  return tauriInvoke<T>(cmd, args);
}

export interface CertFields {
  commonName: string;
  organization: string;
  unit: string;
  email: string;
  taxCode: string;
}

export type CertSource = "store" | "file" | "token-session" | "token";

export interface Certificate {
  id: string;
  fields: CertFields;
  subject: string;
  issuer: string;
  issuerName: string;
  validFrom: string;
  validTo: string;
  validNow: boolean;
  source: CertSource;
  needsPin: boolean;
}

export interface CertList {
  certificates: Certificate[];
  readerStatus: string;
}

/** A USB token found by the last scan, with the certificates it holds. */
export interface TokenDevice {
  label: string;
  manufacturer: string;
  model: string;
  serial: string;
  slot: number;
  pinLocked: boolean;
  pinFinalTry: boolean;
  pinCountLow: boolean;
  /** source is "store" when the same certificate is also in the Windows store. */
  certificates: Certificate[];
}

export interface SignatureTemplate {
  name: string;
  /** 0 image and text, 1 image only, 2 text only */
  appearance: number;
  organization: boolean;
  label: boolean;
  email: boolean;
  unit: boolean;
  position: string;
  signing_time: boolean;
  image_path: string;
}

export interface Preferences {
  templates: SignatureTemplate[];
  defaultTemplate: number;
  timestampEnabled: boolean;
  timestampUrl: string;
  reason: string;
  location: string;
}

export interface SignatureReport {
  signer: string;
  issuer: string;
  serial: string;
  signingTime: string;
  coversWholeFile: boolean;
  digestOk: boolean;
  signatureOk: boolean;
  certificateValidNow: boolean;
  valid: boolean;
  error: string | null;
}

export interface ServiceState {
  available: boolean;
  unavailableReason: string | null;
  running: boolean;
  address: string | null;
  error: string | null;
}

export interface Services {
  esign: ServiceState;
  ctsign: ServiceState;
  vnpt: ServiceState;
  apiSigning: boolean;
}

export interface Settings {
  esign: { enabled: boolean; port: number; fallback_port: number | null; origins: string[] };
  ctsign: { enabled: boolean; port: number };
  vnpt: { enabled: boolean; port: number };
}

export type ServiceName = "esign" | "ctsign" | "vnpt";

export interface PortStatus {
  port: number;
  state: "open" | "closed" | "unknown";
  detail: string;
}

export interface Check {
  module: string;
  name: string;
  outcome: "pass" | "fail" | "skip";
  detail: string;
}

/** One line of the activity history (src/activity.rs). */
export interface ActivityEntry {
  id: number;
  time: string;
  channel: "app" | "esign" | "ctsign";
  action: string;
  client: string;
  target: string;
  certificate: string;
  signer: string;
  signing: boolean;
  ok: boolean;
  status: string;
  message: string;
  durationMs: number;
}

/** A signing request from the e-tax portal waiting for approval (src-tauri/src/bridge.rs). */
export interface BrowserSignPrompt {
  id: number;
  code: string;
  origin: string;
  count: number;
  sha1: boolean;
  serials: string[];
  tin: string | null;
  /** The page sent a PIN; the PIN field may stay empty. */
  pinProvided: boolean;
  /** Certificates matching the request, valid now. */
  certificates: Certificate[];
  timeoutSecs: number;
}

export interface SignRequest {
  input: string;
  output: string;
  certId: string;
  pin?: string;
  page: number;
  x: number;
  y: number;
  mode: number;
  template: SignatureTemplate;
  note: string;
  reason?: string;
  location?: string;
}

export const api = {
  appInfo: () => invoke<{ version: string; platform: string; configPath: string }>("app_info"),
  takeInitialFile: () => invoke<string | null>("take_initial_file"),
  listCertificates: (rescan: boolean) => invoke<CertList>("list_certificates", { rescan }),
  importCertificate: (path: string, password?: string, keyPath?: string) =>
    invoke<CertList>("import_certificate", { path, password, keyPath }),
  readFile: (path: string) => invoke<ArrayBuffer>("read_file", { path }),
  defaultOutput: (path: string) => invoke<string>("default_output", { path }),
  signPdf: (request: SignRequest) =>
    invoke<{ output: string; signatures: number }>("sign_pdf", { request }),
  verifyPdf: (path: string) => invoke<SignatureReport[]>("verify_pdf", { path }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  services: () => invoke<Services>("services_status"),
  setService: (name: ServiceName, running: boolean) =>
    invoke<Services>("set_service", { name, running }),
  vnptDiagnostics: () => invoke<PortStatus[]>("vnpt_diagnostics"),
  setApiSigning: (enabled: boolean) => invoke<Services>("set_api_signing", { enabled }),
  getPreferences: () => invoke<Preferences>("get_preferences"),
  savePreferences: (preferences: Preferences) => invoke<void>("save_preferences", { preferences }),
  runSelftest: (module: "all" | ServiceName) => invoke<Check[]>("run_selftest", { module }),
  sampleSign: (module: "esign" | "ctsign", text: string, certId?: string, pin?: string) =>
    invoke<string>("sample_sign", { module, text, certId, pin }),
  offlineStatus: (path: string) => invoke<string>("offline_status", { path }),
  tokenDevices: () => invoke<TokenDevice[]>("token_devices"),
  activity: () => invoke<ActivityEntry[]>("activity_list"),
  clearActivity: () => invoke<void>("activity_clear"),
  esignPending: () => invoke<BrowserSignPrompt | null>("esign_pending"),
  /** `certId` null declines the request. */
  esignAnswer: (id: number, certId: string | null, pin: string | null) =>
    invoke<void>("esign_answer", { id, certId, pin }),
};
