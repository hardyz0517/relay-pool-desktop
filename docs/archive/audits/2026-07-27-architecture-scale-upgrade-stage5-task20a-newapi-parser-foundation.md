# Architecture Scale Upgrade Stage 5 Task 20.A Audit

## Scope

- Worktree: `D:/Dev/Projects/relay-pool-desktop-architecture-scale-upgrade`
- Branch: `codex/architecture-scale-upgrade`
- Base revision before this shard: `ff59eaa feat: cut over openai compatible collector driver`
- Shard: Stage 5 / Task 20.A, NewAPI collector parser foundation
- Governing documents:
  - `docs/archive/specs/2026-07-22-architecture-scale-upgrade-design.md`
  - `docs/archive/plans/2026-07-22-architecture-scale-upgrade-master-plan.md`
- Persistence V2 boundary: unchanged. The audit diff check for `src-tauri/src/persistence`, `src-tauri/migrations`, and `docs/audits/persistence-v2-boundary-manifest.json` was empty.
- UI/runtime inspection: not performed. This shard used source, unit tests, provider conformance, parser-backed gates, and command output only.

## Shard Decision

Task 20.A is a foundation shard, not a production NewAPI capability cutover. It moves the pure NewAPI collector parser owner into `src-tauri/src/services/collectors/drivers/newapi/parsers.rs` and leaves `src-tauri/src/services/collectors/adapters/newapi/parsers.rs` as a transition shim that re-exports the driver parser for the legacy adapter.

NewAPI remains unsupported in `provider-capability-matrix.json` and in the static ProviderRegistry. No NewAPI collector, remote-key, authorization, HTTP client, session resolution, WebView capture, or remote-key service production path is cut over in this shard.

## Requirements Evidence

| Task 20 requirement | Current evidence | Result |
|---|---|---|
| Lock NewAPI collector facts before driver cutover | NewAPI parser tests now live under `services::collectors::drivers::newapi::parsers` and cover envelope handling, quota conversion, missing quota behavior, non-finite values, fractional request counts, usage totals, group/rate mapping, model string parsing, duplicate handling, and rejection of nonstandard model object entries. | Pass |
| Move provider-specific parsing toward capability driver ownership | `drivers/newapi/parsers.rs` owns parser functions and stable NewAPI group hash construction. The old adapter parser module only re-exports the driver parser so the existing adapter keeps current behavior while the parser owner moves to the driver tree. | Pass |
| Do not migrate NewAPI HTTP by copying `ureq` into a driver | This shard does not introduce any NewAPI driver HTTP client or production request path. Existing `ureq` sites remain in the legacy adapter/client and stay owned by later Task 20 collector cutover and Task 22 final deletion. | Pass |
| Do not mark NewAPI collector supported before cutover | `provider-capability-matrix.json` and `drivers/mod.rs` still declare NewAPI collector, remote-key, and authorization capabilities as unsupported. | Pass |
| Keep provider conformance compiling the driver tree | `provider_conformance` now includes the NewAPI parser module and its tests through `drivers/mod.rs`; the harness supplies only the minimal group-category stub needed for parser tests. | Pass |
| Do not touch Persistence V2 | `git diff -- src-tauri/src/persistence src-tauri/migrations docs/audits/persistence-v2-boundary-manifest.json` printed no diff. | Pass |

## Verification Commands

| Command | Result |
|---|---|
| `cargo fmt --manifest-path src-tauri\Cargo.toml --check` | Pass |
| `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` | Pass |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml services::collectors::drivers::newapi --lib -- --nocapture` | Pass, 9 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml newapi_ --lib -- --nocapture` | Pass, 17 passed / 1 ignored live test |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test provider_conformance -- --nocapture` | Pass, 24 passed |
| Persistence V2 zero-diff check | Pass |

## Residual Work

- Task 20.B must migrate the NewAPI collector HTTP/auth/request path to `AsyncOutboundClient`, implement the NewAPI collector driver, update matrix/fixtures to supported only after the full conformance scenario set exists, and delete the collector string/`ureq` fallback for that capability.
- Task 20.C must migrate NewAPI remote-key capability with explicit idempotency/result-unknown/reconciliation semantics.
- Task 20.D must migrate NewAPI authorization validation without moving WebView window/cookie capture or secret storage into the driver.
- Stage 5 Gate remains open.
