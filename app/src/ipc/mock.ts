// Browser-only stand-in for the Tauri commands (UI development and previews).
// Never used inside the desktop app.
import type { ActivityEntry, CertList, Check, Preferences, Services, Settings, SignatureReport } from "./api";

const certs: CertList = {
  readerStatus: "Đã nhận USB token: SecureMetric ST3Ace",
  certificates: [
    {
      id: "5402BC5CACCE669C202400020009A540",
      fields: {
        commonName: "CÔNG TY TNHH NEXTWAVES INDUSTRIES",
        organization: "CÔNG TY TNHH NEXTWAVES INDUSTRIES",
        unit: "",
        email: "duy.tran@nextwaves.industries",
        taxCode: "0318119134",
      },
      subject: "CN=CÔNG TY TNHH NEXTWAVES INDUSTRIES, O=CÔNG TY TNHH NEXTWAVES INDUSTRIES",
      issuer: "CN=CA2, O=NACENCOMM SCT, C=VN",
      issuerName: "CA2",
      validFrom: "2026-10-20 11:03:15",
      validTo: "2029-08-30 10:15:46",
      validNow: true,
      source: "store",
      needsPin: false,
    },
    {
      id: "5402BC5CACCE669C2024000200083C56",
      fields: {
        commonName: "CÔNG TY TNHH NEXTWAVES INDUSTRIES",
        organization: "CÔNG TY TNHH NEXTWAVES INDUSTRIES",
        unit: "",
        email: "khanh.nguyen@nextwaves.industries",
        taxCode: "0318119134",
      },
      subject: "CN=CÔNG TY TNHH NEXTWAVES INDUSTRIES",
      issuer: "CN=CA2, O=NACENCOMM SCT, C=VN",
      issuerName: "CA2",
      validFrom: "2024-11-12 09:28:12",
      validTo: "2026-11-17 09:28:12",
      validNow: true,
      source: "token",
      needsPin: true,
    },
  ],
};

let services: Services = {
  esign: { running: false, address: null, error: null },
  ctsign: { running: true, address: "127.0.0.1:32101", error: null },
  apiSigning: false,
};

let settings: Settings = {
  esign: { enabled: false, port: 8768, fallback_port: 8769, origins: ["https://thuedientu.gdt.gov.vn"] },
  ctsign: { enabled: true, port: 32101 },
};

let preferences: Preferences = {
  templates: [
    {
      name: "Mẫu mặc định",
      appearance: 2,
      organization: false,
      label: true,
      email: false,
      unit: false,
      position: "",
      signing_time: true,
      image_path: "",
    },
  ],
  defaultTemplate: 0,
  timestampEnabled: false,
  timestampUrl: "",
  reason: "",
  location: "",
};

const reports: SignatureReport[] = [
  {
    signer: "CÔNG TY TNHH NEXTWAVES INDUSTRIES",
    issuer: "CA2",
    serial: "5402BC5CACCE669C202400020009A540",
    signingTime: "20/10/2026 18:20:11",
    coversWholeFile: true,
    digestOk: true,
    signatureOk: true,
    certificateValidNow: true,
    valid: true,
    error: null,
  },
];

const checks: Check[] = [
  { module: "ESIGN", name: "Danh mục lệnh giao thức", outcome: "pass", detail: "24 lệnh, 9 lệnh con" },
  { module: "ESIGN", name: "Vector chữ ký Java", outcome: "pass", detail: "2/2 chữ ký trùng byte-for-byte" },
  { module: "ESIGN", name: "WebSocket: chặn origin lạ", outcome: "pass", detail: "HTTP 403" },
  { module: "CTSIGN", name: "Vector chữ ký .NET", outcome: "pass", detail: "Trùng byte-for-byte" },
  { module: "CTSIGN", name: "API GetCert", outcome: "pass", detail: '{"status":200}' },
  { module: "CHUNG", name: "Kho chứng thư Windows", outcome: "skip", detail: "Chế độ xem trước" },
];

const base = { client: "", target: "", certificate: "", signer: "", message: "" };
let activity: ActivityEntry[] = [
  { ...base, id: 6, time: "2026-10-20T19:02:11+07:00", channel: "ctsign", action: "POST HubClient/signHash", client: "https://hoadon.example.vn", target: "3 hash req-8812", certificate: "5402BC5CACCE669C202400020009A540", signing: true, ok: true, status: "200", message: "SUCCESS", durationMs: 412 },
  { ...base, id: 5, time: "2026-10-20T19:01:58+07:00", channel: "ctsign", action: "GET HubClient/getCert", client: "https://hoadon.example.vn", signing: false, ok: true, status: "200", message: "SUCCESS", durationMs: 38 },
  { ...base, id: 4, time: "2026-10-20T18:55:40+07:00", channel: "esign", action: "ONLINE_XML_CER_001", client: "https://thuedientu.gdt.gov.vn", certificate: "5402BC5CACCE669C202400020009A540", signer: "CN=CÔNG TY TNHH NEXTWAVES INDUSTRIES", signing: true, ok: true, status: "TRUE", message: "", durationMs: 6240 },
  { ...base, id: 3, time: "2026-10-20T18:55:12+07:00", channel: "esign", action: "Kết nối", client: "https://attacker.example", signing: false, ok: false, status: "403", message: "Từ chối kết nối (origin, host hoặc đường dẫn không hợp lệ)", durationMs: 1 },
  { ...base, id: 2, time: "2026-10-20T18:41:03+07:00", channel: "app", action: "Ký PDF", target: "C:\\Users\\DeanOS\\Downloads\\hop-dong.pdf", certificate: "5402BC5CACCE669C2024000200083C56", signer: "CÔNG TY TNHH NEXTWAVES INDUSTRIES", signing: true, ok: false, status: "Lỗi", message: "PIN không đúng", durationMs: 3120 },
  { ...base, id: 1, time: "2026-10-20T18:20:11+07:00", channel: "app", action: "Ký PDF", target: "C:\\Users\\DeanOS\\Downloads\\t.pdf", certificate: "5402BC5CACCE669C202400020009A540", signer: "CÔNG TY TNHH NEXTWAVES INDUSTRIES", signing: true, ok: true, status: "OK", message: "Đã lưu C:\\Users\\DeanOS\\Downloads\\t-signed.pdf", durationMs: 1840 },
];

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function mock<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  await delay(cmd === "list_certificates" && args?.rescan ? 900 : 120);
  const answer = ((): unknown => {
    switch (cmd) {
      case "app_info":
        return { version: "0.2.0", platform: "preview", configPath: "%APPDATA%\\MacSigner\\settings.json" };
      case "take_initial_file":
        return null;
      case "list_certificates":
      case "import_certificate":
        return certs;
      case "read_file":
        return fetch("/dev/sample.pdf").then((r) => r.arrayBuffer());
      case "default_output":
        return String(args?.path).replace(/\.pdf$/i, "-signed.pdf");
      case "sign_pdf":
        return { output: (args?.request as { output: string }).output, signatures: 1 };
      case "verify_pdf":
        return reports;
      case "get_settings":
        return settings;
      case "save_settings":
        settings = args?.settings as Settings;
        return null;
      case "services_status":
        return services;
      case "set_service": {
        const name = args?.name as "esign" | "ctsign";
        const running = Boolean(args?.running);
        services = {
          ...services,
          [name]: {
            running,
            address: running ? `127.0.0.1:${settings[name].port}` : null,
            error: null,
          },
        };
        return services;
      }
      case "set_api_signing":
        services = { ...services, apiSigning: Boolean(args?.enabled) };
        return services;
      case "get_preferences":
        return preferences;
      case "save_preferences":
        preferences = args?.preferences as Preferences;
        return null;
      case "run_selftest":
        return checks;
      case "sample_sign":
        return JSON.stringify({ CODE: "ONLINE_XML_CER_001", SUCCESS: "TRUE", SIGNED_CONTENT: "MIIB…" }, null, 2);
      case "token_devices":
        return [
          {
            label: "CA2 NEXTWAVES",
            manufacturer: "SecureMetric",
            model: "ST3Ace",
            serial: "3B6A0C1F22D4",
            slot: 0,
            pinLocked: false,
            pinFinalTry: false,
            pinCountLow: false,
            certificates: certs.certificates.slice(0, 2),
          },
        ];
      case "offline_status":
        return "Đã chọn XML. Ký XMLDSig tờ khai chưa được triển khai.";
      case "activity_list":
        return activity;
      case "activity_clear":
        activity = [];
        return null;
      case "esign_pending":
      case "esign_answer":
        return null;
      default:
        throw new Error(`mock: lệnh chưa hỗ trợ ${cmd}`);
    }
  })();
  return (await answer) as T;
}
