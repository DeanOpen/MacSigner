import { Button } from "@heroui/react";
import { ArrowUpRight, Star } from "lucide-react";
import kit from "../assets/sponsor/nextwaves-kit.webp";
import nextwavesLogo from "../assets/sponsor/nextwaves-logo.png";
import { isTauri } from "../ipc/api";
import { useAppInfo } from "../queries";
import { Panel, Section } from "./ui/Section";

const REPO_URL = "https://github.com/DeanOpen/MacSigner";

/** Sponsor link, tagged so Nextwaves can see visits that came from MacSigner. */
const SPONSOR_URL =
  "https://nextwaves.com/vi?utm_source=MacSigner&utm_medium=app&utm_campaign=sponsor&utm_content=about&ref=MacSigner";

/** External links open in the system browser (the webview's CSP and navigation stay local). */
export function openExternal(url: string) {
  if (!isTauri()) {
    window.open(url, "_blank", "noopener");
    return;
  }
  import("@tauri-apps/plugin-opener").then((m) => m.openUrl(url));
}

function GitHubMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" className={className} fill="currentColor" aria-hidden="true">
      <path d="M8 0C3.58 0 0 3.58 0 8a8 8 0 0 0 5.47 7.59c.4.07.55-.17.55-.38v-1.33c-2.23.48-2.7-1.07-2.7-1.07-.36-.92-.89-1.17-.89-1.17-.73-.5.06-.49.06-.49.8.06 1.23.83 1.23.83.72 1.23 1.88.87 2.34.67.07-.52.28-.87.5-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.6 7.6 0 0 1 4 0c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48v2.2c0 .21.15.46.55.38A8 8 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
    </svg>
  );
}

export function AboutPanel() {
  const info = useAppInfo();
  return (
    <div className="flex flex-col gap-6">
      <Panel>
        <div className="flex items-center gap-4 p-5">
          <img src="/logo.svg" alt="" width={56} height={56} draggable={false} />
          <div className="min-w-0 flex-1">
            <div className="text-base font-semibold">MacSigner {info.data?.version}</div>
            <p className="text-[13px] text-muted">Ký số PDF · plugin thuế điện tử (ESIGN) · API CTSigningHub (CTSIGN)</p>
            <p className="selectable mt-1 truncate font-mono text-xs text-muted">{info.data?.configPath}</p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3 px-5 py-3.5">
          <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-foreground text-background">
            <GitHubMark className="size-5" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="text-[13px] font-medium">Mã nguồn mở trên GitHub</div>
            <div className="truncate font-mono text-xs text-muted">github.com/DeanOpen/MacSigner</div>
          </div>
          <Button size="sm" variant="secondary" onPress={() => openExternal(REPO_URL)}>
            <Star className="size-4" />
            Xem trên GitHub
          </Button>
        </div>
      </Panel>

      <Section title="Nhà tài trợ" description="MacSigner được phát triển với sự đồng hành của Nextwaves.">
        <Panel className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] divide-y-0 max-[900px]:grid-cols-1">
          <div className="flex flex-col justify-between gap-5 p-6">
            <div className="flex flex-col gap-4">
              <img src={nextwavesLogo} alt="Nextwaves" className="sponsor-logo h-14 w-fit" draggable={false} />
              <div>
                <div className="text-[15px] font-semibold">Thiết bị và giải pháp RFID sản xuất tại Việt Nam</div>
                <p className="mt-1 text-[13px] text-muted">
                  Inlay, tem nhãn RFID, ăng-ten, đầu đọc và phần mềm kiểm kho Sense Inventory cho bán lẻ, kho vận và sản xuất.
                </p>
              </div>
            </div>
            <div>
              <Button onPress={() => openExternal(SPONSOR_URL)}>
                Tìm hiểu Nextwaves
                <ArrowUpRight className="size-4" />
              </Button>
            </div>
          </div>
          <button
            type="button"
            aria-label="Mở nextwaves.com"
            onClick={() => openExternal(SPONSOR_URL)}
            className="group relative min-h-48 cursor-pointer overflow-hidden bg-white"
          >
            <img
              src={kit}
              alt="Bộ RFID của Nextwaves: đầu đọc cầm tay, đế đọc, cuộn tem RFID và phần mềm Sense Inventory"
              draggable={false}
              className="absolute inset-0 size-full object-cover transition-transform duration-500 group-hover:scale-[1.03]"
            />
          </button>
        </Panel>
      </Section>
    </div>
  );
}
