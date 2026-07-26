# Architecture Scale Upgrade Stage 5 Task 19.A Audit

## Scope

- Worktree: `D:/Dev/Projects/relay-pool-desktop-architecture-scale-upgrade`
- Branch: `codex/architecture-scale-upgrade`
- Shard: Stage 5 / Task 19.A, capability contracts and ProviderRegistry
- Governing documents:
  - `docs/superpowers/specs/2026-07-22-architecture-scale-upgrade-design.md`
  - `docs/superpowers/plans/2026-07-22-architecture-scale-upgrade-master-plan.md`
- Persistence V2 boundary: unchanged. The audit diff check for `src-tauri/src/persistence`, `src-tauri/migrations`, and `docs/superpowers/audits/persistence-v2-boundary-manifest.json` was empty.
- UI/runtime inspection: not performed. This shard used source, typed contracts, unit tests, parser-backed gates, and command output only.

## Shard Decision

Task 19.A is implemented as an inert foundation. It freezes provider identity, capability-specific driver traits, typed driver failure/evidence, static ProviderRegistry construction, and auth refresh single-flight ownership without cutting over NewAPI, Sub2API, or OpenAI-compatible production adapter paths.

Task 19.B may start next. Task 19.C and Tasks 20-22 must not assume a migrated production provider path until their own conformance/cutover shards pass.

## Requirements Evidence

| Task 19.A requirement | Current evidence | Result |
|---|---|---|
| `ProviderKind` is closed and unknown historical strings do not map to custom | `ProviderKind::parse` accepts only `sub2api`, `newapi`, and OpenAI-compatible aliases. Unit test rejects `custom` and unknown values. | Pass |
| Capability traits are split rather than a universal provider trait | `CollectorDriver`, `RemoteKeyDriver`, and `AuthorizationDriver` are separate traits in `contract.rs`; `ProviderEntry` holds each capability as an independent optional object. | Pass |
| Contracts avoid Tauri/Application/Persistence DTO inputs | New contract types use canonical station identity, endpoint roles, opaque credential handles, request budget, cancellation token, outbound client, proxy policy, facts, evidence, and diagnostics. No Tauri state, application service, persistence store, query key, or UI DTO is accepted by the traits. | Pass |
| Raw credentials are not Clone/Debug contract data | Driver context carries `OpaqueCredentialHandle`; secrets are resolved through `DriverSecretAccessor` into `CredentialSecret`, which does not derive Clone or Debug and is zeroized on drop. | Pass |
| Registry only returns descriptors/capability objects | `ProviderRegistry` only has descriptor/capability lookup methods. It does not perform network, retry, persistence, scheduling, or provider-name fallback. | Pass |
| Duplicate kind, missing registration, and descriptor/capability mismatch fail closed | `ProviderRegistry::new` validates duplicate kinds, required known provider registration, descriptor/capability presence, and capability kind matching at composition time. Focused registry tests cover duplicate, missing, and unsupported lookup. | Pass |
| Missing capability returns typed Unsupported | `collector`, `remote_key`, and `authorization` return `DriverFailureKind::Unsupported` when a registered provider lacks that capability. Composition tests keep every capability unsupported until later cutover shards. | Pass |
| Driver failure has fixed classification fields | `DriverFailure` carries kind, retry disposition, auth effect, failed endpoint, bounded evidence, and sanitized detail. Tests cover unsupported and auth-rejected decision fields. | Pass |
| Evidence is redacted and bounded | `EvidenceSet` caps evidence item count and bounds method/url/detail text after existing secret redaction. Unit test covers URL and Authorization bearer canaries. | Pass |
| Auth/session refresh single-flight owner is explicit | `AuthRefreshSingleFlight` is the in-memory owner keyed by provider, station id, endpoint revision, credential revision, and credential scope. Tests prove one side effect per revision, separate revision scope, and waiter cancellation without a second refresh. | Pass |
| Static registry is composed but production providers are not cut over | `compose_provider_registry` builds static entries for Sub2API, NewAPI, and OpenAI-compatible and `lib.rs` composes it during setup as a fail-closed startup preflight. All capabilities remain absent for Task 19.B/19.C/20/21 cutover. | Pass |

## Verification Commands

| Command | Result |
|---|---|
| `cargo fmt --manifest-path src-tauri\Cargo.toml --check` | Pass |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml auth_refresh --lib -- --nocapture` | Pass, 4 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml provider_registry --lib -- --nocapture` | Pass, 2 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml evidence_is_redacted_and_bounded --lib -- --nocapture` | Pass, 1 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml failure_carries --lib -- --nocapture` | Pass, 1 passed |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml unsupported_failure --lib -- --nocapture` | Pass, 1 passed |
| `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` | Pass |
| `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` | Pass, 4 passed |
| `pnpm generate:bindings --check` | Pass, 4 artifacts, two-run deterministic |
| `pnpm exec tsc --noEmit` | Pass |
| `node scripts\architecture\check-command-state-boundaries.mjs` | Pass, 104 migrated commands |
| `node scripts\architecture\check-typescript-boundaries.mjs` | Pass, 939 resolved edges |
| `node scripts\architecture\check-build-entries.mjs` | Pass, 422 production modules and 246 demo modules |
| `node scripts\architecture\check-command-registry.mjs` | Pass, 125 commands |
| `node scripts\architecture\check-tauri-security.mjs` | Pass, 2 capabilities |
| `node scripts\architecture\check-artifact-policy.mjs` | Pass, 6 registered legacy roots |
| `node scripts\architecture\check-dependency-lifecycle.mjs` | Pass, 18 entries |
| `node scripts\architecture\check-fixtures.mjs` | Pass |
| JSON parse check for architecture/persistence audit manifests | Pass |
| Persistence V2 zero-diff check | Pass |

Expected panic text was printed by existing request fixture rejection tests during broader filtered runs; those test binaries exited successfully when they were not interrupted by command timeout.

## Residual Work

- Task 19.B must add the provider conformance harness and capability matrix before reference or legacy provider cutovers.
- Task 19.C must move OpenAI-compatible into the reference driver and prove applicable conformance.
- Tasks 20-22 still own NewAPI/Sub2API/updater/probe HTTP cutovers and deletion of legacy `ureq`/blocking provider paths.
- Stage 5 Gate is not claimed by this shard.
