# MICS Only — Firebird backup (Windows)

This repository builds **mics-only.exe** (standby reader) and **mics-only-settings.exe** (native Windows configuration GUI), packaged as a single **MICS-Only-Portable-Windows.zip** by GitHub Actions, with no installer.

## Installation

1. Download `MICS-Only-Portable-Windows.zip` from GitHub Releases after a successful Windows build.
2. Extract the ZIP to a writable folder, then open `mics-only-settings.exe`.
3. Enter backup Agent ID, Firebird host, port, server database path, read-only username and Cloudflare coordinator URL. Enter Firebird password, **backup MICS Sync Token** and **backup Coordinator Token**. These tokens must be different from the existing `mics-print` token and **must be provisioned on the Worker first**.
4. Press Save. Settings are written to `mics.ini`. Secrets are encrypted with Windows DPAPI in `%LOCALAPPDATA%\\MICS-Only\\credentials.dpapi`, never in INI or GitHub. Blank password/token fields keep prior encrypted values. Configure and run under the *same Windows user*.
5. Leave **Enable standby agent OFF** until the coordinator and primary failover control are deployed and verified. The portable bundle does not automatically run the standby reader or connect Firebird.

## Fail-closed behavior

The standby requests a signed/opaque lease from the Cloudflare coordinator. Without a valid, unexpired lease, it will **not query Firebird** and **not upload to D1**. The Worker must reject stale epochs and leases. The current `mics-print` agent does not yet participate in this lease protocol; automatic failover is not operational until that integration is complete.

The coordinator contract is:
- POST `{coordinator_url}/lease`: Bearer backup Coordinator Token, request `{agent_id, role: "backup", ttl_seconds:45}`, response `{granted, epoch, lease_token, expires_at}`.
- POST `{coordinator_url}/sync`: Bearer backup MICS Sync Token and `X-Mics-Agent-Id`, `X-Mics-Lease-Token`, `X-Mics-Lease-Epoch` headers. Accept only active, unexpired lease; response must include `{"ok":true}`.

## Developer commands

```powershell
cargo test
cargo build --release
```

Do not store secrets in public repositories or copy credentials from the existing `mics-print` installation. The Windows EXEs are not signed; Windows SmartScreen may warn until signed/reputation established.

## Local-only dashboard / PWA (Windows)
- The agent itself hosts **http://127.0.0.1:8787/**, whether standby is enabled or disabled.
- The local web server binds **only 127.0.0.1** (not 0.0.0.0). Other PCs, LAN clients, phones and Internet clients **cannot connect**. This is intentional.
- Dashboard HTML/CSS/JS, PWA manifest and service worker are editable files in `www/` next to the two executables. The Agent serves them from local disk; no external server required.
- The web UI shows 14 machine nodes in MICS order, the last saved data, status code, progress, counts and cycle.
- Local snapshot file: `<EXE_FOLDER>\data\machines.json`. It updates only when a valid, leased Firebird snapshot differs from the previous sampled snapshot. The file is not a mirror of every MICS table.
- With **Enable standby agent OFF**, web still opens, displays last saved data or placeholders, and does not touch Firebird or Cloudflare.
- With standby enabled, Firebird reads and Cloudflare upload remain **fail-closed** behind the Coordinator lease. Do not enable until primary `mics-print` participates in the lease protocol.
- The local PWA can be installed from the local Windows browser because `localhost` is a secure context. A PWA installed here does not make the server reachable from other devices, including iPhone.
- The page polls local `/api/machines` and `/api/status` every 3 seconds. These requests never reach D1/Turso/Cloudflare.

### Test Connection / 接続テスト
Open `mics-only-settings.exe` and click **Test Connection** (tests only the **saved** INI):
1. Connects to Firebird **TCP port** for network reachability without login or SQL.
2. GETs the Coordinator `/health` endpoint if configured, without asking for a lease.
3. After Coordinator health succeeds, sends one authenticated POST to Coordinator `/presence` to register the saved `agent_id` and `display_name` in Cloudflare D1. The test requires a saved Coordinator Token and reports `D1_PRESENCE_OK` only after HTTP 200.\n4. Does not run Firebird SQL, request a failover lease, or upload production snapshots. **Assign a unique Agent ID to each PC**; copied IDs overwrite one database record.
CLI equivalent: `mics-only.exe --test-connection`.

**Open Local Web** opens the browser at `http://127.0.0.1:8787/`. Save still restarts the agent. Only two EXEs remain.

## Agent management, portable updates, and JSON sync (v0.2.2)
Settings has **Start Agent**, **Stop Agent**, **Restart Agent**, and **GitHub Update** buttons.
- Only `mics-only.exe` is started/stopped; the print agent is unaffected.
- The release includes only `MICS-Only-Portable-Windows.zip`, containing two EXEs, `www/`, README and example INI. No installer is generated.
- **GitHub Update** downloads the latest GitHub Releases ZIP; it verifies size and the GitHub-published SHA256 digest when present, then runs a separate PowerShell updater to replace the two EXEs after Settings closes. Existing `mics.ini`, DPAPI secrets, and modified `www/` files are preserved.
- A backup `.bak` of each existing EXE is retained during an update. If the update fails, inspect `%TEMP%\MICS-Only-Update\update-error.log`; automatic rollback is not yet implemented.
- If a modified web file should be replaced with the new default, rename it before updating.
- A successful Rust build and published GitHub Release must be confirmed before using the update button.
- `<EXE_FOLDER>\data\machines.json`: last valid Firebird snapshot, updated only when machine values change and a Coordinator lease permits reading.
- `<EXE_FOLDER>\data\sync-status.json`: last sync acknowledgement/status, with no credentials.
- With a valid Coordinator lease, uploads use the existing `/sync` endpoint and retry after failures during a later authorized read. When denied lease, no Firebird reads or uploads occur.
- Only one agent may own the lease; production failover remains disabled until `mics-print` participates in the same Coordinator.

## Three-tab localhost UI / local data (v0.2.2)
The only browser URL is `http://127.0.0.1:8787` on the Windows machine running mics-only.
- **ダッシュボード**: 14 current machine nodes with status, production counts, progress, cycle, and pick count.
- **予定**: current/next JST production calendar weeks, all/color-change/mold-change filters and 14 machine schedules. The online admin color dictionary is not yet mirrored: local change classification compares mold codes and raw schedule color fields.
- **全て**: all keys for each machine's saved JSON snapshot, including additional fields received from the primary mirror.
- Editable files: `www/index.html`, `www/style.css`, `www/app.js` (plus service worker, manifest, SVG).
- `<EXE_FOLDER>\data\machines.json` and `schedule-local.json` persist across restarts. The latter holds up to 30 days when mics-only is ACTIVE.
- **MIRROR_ONLY** (Enable standby OFF): reads the token-authenticated Cloudflare machine+schedule mirror at most once every 5 minutes if the coordinator URL and backup sync token are available. No Firebird access and no lease acquisition. The local web remains private to this PC.
- **STANDBY** (Enable standby ON, lease denied): also refreshes the read-only primary mirror at most every 5 minutes.
- **ACTIVE** (lease granted): reads Firebird, writes snapshots to disk, and posts changed machine and 14-day schedule data through coordinator-fenced endpoints.
- `MICS_FAILOVER_ENABLED` stays OFF on Cloudflare until the **new primary** `mics-print` binaries are installed and configured for the lease. Do not activate backup writes in production before verifying the new primary.

## Portable JSON data folder

Machine and schedule data now live **beside mics-only.exe** in its writable `data/` folder:

```text
MICS-Only/
  mics-only.exe
  mics-only-settings.exe
  mics.ini
  www/
  data/
    machines.json
    schedule-local.json
    sync-status.json
    schedule-last-ack.sha256
```

The app creates `data/` when a valid snapshot or sync status is first saved. On the first run after updating, it **copies** any existing JSON from the older `%LOCALAPPDATA%\MICS-Only\data\` folder into `data/`; existing destination files are never overwritten and the originals are not deleted. This change does **not** move the separate Windows DPAPI credentials, which stay under `%LOCALAPPDATA%\MICS-Only\credentials.dpapi`.

Use a folder with Windows write permissions (for example a user-owned directory, not Program Files), so local JSON can be updated.

## All schedule tab (全て)
The **全て** tab now shows **every row currently available** in `data/schedule-local.json` grouped by MICS machine number. This is not limited to this week, next week, or a 7-day filter. Rows are sorted by `start_at`, with columns for start/end, mold, product, color, order, and inferred change type. Machine snapshot fields remain available under expandable `現在の機械データを表示` on each machine panel.

**Scope:** "all" means all rows currently saved in the local JSON, **not** the entire lifetime of the remote Firebird database. ACTIVE currently fetches up to 30 days; MIRROR_ONLY/STANDBY reflects the available Cloudflare schedule mirror (normally 14 days). The app does not delete or change any Firebird schedule.

The web is served from editable `www/` next to the executable. To update only this UI, copy the newest `www/index.html`, `www/app.js`, `www/style.css`, and `www/sw.js` beside the installed EXEs and refresh the browser; an EXE rebuild is not required. The portable updater intentionally preserves existing local WWW files, so installing a new ZIP by itself may not replace an older UI.

## Automatic updates and rollback (v0.2.4)

- Every running mics-only.exe checks the latest stable GitHub Release after an initial 5-minute delay, then every 6 hours. Nothing is installed when the coordinator state is ACTIVE. Failed network checks are non-fatal.
- The Settings "GitHub Update" button uses the same verified downloader. Existing v0.2.2/v0.2.3 installations still require one manual update; automatic checks begin only after v0.2.4 is installed.
- Only a newer semantic version with a GitHub Release portable ZIP is applied. The ZIP download is checked against the GitHub asset size and SHA-256 asset digest when provided.
- A standalone PowerShell helper waits for the invoking process to close, extracts and validates the ZIP, saves BOTH old EXEs in data/update-backup, replaces both EXEs, restarts the agent and probes the local /api/status endpoint for up to 50 seconds. The health response must identify the exact new version.
- If extraction, replacement, startup, or health verification fails after replacement begins, both EXEs are restored and the old agent is restarted. Progress and failures are recorded in data/update.log.
- mics.ini, %LOCALAPPDATA%/MICS-Only/credentials.dpapi, data snapshots and existing www files are retained without overwrites. No print agent is updated or stopped.
- Auto-update is not a substitute for coordinator lease fencing. Do not run two installations with the same agent_id, especially when enabled for standby failover.
- If the installation folder is not writable or PowerShell is blocked by Windows policy, the update is not applied. The last known working EXEs are kept at data/update-backup after success for manual recovery.

## Admin-managed mics-only versions (v0.2.5)

Every Coordinator /presence heartbeat now includes the installed mics-only version. The Cloudflare Admin > MICS Lease table shows each agent's current version and the latest downloadable GitHub Release version. Only authenticated admins can request an update, and only agents running v0.2.5+ support the remote-update command.

A remote update request is queued in Cloudflare D1, returned on the normal 60-second /presence response, and applied using the existing GitHub Release downloader and rollback helper. The agent does not stop during an ACTIVE Firebird lease. An enabled standby waits until its local mode is STANDBY, while a disabled standby can update in MIRROR_ONLY mode. Existing mics.ini, DPAPI secrets, production snapshots, and print agents are not modified. Presence reports the new version after restart so Admin can mark the queued request completed.

Note: Earlier v0.2.4 and older versions do not support remote commands. Upgrade those once using the local Settings GitHub Update button; subsequent releases can be requested from Admin.

Admin remote update diagnostics: a failed Windows health check writes data/update-blocked-version.txt. The next /presence heartbeat reports that version; the Admin request transitions to failed rather than remaining pending. Examine data/update.log and wait for a corrected Release before retrying.

## Vue 3 Local Web (v0.2.6)

The three local tabs (ダッシュボード, 予定, 全て) now use Vue 3/Vite. The browser still requests the local Rust endpoints `/api/machines`, `/api/schedule`, and `/api/status` every 3 seconds and does not communicate with Cloudflare directly. The local Rust server stays restricted to 127.0.0.1:8787.

GitHub Actions builds Vue using Node.js and includes the prebuilt `www/vue-v0.2.6/` folder in the Windows portable ZIP. Windows clients need no Node.js or npm. The Rust server prefers the versioned Vue index and serves its restricted asset paths. This layout is compatible with the previous updater, which preserves old `www` files but copies new version directories. If the Vue bundle is missing, the legacy static UI remains available.

The v0.2.6 update helper copies the new version's Vue bundle before starting the agent, checks local API health and version, and restores the prior EXEs if startup fails. Existing `mics.ini`, encrypted DPAPI tokens, `data/`, and legacy custom `www` files are untouched. Vue files are only updated through new Release versions.

Frontend development: `cd local-vue && npm install && npm run build`. The generated output is `www/vue-v0.2.6/`. The Windows Release workflow performs this automatically.

## v0.2.7 — per-shift NG (不良数)

The Rust local snapshot adds `qty_ng_shift`, `qty_ng_shift_key`, and `qty_ng_shift_status` to each machine. `qty_ng` remains the original cumulative Firebird value and uploads to Cloudflare are unchanged. The Vue Dashboard displays **不良数（今シフト）** as the primary number and **累計** below it. Flame animation starts above 5 defects within the shift; the warning banner starts above 20 shift defects.

Day shift: 07:30–19:00 JST. Night shift: 19:00–07:30 JST next morning. `data/shift-ng.json` persists the last observed Firebird counter per machine/order and its accumulated shift-only increments. At each shift boundary, the display resets, even with unchanged Firebird totals. A counter decrease or order change does not create negative deltas or invent unseen defects. The first observation of a shift is the baseline (zero); exact historical defects before the first observation require time-stamped defect events, which the current cumulative counter does not provide. During a Firebird outage/standby period, missed increments cannot be attributed precisely to their shifts.

`www/vue-v0.2.7/` is the versioned frontend build. Older frontends do not show the new metric until they update. The 0.2.7 Release workflow builds Vue and Windows executables and packages both with the existing rollback flow.

## v0.2.8 defect source correction

Important correction: for machine 19, the SQL-confirmed values were `MACHINEWORK.TOTALQTYNG=24` (cumulative) and `MACHINEWORK.TODAYQTYNG=22` (MICS current-period NG). v0.2.7 incorrectly calculated an observed delta from TOTALQTYNG.

The active Firebird reader now reads TODAYQTYNG explicitly and sends `qty_ng_shift` with `qty_ng_shift_status=mics_today`. Dashboard uses it for NG display, fire animation, and warnings; cumulative `qty_ng` remains below for context. No subtractive estimate is used. If the machine is MIRROR_ONLY/STANDBY and its upstream snapshot lacks TODAYQTYNG, current-period NG displays "—" rather than a misleading zero. The upstream primary agent must supply TODAYQTYNG for a correct standby display.

MICS owns the current-period reset. Do not force 19:00 or 19:30 time boundaries in the Local Web: previously observed MICS SYSDEFINE shift settings included 07:30 and 19:30, and the server may use its own counters.


## v0.2.20 — Original template, browser-native JavaScript

The original uploaded four-page MICS UI is retained: dashboard, change schedule,
status colors, and alias editor. Its original HTML markup and CSS classes are
preserved. Vue/Vite and their browser dependencies are removed: a lightweight
native JavaScript renderer evaluates precompiled, trusted template expressions
without browser eval or remote scripts. Tailwind CSS is compiled at release
build time (no Tailwind CDN on installed PCs).

The EXE serves `www/vue-v0.2.20/index.html` as the default localhost page.
Legacy dashboard remains available via `/?legacy=1`.
JSON is sourced only from the local executable endpoints:
`/api/machines` (data/machines.json),
`/api/schedule` (data/schedule-local.json), and
`/api/ui-template/aliases` (data/change-aliases.json).
The EXE alone authenticates to D1 using the saved Coordinator Token.

The versioned directory name `vue-v0.2.20` remains **only as a compatibility
path** so updater scripts embedded in prior 0.2.x EXEs can copy the files.
The website does not load Vue. Releases are checked on Windows by GitHub Actions
before publishing; the workflow is limited to one active release build.

### v0.2.21 conditional-chain fix
The original template inserts HTML comments between `v-if` and `v-else-if` branches. The vanilla renderer now ignores those comments when choosing a branch, restoring machine cards and change-schedule rows. A build-time regression test checks the two original templates. The `vue-v0.2.21` directory name is retained solely for older updaters; Vue is not loaded.

### v0.2.22 — No automatic rollback

After a valid release is installed, `mics-only` will **never automatically
replace the new EXE with the old one** if a subsequent health or web check
fails. The installer keeps old binaries in `data/update-backup` solely for
**manual recovery**, and records `UPDATE_FAILED_NO_ROLLBACK` in
`data/update.log` when installation did not finish cleanly. Bad ZIPs are
rejected before the old agent is stopped; ACTIVE agents are never interrupted.

**Important:** the updater PowerShell script is embedded in the installed
EXE. Upgrading from a version prior to v0.2.22 still uses its old updater,
which can roll back. To switch to no-rollback behavior reliably, install the
v0.2.22 portable EXEs manually once, retaining existing `mics.ini` and
`data/` files. Subsequent updates use the new no-rollback installer.


### v0.2.24 — Direct local JSON and faster first dashboard render

The original ZIP template uses `/data/machines.json` and `/data/schedule-local.json` served from the executable's `data/` directory over localhost. These read-only routes never contact Firebird, D1 or Turso. Machine cards are displayed before the schedule file finishes loading; the schedule page waits for schedules. Identical JSON snapshots reuse their normalized result instead of repeating large schedule transformations. The agent still collects and writes Firebird data in the background, and `/?legacy=1` remains the fallback. Existing local JSON files and settings are preserved.


### v0.2.25 — Restore machine cards and correct render warnings

On each new page load the original template opens in grid mode, ignoring an old Gantt/list selection stored in the browser. Selecting Gantt explicitly waits for the saved local schedule JSON. The old SQLite table-name error is replaced with the name of the real local JSON file. Boot diagnostics inspect the actual `[data-machine]` cards and do not flag an intentionally card-free Gantt/list display. The Windows release build seeds fourteen local machine rows and checks rendered cards in a headless browser, instead of relying on HTTP status alone.


### v0.2.26 — Faster local dashboard rendering

The framework-free renderer now memoizes template computed expressions during each render, avoiding repeated calculations for every field of each machine card. The Dashboard reads machine JSON immediately; the larger local schedule is processed after the first paint and cached until it changes. Status/alias configuration refreshes in parallel instead of blocking node display. Unchanged machine JSON and settings do not trigger a full redraw. Schedule changes still update forecast displays, and the original 4-page template, data files, and no-rollback installer remain unchanged.


### v0.2.28 — Fix Settings & auto-update executable lock and ParentId detection

- Fixed `mics-only-settings.exe` passing `mics-only.exe` instead of `current_exe()` to `updater::install_with_progress`, which previously caused `-ParentId 0` to be passed on manual Settings updates.
- Settings now exits immediately after staging an update without holding a modal MessageBox lock on `mics-only-settings.exe`.
- Manual updates from Settings clear any stale `data/update-blocked-version.txt`, and `dst` is closed before `Expand-Archive` is spawned.
- `apply-update.ps1` now stops any open `mics-only-settings` process and retries executable replacement up to 10 times if Windows briefly holds a file handle.

- The loopback HTTP server (`127.0.0.1:8787`) now serves concurrent connections with `TCP_NODELAY` and a 400ms read timeout so browser preconnect sockets never stall HTML/CSS/JS/JSON requests.
- Validated `data/machines.json` and `data/schedule-local.json` payloads are cached in memory by file modification timestamp, avoiding repeated disk reads and JSON parsing on unchanged snapshots.
- Versioned static assets under `/vue-v0.2.27/` use browser caching (`max-age=3600`) for instant 0ms navigation across the 4 pages.
- Direct `/data/change-aliases.json` and `/data/status-colors.json` routes are served from the local `data/` directory, and alias edits persist locally first so offline local web editing works even without Cloudflare D1 connectivity.
- Component `<template>` fragments are parsed once and cached in `vanilla-runtime.js`, alias rules are precompiled and memoized with `WeakMap`, and `sessionStorage` snapshot hydration renders nodes on frame 0 while refreshing local JSON in the background.


## v0.2.31 — one-way alias sync (D1 → mics-only)

Alias changes are authored in Machine-Control Admin. Local Windows agents only GET `/api/mics-failover/aliases` and atomically cache validated content in `data/change-aliases.json`; they never PUT aliases or replay older `change-aliases.pending.json`. The agent pulls on startup and approximately every five minutes. The Windows Settings **Sync Alias D1 → Local** button sends PUT to the loopback agent at `/api/aliases/sync` for an immediate pull, with confirmation based on a successful GET and local file write. Browser alias PUT is read-only/rejected.

For targeted admin sync, Cloudflare queues one command per online mics-only agent in D1. The existing coordinator `/presence` heartbeat (about 60s) delivers the command, the agent pulls alias JSON, and its next heartbeat acknowledges success. Admin shows pending until the agent confirms. This does not affect Firebird leases, printing, `mics.ini`, or DPAPI.


## v0.2.32 — Alias + Status Color D1 → local

Both `change-aliases.json` and `status-colors.json` are fetched from authenticated Cloudflare D1 endpoints and validated before updating local files. The existing Settings button and Admin per-agent command download both files in one operation, acknowledging only after the two files were successfully saved. The local status color frontend reads `/data/status-colors.json` from the EXE and no longer uses browser localStorage for status colors. Local alias and status-color writes are disabled. Old local data is retained when either remote fetch fails.


## v0.2.33 — 乾燥 and stamp transitions in the local change schedule

The change schedule now includes drying-assigned mold rows even without color/mold/stamp changes, showing the persistent ♨ 乾燥 badge in daily/weekly grids, chronological list and machine modal. It does not generate false 型替え or 色替え events. Stamp-only transitions on the same mold show 刻印替え; a mold switch involving a stamp shows 刻印確認. This is presentation logic; D1 alias/status sync remains one-way.


## v0.2.34 — Only display 乾燥 on actual change events

A mold's drying alias indicates that drying is relevant, but the production pages only show `♨ 乾燥` when a row actually changes **mold (型替え)** or **color (色替え)**. Repeated production product codes of the same mold without a change do not generate drying tags or extra schedule events. Online and local use identical rules; the D1 → local alias/status-color synchronization and cached JSON data remain unchanged.


## v0.2.35 — Status Animation configuration

Machine-Control Alias Settings now owns an `animations` group inside D1's `ui_template_settings.aliases_json`. Each entry assigns an exact status code to one of `blink`, `pulse`, or `glow`. Local `mics-only` only GETs this configuration through its existing one-way Alias + Status Color sync, stores it in `data/change-aliases.json`, and applies the animation to the entire machine card when `status_code` matches. The local alias animation editor is read-only; edits are made online and pulled down to each agent. The animation is slow and obeys reduced-motion accessibility settings.


## v0.2.36 — Local JSON live socket (SSE)

Local Windows loopback agent keeps a long-lived Server-Sent Events stream at `http://127.0.0.1:8787/api/live`. A single 1-second metadata-only file watcher monitors `data/machines.json`, `data/schedule-local.json`, and the two local settings JSON files; when their file signatures change it pushes a small `mics-json` event (kind `machines`, `schedule`, or `settings`) to connected local browser tabs. The browser reads the affected JSON file on notification and rerenders only if the content differs. Its former 60-second local JSON polling becomes a fallback during SSE disconnection. No additional Cloudflare D1, Turso, or Firebird reads are generated. Only localhost is bound and event Origin is checked. EventSource automatically reconnects when the agent restarts.


## v0.2.37 — Unified change tags in machine nodes and 区分

Machine nodes and planned color/mold-change schedule tables now show `♨ 乾燥` (only when a mold/color change to a drying-tagged mold occurs) together with existing `刻印替え` (same-mold stamp change) or `刻印確認` (mold switch involving a stamp). All view layouts use one display-tags helper and no longer render duplicate drying badges. Alias data remains D1 → local read-only; the existing local SSE watcher and JSON data pipeline are unchanged.


## v0.2.38 — Correct incoming-mold 刻印確認

On mold transitions, `刻印確認` now appears only if the **incoming** job's product name matches a configured stamp alias. A stamp found on the previous mold cannot cause an incorrect tag on a following unmarked mold. Same-mold `刻印替え` and event-only `乾燥` are unchanged. Machine node badges and 区分 tables use this rule. This release supersedes v0.2.37.


## v0.2.39 — Near-realtime local next-change prediction

Local machine nodes now show 次の変更 (型替え / 色替え / 品番替え), the next product/order, and 変更予測. The next change is found in the existing `data/schedule-local.json` using the current order from `data/machines.json`. Product-only changes are detected even when the mold/color remain the same. Forecasts use cycle × REALPICKQTY × remaining quantity only for running status 200/210 and recent machine samples. When a reliable forecast is unavailable, the node explicitly shows the scheduled start with 予定. Schedule update notifications from the local SSE stream bypass the short cache throttle. No new D1/Turso/Firebird requests are needed.
