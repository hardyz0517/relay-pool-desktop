# Routing hierarchical pre-migration development checkpoint

Status: development checkpoint. Public signed release and install/upgrade qualification are intentionally deferred because the project is not yet a stable product.

This document is the tracked qualification checklist for the routing hierarchical pre-migration checkpoint. It intentionally does not record runtime artifacts, installer hashes, signing logs, screenshots, local databases, API keys, cookies, request payloads, model names from user traffic, station URLs, or entity identifiers.

## Development-stage release policy

Relay Pool Desktop is still in a non-stable development phase. The project does not currently promise public signed releases, automatic update continuity, or old-binary rollback support for this routing upgrade. Users may recover by reinstalling, clearing local data, or reimporting configuration.

The former Task 11 release gate is therefore downgraded to a local development qualification gate. Keep the safety checks that prevent secret leakage, schema breakage, and accidental production selector changes; do not block Task 12 on release authorization, signed installers, or published-channel evidence. If the project enters a stable product phase, re-enable the release gate before shipping to users.

## Candidate scope

- Branch: `codex/routing-operational-upgrade`
- Plan: `docs/superpowers/plans/2026-07-30-routing-operational-unification-upgrade.md`, Task 11
- Spec reference: `docs/superpowers/specs/2026-07-30-routing-operational-unification-upgrade-spec.md`
- Checkpoint purpose: keep the legacy production router with hierarchical routing configuration readiness and safe request-log URL write boundaries before the real hierarchical selector cutover.
- Production selector change: none. The pre-migration binary must keep unmigrated users on the existing router behavior, and saved `hierarchical_v1` configuration must not receive production traffic in this release.

## Checkpoint fields

These fields should be filled from a clean worktree after the tracked Task 11 preparation commit exists. If any tracked file changes after the checkpoint revision is recorded, the verification sequence must restart from the new commit.

| Field | Value |
|---|---|
| `premigration_revision` | `14d5a3a40ecb03acd2090203df75becd66f9df58` |
| Release tag | Deferred for stable-product phase |
| Baseline installer | Deferred for stable-product phase |
| Candidate installer | Deferred for stable-product phase |
| Baseline version | Deferred for stable-product phase |
| Candidate version | Deferred for stable-product phase |
| Install/upgrade report | Deferred for stable-product phase |
| Published channel | Deferred for stable-product phase |

## Required local contracts before Task 12

- Fresh schema, released schema, and the existing five legacy routing policy fixtures must remain bootable.
- Readiness checks must be read-only and must not alter legacy route selection.
- Confirmed hierarchical migration config must be saved completely, but the production selector must not consume it before the cutover tasks.
- Import/export must preserve legacy routing fields and mark fields that become ignored after cutover rather than dropping user data.
- Request-log queries must tolerate historical `request_logs.upstream_base_url` values being `NULL` or already redacted.
- New request-log writes must persist `request_logs.upstream_base_url` as `NULL`; station/key identity and safe endpoint classification are the retained routing evidence.
- Logs UI/query code must not reconstruct historical upstream URLs from the current Station URL.
- Configuration readiness statistics must only persist low-cardinality aggregates; they must not include entity IDs, model names, or URLs.
- The install/upgrade matrix script must require explicit baseline/candidate installer paths, explicit baseline/candidate versions, and an explicit output path. It must not contain versioned defaults or version-specific labels.

## Required development evidence

The following evidence is intentionally not tracked in the repository. It belongs in ignored local output or CI artifacts for the checkpoint revision.

```powershell
pnpm.cmd verify:fast
pnpm.cmd verify:full
node scripts/install-upgrade-matrix-contract.test.mjs
node scripts/local-routing-redaction.test.mjs
cargo build --release --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```

The install/upgrade matrix command remains available as an optional stable-release check only when all five environment variables above have been explicitly set. Script defaults are not allowed for the installers, versions, or output path.

## Deferred stable-release work

Before treating Relay Pool Desktop as a stable product, restore the signed release workflow:

- Create a release tag for the exact qualified revision.
- Build and sign the candidate installer.
- Publish through the supported update/download channel.
- Run the install/upgrade matrix from an explicit supported baseline to the candidate.
- Record only redacted evidence in ignored output or CI artifacts.

This deferred work does not block Task 12 during the current development phase.
