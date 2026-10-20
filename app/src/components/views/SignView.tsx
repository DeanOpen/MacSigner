import { Button, cn, Spinner } from "@heroui/react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  FileText,
  FolderOpen,
  Image as ImageIcon,
  PenLine,
  RefreshCw,
  ShieldCheck,
  Upload,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import type { PDFDocumentProxy } from "pdfjs-dist";
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { api, isTauri, type Certificate, type Preferences } from "../../ipc/api";
import { openPdf } from "../../lib/pdf";
import { stampLines } from "../../lib/stamp";
import { useCertificates, usePreferences } from "../../queries";
import { fileName, useApp } from "../../stores/app";
import { formatDate, validity } from "../CertCard";
import { PdfViewer } from "../PdfViewer";
import { toast } from "../Toasts";
import { Dialog } from "../ui/Dialog";
import { Dropzone } from "../ui/Dropzone";
import { SecretInput, SelectField, TextInput } from "../ui/Fields";
import { IconButton } from "../ui/IconButton";
import { Panel } from "../ui/Section";
import { Segmented } from "../ui/Segmented";
import { StatusChip } from "../ui/StatusChip";
import { VerifyResults } from "./VerifyView";

export async function pickPdf(): Promise<string | null> {
  if (!isTauri()) return "C:\\Users\\DeanOS\\Downloads\\t.pdf";
  const path = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  return typeof path === "string" ? path : null;
}

/** The template picked on this screen, or the default one from settings. */
function templateIndex(prefs: Preferences | undefined, picked: number | null) {
  if (!prefs) return 0;
  const i = picked ?? prefs.defaultTemplate;
  return i < prefs.templates.length ? i : prefs.defaultTemplate;
}

function StartScreen() {
  const { openFile, recent } = useApp();
  const choose = async () => {
    const path = await pickPdf();
    if (path) openFile(path);
  };
  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto flex max-w-2xl flex-col gap-8 px-8 pt-[10vh] pb-10">
        <div>
          <h1 className="text-xl leading-8 font-semibold tracking-tight">Ký tài liệu</h1>
          <p className="text-[13px] text-muted">Chọn tệp PDF, đặt vị trí chữ ký rồi ký bằng USB token hoặc chứng thư.</p>
        </div>
        <Dropzone
          icon={Upload}
          title="Kéo thả tệp PDF vào đây"
          hint="hoặc chọn tài liệu cần ký số trên máy tính"
          onActivate={choose}
          action={
            <Button onPress={choose}>
              <FolderOpen className="size-4" />
              Chọn tệp PDF
            </Button>
          }
        />
        {recent.length > 0 && (
          <div className="flex flex-col gap-2">
            <div className="text-[11px] font-medium tracking-wide text-muted uppercase">Tài liệu gần đây</div>
            <Panel>
              {recent.map((path) => (
                <button
                  key={path}
                  type="button"
                  onClick={() => openFile(path)}
                  className="flex w-full cursor-pointer items-center gap-3 px-4 py-2.5 text-left transition-colors hover:bg-surface-secondary/60"
                >
                  <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-danger-soft text-danger">
                    <FileText className="size-4" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px] font-medium">{fileName(path)}</span>
                    <span className="block truncate text-xs text-muted">{path.replace(fileName(path), "")}</span>
                  </span>
                </button>
              ))}
            </Panel>
          </div>
        )}
      </div>
    </div>
  );
}

const sourceText: Record<Certificate["source"], string> = {
  store: "Kho Windows",
  file: "Tệp P12",
  "token-session": "USB token",
  token: "USB token · cần PIN",
};

/** Stamp preview in the PDF's "Signed by" bracket style. */
export function StampPreview({ lines }: { lines: string[] }) {
  return (
    <div className="signed-by">
      {lines.map((l, i) => {
        const [k, ...v] = l.split(": ");
        return v.length ? (
          <div key={i}>
            <span className="text-[#555]">{k}:</span> {v.join(": ")}
          </div>
        ) : (
          <div key={i}>{l}</div>
        );
      })}
    </div>
  );
}

function PanelSection({ title, aside, children }: { title: string; aside?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2 px-4 py-4">
      <div className="flex items-center justify-between">
        <div className="text-xs font-semibold tracking-wide text-muted uppercase">{title}</div>
        {aside}
      </div>
      {children}
    </section>
  );
}

function SignPanel({ doc, lines, onSigned }: { doc: PDFDocumentProxy; lines: string[]; onSigned: (output: string) => void }) {
  const app = useApp();
  const certs = useCertificates();
  const prefs = usePreferences();
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const list = certs.data?.certificates ?? [];
  const cert = list.find((c) => c.id === app.certId) ?? list[0];
  const tplIndex = templateIndex(prefs.data, app.templateIndex);
  const template = prefs.data?.templates[tplIndex];

  useEffect(() => {
    if (list[0] && !list.some((c) => c.id === app.certId)) app.setCert(list[0].id);
  }, [list.length]); // eslint-disable-line react-hooks/exhaustive-deps

  const ready = !!cert && !!template && (app.mode === 2 || !!app.placement) && !!app.output;

  const sign = async () => {
    if (!cert || !template || !app.file) return;
    setBusy(true);
    try {
      const result = await api.signPdf({
        input: app.file,
        output: app.output,
        certId: cert.id,
        pin: pin || undefined,
        page: app.placement?.page ?? 1,
        x: app.placement?.x ?? 0,
        y: app.placement?.y ?? 0,
        mode: app.mode,
        template,
        note: app.note,
        reason: prefs.data?.reason,
        location: prefs.data?.location,
      });
      setConfirming(false);
      toast.ok("Đã ký số", `${fileName(result.output)} · ${result.signatures} chữ ký`);
      onSigned(result.output);
    } catch (e) {
      toast.error("Không ký được tài liệu", e);
    } finally {
      setPin("");
      setBusy(false);
    }
  };

  const chooseOutput = async () => {
    if (!isTauri()) return;
    const path = await save({ defaultPath: app.output, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (path) app.setOutput(path);
  };

  const placed = !!app.placement || app.mode === 2;
  const hint =
    app.mode === 2
      ? "Chữ ký không hiển thị trên trang."
      : app.placement
        ? `Trang ${app.placement.page} · kéo hoặc bấm chỗ khác để di chuyển`
        : "Bấm vào trang để đặt chữ ký";

  return (
    <aside className="flex w-[22rem] shrink-0 flex-col border-l border-border bg-surface max-[1100px]:w-80">
      <div className="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
        <div className="text-sm font-semibold">Ký tài liệu</div>
        <IconButton label="Tìm lại chứng thư" placement="left" onPress={() => certs.rescan.mutate()} isDisabled={certs.rescan.isPending}>
          <RefreshCw className={cn("size-4", certs.rescan.isPending && "animate-spin")} />
        </IconButton>
      </div>
      <div className="flex-1 divide-y divide-separator overflow-y-auto">
        <PanelSection title="Chứng thư">
          {certs.isLoading ? (
            <div className="flex items-center gap-2 text-[13px] text-muted">
              <Spinner size="sm" /> Đang đọc chứng thư…
            </div>
          ) : list.length ? (
            <div role="radiogroup" aria-label="Chứng thư" className="flex flex-col gap-1.5">
              {list.map((c) => {
                const st = validity(c);
                const on = c.id === cert?.id;
                return (
                  <label
                    key={c.id}
                    className={cn(
                      "flex cursor-pointer gap-3 rounded-lg border p-3 transition-colors",
                      on ? "border-accent bg-accent-soft/40 ring-1 ring-accent" : "border-border hover:bg-surface-secondary/60",
                    )}
                  >
                    <input type="radio" name="cert" className="sr-only" checked={on} onChange={() => app.setCert(c.id)} />
                    <span
                      className={cn(
                        "mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border",
                        on ? "border-accent bg-accent" : "border-border-secondary",
                      )}
                    >
                      {on && <span className="size-1.5 rounded-full bg-white" />}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[13px] font-medium">{c.fields.commonName || c.subject}</span>
                      <span className="block truncate text-xs text-muted">
                        {[c.fields.taxCode && `MST ${c.fields.taxCode}`, c.issuerName, sourceText[c.source]].filter(Boolean).join(" · ")}
                      </span>
                      <span className="mt-1.5 flex items-center gap-2">
                        <StatusChip tone={st.tone}>{st.text}</StatusChip>
                        <span className="text-xs text-muted">đến {formatDate(c.validTo)}</span>
                      </span>
                    </span>
                  </label>
                );
              })}
            </div>
          ) : (
            <p className="text-[13px] text-muted">Chưa có chứng thư. Cắm USB token rồi bấm làm mới.</p>
          )}
        </PanelSection>

        <PanelSection title="Hiển thị">
          <Segmented
            label="Hiển thị"
            fullWidth
            value={String(app.mode)}
            onChange={(v) => app.setMode(Number(v))}
            items={[
              { id: "0", label: "Chữ ký" },
              { id: "1", label: "Ghi chú" },
              { id: "2", label: "Vô hình" },
            ]}
          />
          {app.mode === 1 && <TextInput label="Ghi chú" hideLabel placeholder="Nội dung ghi chú" value={app.note} onChange={app.setNote} />}
          <p className={cn("flex items-center gap-1.5 text-xs", placed ? "text-muted" : "font-medium text-accent-text")}>
            {!placed && <span className="size-1.5 animate-pulse rounded-full bg-accent" />}
            {hint}
          </p>
        </PanelSection>

        {app.mode === 0 && prefs.data && (
          <PanelSection
            title="Mẫu"
            aside={
              <button type="button" className="cursor-pointer text-xs font-medium text-accent-text hover:underline" onClick={() => app.setRoute("settings")}>
                Sửa mẫu
              </button>
            }
          >
            <SelectField
              label="Mẫu"
              hideLabel
              value={String(tplIndex)}
              onChange={(v) => app.setTemplateIndex(Number(v))}
              options={prefs.data.templates.map((t, i) => ({ value: String(i), label: t.name }))}
            />
          </PanelSection>
        )}

        <PanelSection title="Lưu thành">
          <div className="flex items-end gap-1.5">
            <TextInput className="min-w-0 flex-1" label="Lưu thành" hideLabel value={app.output} onChange={app.setOutput} />
            <IconButton label="Chọn nơi lưu" variant="secondary" size="md" placement="top" onPress={chooseOutput}>
              <FolderOpen className="size-4" />
            </IconButton>
          </div>
        </PanelSection>
      </div>
      <div className="shrink-0 border-t border-border p-4">
        <Button fullWidth size="lg" isDisabled={!ready || busy} onPress={() => setConfirming(true)}>
          <PenLine className="size-4" />
          Ký số
        </Button>
      </div>

      <Dialog
        isOpen={confirming}
        onClose={() => setConfirming(false)}
        isDismissable={!busy}
        title="Xác nhận ký số"
        footer={
          <>
            <Button variant="tertiary" isDisabled={busy} onPress={() => setConfirming(false)}>
              Hủy
            </Button>
            <Button isDisabled={busy} onPress={sign}>
              {busy ? <Spinner size="sm" color="current" /> : <PenLine className="size-4" />}
              {busy ? "Đang ký…" : "Ký số"}
            </Button>
          </>
        }
      >
        {cert && (
          <div className="flex flex-col gap-4">
            {app.mode !== 2 && <StampPreview lines={lines} />}
            <dl className="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]">
              <dt className="text-muted">Tài liệu</dt>
              <dd className="truncate">
                {fileName(app.file ?? "")} · {doc.numPages} trang
              </dd>
              <dt className="text-muted">Chứng thư</dt>
              <dd className="truncate">
                {cert.fields.commonName} · {cert.issuerName}
              </dd>
              <dt className="text-muted">Lưu thành</dt>
              <dd className="truncate">{fileName(app.output)}</dd>
            </dl>
            {cert.needsPin && <SecretInput label="Mã PIN token" value={pin} onChange={setPin} showLabel="Hiện mã PIN" hideLabel="Ẩn mã PIN" autoFocus />}
            <p className="text-xs text-muted">
              Chữ ký số có giá trị pháp lý như chữ ký tay. {cert.source === "store" ? "Windows sẽ hỏi PIN của token nếu cần." : ""}
            </p>
          </div>
        )}
      </Dialog>
    </aside>
  );
}

export function SignView() {
  const app = useApp();
  const certs = useCertificates();
  const prefs = usePreferences();
  const [doc, setDoc] = useState<PDFDocumentProxy | null>(null);
  const [loading, setLoading] = useState(false);
  const [page, setPage] = useState(1);
  const [jump, setJump] = useState<number | null>(null);
  const [verifyOpen, setVerifyOpen] = useState(false);

  useEffect(() => {
    if (!app.file) {
      setDoc(null);
      return;
    }
    let alive = true;
    setLoading(true);
    api
      .readFile(app.file)
      .then(openPdf)
      .then((d) => alive && (setDoc(d), setPage(1)))
      .catch((e) => {
        toast.error("Không mở được PDF", e);
        app.openFile(null);
      })
      .finally(() => alive && setLoading(false));
    api.defaultOutput(app.file).then(app.setOutput);
    return () => {
      alive = false;
    };
  }, [app.file]); // eslint-disable-line react-hooks/exhaustive-deps

  const cert = certs.data?.certificates.find((c) => c.id === app.certId) ?? certs.data?.certificates[0];
  const template = prefs.data?.templates[templateIndex(prefs.data, app.templateIndex)];
  const lines = useMemo(() => (template ? stampLines(template, cert?.fields, app.mode, app.note) : []), [template, cert, app.mode, app.note]);

  const stamp = useCallback(
    (scale: number) => (
      <div style={{ display: "flex", alignItems: "center", gap: 6 * scale, height: "100%", fontSize: 7.4 * scale, lineHeight: 1.25 }}>
        {app.mode === 0 && template && template.appearance !== 2 && template.image_path && (
          <div className="stamp-image">
            <ImageIcon size={Math.round(14 * scale)} />
          </div>
        )}
        <div className="stamp-text">
          {lines.map((l, i) => (
            <div key={i} style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
              {l}
            </div>
          ))}
        </div>
      </div>
    ),
    [lines, app.mode, template],
  );
  const onPageChange = useCallback((p: number) => setPage(p), []);

  if (!app.file) return <StartScreen />;

  return (
    <div className="flex h-full min-h-0">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-12 shrink-0 items-center gap-1 border-b border-border px-2">
          <IconButton label="Đóng tài liệu" onPress={() => app.openFile(null)}>
            <ArrowLeft className="size-4" />
          </IconButton>
          <FileText className="mr-1 ml-1 size-4 shrink-0 text-danger" />
          <span className="min-w-0 truncate text-[13px] font-medium" title={app.file}>
            {fileName(app.file)}
          </span>
          <div className="mx-auto flex shrink-0 items-center gap-0.5 px-2">
            <IconButton label="Trang trước" isDisabled={page <= 1} onPress={() => setJump(Math.max(1, page - 1))}>
              <ChevronLeft className="size-4" />
            </IconButton>
            <span className="min-w-14 text-center text-xs text-muted tabular-nums">
              {page} / {doc?.numPages ?? "–"}
            </span>
            <IconButton label="Trang sau" isDisabled={!doc || page >= doc.numPages} onPress={() => setJump(Math.min(doc?.numPages ?? 1, page + 1))}>
              <ChevronRight className="size-4" />
            </IconButton>
            <span className="mx-1.5 h-5 w-px bg-border" />
            <IconButton label="Thu nhỏ" onPress={() => app.setZoom(app.zoom - 0.1)}>
              <ZoomOut className="size-4" />
            </IconButton>
            <Button variant="ghost" size="sm" className="min-w-14 px-1.5 text-xs tabular-nums" onPress={() => app.setZoom(1)} aria-label="Về 100%">
              {Math.round(app.zoom * 100)}%
            </Button>
            <IconButton label="Phóng to" onPress={() => app.setZoom(app.zoom + 0.1)}>
              <ZoomIn className="size-4" />
            </IconButton>
          </div>
          <Button variant="secondary" size="sm" className="shrink-0 max-[1200px]:hidden" onPress={() => setVerifyOpen(true)}>
            <ShieldCheck className="size-4" />
            Kiểm tra chữ ký
          </Button>
          <span className="min-[1201px]:hidden">
            <IconButton label="Kiểm tra chữ ký" onPress={() => setVerifyOpen(true)}>
              <ShieldCheck className="size-4" />
            </IconButton>
          </span>
        </div>
        {loading || !doc ? (
          <div className="pdf-canvas grid flex-1 place-items-center">
            <div className="flex items-center gap-2 text-[13px] text-muted">
              <Spinner size="sm" /> Đang mở tài liệu…
            </div>
          </div>
        ) : (
          <PdfViewer doc={doc} stamp={stamp} onPageChange={onPageChange} scrollTo={jump} />
        )}
      </div>
      {doc && (
        <SignPanel
          doc={doc}
          lines={lines}
          onSigned={(output) => {
            app.openFile(output);
            setVerifyOpen(true);
          }}
        />
      )}
      <Dialog isOpen={verifyOpen} onClose={() => setVerifyOpen(false)} title="Kiểm tra chữ ký" description={fileName(app.file)} className="sm:max-w-[640px]">
        {verifyOpen && <VerifyResults path={app.file} />}
      </Dialog>
    </div>
  );
}
