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
│                                  one SQL store for SQLite, PostgreSQL,   │
│                                  MySQL/MariaDB, and SQL Server           │
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
| Users | Multi-user with logins and roles stored in the database. |
| Certificates | Files stored on a configured shared folder/UNC path; the database stores the path. |
| Change history | Full audit trail: who, when, why, old value, new value. Records are archived, never deleted; only list entries nothing uses yet can be deleted (audited). |
| Hierarchy | A System contains Instruments. Each item keeps its own calibration schedule. |
| Pick lists | Location, Department, Owner, Manufacturer, Calibrated By (providers), and Status are admin-managed lists; technicians can add entries (but not edit or delete them). Service records have a **service type** (seeded: Calibration, Repair); a service is done by either a calibration provider or the item's manufacturer. |

## Core rules

All of these live in `polaris-core` so every backend applies them identically.

* **Asset tag** is required and unique (case-insensitive). **Description** is required.
* **Hierarchy**: only Instruments can have a parent, and the parent must be a non-archived
  System. A System that still contains items can't become an Instrument or be archived.
* **Calibration interval** is a number plus a unit (days or months). Month arithmetic clamps
  to the end of the month (Jan 31 + 1 month = Feb 28/29).
* **Derived dates** (`schedule::derive_dates`):
  * *first calibration* is a recorded fact: kept if entered, otherwise filled with the earliest
    calibration known (the earliest record, or a last date entered before any records existed);
  * *last calibration* is the latest non-archived record's date. While an item has no records
    it is the manually entered value (`last_cal_date_entered`, for items migrated from an older
    system), which is stored separately so it comes back if every record is archived;
  * The **warranty end date** defaults to one year after the date put into service when left empty.
  * Only **service records whose type counts as a calibration** (Calibration by default; not Repair)
    set the last calibration date and a due date. Changing whether a type counts re-derives the
    affected items' dates.
  * *next calibration* is always last + interval and is never edited directly. With **End of month**
    on, it moves to the last day of that month (12 months from Nov 20, 2026 is Nov 30, 2027).
* **Due status**: an item whose status has *tracks due* switched off (Out of Service, Retired,
  Lost, Calibration Not Required) is *not tracked*. Otherwise it is *overdue*, *due soon*
  (within 30 days), *current*, or *no schedule*.
* **Calibration records** can't be dated in the future and can't move to a different item;
  each one stores the due date it established at the time.
* **Concurrency**: every editable row has a `row_version`. Updates must send the version
  they loaded; if someone else saved first, the update is rejected with a conflict instead
  of silently overwriting.
* **No hard deletes**: assets and records are archived; list entries and user accounts are deactivated.

## Users, roles, and the audit trail

* **First run**: when the database has no accounts, the app shows a setup screen that creates the
  first Administrator. There are no default credentials.
* **Roles** (`Role` in `polaris-core`, mirrored by `src/lib/permissions.ts`):

  | Role | Can |
  |---|---|
  | Administrator | Everything: users, pick lists, settings, the full audit log |
  | Technician | Create, edit, and archive Instruments, Systems, and Calibration Records; add list entries |
  | Viewer | Read records, change nothing. The Lists page is hidden; Settings sections they can't change show a red lock |

  The UI hides what a role can't do, but every Tauri command checks the role itself
  (`require` in `src-tauri/src/commands.rs`), so hiding is never the protection.
* **Passwords** are hashed with Argon2id (random salt per hash). Admins can force a change at next
  sign-in; new accounts default to that. A failed sign-in takes the same time whether or not the
  username exists.
* **Password rules** (`PasswordPolicy`, Settings > Administration, stored in `app_settings`): a minimum
  length (8 to 128, default 10), optionally a capital letter, a number, and a special character (not a
  letter, number, or space), never the username, and optionally an expiry in days. Every new password
  is checked (new accounts, resets, changes); existing ones aren't. `users.password_changed_at` is
  stamped whenever a password is set (schema 17 starts existing accounts' clocks at the upgrade). At
  sign-in an expired password is marked `must_change_password` (audited as "Password expired"), so
  the user goes straight to choosing a new one. The rules travel in `auth_status` so the sign-in
  screens can show and check them.
* **Password reset requests**: after a failed sign-in the screen says to ask an administrator; after
  three for the same username it offers "Request password reset", which stamps
  `users.reset_requested_at` (active accounts only; the command succeeds either way, so it can't
  reveal which usernames exist). Administrators get a red count on Users in the sidebar and a badge
  on the user's row until a new password is set. The sidebar checks the count every minute and on
  window focus with `require_passive`, which checks the session like `require` but doesn't count as
  activity, so it never keeps an idle session from locking.
* **Sessions** live in memory in the desktop process. Each command re-reads the account, so a user an
  admin deactivates or demotes is affected on their next action, even on another computer sharing the
  database.
* **Auto-lock**: after the admin-configured idle time (default 15 minutes), the app locks and asks for
  the password. The backend enforces the same timeout, and the UI stays mounted underneath, so unsaved
  form input survives the lock.
* **Reasons**: editing, archiving, or restoring an existing Instrument, System, service record, or
  file asks for a reason. It's recommended, not required, and stored with the audit entry when given.
* **Audit trail**: every write inserts an `audit_log` row in the same transaction: user, time, action,
  subject, reason, and a field-by-field list of old and new values built by `polaris_core::audit`.
  Sign-ins (including failures), sign-outs, password changes, resets and reset requests, and settings changes are
  logged too. In SQLite, triggers reject any `UPDATE` or `DELETE` on the table. Each item's page shows
  its own history; admins see the full log.

## Attachments

* An administrator sets the **attachments folders** in Settings, ideally network shares such as
  `\\server\share\PolarisCal` that every user reaches by the same path: one for calibration
  certificates (`attachments_root`, required before anything can be attached) and optional ones for
  photos, service manuals, and other documents (documents and files of type Other). A type without
  its own folder is saved in the certificates folder. Saving checks that this computer can write to
  each folder that changed.
* Each file records the folder it was saved in (`attachments.storage_folder`, schema 18; files from
  before then are in the certificates folder), so changing a folder later only affects new files.
* **Service manuals** attached to one item are listed on every item with the same manufacturer and
  model number (model compared ignoring case), marked "Shared from" the item they're attached to,
  where they're archived or restored.
* Attaching **copies** the chosen file into its type's folder:
  * `<ASSET-TAG>/<file>` for files on an Instrument or System;
  * `<ASSET-TAG>/<YYYY-MM-DD>/<file>` for files on a calibration record (the calibration date).

  Names are made safe for Windows, and a name that's already taken gets " (2)", " (3)", and so on.
  The database stores the path *relative to the folder*, so the share can move as long as the
  structure under it is kept. Paths are unique within each folder.
* Each file's **SHA-256 fingerprint** and size are recorded. Opening a file re-checks the
  fingerprint: a changed file triggers a warning before it opens, and a missing file is reported with
  where it was expected. The same file can't be attached twice to the same item or record.
* Files are opened in their default Windows program and can be shown in File Explorer. Removing a file
  from the list **archives** it (a reason is recommended); the file itself is never deleted.
* Technicians and Administrators can attach and archive files; everyone can open them. Every attach
  and archive is written to the audit trail.

## Database portability

One `Store` implementation (`polaris-store/src/store.rs`) serves every backend. Its queries are
written once in a small portable dialect (`polaris-store/src/sql`): numbered placeholders (`?1`,
which may repeat), `TRUE`/`FALSE`, `LOWER()` for case-insensitive comparison, and helpers instead of
`RETURNING`, `LIMIT`, and upserts. Just before running, each query is rewritten for the server
(`$1` for PostgreSQL, `?` with the parameters reordered for MySQL, `@P1` and `1`/`0` for SQL Server).

| Backend | Driver | Schema | Windows login |
|---|---|---|---|
| SQLite | sqlx | `migrations/sqlite` (versions 1 to 22) | n/a (a file) |
| PostgreSQL 12+ | sqlx | `migrations/postgres` (5 onward) | No. Use a database account; a blank password falls back to the user's `pgpass.conf`. |
| MySQL 8 / MariaDB 10.5+ | sqlx | `migrations/mysql` (5 onward) | No. Use a database account. |
| SQL Server 2016+ / Azure SQL | tiberius | `migrations/mssql` (5 onward), run by PolarisCal's own migrator | **Yes** (SSPI: each person's own Windows account, no stored password) |

The server schemas match SQLite's version 5, so a schema version means the same thing everywhere
and future migrations share numbers. All store timestamps in UTC to the microsecond, the finest
every backend keeps, so copies between backends are exact. The audit table is append-only on every
backend (triggers reject updates and deletes). Every backend must pass `tests/store_contract.rs`;
CI runs it against SQLite on Windows and against PostgreSQL 17, MySQL 8.4, MariaDB 11.4, and SQL
Server 2022 in service containers on Linux.

SQLite runs in WAL mode for local files and falls back to rollback journaling when the file is
on a network share (UNC path), where WAL is unsafe. A shared SQLite file works for a few users but
a database server is better for more.

### Preparing a server

Create an empty database (for example `polariscal`) and an account that can create tables, indexes,
and triggers in it; PolarisCal creates its tables on first connect. On MySQL with binary logging on,
creating the audit triggers may need `log_bin_trust_function_creators = 1` or extra privileges.
SQL Server's database collation should be case-insensitive (the default). The connection can be
encrypted; "Always, with a trusted certificate" requires a certificate this computer trusts.

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

Administrators change it from **Settings > Database connection**, or before signing in from the
connection button in the top-left corner of the sign-in screens. There, if the current database
works and has accounts, an administrator of it enters their username and password first
(`unlock_connection_settings`, which unlocks the settings for 10 minutes without starting a session;
failures are audited as failed sign-ins). Anyone may change it when PolarisCal can't connect, or when
the database has no accounts yet, since they could edit the file anyway. Saving connects first
(setting PolarisCal up in a new database) and only then writes the file; the user is signed out and
signs in to the new database.

The **connection light** (`connection_status`, available without signing in; shown on the sign-in
screens and in the sidebar) is green when connected and set up (sign-in available), yellow when
connected or the file was found but PolarisCal isn't set up there (no accounts), and red when the
server refuses the connection or the file is missing. A missing SQLite file is only created on a
brand-new install; afterwards it's reported, so a moved or deleted database never silently becomes a
new empty one. **Try again** reopens the configured database (`retry_connection`).

### Shared settings file

So IT configures the connection once, every computer can follow a **shared settings file** on a
network share instead:

```json
{ "shared_settings_file": "\\\\server\\share\\PolarisCal\\polariscal-connection.json", "database": { ... } }
```

The shared file holds `{ "database": { ... } }` in the same format. An administrator can create or
change it from the connection dialog. A relative SQLite path in it is relative to the file. A
password saved in it is readable by anyone who can read the share, so restrict the share, or use SQL
Server with Windows login so no password is stored at all. A new computer's first-run screen offers
**Connect to an existing database** to follow the shared file instead of starting a new database.

If the database can't be opened, the app still starts and shows the problem with options to retry
(which really reconnects) or change the connection.

## Copying data between databases

**Settings > Copy data to another database** (administrators) copies every table, ids included, into
a new, empty database of any type: users with their password hashes, items, calibration records,
file records, lists, settings, and the whole audit trail. It runs in one transaction on the target,
refuses a target that already holds data, verifies each table's row count, and records the copy in
both audit trails (`Data copied out` / `Data copied in`). The copy doesn't switch computers over;
the dialog offers to switch this one, and a shared settings file switches everyone. Attached files
stay where they are, since the database stores paths relative to the attachments folder.

## Service records and currency

Calibration records are called **service records** in the app (the table is still
`calibration_records`). Each has a service type from an admin-managed list; the type's
"Counts as a calibration" switch decides whether the record moves the item's schedule. Costs are
shown with the organization's **currency symbol** (Settings; $ by default), in the app, reports, and
Excel exports. The symbol is display only; amounts are never converted.

### Billing

The **Billing** page (Technicians and Administrators) lists the service records of every outside
vendor (a calibration provider not marked Internal), a section per vendor, with what is and isn't
invoiced. **Enter invoice** takes the invoice number and date, the vendor's records it bills (only
those not on another invoice), each one's line number, and the invoiced prices, shown against the
recorded costs. Saving (`save_invoice`, schema 22: `invoice_number`, `invoice_line`, and
`invoice_date` on `calibration_records`) logs the invoice on those records and sets their cost to
the invoiced price, audited with the reason "Invoice <number>". An invoice is identified by vendor
and number: typing the number of one that's already entered (or clicking it in the list) opens it,
and saving it again with fewer records takes the others off it. A record whose performer is changed
to someone else comes off its invoice.

### Calibration extensions

The built-in **Calibration Extension** service type (it can be renamed or made inactive, but not
deleted, and never counts as a calibration) authorizes an item for use past its due date. An
extension record has no performer, certificate, or cost; it records who authorized it (the
Technician, filled with the user's name) and a length in days. The item's next calibration becomes
the extension's date plus those days, and the last calibration stays as it was. An extension never
shortens the schedule, and the next calibration on or after its date ends it. While it lasts the
item's status is **Calibration extended**; it is still counted and listed with the overdue items.

### Aggregate Systems

A System either has its own cycle or takes its schedule from its Instruments
(`assets.cal_aggregate`). An aggregate System has no interval or calibration dates of its own;
its next calibration is the soonest `next_cal_date` among its tracked, unarchived Instruments,
worked out in the item query when read, so it follows every calibration, status change and
move without being stored. The Dashboard leaves aggregate Systems out, since their Instruments
are already counted. Instruments chosen when creating a System are moved into it in one
transaction (`Store::add_to_system`), each audited as an edit.

An Instrument in a System is wherever the System is: saving it, or moving it into a System,
gives it the System's location and department, and changing a System's location or department
passes the change to its Instruments in the same transaction (each audited as an edit).

### Failed calibrations and built-in statuses

Four statuses are built in (a `builtin_role` column marks them, so they can be renamed but not
deleted): **Out for Calibration** and **Out for Repair** always track due dates, so an item away
for service still shows when it's overdue or due soon (the Dashboard gives it a second badge with
its status). **Retired** and **Storage** never do. Storage is for items out of active use but
not retired: their next calibration date is left as it was, so an item put back into service is
due on its original schedule, overdue straight away if that date passed while it was stored. A calibration returned as **Fail** doesn't move the item's
last or next calibration date, since the item isn't fit for use. Saving one asks whether the item
is being sent for repair or retired, and the record and the status change are saved together (the
status change is audited as an edit of the item).

Service types marked **Repair** (a switch on the list; the upgrade sets it on types already named
like a repair) close the loop: a repair returned as Fail retires the item, and one returned as Pass
or Limited asks what status the item gets back, suggesting the status it had before the failure.
An item remembers that status (`previous_status_id`) whenever a service record changes its status.
A failed repair that also counts as a calibration retires the item without the repair-or-retire
question.

## Check-out (Available Inventory)

An Instrument's **Owner** is who it's checked out to. The **Available Inventory** page lists
Instruments with no owner that aren't retired or away for calibration or repair, and, under
**Checked out**, the rest by owner. Technicians and Administrators select instruments and
**check out** (set the owner), **check in** (clear it), or **transfer** (owner to owner) through
`Store::change_owner`: one transaction for all the chosen items, refused as a whole if any is
archived, a System, or in the wrong state (checking out one that's already out, and so on). Each
item gets an audit entry (`Checked out` / `Checked in` / `Transferred`) with the Owner change and
the optional note as its reason. `assets.owner_since` (schema 19) records when the current owner got
it, also when the owner is changed on the edit form; it's empty for assignments made before then.
The check-out dialog warns about instruments that are overdue or in Storage.

**Requests.** Viewers can't change owners themselves; they select instruments and **request a check
out or transfer** to an owner, with an optional note (`owner_requests`, schema 20). Check-in can't
be requested, since it means handling the item back into the store. One request may wait per item,
and a request is refused if the change couldn't be made now. Waiting requests are listed under
**Requests** for everyone, with a red count on Available Inventory for Technicians and
Administrators (checked in the background, not counted as activity). They **approve** it, which
makes the change exactly as a direct check-out or transfer would (and fails the same way if the
item has moved on, leaving the request waiting), recorded with "Requested by ..." as its reason;
or **decline** it, with an optional note. The person who asked can **cancel** a waiting request.
Requesting, declining, and cancelling are audited on the item too. Requests decided in the last 30
days stay listed with their outcome.

**Users and owners.** An administrator links a user account to an Owner entry (Users page,
`owners.user_id`, schema 21; one each way). Owner dropdowns show a user icon on linked owners. A
transfer request for an item held by a linked owner is addressed to that user
(`owner_requests.approver_user_id`), who approves or declines it (Technicians and Administrators
still can); requests for unlinked owners, and check-out requests, go to Technicians and
Administrators. **Request instrument** on an item's page asks its holder to hand it over to the
requester's own Owner entry, so it needs the requester to be linked. A linked owner can't be edited
or deleted on the Lists page (it's managed with the account), and it's active exactly when its
user is: deactivating or reactivating the user, or linking, sets it.

## Reservations

Anyone can reserve an Instrument for a span of days (`reservations`, schema 21). Reserving only
books the days; the instrument is handed over (checked out) when it starts and checked in when it's
returned. The reserve dialog checks the dates as they're picked (`check_reservation`):

* free: **Reserve** books it straight away;
* checked out to someone: it can't be reserved until it's returned, so the dialog offers
  **Request instrument** instead;
* clashing with other reservations: it offers the **next free dates** of the same length, and when
  the only clash is a reservation that started earlier, **asking its holder to end theirs early**.
  That books the new reservation as *pending*; the holder accepts (theirs then ends the day before)
  or declines. Cancelling the earlier reservation lets a pending one go ahead.

While an instrument has a booked reservation that hasn't ended, it can only be checked out or
transferred (or requested) to the person it's reserved for, through their linked Owner entry:
everything else is refused until that reservation ends or is cancelled. Available Inventory shows
such instruments with a **Pending reservation** badge. Check-in is never held up. The person a
reservation is for (or a Technician or Administrator) can **edit** its dates and note while it's
booked; once it has started only its end can move, and the change is checked like a new
reservation (not counting itself, nor the instrument being checked out to them).

The **Reservations** page lists instruments free to reserve now (not checked out, retired, away for
service, or reserved today) with each one's next booking, the user's own reservations, and a month
calendar of everyone's (past ones included). Reservations can be cancelled by the person they're
for or by a Technician or Administrator. Reserving, early-return requests and answers, and
cancelling are audited on the item.

The **Dashboard** has a **Waiting for you** section below Needs attention: requests addressed to the
user, requests to end their reservations early, and for Technicians and Administrators, check-out
requests for no one in particular, reservations that have started but haven't been handed over
(with a one-click check-out to the person's Owner entry), and ones that have ended with the
instrument still out (one-click check-in). The sidebar badges on Available Inventory and
Reservations count the same things.

## Reports and labels

**Reports** (everyone can view them) are built in the UI from the same data the screens use
(`src/lib/reports.ts`), so a report on screen, on paper, and in Excel always match:

* **Due and overdue**: items overdue or due within a window (overdue only, 30/60/90 days, a year),
  filterable by location, department, and owner, soonest first with days left. Items whose status
  doesn't track due dates are left out.
* **Out of tolerance**: calibrations in a date range where the item arrived out of tolerance or
  inoperative, with how it was returned, for impact assessments.
* **Instrument register**: every item with its identity, assignment, status, and schedule.
* **Invoiced calibrations**: service records on outside vendors' invoices, by vendor, invoice, and
  line, for one vendor or all and one year (of the invoice date) or all, with a total.
* **Calibration history**: one item's calibrations (from its page, "Print or export").

**Export to Excel** writes a real `.xlsx` (via `rust_xlsxwriter`): dates as Excel dates, money as
numbers, a frozen header row with filters, and a line saying what was filtered and when.
**Print or save PDF** uses the Windows print dialog; print styles hide the app and switch wide
reports to landscape.

**Calibration labels** print each item's tag, last calibration date, who calibrated it (from the
latest calibration record), and due date, on Letter 30-up (Avery 5160 size), Letter 10-up (5163),
A4 14-up (L7163), or one label per page for label printers. A part-used sheet can be reused by
skipping its used positions.

## Calibration certificates

Generate Certificate issues a certificate for an internal item or an external customer's
equipment. The details are entered on the left and the certificate is drawn on the right by
`CertificateView`, an HTML page laid out at its real paper size. Saving the PDF prints that
same element through WebView2's `PrintToPdf` (`src-tauri/src/certificates.rs`), with
everything else on the page hidden for print, so the file always matches the preview and
its text stays selectable.

Settings > Certificate Template holds the organization's layout (name, address, logo as a
data URL, accent color, border, font, paper, temperature unit, statement, footer) and the
numbering pattern, as JSON in `app_settings` (`certificate_template`). The counter lives in
its own keys (`certificate_next_number`, `certificate_number_year`) and is read and advanced
in the same transaction that inserts the certificate; the unique index on
`certificates.certificate_number` stops two computers issuing the same number.

Each issued certificate is kept in `certificates` with its details and a copy of the
template as JSON, so a reprint from Issued Certificates looks the same after the template
changes. For internal certificates the PDF is also attached to the item when an attachments
folder is set.

Calibrated by and Approved by are free text, or a PolarisCal user picked from a list. A
user's signature (an image saved on the Users page, in `users.signature`) is printed above
their line; the store copies it from the user into the certificate when issuing, never from
what the page sends. A blank Approved by means the person who calibrated it approved it.

For an internal item picked from the list, Generate Certificate can also add a service
record to it (service type and provider chosen on the page, the certificate number filled
in) and attach the PDF to that record. The record is created right after the certificate
is issued, with the regular service record command; if it fails, the certificate stands
and the page says so. It never changes the item's status.

Test data is an optional PDF appended to the certificate PDF after it is printed
(`src-tauri/src/pdf_merge.rs`, using lopdf). Only its file name is kept with the
certificate; to include it in a later copy, attach the file again.

## Installers and updates

`npm run tauri build` produces a per-machine `.msi` for IT deployment and a per-user NSIS `.exe`.
Pushing a `v*` tag runs the **Release** workflow, which builds both plus update bundles signed with
the updater key and a `latest.json`, and attaches them to a draft GitHub release.

The app checks that `latest.json` on GitHub once per launch (unless an administrator turns checks
off in Settings, for example where IT deploys updates) and in Settings on request. A new version is
shown in the sidebar and installed from Settings; Tauri verifies its signature against the public
key in `tauri.conf.json` before installing, then restarts the app. While the repository is private,
checks report that the update server can't be reached. Installers aren't code-signed yet.

## Roadmap

1. **Foundation** ✅: Tauri/React/HeroUI shell, core model and rules, `Store` trait, SQLite backend and migrations, read-only screens, CI.
2. **Core data entry** ✅: forms for Instruments/Systems and Calibration Records, list management, detail views with child items and calibration history.
3. **Users and audit trail** ✅: logins, roles (Admin / Technician / Viewer), reasons for change, field-level change history, idle auto-lock.
4. **Certificates** ✅: attach files from the configured share to items and calibration records, with fingerprint checks.
5. **Remote databases** ✅: PostgreSQL, MySQL/MariaDB, and SQL Server (with Windows login), a connection settings screen, shared settings files, and copying data between databases.
6. **Reports and release** ✅: due/overdue, out-of-tolerance, register, and item-history reports with Excel export and printing; calibration labels; installers, a release workflow, and signed auto-updates.
