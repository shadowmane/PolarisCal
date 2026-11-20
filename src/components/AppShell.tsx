import { Button, Toast, Tooltip, useTheme } from "@heroui/react";
import {
  ClipboardCheck,
  Database,
  Gauge,
  LayoutDashboard,
  ListChecks,
  Moon,
  Settings,
  Sparkle,
  Sun,
  type LucideIcon,
} from "lucide-react";
import { NavLink, Outlet } from "react-router";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";

interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
}

const MAIN_NAV: NavItem[] = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard },
  { to: "/assets", label: "Instruments & Systems", icon: Gauge },
  { to: "/calibrations", label: "Calibration Records", icon: ClipboardCheck },
];

const ADMIN_NAV: NavItem[] = [
  { to: "/lists", label: "Lists", icon: ListChecks },
  { to: "/settings", label: "Settings", icon: Settings },
];

function NavSection({ title, items }: { title?: string; items: NavItem[] }) {
  return (
    <div className="flex flex-col gap-0.5">
      {title && (
        <div className="px-3 pt-4 pb-1.5 text-[11px] font-semibold tracking-wider text-muted uppercase">
          {title}
        </div>
      )}
      {items.map(({ to, label, icon: Icon }) => (
        <NavLink
          key={to}
          to={to}
          end={to === "/"}
          className={({ isActive }) =>
            [
              "flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors",
              "outline-none focus-visible:ring-2 focus-visible:ring-accent",
              isActive
                ? "bg-accent-soft font-medium text-accent-soft-foreground"
                : "text-foreground/80 hover:bg-default hover:text-foreground",
            ].join(" ")
          }
        >
          <Icon className="size-4 shrink-0" strokeWidth={2} />
          {label}
        </NavLink>
      ))}
    </div>
  );
}

function ConnectionStatus() {
  const { data, error } = useAsync(() => api.appInfo(), []);
  const connected = !!data?.store;
  const problem = data?.connection_error ?? error?.message;
  const label = connected
    ? `${data!.store!.backend === "sqlite" ? "SQLite" : data!.store!.backend} · schema v${data!.store!.schema_version}`
    : data || error
      ? "Not connected"
      : "Connecting…";

  return (
    <Tooltip delay={300}>
      <Tooltip.Trigger>
        <NavLink
          to="/settings"
          className="flex min-w-0 items-center gap-2 rounded-lg px-3 py-2 text-xs text-muted hover:bg-default"
        >
          <span
            className={`size-2 shrink-0 rounded-full ${connected ? "bg-success" : problem ? "bg-danger" : "bg-warning"}`}
          />
          <Database className="size-3.5 shrink-0" />
          <span className="truncate">{label}</span>
        </NavLink>
      </Tooltip.Trigger>
      <Tooltip.Content placement="right">
        <p className="max-w-80 text-xs break-all">
          {connected ? data!.store!.location : (problem ?? "Opening the database…")}
        </p>
      </Tooltip.Content>
    </Tooltip>
  );
}

export function AppShell() {
  const { resolvedTheme, setTheme } = useTheme("system");
  const dark = resolvedTheme === "dark";

  return (
    <div className="flex h-full">
      <nav className="flex w-64 shrink-0 flex-col border-r border-separator bg-surface px-3 py-4">
        <div className="flex items-center gap-2.5 px-3 pb-5">
          <div className="grid size-8 place-items-center rounded-lg bg-accent text-accent-foreground">
            <Sparkle className="size-4.5" strokeWidth={2.5} />
          </div>
          <div className="leading-tight">
            <div className="text-[15px] font-semibold tracking-tight">PolarisCal</div>
            <div className="text-[11px] text-muted">Calibration tracking</div>
          </div>
        </div>

        <NavSection items={MAIN_NAV} />
        <NavSection title="Administration" items={ADMIN_NAV} />

        <div className="mt-auto flex items-center gap-1 border-t border-separator pt-3">
          <div className="min-w-0 flex-1">
            <ConnectionStatus />
          </div>
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label={dark ? "Switch to light theme" : "Switch to dark theme"}
            onPress={() => setTheme(dark ? "light" : "dark")}
          >
            {dark ? <Sun className="size-4" /> : <Moon className="size-4" />}
          </Button>
        </div>
      </nav>

      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-7xl px-8 py-7">
          <Outlet />
        </div>
      </main>
      <Toast.Provider placement="bottom end" />
    </div>
  );
}
