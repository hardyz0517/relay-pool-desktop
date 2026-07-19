# Persistence V2 migration baseline

Captured: 2026-07-19

Worktree: `D:\Dev\Projects\relay-pool-desktop\.worktrees\persistence-v2-upgrade`

Branch: `codex/persistence-v2-upgrade`

Accepted baseline HEAD: `f709807` (`test: stabilize newapi fixture request reads`)

## Decision

Task 0 has a green, reproducible correctness baseline when the Rust lib suite is run serially with `--test-threads=1`. The default parallel libtest mode is not an accepted correctness gate for this tree: it has failed nondeterministically in unrelated loopback fixtures, including both 16-thread and 4-thread runs. Two independent clean serial passes were recorded after P1 (153.7 s and 153.2 s in the P1 evidence); this final baseline adds one required serial run at `f709807`.

P0 (`c531e11`) stabilized the channel-monitor counting server. P1 (`f709807`) stabilized NewAPI fixture request reads. Both commits were independently spec-reviewed and quality-reviewed. They touch test helpers/regressions only and do not change persistence ownership, released schema, or production `AppDatabase` consumers.

Task 1 must formalize the serial Rust correctness gate and keep a separate bounded parallel stress qualification. Parallel flake results remain diagnostic evidence, never a green product claim.

## Repository state

The isolated worktree contained only the four Task 0 audit artifacts before staging. Current history:

```text
f709807 test: stabilize newapi fixture request reads
c531e11 test: stabilize channel monitor counting server
a611ff0 docs: plan persistence architecture v2 upgrade
7f45276 docs: advance persistence v2 architecture
6d6b4f3 docs: harden persistence v2 reliability design
```

The main checkout remained dirty and was neither copied nor reverted. Snapshot:

```text
M scripts/station-key-capability-defaults.test.mjs
M scripts/station-list-risk-tags.test.mjs
M src-tauri/Cargo.toml
M src-tauri/src/services/proxy/endpoint_adapter.rs
M src-tauri/src/services/proxy/error.rs
M src-tauri/src/services/proxy/execution.rs
M src-tauri/src/services/proxy/ingress.rs
M src-tauri/src/services/proxy/limits.rs
M src-tauri/src/services/proxy/request.rs
M src-tauri/src/services/proxy/response_body.rs
M src-tauri/src/services/proxy/routing_repository.rs
M src-tauri/src/services/proxy/runtime.rs
M src-tauri/src/services/proxy/upstream.rs
M src/features/stations/StationsPage.tsx
M src/lib/stationKeyCapabilityDefaults.ts
?? output/dev-launch/
?? output/local-routing-v2-target/
?? output/review-target/
?? output/tauri-dev-target/
?? src-tauri/target-codex-local-routing-system/
?? src-tauri/target-codex-release-027/
```

These paths are mainline drift intake. They are not part of this baseline branch.

## Structural evidence

CodeGraph MCP was used because the planned CLI query syntax did not expose equivalent JSON in this environment. Current global index status is 830 files, 22,307 nodes, and 31,746 edges on the SQLite/WAL backend. Global counts include generated/build sources; the checked-in boundary manifest freezes the task-scoped paths and direct reference count.

- `AppDatabase`: CodeGraph returned 100 callers at its output cap; current impact traversal reported 550 indexed symbols. The complete 21-path Rust scope contains 507 direct reference lines.
- `DataDirConfigV2`: impact traversal reports two symbols; no consumer ownership changed.
- `FinalizationDispatcher`: current impact traversal reports 45 symbols after the expanded proxy test/runtime index; no ownership changed.
- A full `rg -l AppDatabase src-tauri/src --glob '*.rs'` scan finds 21 unique physical Rust source paths. The manifest covers all 21 paths and 22 canonical consumer units: NewAPI `mod.rs` is one physical path with separately owned production and test units. The manifest also contains 33 consumer evidence entries; this is a symbol/path inventory, not the canonical unit collection, and multiple evidence entries can belong to one unit. Production symbols are separated from test-only consumers and grouped as command, proxy, collector, monitor, query, settings, and test.
- The field ledger covers 23 tables and their `ALTER TABLE` / `add_column_if_missing` fields.
- Public tags `v0.1.0` through `v0.3.1` still resolve to the same 11 source profiles; P0/P1 are after all release tags and do not modify `database.rs`.

The released-schema `provisional_fixture_hash` values remain explicitly provisional source-descriptor hashes. They are not SQLite fixture hashes. Task 12 must generate sanitized databases from every public release, hash canonical ordered `sqlite_schema` plus deterministic canaries, and replace these values before importer acceptance.

## Accepted gates

All commands ran from the isolated worktree at `f709807`:

| Gate | Result | Exact output | Duration |
| --- | --- | --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --nocapture --test-threads=1` | PASS | `running 671 tests`; `670 passed; 0 failed; 1 ignored` (671 discovered, 670 executed successfully, 1 ignored) | 169.611 s (test body 151.32 s) |
| `pnpm.cmd test:contracts` | PASS | exit 0; all contract subprocesses passed | 38.557 s |
| `pnpm.cmd build` | PASS | theme audit passed (200 files); TypeScript passed; Vite transformed 2,202 modules and built three assets | 86.487 s |
| `cargo check --manifest-path src-tauri/Cargo.toml` | PASS | exit 0; warnings only | 5.002 s |

The frontend build retains the existing warning that the main JavaScript chunk exceeds 500 kB. Rust checks retain existing dead-code warnings. Neither is treated as a hidden pass or silently suppressed.

## Classified preflight failures

Before P0, the default full suite failed in `station_monitor_applies_max_concurrency_to_key_probes`: a Windows nonblocking socket read returned OS error 10035 and the test observed one probe instead of two. P0 stabilized that counting server and added its regression.

After P0, the default parallel suite still failed nondeterministically in `create_remote_key_posts_token_then_reconciles_and_reveals_secret`, with `requests[0]` not guaranteed to be the token request. P1 stabilized fixture request reads and added its regression. Independent default 16-thread and 4-thread runs had also shown unrelated loopback fixture failures. These failures are classified as test-harness concurrency defects because independent serial runs passed; they are not accepted as product failures and are not ignored by the architecture plan.

## Forward obligations

1. Task 1 must add an explicit serial correctness gate and a separate bounded parallel stress qualification.
2. Task 12 must replace provisional release source hashes with real sanitized SQLite fixture hashes.
3. Architecture gates must consume the checked-in boundary manifest and reject new production `AppDatabase` consumers.
4. Existing user-visible history remains `retain-until-explicit-user-cleanup`; fixture size is not permission to delete it.
