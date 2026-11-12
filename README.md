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
