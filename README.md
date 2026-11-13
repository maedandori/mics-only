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
