# MICS Only — Firebird backup (Windows)

This repository builds **mics-only.exe** (standby reader) and **mics-only-settings.exe** (native Windows configuration GUI), packaged as **MICS-Only-Setup.exe** by GitHub Actions.

## Installation

1. Download the `MICS-Only-Windows-Installer` artifact from GitHub Actions after a successful Windows build.
2. Run `MICS-Only-Setup.exe`. Setup launches `mics-only-settings.exe` at completion.
3. Enter backup Agent ID, Firebird host, port, server database path, read-only username and Cloudflare coordinator URL. Enter Firebird password, **backup MICS Sync Token** and **backup Coordinator Token**. These tokens must be different from the existing `mics-print` token and **must be provisioned on the Worker first**.
4. Press Save. Settings are written to `mics.ini`. Secrets are encrypted with Windows DPAPI in `%LOCALAPPDATA%\\MICS-Only\\credentials.dpapi`, never in INI or GitHub. Blank password/token fields keep prior encrypted values. Configure and run under the *same Windows user*.
5. Leave **Enable standby agent OFF** until the coordinator and primary failover control are deployed and verified. The installer does not automatically run the standby reader or connect Firebird.

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
- Dashboard HTML/CSS/JS, PWA manifest and service worker are compiled into `mics-only.exe`; no additional web executable or external server required.
- The web UI shows 14 machine nodes in MICS order, the last saved data, status code, progress, counts and cycle.
- Local snapshot file: `%LOCALAPPDATA%\MICS-Only\data\machines.json`. It updates only when a valid, leased Firebird snapshot differs from the previous sampled snapshot. The file is not a mirror of every MICS table.
- With **Enable standby agent OFF**, web still opens, displays last saved data or placeholders, and does not touch Firebird or Cloudflare.
- With standby enabled, Firebird reads and Cloudflare upload remain **fail-closed** behind the Coordinator lease. Do not enable until primary `mics-print` participates in the lease protocol.
- The local PWA can be installed from the local Windows browser because `localhost` is a secure context. A PWA installed here does not make the server reachable from other devices, including iPhone.
- The page polls local `/api/machines` and `/api/status` every 3 seconds. These requests never reach D1/Turso/Cloudflare.

### Test Connection / 接続テスト
Open `mics-only-settings.exe` and click **Test Connection** (tests only the **saved** INI):
1. Connects to Firebird **TCP port** for network reachability without login or SQL.
2. GETs the Coordinator `/health` endpoint if configured, without asking for a lease.
3. Never uploads or reads rows from Firebird. This is **not** an authentication or database-read validation test.
CLI equivalent: `mics-only.exe --test-connection`.

**Open Local Web** opens the browser at `http://127.0.0.1:8787/`. Save still restarts the agent. Only two EXEs remain.

## Agent management and GitHub Release updates (v0.2.0)
Settings now has **Start Agent**, **Stop Agent**, **Restart Agent**, and **GitHub Update** buttons.
- Start launches only `mics-only.exe`. Stop terminates only the `mics-only.exe` process on this Windows computer. The printing agent is not touched.
- Restart stops `mics-only.exe` and starts it with the saved settings. When stopped, the localhost web dashboard is also unavailable.
- Stop does not disable **Start agent with Windows**; uncheck that setting and save to prevent startup on the next Windows sign-in.
- **GitHub Update** queries `https://api.github.com/repos/maedandori/mics-only/releases/latest`, downloads `MICS-Only-Setup.exe` from the matching release if the local version differs, checks the published size, then opens the installer for user approval. There is no background auto-update.
- GitHub Actions compiles and publishes versioned release `v0.2.0` only after the Windows build and installer steps succeed. Artifacts include the two executables and editable `www/`.
- The installer preserves existing `mics.ini`, Windows DPAPI credentials and locally modified `www/` files. Remove or rename a modified asset if you intentionally want the newest default version installed.
- No release or Windows build result has been verified solely by committing source code.
