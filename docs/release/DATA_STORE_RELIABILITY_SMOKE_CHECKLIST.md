# Data Store Reliability Smoke Checklist

Run this checklist on disposable Windows profiles before publishing a release that changes startup, update, or data-directory code. Do not attach real API keys, cookies, logs, or raw databases to the checklist record.

## Profile A: v0.3.1, default data directory

- Install the signed v0.3.1 public release.
- Create or import at least one station, one station key, one settings change, one channel monitor, and one request/log-like activity if available.
- Record only sanitized counts and the active data path category: default/custom, not the full username path.
- Update through the real updater.
- Verify the first v0.3.2 start creates and validates generation 2 before it commits the active configuration.
- Verify a generation-1 verified backup exists; record only its category and sanitized hash, not its path or contents.
- Open Pricing, Channels, Change Center, Stations, Settings.
- Exit from tray, cold start, and verify the same sanitized counts remain visible.

## Profile B: v0.3.1, custom or pending data directory

- Install the signed v0.3.1 public release.
- Move the data directory through Settings and restart as instructed.
- Record sanitized counts and whether active path is custom.
- Update through the real updater.
- Cold start and verify the selected custom data directory remains active and counts match.

## Recovery injections

- After the upgrade, rename the active `relay-pool-desktop-v2.sqlite3` out of the way; app must show recovery before business pages mount.
- Create a source/target conflict; app must show conflict recovery and must not overwrite either database.
- Select a healthy candidate; verify backup creation, config commit, restart requirement, and intact unselected database.
- Export diagnostics and inspect JSON for absence of usernames, station names, URLs, API keys, cookies, ciphertext, nonce, AAD, and request bodies.

## Downgrade boundary

- Fully exit v0.3.2, then launch the signed v0.3.1 binary against the upgraded data directory.
- Verify v0.3.1 fails closed after the generation-2 tombstone and does not create or mutate SQLite, WAL, or SHM files.
- Restore generation 1 only through the documented controlled recovery process; never rename the V2 database into the V1 filename.

## Release gate commands

```powershell
pnpm verify:release
cargo test --manifest-path src-tauri/Cargo.toml services::data_store -- --nocapture
pnpm tauri:build -- --target x86_64-pc-windows-msvc
```
