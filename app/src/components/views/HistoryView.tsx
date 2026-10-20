import { Button, cn, SearchField, Spinner } from "@heroui/react";
import { ChevronDown, Copy, RefreshCw, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import type { ActivityEntry } from "../../ipc/api";
import { useActivity, useClearActivity } from "../../queries";
import { fileName } from "../../stores/app";
import { Page } from "../Page";
import { ServiceLogo, serviceBrands } from "../ServiceBrand";
import { toast } from "../Toasts";
import { Dialog } from "../ui/Dialog";
import { IconButton } from "../ui/IconButton";
import { Panel, PanelNote } from "../ui/Section";
import { Segmented } from "../ui/Segmented";
import { StatusChip } from "../ui/StatusChip";

/** Where the entry came from: the app itself, or a caller of ESIGN / CTSIGN. */
function Source({ channel }: { channel: ActivityEntry["channel"] }) {
  const name = channel === "app" ? "MacSigner" : serviceBrands[channel].name;
  return (
    <span className="flex min-w-0 items-center gap-2" title={channel === "app" ? "Ký trong ứng dụng" : `Lệnh gọi tới ${name}`}>
      {channel === "app" ? (
        <img src="/logo.svg" alt="" className="size-5 shrink-0" draggable={false} />
      ) : (
        <ServiceLogo service={channel} className="size-5 shrink-0 object-contain" />
      )}
      <span className="truncate text-[13px] font-medium">{name}</span>
    </span>
  );
}

const normalize = (s: string) =>
  s.normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/đ/g, "d").toLowerCase();

function formatTime(iso: string) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

const duration = (ms: number) => (ms >= 1000 ? `${(ms / 1000).toFixed(1)} s` : `${ms} ms`);

function copy(text: string, what: string) {
  navigator.clipboard.writeText(text).then(
    () => toast.ok(`Đã sao chép ${what}`),
    (e) => toast.error("Không sao chép được", e),
  );
}

function Details({ e }: { e: ActivityEntry }) {
  const rows: [string, string, boolean?][] = [
    ["Thời gian", formatTime(e.time)],
    ["Thao tác", e.action, e.channel !== "app"],
    ["Nguồn gọi", e.client || (e.channel === "app" ? "Ứng dụng MacSigner" : "")],
    ["Tài liệu / yêu cầu", e.target],
    ["Người ký", e.signer],
    ["Số sê-ri chứng thư", e.certificate, true],
    ["Mã trạng thái", e.status, true],
    ["Kết quả", e.message],
    ["Thời lượng", duration(e.durationMs)],
  ];
  return (
    <dl className="grid grid-cols-1 gap-x-8 gap-y-3 bg-surface-secondary/50 px-4 py-4 sm:grid-cols-2">
      {rows
        .filter(([, v]) => v)
        .map(([k, v, mono]) => (
          <div key={k} className="min-w-0">
            <dt className="text-xs text-muted">{k}</dt>
            <dd className="flex items-start gap-1">
              <span className={cn("selectable min-w-0 text-[13px] break-all", mono && "font-mono text-xs leading-5")}>{v}</span>
              <IconButton label={`Sao chép ${k.toLowerCase()}`} className="-mt-1 size-6 min-w-6 shrink-0" onPress={() => copy(v, k.toLowerCase())}>
                <Copy className="size-3.5" />
              </IconButton>
            </dd>
          </div>
        ))}
    </dl>
  );
}

function Row({ e, open, onToggle }: { e: ActivityEntry; open: boolean; onToggle: () => void }) {
  const title = e.channel === "app" || e.target.includes("\\") || e.target.includes("/") ? fileName(e.target) : e.target;
  const meta = [e.client, e.signer, e.certificate && `Sê-ri …${e.certificate.slice(-8)}`, duration(e.durationMs)].filter(Boolean);
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="grid w-full cursor-pointer grid-cols-[8.5rem_minmax(0,1fr)_9.5rem_6.5rem_1rem] items-center gap-4 px-4 py-3 text-left transition-colors hover:bg-surface-secondary/60"
      >
        <Source channel={e.channel} />
        <span className="min-w-0">
          <span className="block truncate text-[13px] font-medium">
            {e.action}
            {title && <span className="font-normal text-muted"> · {title}</span>}
          </span>
          <span className="block truncate text-xs text-muted">{meta.join("  ·  ")}</span>
        </span>
        <span className="text-xs text-muted tabular-nums">{formatTime(e.time)}</span>
        <span title={e.message}>
          <StatusChip tone={e.ok ? "ok" : "fail"}>{e.ok ? "Thành công" : "Lỗi"}</StatusChip>
        </span>
        <ChevronDown className={cn("size-4 text-muted transition-transform", open && "rotate-180")} />
      </button>
      {open && <Details e={e} />}
    </div>
  );
}

type Tab = "signing" | "calls";
type Outcome = "all" | "ok" | "fail";

export function HistoryView() {
  const activity = useActivity();
  const clear = useClearActivity();
  const [tab, setTab] = useState<Tab>("signing");
  const [outcome, setOutcome] = useState<Outcome>("all");
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<number | null>(null);
  const [confirm, setConfirm] = useState(false);

  const all = activity.data ?? [];
  const list = useMemo(() => {
    const q = normalize(query.trim());
    return all.filter(
      (e) =>
        (tab === "signing" ? e.signing : e.channel !== "app") &&
        (outcome === "all" || e.ok === (outcome === "ok")) &&
        (!q || normalize([e.action, e.client, e.target, e.signer, e.certificate, e.message, e.status].join(" ")).includes(q)),
    );
  }, [all, tab, outcome, query]);
  const counts = {
    signing: all.filter((e) => e.signing).length,
    calls: all.filter((e) => e.channel !== "app").length,
  };

  return (
    <Page
      title="Lịch sử"
      subtitle="Các lần ký và mọi lệnh gọi tới ESIGN (WebSocket) và CTSIGN (API). Không lưu nội dung, hash hay PIN."
      actions={
        <>
          <Button variant="secondary" size="sm" onPress={() => activity.refetch()}>
            <RefreshCw className="size-4" />
            Làm mới
          </Button>
          <Button variant="danger-soft" size="sm" isDisabled={!all.length} onPress={() => setConfirm(true)}>
            <Trash2 className="size-4" />
            Xóa lịch sử
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <SearchField aria-label="Tìm" value={query} onChange={setQuery} className="w-full">
          <SearchField.Group>
            <SearchField.SearchIcon />
            <SearchField.Input placeholder="Tìm theo tệp, origin, sê-ri, lỗi…" />
            <SearchField.ClearButton aria-label="Xóa nội dung tìm" />
          </SearchField.Group>
        </SearchField>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <Segmented
            label="Loại"
            value={tab}
            onChange={(v) => {
              setTab(v);
              setOpen(null);
            }}
            items={[
              { id: "signing", label: `Lịch sử ký (${counts.signing})` },
              { id: "calls", label: `API / WebSocket (${counts.calls})` },
            ]}
          />
          <Segmented
            label="Kết quả"
            value={outcome}
            onChange={setOutcome}
            items={[
              { id: "all", label: "Tất cả" },
              { id: "ok", label: "Thành công" },
              { id: "fail", label: "Lỗi" },
            ]}
          />
        </div>
      </div>

      <Panel>
        <div className="grid grid-cols-[8.5rem_minmax(0,1fr)_9.5rem_6.5rem_1rem] gap-4 bg-surface-secondary/50 px-4 py-2 text-xs font-medium text-muted">
          <span>Nguồn</span>
          <span>Thao tác</span>
          <span>Thời gian</span>
          <span>Trạng thái</span>
          <span />
        </div>
        {activity.isLoading && (
          <PanelNote>
            <Spinner size="sm" /> Đang đọc lịch sử…
          </PanelNote>
        )}
        {list.map((e) => (
          <Row key={e.id} e={e} open={open === e.id} onToggle={() => setOpen(open === e.id ? null : e.id)} />
        ))}
        {!activity.isLoading && !list.length && (
          <PanelNote>
            {all.length
              ? "Không có mục nào khớp bộ lọc."
              : tab === "signing"
                ? "Chưa có lần ký nào."
                : "Chưa có lệnh gọi nào tới ESIGN hoặc CTSIGN."}
          </PanelNote>
        )}
      </Panel>

      <Dialog
        isOpen={confirm}
        onClose={() => setConfirm(false)}
        title="Xóa toàn bộ lịch sử?"
        footer={
          <>
            <Button variant="tertiary" onPress={() => setConfirm(false)}>
              Hủy
            </Button>
            <Button
              variant="danger"
              isDisabled={clear.isPending}
              onPress={() =>
                clear.mutate(undefined, {
                  onSuccess: () => {
                    setConfirm(false);
                    toast.ok("Đã xóa lịch sử");
                  },
                  onError: (e) => toast.error("Không xóa được", e),
                })
              }
            >
              Xóa
            </Button>
          </>
        }
      >
        Lịch sử ký và lệnh gọi API sẽ bị xóa khỏi máy này. Tệp PDF đã ký không bị ảnh hưởng.
      </Dialog>
    </Page>
  );
}
