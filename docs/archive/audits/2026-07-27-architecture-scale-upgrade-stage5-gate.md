# Architecture Scale Upgrade Stage 5 Gate Audit

Date: 2026-07-27

## Scope

- Close Stage 5 after Task 19-22 provider registry, capability driver, async outbound and production `ureq` cutover shards.
- Use deterministic CLI, Rust tests, TypeScript checks and parser-backed architecture gates only.
- Do not launch the desktop app and do not inspect screenshots.
- Keep Persistence V2 untouched.

## Gate Decision

Stage 5 Gate passes for the deterministic architecture scope.

The current revision satisfies the Stage 5 requirements:

- Provider-specific capability code is registered through `src-tauri/src/services/collectors/drivers/mod.rs`.
- Supported capabilities have complete conformance fixture coverage in `docs/audits/provider-capability-matrix.json` and `src-tauri/tests/fixtures/providers/manifest.json`.
- Unsupported capabilities are explicit typed `Unsupported` entries, not guessed by generic provider code.
- Production provider/probe/management HTTP no longer depends on `ureq`; `ureq` remains only as a dev-dependency for legacy test fixtures.
- The parser-backed HTTP construction gate contains only the shared async outbound reqwest client and verified proxy upstream client construction sites.
- Adding another built-in provider now requires registry, driver and fixture/matrix changes instead of changing the supervisor, query ownership, persistence workflow or generic HTTP transport.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic
- `pnpm exec tsc --noEmit`
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `node scripts\architecture\check-typescript-boundaries.mjs` - 939 resolved edges
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside Stage 5 Gate scope
- `cargo test --locked --manifest-path src-tauri\Cargo.toml --test provider_conformance -- --nocapture` - 31 passed
- `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` - 4 passed
- `cargo test --locked --manifest-path src-tauri\Cargo.toml login_probe --lib -- --nocapture` - 2 passed
- `cargo tree --locked --manifest-path src-tauri\Cargo.toml -e normal -i ureq` - no normal dependency path printed
- `git diff --check` - passed; only Git line-ending conversion notices were printed
- Persistence V2 zero diff: `git diff -- src-tauri/src/persistence src-tauri/migrations docs/audits/persistence-v2-boundary-manifest.json`

## Stage 6 Entry

Stage 6 may start after this audit is committed. Task 23 should begin with command module physical split work and must preserve command registry, ACL, generated bindings, DTO conversion and facade boundaries after each shard.

## Not Claimed

- Stage 7 release qualification is not claimed.
- Live provider qualification is not claimed; it remains a Stage 7 release blocker.
- Existing Cargo dead-code warnings in Persistence V2, credentials and request recovery are not closed by this Stage 5 audit.
- Broad legacy adapter physical deletion belongs to Task 25 and should proceed by category after replacement coverage is in place.
