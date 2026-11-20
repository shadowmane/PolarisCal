# PolarisCal architecture

PolarisCal is a standalone Windows desktop app for tracking instruments, systems, and
their calibration records. It is built with [Tauri 2](https://tauri.app): a Rust
backend owns all data access and business rules, and a React + [HeroUI v3](https://heroui.com)
frontend renders the UI inside the Windows WebView2.

```
┌───────────────────────────── PolarisCal.exe ─────────────────────────────┐
│                                                                          │
│  React + HeroUI (src/)          Tauri commands (src-tauri/)              │
│  ─────────────────────   invoke ──────────────────────────               │
│  pages, components  ───────────▶ commands.rs  ── thin wrappers           │
│  lib/api.ts (typed)  ◀───────── CommandError  ── errors → UI shape       │
│        ▲                              │                                  │
│        │ generated types              ▼                                  │
│  src/bindings/*.ts ◀── ts-rs ── polaris-core (crates/polaris-core)       │
│                                  model, validation, schedule rules,      │
│                                  Store trait                             │
│                                       ▲                                  │
│                                       │ implements                       │
│                                 polaris-store (crates/polaris-store)     │
│                                  SQLite today; PostgreSQL, MySQL,        │
│                                  SQL Server next                         │
└──────────────────────────────────────────────────────────────────────────┘
```

## Repository layout

| Path | What it is |
|---|---|
| `crates/polaris-core` | Domain types, validation, calibration-date rules, and the `Store` trait. No I/O. |
| `crates/polaris-store` | Database backends. `migrations/<backend>/` holds each backend's SQL migrations. `tests/store_contract.rs` is the behavior every backend must pass. |
| `src-tauri` | The desktop shell: config file, app state, and the Tauri commands the UI calls. |
| `src` | React frontend. `src/bindings` is **generated**; don't edit it by hand. |
| `docs` | Design notes like this one. |

## Decisions

These were agreed on before development started (2026-11-20).

| Area | Decision |
|---|---|
| App shell | Tauri 2 (Rust) + React 19 + HeroUI v3 + Tailwind CSS v4. |
| Databases | Local SQLite file, **or** remote PostgreSQL, Microsoft SQL Server, or MySQL/MariaDB. |
| Users | Multi-user with logins and roles stored in the database (Phase 3). |
| Certificates | Files stored on a configured shared folder/UNC path; the database stores the path (Phase 4). |
| Change history | Full audit trail: who, when, old value, new value. Nothing is hard-deleted (Phase 3). |
| Hierarchy | A System contains Instruments. Each item keeps its own calibration schedule. |
| Pick lists | Location, Department, Owner, Manufacturer, Calibrated By (providers), and Status are admin-managed lists. |

## Core rules

All of these live in `polaris-core` so every backend applies them identically.

* **Asset tag** is required and unique (case-insensitive). **Description** is required.
* **Hierarchy**: only Instruments can have a parent, and the parent must be a non-archived
  System. A System that still contains items can't become an Instrument or be archived.
* **Calibration interval** is a number plus a unit (days or months). Month arithmetic clamps
  to the end of the month (Jan 31 + 1 month = Feb 28/29).
* **Derived dates** (`schedule::derive_dates`):
  * *first calibration* is a recorded fact: kept if entered, otherwise filled from the earliest record;
  * *last calibration* is the latest non-archived record's date, or the manually entered
    value while an item has no records (for items migrated from an older system);
  * *next calibration* is always last + interval and is never edited directly.
* **Due status**: an item whose status has *tracks due* switched off (Out of Service, Retired,
  Lost, Calibration Not Required) is *not tracked*. Otherwise it is *overdue*, *due soon*
  (within 30 days), *current*, or *no schedule*.
* **Calibration records** can't be dated in the future and can't move to a different item;
  each one stores the due date it established at the time.
* **Concurrency**: every editable row has a `row_version`. Updates must send the version
  they loaded; if someone else saved first, the update is rejected with a conflict instead
  of silently overwriting.
* **No hard deletes**: assets and records are archived; list entries are deactivated.

## Database portability

`sqlx` covers SQLite, PostgreSQL, and MySQL but not SQL Server, so the app talks to storage
only through the `Store` trait. Each backend:

1. lives in its own module behind a cargo feature (`sqlite`, later `postgres`, `mysql`, `mssql`);
2. has its own migration folder, because the SQL dialects differ;
3. stores dates as ISO `YYYY-MM-DD`, money as integer cents, and enums as lowercase text;
4. must pass `tests/store_contract.rs`.

SQLite runs in WAL mode for local files and falls back to rollback journaling when the file is
on a network share (UNC path), where WAL is unsafe. A shared SQLite file is not recommended
for multi-user use; use a database server instead.

## Configuration

On first run the app writes `%APPDATA%\com.polariscal.desktop\config.json`, pointing at a
SQLite database in the same folder:

```json
{
  "database": {
    "backend": "sqlite",
    "path": "C:\\Users\\<you>\\AppData\\Roaming\\com.polariscal.desktop\\polariscal.db"
  }
}
```

If the database can't be opened, the app still starts and shows the problem on the
Settings page instead of crashing.

## Roadmap

1. **Foundation** ✅: Tauri/React/HeroUI shell, core model and rules, `Store` trait, SQLite backend and migrations, read-only screens, CI.
2. **Core data entry**: forms for Instruments/Systems and Calibration Records, list management, detail views with child items and calibration history.
3. **Users and audit trail**: logins, roles (Admin / Technician / Viewer), field-level change history.
4. **Certificates**: attach files from the configured share to records.
5. **Remote databases**: PostgreSQL, then MySQL/MariaDB, then SQL Server, plus a connection settings screen.
6. **Release**: reports and exports, then the installer.
