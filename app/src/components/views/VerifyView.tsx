import { Alert, Button, Spinner } from "@heroui/react";
import { useQuery } from "@tanstack/react-query";
import { CircleCheck, CircleDashed, CircleX, FileText, FolderOpen, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { api, type SignatureReport } from "../../ipc/api";
import { fileName } from "../../stores/app";
import { Page } from "../Page";
import { Dropzone } from "../ui/Dropzone";
import { KeyValueList } from "../ui/KeyValueList";
import { Panel } from "../ui/Section";
import { StatusChip } from "../ui/StatusChip";
import { pickPdf } from "./SignView";

function Mark({ ok }: { ok: boolean | null }) {
  if (ok === null) return <CircleDashed className="size-4 text-muted" />;
  return ok ? <CircleCheck className="size-4 text-success" /> : <CircleX className="size-4 text-danger" />;
}

function ReportCard({ report, index }: { report: SignatureReport; index: number }) {
  return (
    <Panel>
      <div className="flex items-center gap-3 px-4 py-3.5">
        <span
          className={
            report.valid
              ? "grid size-9 place-items-center rounded-full bg-success-soft text-success"
              : "grid size-9 place-items-center rounded-full bg-danger-soft text-danger"
          }
        >
          {report.valid ? <CircleCheck className="size-5" /> : <CircleX className="size-5" />}
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-semibold">{report.signer || "Không rõ người ký"}</div>
          <div className="text-xs text-muted">
            Chữ ký {index + 1} · {report.signingTime || "không có thời gian ký"}
          </div>
        </div>
        <StatusChip tone={report.valid ? "ok" : "fail"}>{report.valid ? "Hợp lệ" : "Không hợp lệ"}</StatusChip>
      </div>
      <KeyValueList
        items={[
          { icon: <Mark ok={report.digestOk} />, label: "Toàn vẹn dữ liệu", value: report.digestOk ? "Không bị thay đổi" : "Đã bị thay đổi sau khi ký" },
          { icon: <Mark ok={report.signatureOk} />, label: "Chữ ký mật mã", value: report.signatureOk ? "Đúng với chứng thư" : "Không khớp chứng thư" },
          {
            icon: <Mark ok={report.coversWholeFile ? true : null} />,
            label: "Phạm vi",
            value: report.coversWholeFile ? "Toàn bộ tài liệu" : "Có bổ sung sau khi ký (ví dụ chữ ký khác)",
          },
          {
            icon: <Mark ok={report.certificateValidNow} />,
            label: "Chứng thư",
            value: report.certificateValidNow ? "Còn hiệu lực" : "Hết hoặc chưa có hiệu lực",
          },
          { icon: <Mark ok={null} />, label: "Đơn vị cấp", value: report.issuer || "—" },
          { icon: <Mark ok={null} />, label: "Số sê-ri", value: report.serial || "—", mono: true },
        ]}
      />
      {report.error && (
        <div className="p-3">
          <Alert status="danger">
            <Alert.Indicator />
            <Alert.Content>
              <Alert.Title>Lỗi</Alert.Title>
              <Alert.Description>{report.error}</Alert.Description>
            </Alert.Content>
          </Alert>
        </div>
      )}
    </Panel>
  );
}

export function VerifyResults({ path }: { path: string }) {
  const { data, isLoading, error } = useQuery({ queryKey: ["verify", path], queryFn: () => api.verifyPdf(path) });
  if (isLoading)
    return (
      <div className="flex items-center gap-2 py-6 text-[13px] text-muted">
        <Spinner size="sm" /> Đang kiểm tra chữ ký…
      </div>
    );
  if (error)
    return (
      <Alert status="danger">
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>Không kiểm tra được</Alert.Title>
          <Alert.Description>{String(error)}</Alert.Description>
        </Alert.Content>
      </Alert>
    );
  if (!data?.length)
    return (
      <Alert>
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>Tài liệu chưa có chữ ký số</Alert.Title>
        </Alert.Content>
      </Alert>
    );
  const valid = data.every((r) => r.valid);
  return (
    <div className="flex flex-col gap-4">
      <Alert status={valid ? "success" : "danger"}>
        <Alert.Indicator />
        <Alert.Content>
          <Alert.Title>{valid ? `Tất cả ${data.length} chữ ký hợp lệ` : "Có chữ ký không hợp lệ"}</Alert.Title>
          <Alert.Description>Chưa kiểm tra chuỗi CA tin cậy và trạng thái thu hồi (OCSP/CRL).</Alert.Description>
        </Alert.Content>
      </Alert>
      {data.map((r, i) => (
        <ReportCard key={i} report={r} index={i} />
      ))}
    </div>
  );
}

export function VerifyView() {
  const [path, setPath] = useState<string | null>(null);
  const choose = async () => setPath((await pickPdf()) ?? path);
  return (
    <Page
      title="Kiểm tra chữ ký"
      subtitle="Xác minh toàn vẹn và chữ ký số của tài liệu PDF"
      width="narrow"
      actions={
        path && (
          <Button variant="secondary" size="sm" onPress={choose}>
            <FolderOpen className="size-4" />
            Chọn PDF khác
          </Button>
        )
      }
    >
      {path ? (
        <>
          <div className="flex items-center gap-2.5 rounded-lg border border-border bg-surface-secondary/50 px-3 py-2">
            <FileText className="size-4 shrink-0 text-danger" />
            <span className="truncate text-[13px] font-medium" title={path}>
              {fileName(path)}
            </span>
          </div>
          <VerifyResults path={path} />
        </>
      ) : (
        <Dropzone
          icon={ShieldCheck}
          title="Chọn tài liệu để kiểm tra"
          hint="Kiểm tra từng chữ ký trong tệp: dữ liệu có bị sửa không, chữ ký có khớp chứng thư không."
          onActivate={choose}
          action={
            <Button onPress={choose}>
              <FolderOpen className="size-4" />
              Chọn tệp PDF
            </Button>
          }
        />
      )}
    </Page>
  );
}
