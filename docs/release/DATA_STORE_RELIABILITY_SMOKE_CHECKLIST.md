# Data Store Reliability Smoke Checklist

Run this checklist on disposable Windows profiles before publishing a release that changes startup, update, or data-directory code. Do not attach real API keys, cookies, logs, or raw databases to the checklist record.

## Profile A: previous release, default data directory

- Install the previous public release.
- Create or import at least one station, one station key, one settings change, one channel monitor, and one request/log-like activity if available.
- Record only sanitized counts and the active data path category: default/custom, not the full username path.
- Update through the real updater.
- Open Pricing, Channels, Change Center, Stations, Settings.
- Exit from tray, cold start, and verify the same sanitized counts remain visible.

## Profile B: previous release, custom or pending data directory

- Install the previous public release.
- Move the data directory through Settings and restart as instructed.
- Record sanitized counts and whether active path is custom.
- Update through the real updater.
- Cold start and verify the selected custom data directory remains active and counts match.

## Recovery injections

- Rename the active `relay-pool-desktop.sqlite3` out of the way; app must show recovery before business pages mount.
- Create a source/target conflict; app must show conflict recovery and must not overwrite either database.
- Select a healthy candidate; verify backup creation, config commit, restart requirement, and intact unselected database.
- Export diagnostics and inspect JSON for absence of usernames, station names, URLs, API keys, cookies, ciphertext, nonce, AAD, and request bodies.

## Release gate commands

```powershell
pnpm verify:release
cargo test --manifest-path src-tauri/Cargo.toml services::data_store -- --nocapture
pnpm tauri:build -- --target x86_64-pc-windows-msvc
```
