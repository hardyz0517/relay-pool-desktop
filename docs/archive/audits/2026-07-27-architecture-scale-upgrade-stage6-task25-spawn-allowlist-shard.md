# Stage 6 Task 25.B/E Shard - Remote-Key and Shell Spawn Allowlist Closeout

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Remove direct remote-key `tokio::task::spawn_blocking` sites from the command facade.
- Delete expired remote-key and shell-open spawn allowlist entries from the Rust architecture boundary manifest.
- Keep unsupported remote-key scan behavior for non-provider station types while removing supported-provider synchronous fallback paths.

## Changes

- Updated `RemoteKeysCommandFacade`.
  - Injects `BlockingExecutor` through composition.
  - Routes synchronous provider context preparation through the managed blocking owner.
  - Removes direct `tokio::task::spawn_blocking` from scan/create/reveal command flows.
- Updated `remote_keys` service.
  - Replaced legacy sync scan fallback with `prepare_unsupported_remote_key_scan_v2`.
  - Removed sync create/reveal fallback functions that only redirected supported providers to async drivers.
- Updated shell-open command helpers.
  - Replaced direct `Command::spawn()` with bounded `Command::status()` result handling in external URL and data-store backup directory open helpers.
- Updated `architecture-scale-boundary-manifest.json`.
  - Removed four remote-key direct spawn allowlist entries.
  - Removed two expired shell-open spawn allowlist entries.
- Tightened legacy adapter test fixtures.
  - `CreatedRemoteKey` is now test-only.
  - Legacy NewAPI/Sub2API create tests assert the compatibility message field.

## Boundary Notes

- `rg` found no direct remote-key facade `spawn_blocking` sites after the change.
- Spawn scan on touched files now reports only the managed `BlockingExecutor` allowlist entry.
- `cargo test --test architecture_scale_boundaries` passed after manifest cleanup.
- No Persistence V2 paths were modified in this shard:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/audits/persistence-v2-boundary-manifest.json`

## Evidence

- `cargo fmt` - passed
- `cargo check` - passed with existing 12 warnings
- `cargo test --test architecture_scale_boundaries` - 4 tests passed
- `cargo test --test provider_conformance` - 34 tests passed
- `cargo test --lib create_remote_key_posts_token_then_reconciles_and_reveals_secret` - 1 test passed
- `cargo test --lib create_remote_key_posts_selected_group_id_from_binding` - 1 test passed
- `cargo test --lib commands::` - 54 tests passed

## Verification Notes

- A first parallel `cargo test --test provider_conformance` run timed out and hit a broken pipe while waiting on Cargo locks; it is not counted as passing evidence. The command was rerun serially and passed.
- `git diff --name-only | rg "^(src-tauri/src/persistence|src-tauri/migrations|docs/audits/persistence-v2-boundary-manifest\\.json)"` returned no matches.
