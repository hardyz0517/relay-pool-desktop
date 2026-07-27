# 2026-07-27 Architecture Scale Upgrade Stage 6 Task 25 Sync Login Closeout Shard

## Scope

- Removed the test-only legacy Sub2API synchronous login fixture from `src-tauri/src/services/collectors/sub2api.rs`.
- Removed `collectors::sub2api`, `prepare_station_login_test_v2`, and the legacy synchronous `test_station_login_input` wrapper from `src-tauri/src/services/collectors/mod.rs`.
- Removed test-only NewAPI password-login helpers from `src-tauri/src/services/collectors/drivers/newapi/auth.rs`; async login probing remains owned by `login_probe`.
- Removed the test-only `ureq` outbound builder helper and the `ureq` dev dependency.

## Boundary Notes

- Production login command flow remains async through `commands::station_collection::test_station_login_input` -> `collectors::test_station_login_input_async` -> `login_probe::test_station_login_input`.
- `src-tauri/src/services/collectors/drivers/sub2api` remains the active provider driver module and is not the deleted legacy fixture.
- Rust source no longer constructs `ureq` clients; `architecture_scale_boundaries.rs` still mentions `ureq::AgentBuilder::new` only as a forbidden construction pattern for the parser gate.
- This shard closes one Task 25 legacy-path blocker but does not by itself complete Task 25.E final graph or the Stage 6 Gate.

## Verification

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo fmt --manifest-path src-tauri\Cargo.toml -- --check`
- `cargo check --manifest-path src-tauri\Cargo.toml`
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries`
- `cargo test --manifest-path src-tauri\Cargo.toml --lib login`
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance`
- `rg -n "sub2api::test_login_credentials|collectors::sub2api|pub mod sub2api|login_access_token_with_budget_and_proxy|login_access_token_with_proxy|credential_agent_builder_for_proxy|login_with_password|test_login_credentials\(" src-tauri/src src-tauri/tests -g "*.rs"`
- `rg -n "ureq" src-tauri/src src-tauri/tests src-tauri/Cargo.toml -g "*.rs" -g "Cargo.toml"`
- Lightweight protected-path check: no Persistence V2 protected paths were modified.
