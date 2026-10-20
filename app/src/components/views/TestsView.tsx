import { Button, Spinner } from "@heroui/react";
import { useMutation } from "@tanstack/react-query";
import { FlaskConical, Play } from "lucide-react";
import { useState } from "react";
import { api, type Check } from "../../ipc/api";
import { useCertificates } from "../../queries";
import { Page } from "../Page";
import { toast } from "../Toasts";
import { CodeBlock } from "../ui/CodeBlock";
import { SelectField, TextInput } from "../ui/Fields";
import { Panel, PanelNote, Section } from "../ui/Section";
import { StatusChip, type Tone } from "../ui/StatusChip";

const outcome: Record<Check["outcome"], { tone: Tone; text: string }> = {
  pass: { tone: "ok", text: "Đạt" },
  fail: { tone: "fail", text: "Lỗi" },
  skip: { tone: "neutral", text: "Bỏ qua" },
};

function ModuleTag({ children }: { children: string }) {
  return (
    <span className="w-16 shrink-0 rounded-md bg-default px-1.5 py-0.5 text-center font-mono text-[10px] font-medium tracking-wide text-muted uppercase">
      {children}
    </span>
  );
}

export function TestsView() {
  const certs = useCertificates();
  const [text, setText] = useState("Nội dung ký thử");
  const [module, setModule] = useState<"esign" | "ctsign">("ctsign");
  const [certChoice, setCertChoice] = useState<string>("sample");
  const [result, setResult] = useState("");
  const tests = useMutation({ mutationFn: () => api.runSelftest("all") });
  const sample = useMutation({
    mutationFn: () => api.sampleSign(module, text, certChoice === "sample" ? undefined : certChoice),
    onSuccess: (r) => {
      setResult(r);
      toast.ok("Ký thử thành công", "Chữ ký khớp khóa công khai của chứng thư.");
    },
    onError: (e) => toast.error("Ký thử thất bại", e),
  });
  const list = certs.data?.certificates ?? [];
  const failed = tests.data?.filter((c) => c.outcome === "fail").length ?? 0;

  return (
    <Page title="Kiểm thử" subtitle="Kiểm tra ESIGN, CTSIGN và chứng thư mà không cần tài liệu.">
      <Section
        title="Tự kiểm tra"
        description={
          tests.data
            ? failed
              ? `${failed} mục lỗi trên ${tests.data.length}`
              : `Tất cả ${tests.data.length} mục đạt hoặc bỏ qua`
            : "Gọi ESIGN và CTSIGN bằng client thật, so chữ ký với bản gốc Java/.NET."
        }
        actions={
          <Button size="sm" variant="secondary" isDisabled={tests.isPending} onPress={() => tests.mutate()}>
            {tests.isPending ? <Spinner size="sm" color="current" /> : <Play className="size-4" />}
            {tests.isPending ? "Đang chạy…" : tests.data ? "Chạy lại" : "Chạy"}
          </Button>
        }
      >
        <Panel>
          {tests.isPending && (
            <PanelNote>
              <Spinner size="sm" /> Đang kiểm tra…
            </PanelNote>
          )}
          {!tests.isPending && !tests.data && (
            <PanelNote>
              <FlaskConical className="size-4" /> Chưa chạy. Bấm Chạy để kiểm tra toàn bộ.
            </PanelNote>
          )}
          {tests.data?.map((c, i) => (
            <div key={i} className="flex items-center gap-3 px-4 py-3">
              <ModuleTag>{c.module}</ModuleTag>
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px] font-medium">{c.name}</div>
                <div className="truncate text-xs text-muted" title={c.detail}>
                  {c.detail}
                </div>
              </div>
              <StatusChip tone={outcome[c.outcome].tone}>{outcome[c.outcome].text}</StatusChip>
            </div>
          ))}
        </Panel>
      </Section>

      <Section title="Ký thử" description="Ký một đoạn văn bản để kiểm tra chứng thư và PIN mà không cần tài liệu.">
        <Panel>
          <div className="flex flex-wrap items-end gap-3 p-4">
            <TextInput className="min-w-48 flex-[2]" label="Nội dung" value={text} onChange={setText} />
            <SelectField
              className="w-32"
              label="Mô-đun"
              value={module}
              onChange={setModule}
              options={[
                { value: "ctsign", label: "CTSIGN" },
                { value: "esign", label: "ESIGN" },
              ]}
            />
            <SelectField
              className="min-w-64 flex-[1.4]"
              label="Chứng thư"
              value={certChoice}
              onChange={setCertChoice}
              options={[
                { value: "sample", label: "Chứng thư mẫu (TEST ONLY)" },
                ...list.map((c) => ({ value: c.id, label: `${c.fields.commonName} · …${c.id.slice(-6)}` })),
              ]}
            />
            <Button isDisabled={sample.isPending || !text} onPress={() => sample.mutate()}>
              {sample.isPending && <Spinner size="sm" color="current" />}
              {sample.isPending ? "Đang ký…" : "Ký thử"}
            </Button>
          </div>
          {result && (
            <div className="p-4">
              <CodeBlock label="Kết quả ký thử">{result}</CodeBlock>
            </div>
          )}
        </Panel>
      </Section>
    </Page>
  );
}
