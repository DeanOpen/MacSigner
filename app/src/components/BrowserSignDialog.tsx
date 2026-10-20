// Approval prompt for signing requests from the e-tax portal (ESIGN WebSocket),
// sent by src-tauri/src/bridge.rs. Nothing is signed until the user picks a
// certificate here and presses "Ký"; closing or "Từ chối" declines.
import { Button } from "@heroui/react";
import { PenLine } from "lucide-react";
import { useEffect, useState } from "react";
import { api, isTauri, type BrowserSignPrompt } from "../ipc/api";
import { formatDate } from "./CertCard";
import { Dialog } from "./ui/Dialog";
import { SecretInput, SelectField } from "./ui/Fields";

const commandLabels: Record<string, string> = {
  CER_001: "Ký xác thực chứng thư",
  ONLINE_XML_CER_001: "Ký tờ khai trực tuyến",
  ONLINE_SUBMIT_SIGNING_001: "Ký nộp hồ sơ trực tuyến",
};

export function BrowserSignDialog() {
  const [prompt, setPrompt] = useState<BrowserSignPrompt | null>(null);
  const [certId, setCertId] = useState("");
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const unlisten: (() => void)[] = [];
    const show = (p: BrowserSignPrompt | null) => {
      if (disposed || !p) return;
      setPrompt(p);
      setCertId(p.certificates[0]?.id ?? "");
      setPin("");
      setBusy(false);
    };
    api.esignPending().then(show);
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen<BrowserSignPrompt>("esign-sign-request", (e) => show(e.payload)).then((fn) => (disposed ? fn() : unlisten.push(fn)));
      listen<number>("esign-sign-closed", (e) => setPrompt((p) => (p?.id === e.payload ? null : p))).then((fn) =>
        disposed ? fn() : unlisten.push(fn),
      );
    });
    return () => {
      disposed = true;
      unlisten.forEach((fn) => fn());
    };
  }, []);

  if (!prompt) return null;
  const cert = prompt.certificates.find((c) => c.id === certId);
  const needsPin = !!cert?.needsPin && !prompt.pinProvided;

  const answer = (sign: boolean) => {
    setBusy(true);
    const id = prompt.id;
    // The PIN goes straight to the signing request and is cleared here.
    api.esignAnswer(id, sign ? certId : null, sign && pin ? pin : null).finally(() => {
      setPin("");
      setPrompt((p) => (p?.id === id ? null : p));
    });
  };

  const label = commandLabels[prompt.code] ?? prompt.code;
  return (
    <Dialog
      isOpen
      onClose={() => answer(false)}
      isDismissable={!busy}
      size="md"
      title="Trang thuế điện tử yêu cầu ký số"
      description={prompt.origin}
      footer={
        <>
          <Button variant="tertiary" isDisabled={busy} onPress={() => answer(false)}>
            Từ chối
          </Button>
          <Button isDisabled={busy || !cert || (needsPin && !pin)} onPress={() => answer(true)}>
            <PenLine className="size-4" />
            Ký
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <dl className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]">
          <dt className="text-muted">Yêu cầu</dt>
          <dd>
            {label} <span className="font-mono text-xs text-muted">{prompt.code}</span>
          </dd>
          {prompt.count > 1 && (
            <>
              <dt className="text-muted">Số mục ký</dt>
              <dd>{prompt.count}</dd>
            </>
          )}
          {prompt.tin && (
            <>
              <dt className="text-muted">Mã số thuế</dt>
              <dd className="font-mono text-xs leading-5">{prompt.tin}</dd>
            </>
          )}
        </dl>
        {prompt.certificates.length > 1 ? (
          <SelectField
            label="Chứng thư"
            value={certId}
            onChange={setCertId}
            options={prompt.certificates.map((c) => ({
              value: c.id,
              label: `${c.fields.commonName} · ${c.fields.taxCode || c.issuerName} · đến ${formatDate(c.validTo)}`,
            }))}
          />
        ) : (
          cert && (
            <dl className="grid grid-cols-[7rem_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]">
              <dt className="text-muted">Chứng thư</dt>
              <dd className="truncate">
                {cert.fields.commonName} · {cert.issuerName}
              </dd>
              <dt className="text-muted">Hiệu lực đến</dt>
              <dd>{formatDate(cert.validTo)}</dd>
            </dl>
          )
        )}
        {needsPin && <SecretInput label="Mã PIN token" value={pin} onChange={setPin} showLabel="Hiện mã PIN" hideLabel="Ẩn mã PIN" autoFocus />}
        <p className="text-xs text-muted">
          Chỉ bấm Ký khi bạn vừa thao tác ký trên trang thuế. Chữ ký số có giá trị pháp lý như chữ ký tay.
          {prompt.sha1 && " Lệnh này dùng SHA-1 theo đúng plugin gốc của cổng thuế."}
          {cert?.source === "store" && " Windows sẽ hỏi PIN của token nếu cần."} Yêu cầu tự hủy sau {Math.round(prompt.timeoutSecs / 60)} phút.
        </p>
      </div>
    </Dialog>
  );
}
