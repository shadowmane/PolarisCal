import { Button, Modal, Toast, Tooltip, toast, useTheme } from "@heroui/react";
import {
  ArrowDownCircle,
  ClipboardCheck,
  Database,
  FileChartColumn,
  Gauge,
  History,
  KeyRound,
  LayoutDashboard,
  ListChecks,
  LogOut,
  Moon,
  Settings,
  Sparkle,
  Sun,
  Users,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState } from "react";
import { NavLink, Outlet, useNavigate } from "react-router";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { roleMeta } from "../lib/permissions";
import { checkOnStartup, useAvailableUpdate } from "../lib/updates";
import { useAsync } from "../lib/useAsync";
import { ChangePasswordForm } from "./AuthScreens";

interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
}

const MAIN_NAV: NavItem[] = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard },
  { to: "/assets", label: "Instruments & Systems", icon: Gauge },
  { to: "/calibrations", label: "Calibration Records", icon: ClipboardCheck },
  { to: "/reports", label: "Reports", icon: FileChartColumn },
];

const ADMIN_NAV: (NavItem & { adminOnly?: boolean })[] = [
  { to: "/lists", label: "Lists", icon: ListChecks },
  { to: "/users", label: "Users", icon: Users, adminOnly: true },
  { to: "/audit", label: "Audit Log", icon: History, adminOnly: true },
  { to: "/settings", label: "Settings", icon: Settings },
];

function initials(name: string): string {
  const parts = name.trim().split(/\s+/);
  return ((parts[0]?.[0] ?? "") + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase();
}

function UserPanel() {
  const { user, signOut } = useAuth();
  const [changing, setChanging] = useState(false);
  return (
    <div className="flex items-center gap-2.5 px-2 pb-2">
      <div className="grid size-8 shrink-0 place-items-center rounded-full bg-accent-soft text-xs font-semibold text-accent-soft-foreground">
        {initials(user.display_name)}
      </div>
      <div className="min-w-0 flex-1 leading-tight">
        <div className="truncate text-sm font-medium">{user.display_name}</div>
        <div className="truncate text-[11px] text-muted">{roleMeta[user.role].label}</div>
      </div>
      <Tooltip delay={300}>
        <Button isIconOnly size="sm" variant="ghost" aria-label="Change password" onPress={() => setChanging(true)}>
          <KeyRound className="size-4" />
        </Button>
        <Tooltip.Content>Change password</Tooltip.Content>
      </Tooltip>
      <Tooltip delay={300}>
        <Button isIconOnly size="sm" variant="ghost" aria-label="Sign out" onPress={() => void signOut()}>
          <LogOut className="size-4" />
        </Button>
        <Tooltip.Content>Sign out</Tooltip.Content>
      </Tooltip>

      <Modal>
        <Modal.Backdrop isOpen={changing} onOpenChange={setChanging}>
          <Modal.Container size="sm">
            <Modal.Dialog>
              <Modal.CloseTrigger />
              <Modal.Header>
                <Modal.Heading>Change your password</Modal.Heading>
              </Modal.Header>
              <Modal.Body>
                <div className="p-1">
                  <ChangePasswordForm
                    onDone={() => {
                      setChanging(false);
                      toast.success("Password changed");
                    }}
                  />
                </div>
              </Modal.Body>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
    </div>
  );
}

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
  const { can } = useAuth();
  const adminNav = ADMIN_NAV.filter((item) => !item.adminOnly || can("admin"));
  const update = useAvailableUpdate();
  const navigate = useNavigate();

  // One quiet check for a new version per launch, unless an admin turned checks off.
  useEffect(() => {
    api
      .getSettings()
      .then((settings) => settings.update_checks && checkOnStartup())
      .catch(() => undefined);
  }, []);

  return (
    <div className="flex h-full print:block print:h-auto">
      <nav className="flex w-64 shrink-0 flex-col border-r border-separator bg-surface px-3 py-4 print:hidden">
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
        <NavSection title="Administration" items={adminNav} />

        {update && (
          <button
            type="button"
            onClick={() => navigate("/settings")}
            className="mx-1 mt-auto mb-3 flex items-center gap-2 rounded-lg bg-accent-soft px-3 py-2 text-left text-sm text-accent-soft-foreground hover:opacity-90"
          >
            <ArrowDownCircle className="size-4 shrink-0" />
            <span>
              Version {update.version} is available
              <span className="block text-xs opacity-80">Open Settings to install it</span>
            </span>
          </button>
        )}
        <div className={`${update ? "" : "mt-auto "}border-t border-separator pt-3`}>
          <UserPanel />
        </div>
        <div className="flex items-center gap-1">
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

      <main className="min-w-0 flex-1 overflow-y-auto print:overflow-visible">
        <div className="mx-auto max-w-7xl px-8 py-7 print:max-w-none print:p-0">
          <Outlet />
        </div>
      </main>
      <Toast.Provider placement="bottom end" />
    </div>
  );
}
