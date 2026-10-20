import { cn, Tooltip } from "@heroui/react";
import { Button } from "react-aria-components";
import type { ServiceState } from "../../ipc/api";
import { useServices } from "../../queries";
import { useApp } from "../../stores/app";
import { StatusDot } from "../ui/StatusChip";
import { ServiceLogo, serviceBrands } from "../ServiceBrand";
import { navGroups, settingsNav, type NavItem } from "./nav";

function NavLink({ item, rail }: { item: NavItem; rail: boolean }) {
  const route = useApp((s) => s.route);
  const setRoute = useApp((s) => s.setRoute);
  const active = route === item.route;
  const Icon = item.icon;
  const link = (
    <Button
      aria-label={item.label}
      aria-current={active ? "page" : undefined}
      onPress={() => setRoute(item.route)}
      className={cn(
        "flex h-8 w-full cursor-pointer items-center gap-2.5 rounded-lg px-2.5 text-[13px] font-medium outline-none transition-colors",
        "focus-visible:ring-2 focus-visible:ring-focus",
        rail && "justify-center px-0",
        active
          ? "bg-surface text-foreground shadow-[var(--card-shadow)] ring-1 ring-border"
          : "text-foreground/70 hover:bg-foreground/[0.05] hover:text-foreground",
      )}
    >
      <Icon className={cn("size-4 shrink-0", active ? "text-accent" : "text-muted")} strokeWidth={2} />
      {!rail && <span className="truncate">{item.label}</span>}
    </Button>
  );
  if (!rail) return link;
  return (
    <Tooltip delay={200} closeDelay={0}>
      {link}
      <Tooltip.Content placement="right">{item.label}</Tooltip.Content>
    </Tooltip>
  );
}

function ServiceLine({ service, state, rail }: { service: keyof typeof serviceBrands; state?: ServiceState; rail: boolean }) {
  const { name, version } = serviceBrands[service];
  const tone = state?.running ? "ok" : state?.error ? "fail" : "neutral";
  const port = state?.address?.split(":")[1];
  const title = `${name} ${version}: ${port ? `cổng ${port}` : (state?.error ?? "tắt")}`;
  if (rail)
    return (
      <span title={title} className="relative grid size-6 place-items-center">
        <ServiceLogo service={service} className="size-6 object-contain" />
        <span className="absolute -right-1 -bottom-0.5"><StatusDot tone={tone} /></span>
      </span>
    );
  return (
    <div className="flex items-center gap-2 text-xs" title={title}>
      <ServiceLogo service={service} className="size-5 shrink-0 object-contain" />
      <StatusDot tone={tone} />
      <span className="font-medium">{name}</span>
      <span className="ml-auto font-mono text-[11px] text-muted">{port ?? "tắt"}</span>
    </div>
  );
}

export function Sidebar({ rail }: { rail: boolean }) {
  const services = useServices();
  const setRoute = useApp((s) => s.setRoute);
  return (
    <nav aria-label="Điều hướng" className={cn("flex shrink-0 flex-col pb-2", rail ? "w-14 px-2" : "w-[232px] px-3")}>
      {navGroups.map((g) => (
        <div key={g.label} className="mt-3 flex flex-col gap-0.5 first:mt-1">
          {rail ? (
            <div className="mx-2 mb-1.5 h-px bg-border first:hidden" />
          ) : (
            <div className="px-2.5 pb-1 text-[11px] font-medium tracking-wide text-muted uppercase">{g.label}</div>
          )}
          {g.items.map((item) => (
            <NavLink key={item.route} item={item} rail={rail} />
          ))}
        </div>
      ))}
      <div className="mt-auto flex flex-col gap-2">
        <button
          type="button"
          onClick={() => setRoute("services")}
          className={cn(
            "flex cursor-pointer flex-col gap-1.5 rounded-lg border border-border bg-surface/60 text-left transition-colors hover:bg-surface",
            rail ? "items-center px-0 py-2" : "px-3 py-2.5",
          )}
        >
          {!rail && <div className="text-[11px] font-medium tracking-wide text-muted uppercase">Dịch vụ cục bộ</div>}
          <ServiceLine service="esign" state={services.data?.esign} rail={rail} />
          <ServiceLine service="ctsign" state={services.data?.ctsign} rail={rail} />
        </button>
        <NavLink item={settingsNav} rail={rail} />
      </div>
    </nav>
  );
}
