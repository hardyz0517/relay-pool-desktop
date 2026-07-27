# 2026-07-27 Architecture Scale Upgrade Stage 6 Task 25 Final Graph Shard

## Scope

- Advanced `architecture-scale-boundary-manifest.json` to `current_stage: 6` with source graph revision `a628a5d511a9039f1f8f1ad7141392136ee881b2`.
- Updated `architecture-scale-upgrade-inventory.json` to remove stale provider adapter paths and clear expired `temporary_adapters`.
- Reconciled the provider/outbound ledger after deleting the legacy NewAPI adapter island and the synchronous Sub2API login fixture.
- Recorded that `src-tauri/src/services/outbound.rs` remains a proxy-config compatibility facade, not an HTTP client construction owner.

## Final Graph State

- `temporary_adapters`: 0
- `temporary_architecture_allowlist`: 0
- boundary `temporary_edges`: 0
- production `ureq` source/dependency: removed
- remaining production HTTP client construction: `src-tauri/src/outbound/client.rs` and `src-tauri/src/services/proxy/upstream.rs`, both covered by owned boundary allowlist entries expiring after Stage 7.
- deleted legacy provider adapter paths are no longer present in the machine inventory.

## Gate Result

Task 25.E final graph passes. Stage 6 Gate passes for the current graph and inventory. Stage 7 qualification remains unopened and must start from this gated Stage 6 baseline.

## Verification

- `pnpm.cmd run architecture:typescript`
- `pnpm.cmd run architecture:commands`
- `pnpm.cmd run architecture:security`
- `pnpm.cmd run architecture:artifacts`
- `pnpm.cmd run architecture:fixtures`
- `pnpm.cmd run architecture:dependencies`
- `cargo fmt --manifest-path src-tauri\Cargo.toml -- --check`
- `cargo check --manifest-path src-tauri\Cargo.toml`
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries`
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance`
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi`
- `git diff --check`
- JSON parse: `architecture-scale-boundary-manifest.json`, `architecture-scale-upgrade-inventory.json`
- stale path check: no `collectors/adapters`, legacy `collectors/sub2api`, expired temporary adapter ids, or stale `src-tauri/src/services/outbound.rs` delete path remains in the machine manifests.
- Lightweight protected-path check: no Persistence V2 protected paths were modified.
