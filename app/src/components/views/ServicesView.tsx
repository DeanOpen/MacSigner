import { Button, Label, NumberField } from "@heroui/react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Power, RefreshCcw, ShieldCheck } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { api, type ServiceName, type ServiceState, type Settings } from "../../ipc/api";
import { keys, useSaveSettings, useServiceMutation, useServices, useSettings } from "../../queries";
import { Page } from "../Page";
import { ServiceLogo, serviceBrands } from "../ServiceBrand";
import { toast } from "../Toasts";
import { Panel, SettingRow } from "../ui/Section";
import { StatusChip } from "../ui/StatusChip";
import { Toggle } from "../ui/Toggle";

function status(s?: ServiceState) {
  if (s?.available === false) return <StatusChip tone="neutral">Chưa sẵn sàng</StatusChip>;
  if (s?.running) return <StatusChip tone="ok">Đang chạy · {s.address}</StatusChip>;
  if (s?.error) return <StatusChip tone="fail" title={s.error}>{s.error}</StatusChip>;
  return <StatusChip tone="neutral">Đã tắt</StatusChip>;
}

function Service({
  id,
  title,
  desc,
  state,
  port,
  autostart,
  busy,
  onToggle,
  onPort,
  onAutostart,
  children,
}: {
  id: ServiceName;
  title: string;
  desc: string;
  state?: ServiceState;
  port: number;
  autostart: boolean;
  busy: boolean;
  onToggle: (v: boolean) => void;
  onPort: (p: number) => void;
  onAutostart: (v: boolean) => void;
  children?: ReactNode;
}) {
  const [draftPort, setDraftPort] = useState(port);
  useEffect(() => setDraftPort(port), [port]);
  const commitPort = () => {
    if (!Number.isInteger(draftPort) || draftPort < 1 || draftPort > 65535) {
      setDraftPort(port);
      toast.error("Cổng không hợp lệ", "Cổng phải là số nguyên từ 1 đến 65535.");
      return;
    }
    if (draftPort !== port) onPort(draftPort);
  };
  return (
    <Panel>
      <div className="flex items-center gap-4 bg-surface-secondary/40 px-4 py-4">
        <span className="grid size-10 shrink-0 place-items-center rounded-xl border border-border bg-surface text-accent shadow-[var(--card-shadow)]">
          <ServiceLogo service={id} className="size-8 object-contain" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-sm font-semibold">{title}</div>
          <div className="mt-1">{status(state)}</div>
        </div>
        <Toggle label={`Bật ${title}`} isSelected={!!state?.running} isDisabled={busy || !state || !state.available} onChange={onToggle} />
      </div>
      <SettingRow icon={Power} title="Cổng" description={desc}>
        <NumberField
          aria-label={`Cổng ${title}`}
          className="w-28"
          isDisabled={busy}
          minValue={1}
          maxValue={65535}
          value={draftPort}
          formatOptions={{ useGrouping: false }}
          onChange={setDraftPort}
        >
          <Label className="sr-only">Cổng</Label>
          <NumberField.Group>
            <NumberField.Input
              className="text-center font-mono"
              onBlur={commitPort}
              onKeyDown={(event) => {
                if (event.key === "Enter") event.currentTarget.blur();
              }}
            />
          </NumberField.Group>
        </NumberField>
      </SettingRow>
      <SettingRow icon={RefreshCcw} title="Tự bật khi mở MacSigner" description="Dịch vụ chạy ngay khi ứng dụng khởi động.">
        <Toggle label="Tự bật" isSelected={autostart} isDisabled={busy || !state || !state.available} onChange={onAutostart} />
      </SettingRow>
      {children}
    </Panel>
  );
}

export function ServicesView() {
  const services = useServices();
  const settings = useSettings();
  const save = useSaveSettings();
  const toggle = useServiceMutation();
  const qc = useQueryClient();
  const diagnostics = useMutation({ mutationFn: api.vnptDiagnostics });
  const s = settings.data;
  const update = (next: Settings) =>
    save.mutate(next, {
      onSuccess: () => toast.ok("Đã lưu", "Cấu hình áp dụng khi dịch vụ sẵn sàng và được bật lại."),
      onError: (e) => toast.error("Cài đặt không hợp lệ", e),
    });
  const setRunning = (name: ServiceName, running: boolean) =>
    toggle.mutate({ name, running }, { onError: (e) => toast.error("Không bật được dịch vụ", e) });

  return (
    <Page title="Dịch vụ" subtitle="Chỉ lắng nghe trên máy này (127.0.0.1)." width="narrow">
      {s && (
        <>
          <Service
            id="esign"
            title={`${serviceBrands.esign.name} ${serviceBrands.esign.version}`}
            desc="WebSocket /plugin/sign cho thuedientu.gdt.gov.vn"
            state={services.data?.esign}
            port={s.esign.port}
            autostart={s.esign.enabled}
            busy={toggle.isPending || save.isPending}
            onToggle={(v) => setRunning("esign", v)}
            onPort={(port) => update({ ...s, esign: { ...s.esign, port } })}
            onAutostart={(enabled) => update({ ...s, esign: { ...s.esign, enabled } })}
          />
          <Service
            id="ctsign"
            title={`${serviceBrands.ctsign.name} ${serviceBrands.ctsign.version}`}
            desc="HTTP GetCert, SignHash, signpdf — tương thích CTSigningHub (gốc dùng 32001)"
            state={services.data?.ctsign}
            port={s.ctsign.port}
            autostart={s.ctsign.enabled}
            busy={toggle.isPending || save.isPending}
            onToggle={(v) => setRunning("ctsign", v)}
            onPort={(port) => update({ ...s, ctsign: { ...s.ctsign, port } })}
            onAutostart={(enabled) => update({ ...s, ctsign: { ...s.ctsign, enabled } })}
          >
            <SettingRow icon={ShieldCheck} title="Cho phép trang web ký qua API" description="Khi tắt, SignHash và signpdf trả 403.">
              <Toggle
                label="Cho phép ký qua API"
                isSelected={!!services.data?.apiSigning}
                onChange={(v) => api.setApiSigning(v).then((d) => qc.setQueryData(keys.services, d))}
              />
            </SettingRow>
          </Service>
          <Service
            id="vnpt"
            title={`${serviceBrands.vnpt.name} · ${serviceBrands.vnpt.version}`}
            desc="Dự kiến WSS /plugin; lưu cổng không mở máy chủ hoặc thay đổi trust store."
            state={services.data?.vnpt}
            port={s.vnpt.port}
            autostart={s.vnpt.enabled}
            busy={save.isPending || diagnostics.isPending}
            onToggle={(running) => setRunning("vnpt", running)}
            onPort={(port) => {
              diagnostics.reset();
              update({ ...s, vnpt: { ...s.vnpt, port } });
            }}
            onAutostart={(enabled) => update({ ...s, vnpt: { ...s.vnpt, enabled } })}
          >
            <div className="space-y-3 px-4 py-4 text-[13px]">
              <p role="status" className="text-muted">
                {services.data?.vnpt.unavailableReason ?? "Đang tải trạng thái VNPT…"}
              </p>
              <p className="text-muted">Cổng client gốc thử: <span className="font-mono">4433, 4434, 4435, 9201, 9202</span>. Đổi cổng chỉ có tác dụng nếu trang web cũng gọi cổng đó.</p>
              <p className="text-muted">Đã đối chiếu khung giao thức và phản hồi phiên bản. Chưa hỗ trợ ký VNPT, TLS tin cậy hoặc xác thực thử thách của hãng; không dùng trạng thái ESIGN/CTSIGN để thay thế.</p>
              <Button size="sm" variant="secondary" isDisabled={diagnostics.isPending || save.isPending} onPress={() => diagnostics.mutate()}>
                <RefreshCcw className="size-4" />{diagnostics.isPending ? "Đang kiểm tra…" : "Kiểm tra cổng VNPT"}
              </Button>
              {diagnostics.error && <p role="alert" className="text-danger">{String(diagnostics.error)}</p>}
              {diagnostics.data && (
                <div aria-live="polite" className="space-y-2">
                  <p className="text-xs text-muted">Kiểm tra TCP tại 127.0.0.1, không gửi lệnh ký hoặc PIN. Có kết nối không có nghĩa VNPT đang chạy.</p>
                  {diagnostics.data.map((result) => (
                    <div key={result.port} className="grid grid-cols-[3rem_6rem_minmax(0,1fr)] items-start gap-3 rounded-lg bg-surface-secondary/40 px-3 py-2">
                      <span className="w-12 shrink-0 font-mono">{result.port}</span>
                      <StatusChip tone={result.state === "open" ? "ok" : "neutral"}>
                        {result.state === "open" ? "Có kết nối" : result.state === "closed" ? "Chưa mở" : "Chưa rõ"}
                      </StatusChip>
                      <span className="min-w-0 break-words text-muted">{result.detail}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </Service>
        </>
      )}
    </Page>
  );
}
