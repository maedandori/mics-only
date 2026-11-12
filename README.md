# mics-only — MICS Firebird standby reader

Rust standby agent extracted from the read-only MICS sync functionality of `maedandori/mics-print`. **No printing**.

## Safe operation
This application **never reads Firebird or uploads to D1 without a valid lease** from the Cloudflare coordinator. The coordinator endpoint must be implemented/deployed separately; do not enable a second reader on an unprotected legacy sync endpoint.

1. Copy `mics.ini.example` to `mics.ini` beside `mics-only.exe`.
2. Configure Firebird host/database/user and HTTPS coordinator URL.
3. Set environment variables `MICS_DB_PASSWORD`, `MICS_SYNC_TOKEN`, and `MICS_COORDINATOR_TOKEN` on the Windows account running the app. Do **not** commit credentials.
4. Run `mics-only.exe --check-config` then `mics-only.exe`.
5. Install as a background process via Task Scheduler, using a dedicated least-privilege account.

## Required coordinator contract
POST `<coordinator_url>/lease` with Bearer coordinator token and JSON `{"agent_id":"...","role":"backup","ttl_seconds":45}` returns `{"granted":true,"epoch":123,"lease_token":"opaque","expires_at":"RFC3339"}` only if PRIMARY is not healthy.  A denial returns `{"granted":false}`.
POST `<coordinator_url>/sync` with Bearer sync token and headers `X-Mics-Agent-Id`, `X-Mics-Lease-Token`, `X-Mics-Lease-Epoch` accepts the normal mics-print snapshot **only for the current, unexpired lease**. It must atomically validate lease and serialize the D1 write, rejecting stale epochs. A valid lease only controls D1 ingestion, not existing Firebird sockets.

The PRIMARY mics-print agent must also use this lease protocol and report MICS-specific health, independently of printing status, before automatic failover can be enabled. Until then this repo provides a safe standby foundation, **not a deployed end-to-end failover**.

Build: `cargo test && cargo build --release`.
