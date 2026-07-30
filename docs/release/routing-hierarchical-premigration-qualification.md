# Routing hierarchical pre-migration qualification

Status: engineering preparation in progress; signed release and install/upgrade qualification are not complete.

This document is the tracked release-freeze checklist for the routing hierarchical pre-migration binary. It intentionally does not record runtime artifacts, installer hashes, signing logs, screenshots, local databases, API keys, cookies, request payloads, model names from user traffic, station URLs, or entity identifiers.

## Candidate scope

- Branch: `codex/routing-operational-upgrade`
- Plan: `docs/superpowers/plans/2026-07-30-routing-operational-unification-upgrade.md`, Task 11
- Spec reference: `docs/superpowers/specs/2026-07-30-routing-operational-unification-upgrade-spec.md`
- Release purpose: ship the legacy production router with hierarchical routing configuration readiness and safe request-log URL write boundaries before the real hierarchical selector cutover.
- Production selector change: none. The pre-migration binary must keep unmigrated users on the existing router behavior, and saved `hierarchical_v1` configuration must not receive production traffic in this release.

## Freeze fields

These fields must be filled from a clean worktree after the tracked Task 11 preparation commit exists. If any tracked file changes after the freeze revision is recorded, the freeze is invalid and the verification sequence must restart from the new commit.

| Field | Value |
|---|---|
| `premigration_revision` | Pending clean Task 11 preparation commit |
| Release tag | Pending release authorization |
| Baseline installer | Pending explicit `RELAY_BASELINE_INSTALLER` |
| Candidate installer | Pending signed candidate bundle |
| Baseline version | Pending explicit `RELAY_BASELINE_VERSION` |
| Candidate version | Pending explicit `RELAY_CANDIDATE_VERSION` |
| Install/upgrade report | Pending explicit `RELAY_UPGRADE_REPORT` under ignored release qualification output |
| Published channel | Pending release authorization |

## Required local contracts before release authorization

- Fresh schema, released schema, and the existing five legacy routing policy fixtures must remain bootable.
- Readiness checks must be read-only and must not alter legacy route selection.
- Confirmed hierarchical migration config must be saved completely, but the production selector must not consume it before the cutover tasks.
- Import/export must preserve legacy routing fields and mark fields that become ignored after cutover rather than dropping user data.
- Request-log queries must tolerate historical `request_logs.upstream_base_url` values being `NULL` or already redacted.
- New request-log writes must persist `request_logs.upstream_base_url` as `NULL`; station/key identity and safe endpoint classification are the retained routing evidence.
- Logs UI/query code must not reconstruct historical upstream URLs from the current Station URL.
- Configuration readiness statistics must only persist low-cardinality aggregates; they must not include entity IDs, model names, or URLs.
- The install/upgrade matrix script must require explicit baseline/candidate installer paths, explicit baseline/candidate versions, and an explicit output path. It must not contain versioned defaults or version-specific labels.

## Required release evidence

The following evidence is intentionally not tracked in the repository. It belongs in ignored local output or CI artifacts for the frozen revision.

```powershell
pnpm.cmd verify:fast
pnpm.cmd verify:full
node scripts/install-upgrade-matrix-contract.test.mjs
node scripts/local-routing-redaction.test.mjs
pnpm.cmd verify:release-version --require-tag
pnpm.cmd verify:release
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-install-upgrade-matrix.ps1 -OldInstaller $env:RELAY_BASELINE_INSTALLER -NewInstaller $env:RELAY_CANDIDATE_INSTALLER -OldVersion $env:RELAY_BASELINE_VERSION -NewVersion $env:RELAY_CANDIDATE_VERSION -OutputPath $env:RELAY_UPGRADE_REPORT
```

The install/upgrade matrix command is valid only when all five environment variables above have been explicitly set for the current release. Script defaults are not allowed for the installers, versions, or output path.

## Current release blocker

Task 11 cannot be marked complete until a signed pre-migration version is actually released to the supported update/download channel and at least one supported baseline-to-candidate install/upgrade verification has passed against that published artifact.

Without release authorization and signed installer artifacts, this task remains blocked after the repository-local preparation commit. Do not start Task 12 from this branch state by treating this document as release evidence.
