# PolarisCal

Calibration tracking for Windows. PolarisCal keeps track of instruments and systems (manufacturer,
model, serial number, location, department, owner, warranty, and calibration schedule) along with
the calibration records for each one: when it was calibrated, by whom, at what cost, the certificate
number, and its condition as received and as returned.

It runs as a standalone desktop app on a local SQLite database. Support for shared PostgreSQL, SQL
Server, and MySQL/MariaDB databases is on the roadmap.

> **Status:** early development (Phase 4 of 6). See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
> for the design, the business rules, and the roadmap.

## Tech stack

* [Tauri 2](https://tauri.app) with a Rust backend
* [React 19](https://react.dev), [HeroUI v3](https://heroui.com), and [Tailwind CSS v4](https://tailwindcss.com)
* [sqlx](https://github.com/launchbadge/sqlx) for database access
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

`src/bindings/` is generated from the Rust types by `cargo test`. Commit it when it changes; CI
fails if it is out of date.

### Build an installer

```powershell
npm run tauri build
```

The `.msi` and NSIS `.exe` installers are written to `target/release/bundle/`.
