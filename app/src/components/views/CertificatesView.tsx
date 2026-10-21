import { Button, cn, Spinner } from "@heroui/react";
import { useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { BadgeCheck, ChevronDown, Copy, FileKey, Lock, Plus, RefreshCw, Usb } from "lucide-react";
import { useState, type ReactNode } from "react";
import { api, isTauri, type Certificate, type TokenDevice } from "../../ipc/api";
import { caBrand, caBrands, issuerLabel, supportLabel, tokenBrand, type Brand, type Support } from "../../lib/brands";
import { useScan } from "../../lib/scan";
import { shortName, useUsb, type Reader } from "../../lib/usb";
import { keys, useCertificates, useTokenDevices } from "../../queries";
import { useApp } from "../../stores/app";
import { BrandMark, logo, WindowsIcon } from "../BrandMark";
import { daysLeft, formatDate, validity } from "../CertCard";
import { Page } from "../Page";
import { toast } from "../Toasts";
import { Dialog } from "../ui/Dialog";
import { SecretInput, TextInput } from "../ui/Fields";
import { IconButton } from "../ui/IconButton";
import { Panel, PanelNote, Section } from "../ui/Section";
import { Segmented } from "../ui/Segmented";
import { StatusChip, type Tone } from "../ui/StatusChip";

const groups: {
  key: string;
  title: string;
  desc: string;
  match: (c: Certificate) => boolean;
}[] = [
  {
    key: "store",
    title: "Kho chứng thư Windows",
    desc: "Chứng thư token đã đăng ký vào Windows. Windows hỏi PIN khi ký.",
    match: (c) => c.source === "store",
  },
  {
    key: "file",
    title: "Tệp chứng thư",
    desc: "Nạp từ tệp P12/PFX trong phiên này; không lưu lại.",
    match: (c) => c.source === "file",
  },
];

const cols = "grid-cols-[minmax(0,1.6fr)_minmax(0,1.2fr)_7.5rem_8rem_1rem]";

function Tag({ children }: { children: ReactNode }) {
  return <span className="inline-flex items-center gap-1 rounded-md bg-default px-1.5 py-px text-[11px] font-medium text-muted">{children}</span>;
}

function Details({ cert }: { cert: Certificate }) {
  const rows: [string, string][] = [
    ["Tổ chức", cert.fields.organization],
    ["Email", cert.fields.email],
    ["Đơn vị cấp", cert.issuer],
    ["Hiệu lực", `${formatDate(cert.validFrom)} – ${formatDate(cert.validTo)}`],
  ];
  const copy = () =>
    navigator.clipboard.writeText(cert.id).then(
      () => toast.ok("Đã chép số sê-ri"),
      (e) => toast.error("Không chép được", e),
    );
  return (
    <dl className="grid grid-cols-1 gap-x-8 gap-y-3 bg-surface-secondary/50 px-4 py-4 sm:grid-cols-2">
      {rows
        .filter(([, v]) => v)
        .map(([k, v]) => (
          <div key={k} className="min-w-0">
            <dt className="text-xs text-muted">{k}</dt>
            <dd className="selectable text-[13px] break-words">{v}</dd>
          </div>
        ))}
      <div className="min-w-0 sm:col-span-2">
        <dt className="text-xs text-muted">Số sê-ri</dt>
        <dd className="flex items-center gap-1">
          <span className="selectable font-mono text-xs break-all">{cert.id}</span>
          <IconButton label="Chép số sê-ri" className="size-6 min-w-6" onPress={copy}>
            <Copy className="size-3.5" />
          </IconButton>
        </dd>
      </div>
    </dl>
  );
}

function TableHead() {
  return (
    <div className={cn("grid gap-4 bg-surface-secondary/50 px-4 py-2 text-xs font-medium text-muted", cols)} aria-hidden="true">
      <span>Chủ thể</span>
      <span>Nhà cung cấp</span>
      <span>Hết hạn</span>
      <span>Trạng thái</span>
      <span />
    </div>
  );
}

function Row({ cert, open, onToggle, inStore }: { cert: Certificate; open: boolean; onToggle: () => void; inStore?: boolean }) {
  const st = validity(cert);
  const days = daysLeft(cert.validTo);
  const selected = useApp((s) => s.certId) === cert.id;
  return (
    <div>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        className={cn("grid w-full cursor-pointer items-center gap-4 px-4 py-3 text-left transition-colors hover:bg-surface-secondary/60", cols)}
      >
        <span className="min-w-0">
          <span className="flex items-center gap-1.5">
            <span className="truncate text-[13px] font-medium" title={cert.fields.commonName || cert.subject}>
              {cert.fields.commonName || cert.subject}
            </span>
            {selected && <span className="size-1.5 shrink-0 rounded-full bg-accent" title="Đang dùng để ký" />}
          </span>
          <span className="mt-0.5 flex flex-wrap items-center gap-1.5 text-xs text-muted">
            {cert.fields.taxCode && <span>MST {cert.fields.taxCode}</span>}
            {cert.needsPin && (
              <Tag>
                <Lock className="size-3" />
                Cần PIN
              </Tag>
            )}
            {inStore && (
              <Tag>
                <WindowsIcon size={10} />
                Có trong kho Windows
              </Tag>
            )}
          </span>
        </span>
        <span className="flex min-w-0 items-center gap-2 text-[13px]">
          <BrandMark brand={caBrand(cert)} fallback={BadgeCheck} size="sm" />
          <span className="truncate">{cert.issuerName || "—"}</span>
        </span>
        <span className="text-[13px] tabular-nums">
          <span className={cn(st.tone === "fail" && "text-danger", st.tone === "warn" && "text-warning")}>{formatDate(cert.validTo)}</span>
          {st.tone === "warn" && days !== null && days >= 0 && <span className="block text-xs text-warning">còn {days} ngày</span>}
        </span>
        <span>
          <StatusChip tone={st.tone}>{st.text}</StatusChip>
        </span>
        <ChevronDown className={cn("size-4 text-muted transition-transform", open && "rotate-180")} />
      </button>
      {open && <Details cert={cert} />}
    </div>
  );
}

/** CA providers of the certificates on a token ("CA2 · NACENCOMM SCT"), each with its brand. */
function providers(device: TokenDevice) {
  const seen = new Map<string, Certificate>();
  for (const c of device.certificates) {
    const label = issuerLabel(c);
    if (label && !seen.has(label)) seen.set(label, c);
  }
  return [...seen].map(([label, c]) => ({ label, brand: caBrand(c) }));
}

function pinState(d: TokenDevice): { tone: Tone; text: string } | null {
  if (d.pinLocked) return { tone: "fail", text: "PIN đã khóa" };
  if (d.pinFinalTry) return { tone: "fail", text: "Còn 1 lần nhập PIN" };
  if (d.pinCountLow) return { tone: "warn", text: "Đã nhập sai PIN" };
  return null;
}

/** Pair each readable token with the reader it sits in (by maker name, else in order). */
function pairReaders(devices: TokenDevice[], readers: Reader[]) {
  const free = [...readers];
  const take = (i: number) => (i >= 0 ? free.splice(i, 1)[0] : undefined);
  const paired = devices.map((d) => {
    const maker = d.manufacturer.split(/\s+/)[0]?.toLowerCase() ?? "";
    return { device: d, reader: take(maker ? free.findIndex((r) => r.name.toLowerCase().includes(maker)) : -1) };
  });
  for (const p of paired) p.reader ??= take(free.length ? 0 : -1);
  return { paired, unread: free };
}

function DeviceHead({ mark, name, meta, aside }: { mark: ReactNode; name: string; meta: ReactNode; aside?: ReactNode }) {
  return (
    <div className="flex items-center gap-3 px-4 py-4">
      {mark}
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-semibold">{name}</div>
        <div className="truncate text-xs text-muted">{meta}</div>
      </div>
      {aside}
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-xs text-muted">{label}</dt>
      <dd className="mt-1 flex flex-col gap-1 text-[13px]">{children}</dd>
    </div>
  );
}

/** A plugged token nothing on this PC can read yet. */
function UnreadPanel({ reader, busy, onRescan }: { reader: Reader; busy: boolean; onRescan: () => void }) {
  return (
    <Panel>
      <DeviceHead
        mark={<BrandMark brand={tokenBrand(undefined, reader.name)} fallback={Usb} size="lg" />}
        name={shortName(reader.name)}
        meta={busy ? "Đang đọc chứng thư trên token…" : "Đã cắm nhưng chưa đọc được chứng thư"}
        aside={busy ? <Spinner size="sm" /> : <StatusChip tone="warn">Chưa đọc được</StatusChip>}
      />
      {!busy && (
        <div className="flex flex-wrap items-center justify-between gap-3 bg-warning-soft/40 px-4 py-3 text-[13px]">
          <span>MacSigner tự tìm bộ kết nối có sẵn hoặc đi kèm ứng dụng, kể cả E-CA và CA2. Chưa đọc được chứng thư từ USB này; chưa thể kết luận máy thiếu phần mềm của hãng. Thử cắm lại USB hoặc bấm Tìm lại.</span>
          <Button size="sm" variant="secondary" onPress={onRescan}>
            <RefreshCw className="size-4" />
            Tìm lại
          </Button>
        </div>
      )}
    </Panel>
  );
}

function TokenPanel({
  device,
  reader,
  expanded,
  onToggle,
}: {
  device: TokenDevice;
  reader?: Reader;
  expanded: string | null;
  onToggle: (c: Certificate) => void;
}) {
  const name = reader ? shortName(reader.name) : device.model || device.label || "USB token";
  const ca = providers(device);
  const pin = pinState(device);
  const maker = tokenBrand(device, reader?.name);
  return (
    <Panel>
      <DeviceHead
        mark={<BrandMark brand={maker} fallback={Usb} size="lg" />}
        name={name}
        meta={[device.label !== name && device.label, device.serial && `Số ${device.serial}`].filter(Boolean).join("  ·  ")}
        aside={pin ? <StatusChip tone={pin.tone}>{pin.text}</StatusChip> : <StatusChip tone="ok">Đã đọc</StatusChip>}
      />
      <dl className="grid grid-cols-3 gap-6 px-4 py-3">
        <Fact label="Nhà cung cấp chứng thư">
          {ca.length
            ? ca.map((p) => (
                <span key={p.label} className="flex items-center gap-1.5">
                  <BrandMark brand={p.brand} fallback={BadgeCheck} size="xs" />
                  {p.label}
                </span>
              ))
            : "—"}
        </Fact>
        <Fact label="Hãng sản xuất token">
          <span className="flex items-center gap-1.5">
            <BrandMark brand={maker} fallback={Usb} size="xs" />
            {[device.manufacturer, device.model].filter(Boolean).join(" ") || "—"}
          </span>
        </Fact>
        <Fact label="Chứng thư">{device.certificates.length}</Fact>
      </dl>
      {device.certificates.length ? (
        <div className="divide-y divide-separator">
          <TableHead />
          {device.certificates.map((c) => (
            <Row key={c.id} cert={c} open={expanded === `t:${c.id}`} onToggle={() => onToggle(c)} inStore={c.source === "store"} />
          ))}
        </div>
      ) : (
        <PanelNote>Token không có chứng thư.</PanelNote>
      )}
    </Panel>
  );
}

const supportOrder: Support[] = ["verified", "driver", "none"];
const supportText: Record<Support, string> = { verified: "text-success", driver: "text-warning", none: "text-muted" };
const supportHint: Record<Support, string> = {
  verified: "Đã đọc chứng thư trên USB thật trên Windows; chưa xác nhận E-CA/CA2 trên macOS/M2",
  driver: "Đã có bộ kết nối, chưa thử trên USB thật",
  none: "Chưa có bộ kết nối USB",
};
const supportOf = (b: Brand) => b.support ?? "none";

function CaCard({ brand, inUse }: { brand: Brand; inUse: boolean }) {
  const s = supportOf(brand);
  const src = logo(brand.key);
  return (
    <div
      title={supportHint[s]}
      className={cn(
        "relative flex flex-col overflow-hidden rounded-xl border bg-surface shadow-[var(--card-shadow)] transition-shadow hover:shadow-md",
        s === "verified" ? "border-success/50 ring-1 ring-success/20" : "border-border",
      )}
    >
      {/* Logos are published on white, so the frame stays white in dark mode too. */}
      <div className="grid h-20 place-items-center bg-white px-5">
        {src ? (
          <img
            src={src}
            alt={brand.name}
            draggable={false}
            className={cn("max-h-12 max-w-full object-contain", s === "none" && "opacity-50 grayscale")}
          />
        ) : (
          <BrandMark brand={brand} fallback={BadgeCheck} size="lg" />
        )}
      </div>
      <div className="flex flex-col gap-0.5 border-t border-separator px-3 py-2.5">
        <span className="truncate text-[13px] font-medium">{brand.name}</span>
        <span className="flex items-center gap-2">
          <span className={cn("text-xs font-medium", supportText[s])}>{supportLabel[s]}</span>
          {inUse && (
            <span className="ml-auto rounded-md bg-accent-soft px-1.5 py-px text-[10px] font-semibold text-accent-soft-foreground" title="Có chứng thư của CA này trên máy">
              Đang dùng
            </span>
          )}
        </span>
      </div>
      {s === "verified" && <BadgeCheck className="absolute top-2 right-2 size-4 text-success" aria-hidden="true" />}
    </div>
  );
}

/** Every licensed CA with its logo and how far MacSigner supports its tokens. */
function CaDirectory({ inUse }: { inUse: Set<string> }) {
  const [filter, setFilter] = useState<Support | "all">("all");
  const count = (s: Support) => caBrands.filter((b) => supportOf(b) === s).length;
  const list = supportOrder
    .filter((s) => filter === "all" || filter === s)
    .flatMap((s) => caBrands.filter((b) => supportOf(b) === s));
  return (
    <Section
      title="Nhà cung cấp chữ ký số"
      description={`${caBrands.length} CA công cộng được NEAC cấp phép.`}
      actions={
        <Segmented
          label="Lọc theo mức hỗ trợ"
          value={filter}
          onChange={setFilter}
          items={[
            { id: "all", label: `Tất cả ${caBrands.length}` },
            ...supportOrder.map((s) => ({ id: s, label: `${supportLabel[s]} ${count(s)}` })),
          ]}
        />
      }
    >
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
        {list.map((b) => (
          <CaCard key={b.key} brand={b} inUse={inUse.has(b.key)} />
        ))}
      </div>
      <p className="text-xs text-muted">
        <span className="font-medium text-success">Đã đọc trên Windows</span>: đã đọc chứng thư trên USB thật trên máy thử; chưa xác nhận E-CA/CA2 trên macOS/M2.{" "}
        <span className="font-medium text-warning">Chưa test</span>: đã có bộ kết nối, chưa thử trên USB thật. Ký bằng PIN
        trên USB thật vẫn đang được kiểm chứng với mọi CA.
      </p>
    </Section>
  );
}

export function CertificatesView() {
  const certs = useCertificates();
  const devices = useTokenDevices();
  const qc = useQueryClient();
  const app = useApp();
  const usb = useUsb();
  const [expanded, setExpanded] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [path, setPath] = useState("");
  const [password, setPassword] = useState("");
  const list = certs.data?.certificates ?? [];
  const busy = useScan((s) => s.scanning);
  const present = usb.readers.filter((r) => r.present);
  const { paired, unread } = pairReaders(devices.data ?? [], present);
  const plugged = Math.max(present.length, paired.length);
  const rescan = () => certs.rescan.mutate();

  const submit = async () => {
    try {
      qc.setQueryData(keys.certs, await api.importCertificate(path, password));
      toast.ok("Đã nạp chứng thư");
      setImporting(false);
    } catch (e) {
      toast.error("Không nạp được chứng thư", e);
    } finally {
      setPassword("");
    }
  };
  const browse = async () => {
    if (!isTauri()) return;
    const p = await open({ multiple: false, filters: [{ name: "Chứng thư", extensions: ["p12", "pfx"] }] });
    if (typeof p === "string") setPath(p);
  };

  return (
    <Page
      title="Chứng thư số"
      subtitle={
        plugged
          ? `Đang cắm ${plugged} USB token${unread.length && !busy ? ` · ${unread.length} chưa đọc được` : ""}`
          : "Chưa cắm USB token"
      }
      actions={
        <>
          <Button variant="secondary" size="sm" isDisabled={certs.rescan.isPending} onPress={rescan}>
            {certs.rescan.isPending ? <Spinner size="sm" color="current" /> : <RefreshCw className="size-4" />}
            {certs.rescan.isPending ? "Đang tìm…" : "Tìm lại"}
          </Button>
          <Button size="sm" onPress={() => setImporting(true)}>
            <Plus className="size-4" />
            Nhập P12/PFX
          </Button>
        </>
      }
    >
      {certs.isLoading && (
        <div className="flex items-center gap-2 text-[13px] text-muted">
          <Spinner size="sm" /> Đang đọc chứng thư…
        </div>
      )}
      <Section title="USB token">
        {paired.map(({ device, reader }) => (
          <TokenPanel
            key={`${device.serial}:${device.slot}`}
            device={device}
            reader={reader}
            expanded={expanded}
            onToggle={(c) => {
              setExpanded(expanded === `t:${c.id}` ? null : `t:${c.id}`);
              app.setCert(c.id);
            }}
          />
        ))}
        {unread.map((r) => (
          <UnreadPanel key={r.name} reader={r} busy={busy || devices.isFetching} onRescan={rescan} />
        ))}
        {!plugged && (
          <Panel>
            <DeviceHead
              mark={<BrandMark fallback={Usb} size="lg" />}
              name="Chưa cắm USB token"
              meta="Cắm USB token chữ ký số vào máy, MacSigner sẽ tự nhận."
            />
          </Panel>
        )}
      </Section>
      {groups.map((g) => {
        const items = list.filter(g.match);
        if (!items.length && g.key === "file") return null;
        return (
          <Section key={g.key} title={g.title} description={g.desc}>
            <Panel>
              {items.length ? (
                <>
                  <TableHead />
                  {items.map((c) => (
                    <Row
                      key={c.id}
                      cert={c}
                      open={expanded === c.id}
                      onToggle={() => {
                        setExpanded(expanded === c.id ? null : c.id);
                        app.setCert(c.id);
                      }}
                    />
                  ))}
                </>
              ) : (
                <PanelNote>Không có chứng thư.</PanelNote>
              )}
            </Panel>
          </Section>
        );
      })}
      <CaDirectory inUse={new Set([...list, ...paired.flatMap((p) => p.device.certificates)].map((c) => caBrand(c)?.key ?? ""))} />
      <Dialog
        isOpen={importing}
        onClose={() => setImporting(false)}
        title="Nhập chứng thư từ tệp"
        description="Chứng thư chỉ dùng trong phiên này và không được lưu lại."
        footer={
          <>
            <Button variant="tertiary" onPress={() => setImporting(false)}>
              Hủy
            </Button>
            <Button isDisabled={!path} onPress={submit}>
              Nạp chứng thư
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-4">
          <div className="flex items-end gap-2">
            <TextInput className="flex-1" label="Tệp P12 / PFX" value={path} onChange={setPath} />
            <Button variant="secondary" onPress={browse}>
              <FileKey className="size-4" />
              Chọn…
            </Button>
          </div>
          <SecretInput label="Mật khẩu" value={password} onChange={setPassword} showLabel="Hiện mật khẩu" hideLabel="Ẩn mật khẩu" />
        </div>
      </Dialog>
    </Page>
  );
}
