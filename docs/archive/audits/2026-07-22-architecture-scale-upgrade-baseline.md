# Architecture Scale Upgrade Baseline

## Scope and provenance

- Upgrade worktree: `D:/Dev/Projects/relay-pool-desktop-architecture-scale-upgrade`
- Branch: `codex/architecture-scale-upgrade`
- Frozen architecture source revision: `6a204908e2b3697e4c80951771950e29872942d9`
- Stable production base: `f8c74bb1942c4ab0ce3b48afd15b762920097b2a`
- Baseline date: 2026-07-23 (Asia/Shanghai)
- Host: Windows 11 build 26200, AMD Ryzen 7 3700X, 16 logical CPUs, 17,035,784,192 bytes RAM
- Toolchain: Node 24.14.1, pnpm 11.7.0, rustc/cargo 1.95.0, Rust edition 2021, crate MSRV 1.89
- Persistence V2 files modified by Task 0: none

The main checkout was not used as an implementation surface. It advanced independently after the worktree was created. Baseline evidence is therefore tied to the two revisions above, not to the moving main checkout. Generated schema files that only changed working-tree metadata or line-ending perception are excluded unless `git diff` shows content.

## Reproducible correctness baseline

| Surface | Command | Result | Duration / detail |
|---|---|---|---|
| Dependency hydration | `pnpm.cmd install --frozen-lockfile --reporter=append-only` | pass | 226 packages |
| Frontend contracts | `pnpm.cmd test:contracts` | pass | 332.3 s |
| Frontend unit/component | `pnpm.cmd test` | pass | 16 files, 65 tests, about 78 s |
| Production frontend build | `pnpm.cmd build` | pass with size warning | main JS 1,021,660 bytes, gzip 282,860 bytes; Vite 500 kB warning |
| Rust compile | `cargo check --locked --manifest-path src-tauri/Cargo.toml` | pass | first build about 517.6 s |
| Rust library tests | `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib` | pass | 556 tests |
| Rust integration targets | each locked integration target at frozen HEAD | pass | 16 pre-upgrade targets; combined run about 399 s including lock wait |

One later aggregate `cargo test` attempt was invalid as baseline evidence because a concurrently-created Stage 0 architecture test did not yet compile. It is classified as test-construction overlap, not as a production regression. Stage 0 and later qualification rerun the final staged snapshot serially.

## Architecture inventory snapshot

The machine-readable owner ledger is `architecture-scale-upgrade-inventory.json`. At the frozen baseline it records:

- 121 compiled ACL command identities and 103 command signatures receiving the broad `AppServices` state.
- 115 frontend `invoke` call sites and one `useQueries` station fan-out.
- 26 `thread::spawn`, 5 `tokio::spawn`, 16 `spawn_blocking`, and no single join owner.
- 40 `ureq` references and 21 `reqwest::Client` references across mixed network policy paths.
- `csp: null`, capture URL shells `http://*` and `https://*`, direct exit, and primary drain in `RunEvent::Exit`.

These are intake measurements only. Permanent gates use parser/compiled identities and owned allowlists rather than frozen regex counts.

## Characterization coverage

Existing contract, component, and Rust tests characterize data-store startup, station/key CRUD and ordering, collector facts, monitor state, proxy lifecycle, page navigation, and browser preview. They do not yet prove:

- a generated IPC registry and serialization parity;
- production/demo graph isolation and fail-closed desktop bootstrap;
- exact-origin capture authorization;
- `ExitRequested` bounded asynchronous drain;
- O(1) backend reads for 10/100/500 rows;
- real Tauri IPC or WebView2 commit timings.

Those gaps have one owner and one qualification gate in the inventory, boundary, security, and performance ledgers.

## Scale baseline contract

Stage 0 generates deterministic, masked 10/100/500 station/key datasets under `output/architecture-scale/`. The dataset SHA-256, raw repeated samples, frontend invoke count, projected JSON response bytes, TanStack Query lifecycle, rendered row count, hidden-query starts, and data-ready React commit are measured by the repository-owned harness.

The harness executes the production query factories and Tauri invoke wrappers against deterministic mock IPC in Vitest/jsdom. With five warm-ups and 30 samples, the current topology issues exactly 13, 103, and 503 invokes for 10, 100, and 500 stations (`N + 3`). Projected response payload p50/p95 is 3,526/3,526, 33,385/33,385, and 170,467/170,467 bytes. Data-ready React commit p50/p95 is 31.0411/31.7979, 30.1946/31.4792, and 57.9020/64.7161 ms; each run renders the expected row count and the hidden probe starts zero queries. Raw samples and lifecycle events are retained in `output/architecture-scale/baseline/frontend-report.json`.

This is qualified only as `frontend-jsdom-current-query-topology-baseline-only`. It proves the existing linear fan-out and provides frontend regression evidence, but it is not real Tauri IPC, native WebView2 timing, or backend SQL evidence.

The following are deliberately not inferred from source or mocks: backend read-port round trips, runtime SQL statement count, SQL duration/plan, real Tauri IPC bytes/duration, and WebView2 page commit. They remain typed `blocked` metrics owned by Task 11 and closed by Task 26. A zero, static call count, or jsdom duration is not accepted as a substitute.

## Baseline exit decision

The correctness baseline is green and reproducible enough to start architecture work. Performance qualification is partially established: the deterministic frontend shard is generated and validated in Stage 0. Explicitly-owned backend and native-runtime blockers prevent the Stage 3/7 performance gates until Task 11 and Task 26 supply backend/Tauri/WebView evidence.
