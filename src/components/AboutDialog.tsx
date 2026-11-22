// About PolarisCal: version, license, credits, and links to the source.

import { Button, Modal } from "@heroui/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Code, ExternalLink, Scale, Sparkle } from "lucide-react";

const REPOSITORY = "https://github.com/shadowmane/PolarisCal";

/** What PolarisCal is built with, and what each part does. */
const CREDITS: { name: string; role: string; url: string }[] = [
  { name: "Tauri", role: "Desktop app framework", url: "https://tauri.app" },
  { name: "Rust", role: "Programming language for the app's core", url: "https://www.rust-lang.org" },
  { name: "React", role: "User interface", url: "https://react.dev" },
  { name: "HeroUI", role: "Interface components", url: "https://heroui.com" },
  { name: "Tailwind CSS", role: "Styling", url: "https://tailwindcss.com" },
  { name: "Lucide", role: "Icons", url: "https://lucide.dev" },
  { name: "SQLx", role: "SQLite, PostgreSQL, and MySQL/MariaDB databases", url: "https://github.com/launchbadge/sqlx" },
  { name: "Tiberius", role: "SQL Server databases", url: "https://github.com/prisma/tiberius" },
  { name: "Argon2", role: "Password protection", url: "https://github.com/RustCrypto/password-hashes" },
  { name: "rust_xlsxwriter", role: "Excel exports", url: "https://github.com/jmcnamara/rust_xlsxwriter" },
  { name: "lopdf", role: "Combining PDF files", url: "https://github.com/J-F-Liu/lopdf" },
  { name: "Microsoft Edge WebView2", role: "Display and PDF printing", url: "https://developer.microsoft.com/microsoft-edge/webview2" },
];

/** Opens a web page in the default browser; nothing to report if that fails. */
function open(url: string) {
  void openUrl(url).catch(() => undefined);
}

export function AboutDialog({
  isOpen,
  onOpenChange,
  version,
}: {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  version?: string;
}) {
  return (
    <Modal>
      <Modal.Backdrop isOpen={isOpen} onOpenChange={onOpenChange}>
        <Modal.Container size="md" scroll="inside">
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header className="flex-row items-center gap-3">
              <div className="grid size-11 shrink-0 place-items-center rounded-xl bg-accent text-accent-foreground">
                <Sparkle className="size-6" strokeWidth={2.5} />
              </div>
              <div className="leading-tight">
                <Modal.Heading>PolarisCal</Modal.Heading>
                <p className="text-sm text-muted">{version ? `Version ${version}` : "Calibration tracking"}</p>
              </div>
            </Modal.Header>
            <Modal.Body className="flex flex-col gap-5">
              <p className="text-sm leading-relaxed">
                Free, with no ads or account required. Made by David Zuhoski. Open source under the GNU GPL v3.0: use it,
                change it, and share it, as long as your version stays open source too.
              </p>
              <section>
                <h3 className="mb-2 text-xs font-semibold tracking-wider text-muted uppercase">Built with</h3>
                <ul className="grid grid-cols-1 gap-x-4 gap-y-1.5 sm:grid-cols-2">
                  {CREDITS.map((c) => (
                    <li key={c.name} className="min-w-0 text-sm">
                      <button
                        type="button"
                        onClick={() => open(c.url)}
                        className="font-medium text-accent hover:underline focus-visible:underline focus-visible:outline-none"
                      >
                        {c.name}
                      </button>
                      <div className="text-xs text-muted">{c.role}</div>
                    </li>
                  ))}
                </ul>
              </section>
            </Modal.Body>
            <Modal.Footer>
              <Button variant="secondary" onPress={() => open(REPOSITORY)}>
                <Code className="size-4" /> Source code <ExternalLink className="size-3.5 opacity-60" />
              </Button>
              <Button variant="secondary" onPress={() => open(`${REPOSITORY}/blob/main/LICENSE`)}>
                <Scale className="size-4" /> License <ExternalLink className="size-3.5 opacity-60" />
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
