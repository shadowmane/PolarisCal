# PolarisCal

Calibration tracking for Windows. PolarisCal keeps track of instruments and systems (manufacturer,
model, serial number, location, department, owner, warranty, and calibration schedule) along with
the calibration records for each one: when it was calibrated, by whom, at what cost, the certificate
number, and its condition as received and as returned.

It runs as a standalone desktop app on a local SQLite database, or shared by a team on a PostgreSQL,
Microsoft SQL Server, or MySQL/MariaDB database. An administrator can copy all data between them
(for example from the starter SQLite file to a server) from Settings.

> **Status:** feature-complete first version (all 6 phases done); not yet released. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
> for the design, the business rules, and the roadmap.

## Tech stack

* [Tauri 2](https://tauri.app) with a Rust backend
* [React 19](https://react.dev), [HeroUI v3](https://heroui.com), and [Tailwind CSS v4](https://tailwindcss.com)
* [sqlx](https://github.com/launchbadge/sqlx) for SQLite, PostgreSQL, and MySQL/MariaDB, and
  [tiberius](https://github.com/prisma/tiberius) for SQL Server
* [ts-rs](https://github.com/Aleph-Alpha/ts-rs) to generate TypeScript types from the Rust models

## Development

### Prerequisites (Windows)

* [Rust](https://rustup.rs) (stable, MSVC toolchain)
* [Node.js](https://nodejs.org) LTS
* Visual Studio 2022 Build Tools with the **Desktop development with C++** workload
* WebView2, which ships with Windows 11

One way to install them:

```powershell
winget install Rustlang.Rustup OpenJS.NodeJS.LTS
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

### Run

```powershell
npm install
npm run tauri dev
```

On first launch the app creates its config and database under
`%APPDATA%\com.polariscal.desktop\` and asks you to create the first administrator account.

To try things without touching that database, point the app at a scratch folder:

```powershell
$env:POLARISCAL_HOME = "C:\temp\polariscal-test"
npm run tauri dev
```

### Test

```powershell
cargo test --workspace     # Rust unit and store contract tests; also regenerates src/bindings
npm run build              # TypeScript type-check and production frontend build
npm test                   # frontend unit tests (Vitest)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

The store contract tests use a temporary SQLite file. To run them against a server, set
`POLARISCAL_TEST_DATABASE` to a connection (JSON, as in `config.json`) for an account that may
create databases; every test creates its own database there:

```powershell
$env:POLARISCAL_TEST_DATABASE = '{"backend":"postgres","host":"localhost","database":"postgres","auth":{"method":"password","username":"postgres","password":"..."},"encryption":"off"}'
cargo test -p polaris-store --test store_contract
```

CI does this for PostgreSQL 17, MySQL 8.4, MariaDB 11.4, and SQL Server 2022 on every push.

`src/bindings/` is generated from the Rust types by `cargo test`. Commit it when it changes; CI
fails if it is out of date.

### Build an installer

The build also signs the update files, so it needs the updater's private key:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "$env:USERPROFILE\.tauri\polariscal-updater.key"
npm run tauri build
```

The `.msi` (per-machine, for IT deployment) and NSIS `.exe` (per-user, used by auto-update)
installers are written to `target/release/bundle/`. They aren't code-signed yet, so Windows
SmartScreen warns on first install.

### Release a version

1. Raise `version` under `[workspace.package]` in `Cargo.toml` (the app takes its version from there).
2. Commit, then tag and push: `git tag v0.2.0` and `git push origin v0.2.0`.
3. The **Release** workflow builds the installers and update files and attaches them to a draft
   GitHub release. Edit its notes and publish it.

Installed copies check `https://github.com/shadowmane/PolarisCal/releases/latest/download/latest.json`
when they start (an administrator can turn this off in Settings) and offer to install the new
version. That only works once the repository is public.

The workflow needs the repository secret `TAURI_SIGNING_PRIVATE_KEY` (the contents of
`polariscal-updater.key`; `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` stays empty). Keep a backup of the
key: if it's lost, installed copies can't verify new versions and must be reinstalled by hand.
