import { BadgeCheck, FlaskConical, History, PenLine, Plug, Settings, ShieldCheck, type LucideIcon } from "lucide-react";
import type { Route } from "../../stores/app";

export interface NavItem {
  route: Route;
  label: string;
  icon: LucideIcon;
}

export const navGroups: { label: string; items: NavItem[] }[] = [
  {
    label: "Ký số",
    items: [
      { route: "sign", label: "Ký tài liệu", icon: PenLine },
      { route: "certificates", label: "Chứng thư số", icon: BadgeCheck },
      { route: "verify", label: "Kiểm tra chữ ký", icon: ShieldCheck },
      { route: "history", label: "Lịch sử", icon: History },
    ],
  },
  {
    label: "Hệ thống",
    items: [
      { route: "services", label: "Dịch vụ", icon: Plug },
      { route: "tests", label: "Kiểm thử", icon: FlaskConical },
    ],
  },
];

export const settingsNav: NavItem = { route: "settings", label: "Cài đặt", icon: Settings };
