# Stage 6 Task 23 Closeout - Command Module Split

Date: 2026-07-27

## Scope

- Close Task 23 after moving command handlers out of `src-tauri/src/commands/mod.rs` by domain shard.
- Verify the compiled command registry, command state boundary inventory and generated bindings still agree after the final command split.
- This closeout does not claim Stage 6 Gate; Task 24 and Task 25 remain.

## Result

- `src-tauri/src/commands/mod.rs` is 295 lines.
- `src-tauri/src/commands/mod.rs` contains no `#[tauri::command]`, `pub async fn`, station-key connectivity DTO/helper body, capture command/helper body or data-store startup command/helper body.
- Remaining parent-module responsibilities are:
  - command module declarations
  - shared public error mapping helpers used by sibling command modules
  - shared OS URL launch and external URL validation helpers used by settings/CCSwitch import
  - small unit tests for shared error mapping, external URL validation and CCSwitch deeplink behavior
- The command registry maps production commands to focused modules; there is no new `misc.rs` module.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside Task 23
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic
- Focused final shard tests:
  - `cargo test --locked --manifest-path src-tauri\Cargo.toml located_candidate --lib -- --nocapture` - 2 passed
  - `cargo test --locked --manifest-path src-tauri\Cargo.toml capture --lib -- --nocapture` - 36 passed
  - `cargo test --locked --manifest-path src-tauri\Cargo.toml station_key_connectivity --lib -- --nocapture` - 27 passed

## Remaining Stage 6 Work

- Task 24: split giant frontend pages by state ownership and public feature entries.
- Task 25: provider physical closeout, legacy path deletion, source-contract test replacement, artifact policy cleanup and final graph cleanup.
- Stage 6 Gate remains blocked until Task 24 and Task 25 pass.
