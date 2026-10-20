import { Button, Checkbox, cn, Label, Radio, RadioGroup, Tabs } from "@heroui/react";
import { open } from "@tauri-apps/plugin-dialog";
import { Clock, Image as ImageIcon, MapPin, Monitor, Moon, NotebookPen, Plus, Sun, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { isTauri, type CertFields, type Preferences, type SignatureTemplate } from "../../ipc/api";
import { stampLines } from "../../lib/stamp";
import { useCertificates, usePreferences, useSavePreferences } from "../../queries";
import { useApp, type Theme } from "../../stores/app";
import { AboutPanel } from "../About";
import { Page } from "../Page";
import { toast } from "../Toasts";
import { TextInput } from "../ui/Fields";
import { IconButton } from "../ui/IconButton";
import { Panel, Section, SettingRow } from "../ui/Section";
import { Toggle } from "../ui/Toggle";

const emptyTemplate = (): SignatureTemplate => ({
  name: "Mẫu mới",
  appearance: 2,
  organization: false,
  label: true,
  email: false,
  unit: false,
  position: "",
  signing_time: true,
  image_path: "",
});

// Stand-ins when no certificate is available, so every ticked field shows in the preview.
const sampleFields: CertFields = {
  commonName: "Nguyễn Văn A",
  organization: "Công ty TNHH ABC",
  unit: "Phòng Kế toán",
  email: "nguyenvana@abc.vn",
  taxCode: "",
};

// With a certificate, show its real values and flag the fields it lacks (those are not printed).
function previewFields(cert: CertFields | undefined): CertFields {
  if (!cert) return sampleFields;
  const missing = "(chứng thư không có)";
  return {
    commonName: cert.commonName || missing,
    organization: cert.organization || missing,
    unit: cert.unit || missing,
    email: cert.email || missing,
    taxCode: cert.taxCode,
  };
}

const themes: { value: Theme; label: string; icon: typeof Sun }[] = [
  { value: "system", label: "Theo hệ thống", icon: Monitor },
  { value: "light", label: "Sáng", icon: Sun },
  { value: "dark", label: "Tối", icon: Moon },
];

const appearances = [
  { value: "0", label: "Ảnh và chữ" },
  { value: "1", label: "Chỉ ảnh" },
  { value: "2", label: "Chỉ chữ" },
];

function Check({ label, isSelected, onChange }: { label: string; isSelected: boolean; onChange: (v: boolean) => void }) {
  return (
    <Checkbox isSelected={isSelected} onChange={onChange}>
      <Checkbox.Content>
        <Checkbox.Control>
          <Checkbox.Indicator />
        </Checkbox.Control>
        <span className="text-[13px]">{label}</span>
      </Checkbox.Content>
    </Checkbox>
  );
}

function TemplateEditor({ prefs, onSave }: { prefs: Preferences; onSave: (p: Preferences) => void }) {
  const [index, setIndex] = useState(prefs.defaultTemplate);
  const [draft, setDraft] = useState<SignatureTemplate>(prefs.templates[prefs.defaultTemplate]);
  useEffect(() => setDraft(prefs.templates[index] ?? emptyTemplate()), [index, prefs.templates]);
  const set = (patch: Partial<SignatureTemplate>) => setDraft((d) => ({ ...d, ...patch }));
  const setSignTemplate = useApp((s) => s.setTemplateIndex);
  const persist = (templates: SignatureTemplate[], defaultTemplate = prefs.defaultTemplate) => {
    // Indices may shift; let the sign screen follow the (new) default again.
    setSignTemplate(null);
    onSave({ ...prefs, templates, defaultTemplate: Math.min(defaultTemplate, templates.length - 1) });
  };
  const pickImage = async () => {
    if (!isTauri()) return;
    const p = await open({ multiple: false, filters: [{ name: "Ảnh", extensions: ["png", "jpg", "jpeg"] }] });
    if (typeof p === "string") set({ image_path: p, appearance: draft.appearance === 2 ? 0 : draft.appearance });
  };
  const certs = useCertificates();
  const certId = useApp((s) => s.certId);
  const cert = certs.data?.certificates.find((c) => c.id === certId) ?? certs.data?.certificates[0];
  const lines = stampLines(draft, previewFields(cert?.fields), 0, "");

  return (
    <div className="grid grid-cols-[13rem_minmax(0,1fr)] gap-6 max-[1100px]:grid-cols-1">
      <div className="flex flex-col gap-1">
        {prefs.templates.map((t, i) => (
          <button
            key={i}
            type="button"
            onClick={() => setIndex(i)}
            className={cn(
              "flex h-9 cursor-pointer items-center gap-2 rounded-lg px-3 text-left text-[13px] transition-colors",
              i === index ? "bg-default font-medium text-foreground" : "text-foreground/75 hover:bg-default/60",
            )}
          >
            <NotebookPen className={cn("size-4 shrink-0", i === index ? "text-accent" : "text-muted")} />
            <span className="flex-1 truncate">{(i === index ? draft.name : t.name) || "(Chưa đặt tên)"}</span>
            {i === prefs.defaultTemplate && <span className="text-[11px] text-muted">Mặc định</span>}
          </button>
        ))}
        <Button
          variant="ghost"
          size="sm"
          className="justify-start"
          onPress={() => {
            persist([...prefs.templates, emptyTemplate()]);
            setIndex(prefs.templates.length);
          }}
        >
          <Plus className="size-4" />
          Thêm mẫu
        </Button>
      </div>

      <Panel>
        <div className="flex flex-col gap-5 p-5">
          <TextInput label="Tên mẫu" value={draft.name} onChange={(v) => set({ name: v })} />
          <RadioGroup value={String(draft.appearance)} onChange={(v) => set({ appearance: Number(v) })} orientation="horizontal">
            <Label>Kiểu hiển thị</Label>
            <div className="flex flex-wrap gap-x-6 gap-y-2">
              {appearances.map((a) => (
                <Radio key={a.value} value={a.value}>
                  <Radio.Content>
                    <Radio.Control>
                      <Radio.Indicator />
                    </Radio.Control>
                    <span className="text-[13px]">{a.label}</span>
                  </Radio.Content>
                </Radio>
              ))}
            </div>
          </RadioGroup>
          <fieldset className="flex flex-col gap-2">
            <legend className="mb-2 text-sm font-medium">Thông tin hiển thị</legend>
            <div className="flex flex-wrap gap-x-6 gap-y-2">
              <Check label="Người ký" isSelected={draft.label} onChange={(v) => set({ label: v })} />
              <Check label="Tổ chức" isSelected={draft.organization} onChange={(v) => set({ organization: v })} />
              <Check label="Đơn vị" isSelected={draft.unit} onChange={(v) => set({ unit: v })} />
              <Check label="Email" isSelected={draft.email} onChange={(v) => set({ email: v })} />
              <Check label="Thời gian ký" isSelected={draft.signing_time} onChange={(v) => set({ signing_time: v })} />
            </div>
            <p className="text-xs text-muted">
              {cert
                ? `Lấy tự động từ chứng thư số đang chọn (${cert.fields.commonName || cert.subject}), không cần nhập.`
                : "Lấy tự động từ chứng thư số khi ký; xem trước đang dùng dữ liệu mẫu."}
            </p>
          </fieldset>
          <div className="grid grid-cols-2 items-end gap-4 max-[1100px]:grid-cols-1">
            <TextInput label="Chức vụ" placeholder="Giám đốc" value={draft.position} onChange={(v) => set({ position: v })} />
            <div className="flex items-end gap-2">
              <TextInput className="min-w-0 flex-1" label="Ảnh chữ ký / con dấu" placeholder="PNG hoặc JPG" value={draft.image_path} onChange={(v) => set({ image_path: v })} />
              <IconButton label="Chọn ảnh" variant="secondary" size="md" onPress={pickImage}>
                <ImageIcon className="size-4" />
              </IconButton>
            </div>
          </div>
          <div>
            <div className="mb-1.5 text-sm font-medium">Xem trước</div>
            <div className="pdf-canvas grid place-items-center rounded-lg border border-border p-8">
              <div className="rounded-sm bg-white p-4 shadow-[0_1px_3px_rgb(16_24_40/0.12)]">
                <div className="stamp-preview">
                  {draft.appearance !== 2 && draft.image_path && (
                    <div className="stamp-image">
                      <ImageIcon size={18} />
                    </div>
                  )}
                  {draft.appearance !== 1 && (
                    <div className="min-w-0">
                      {lines.map((l, i) => (
                        <div key={i} className="truncate">
                          {l}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-2 bg-surface-secondary/50 px-5 py-3">
          <Button
            variant="danger-soft"
            size="sm"
            isDisabled={prefs.templates.length <= 1}
            onPress={() => {
              persist(
                prefs.templates.filter((_, i) => i !== index),
                index < prefs.defaultTemplate ? prefs.defaultTemplate - 1 : prefs.defaultTemplate,
              );
              setIndex(0);
            }}
          >
            <Trash2 className="size-4" />
            Xóa mẫu
          </Button>
          <div className="flex gap-2">
            <Button variant="secondary" size="sm" isDisabled={index === prefs.defaultTemplate} onPress={() => persist(prefs.templates, index)}>
              Đặt mặc định
            </Button>
            <Button size="sm" onPress={() => persist(prefs.templates.map((t, i) => (i === index ? draft : t)))}>
              Lưu mẫu
            </Button>
          </div>
        </div>
      </Panel>
    </div>
  );
}

export function SettingsView() {
  const app = useApp();
  const prefs = usePreferences();
  const save = useSavePreferences();
  const persist = (p: Preferences) =>
    save.mutate(p, { onSuccess: () => toast.ok("Đã lưu"), onError: (e) => toast.error("Không lưu được", e) });
  const p = prefs.data;

  return (
    <Page title="Cài đặt" subtitle="Mẫu chữ ký, thông tin ký mặc định và giao diện.">
      <Tabs variant="secondary" defaultSelectedKey="templates">
        <Tabs.ListContainer>
          <Tabs.List aria-label="Cài đặt">
            <Tabs.Tab id="templates">
              Mẫu chữ ký PDF
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="signing">
              Thông tin ký
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="appearance">
              Giao diện
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="about">
              Giới thiệu
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>

        <Tabs.Panel id="templates" className="pt-6">
          {p && <TemplateEditor prefs={p} onSave={persist} />}
        </Tabs.Panel>

        <Tabs.Panel id="signing" className="pt-6">
          {p && (
            <Section title="Thông tin ký mặc định" description="Ghi vào chữ ký số của mọi tài liệu, có thể đổi khi ký.">
              <Panel>
                <SettingRow icon={NotebookPen} title="Lý do ký">
                  <TextInput className="w-72" label="Lý do ký" hideLabel placeholder="Phê duyệt tài liệu" defaultValue={p.reason} onBlur={(v) => persist({ ...p, reason: v })} />
                </SettingRow>
                <SettingRow icon={MapPin} title="Nơi ký">
                  <TextInput className="w-72" label="Nơi ký" hideLabel placeholder="TP. Hồ Chí Minh" defaultValue={p.location} onBlur={(v) => persist({ ...p, location: v })} />
                </SettingRow>
                <SettingRow icon={Clock} title="Dấu thời gian (TSA)" description="Gắn dấu thời gian RFC 3161 từ máy chủ TSA.">
                  <Toggle label="Dấu thời gian (TSA)" isSelected={p.timestampEnabled} onChange={(v) => persist({ ...p, timestampEnabled: v })} />
                </SettingRow>
                {p.timestampEnabled && (
                  <div className="px-4 py-3.5">
                    <TextInput
                      label="Địa chỉ máy chủ TSA"
                      description="Địa chỉ được lưu; gửi yêu cầu RFC 3161 khi ký chưa được hỗ trợ."
                      placeholder="https://tsa.example.vn"
                      defaultValue={p.timestampUrl}
                      onBlur={(v) => persist({ ...p, timestampUrl: v })}
                    />
                  </div>
                )}
              </Panel>
            </Section>
          )}
        </Tabs.Panel>

        <Tabs.Panel id="appearance" className="pt-6">
          <Section title="Giao diện" description="Chọn sáng, tối hoặc theo cài đặt của Windows.">
            <RadioGroup aria-label="Giao diện" value={app.theme} onChange={(v) => app.setTheme(v as Theme)} orientation="horizontal">
              <div className="grid grid-cols-3 gap-3">
                {themes.map((t) => {
                  const Icon = t.icon;
                  const on = app.theme === t.value;
                  return (
                    <Radio
                      key={t.value}
                      value={t.value}
                      className={cn(
                        "cursor-pointer rounded-xl border bg-surface p-4 transition-colors",
                        on ? "border-accent ring-1 ring-accent" : "border-border hover:border-border-secondary",
                      )}
                    >
                      <Radio.Content className="flex w-full items-center gap-3">
                        <span className={cn("grid size-9 place-items-center rounded-lg", on ? "bg-accent-soft text-accent" : "bg-default text-muted")}>
                          <Icon className="size-4" />
                        </span>
                        <span className="flex-1 text-[13px] font-medium">{t.label}</span>
                        <Radio.Control>
                          <Radio.Indicator />
                        </Radio.Control>
                      </Radio.Content>
                    </Radio>
                  );
                })}
              </div>
            </RadioGroup>
          </Section>
        </Tabs.Panel>

        <Tabs.Panel id="about" className="pt-6">
          <AboutPanel />
        </Tabs.Panel>
      </Tabs>
    </Page>
  );
}
