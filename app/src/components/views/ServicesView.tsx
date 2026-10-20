import { Label, NumberField } from "@heroui/react";
import { useQueryClient } from "@tanstack/react-query";
import { Power, RefreshCcw, ShieldCheck } from "lucide-react";
import type { ReactNode } from "react";
import { api, type ServiceState, type Settings } from "../../ipc/api";
import { keys, useSaveSettings, useServiceMutation, useServices, useSettings } from "../../queries";
import { Page } from "../Page";
import { ServiceLogo, serviceBrands } from "../ServiceBrand";
import { toast } from "../Toasts";
import { Panel, SettingRow } from "../ui/Section";
import { StatusChip } from "../ui/StatusChip";
import { Toggle } from "../ui/Toggle";

function status(s?: ServiceState) {
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
  id: "esign" | "ctsign";
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
        <Toggle label={`Bật ${title}`} isSelected={!!state?.running} isDisabled={busy} onChange={onToggle} />
      </div>
      <SettingRow icon={Power} title="Cổng" description={desc}>
        <NumberField
          aria-label="Cổng"
          key={`${id}-${port}`}
          className="w-28"
          minValue={1}
          maxValue={65535}
          defaultValue={port}
          formatOptions={{ useGrouping: false }}
          onChange={(p) => {
            if (p >= 1 && p <= 65535 && p !== port) onPort(p);
          }}
        >
          <Label className="sr-only">Cổng</Label>
          <NumberField.Group>
            <NumberField.Input className="text-center font-mono" />
          </NumberField.Group>
        </NumberField>
      </SettingRow>
      <SettingRow icon={RefreshCcw} title="Tự bật khi mở MacSigner" description="Dịch vụ chạy ngay khi ứng dụng khởi động.">
        <Toggle label="Tự bật" isSelected={autostart} onChange={onAutostart} />
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
  const s = settings.data;
  const update = (next: Settings) =>
    save.mutate(next, {
      onSuccess: () => toast.ok("Đã lưu", "Cổng mới áp dụng khi bật lại dịch vụ."),
      onError: (e) => toast.error("Cài đặt không hợp lệ", e),
    });
  const setRunning = (name: "esign" | "ctsign", running: boolean) =>
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
            busy={toggle.isPending}
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
            busy={toggle.isPending}
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
        </>
      )}
    </Page>
  );
}
