# 2026-07-22 Architecture Scale Upgrade Final Qualification

Date: 2026-07-28

## Scope

- Stage 7 Task 28 release/locked build, artifact and final snapshot qualification.
- Source revision under test: `115f6e1c8ee737d92eebec6efe0ee983bac82e99`.
- Worktree: `D:\Dev\Projects\relay-pool-desktop-architecture-scale-upgrade`.
- Branch: `codex/architecture-scale-upgrade`.
- No desktop app launch, screenshot, or direct visual desktop inspection was used.
- Persistence V2 protected source and migrations were not modified.

## Prior Qualification Inputs

- Task 26 deterministic qualification:
  `docs/superpowers/audits/2026-07-22-architecture-scale-deterministic-qualification.md`
- Task 27 soak/live qualification:
  `docs/superpowers/audits/2026-07-22-architecture-scale-soak-live-qualification.md`

## Passed Evidence

- Release prebundle shared entrypoint:
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify.ps1 -Profile release -ReleasePhase prebundle`
  - result: exit code 0
  - started at: `2026-07-28T03:17:30.7063183Z`
  - finished at: `2026-07-28T03:21:25.8667277Z`
  - revision: `115f6e1c8ee737d92eebec6efe0ee983bac82e99`
  - raw evidence:
    `output/architecture-scale/qualification/release/release-prebundle-clean-2026-07-28.txt`
  - summary:
    `output/architecture-scale/qualification/release/release-prebundle-clean-2026-07-28-summary.json`
  - verified source version: `v0.3.2`
  - covered all deterministic `full` profile gates plus release version contract and locked Rust release build
  - artifact:
    `src-tauri/target/x86_64-pc-windows-msvc/release/relay-pool-desktop.exe`
  - bytes: 36344320
  - sha256: `31d4a90462d967b776da6b60c8341e3cce6c9c6c56bc55a87cb5a62ddb4a33e6`

## Entrypoint Repair

- `scripts\verify.ps1` originally invoked the release version script as `pnpm verify:release-version -- --require-tag`.
- On this pnpm/Windows path, the literal `--` was forwarded to `scripts/verify-release-version.mjs`, causing `unknown argument: --`.
- The entrypoint now invokes `pnpm verify:release-version --require-tag`.
- Focused verification passed:
  - `pnpm verify:release-version --require-tag` with `RELAY_POOL_RELEASE_TAG=v0.3.2`
  - `node scripts\release-verification-entrypoint.test.mjs`

## Blocking Evidence

- Full release shared entrypoint:
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify.ps1 -Profile release`
  - result: failed after passing deterministic/release prebundle steps
  - started at: `2026-07-28T03:21:33.2598993Z`
  - finished at: `2026-07-28T03:25:25.9591361Z`
  - root cause: `TAURI_SIGNING_PRIVATE_KEY is required for release bundling`
  - raw evidence:
    `output/architecture-scale/qualification/release/release-all-signing-blocker-clean-2026-07-28.txt`
- Current `HEAD` is not exactly tagged.
- Existing `v0.3.2` tag points to `db51a12b7b783661fd946952600a7a78595ddb0f`, not `115f6e1c8ee737d92eebec6efe0ee983bac82e99`.
- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are not present in the environment.
- No Tauri bundle directory exists for this revision:
  `src-tauri/target/x86_64-pc-windows-msvc/release/bundle`
- Signed Tauri bundle, final bundle scan, fresh install, supported upgrade, update relaunch, offline startup, old asset/new binary mismatch, single-instance launch and tray/exit matrix were not run.
- Task 27 authenticated live provider qualification is blocked, so Task 28 cannot claim Stage 7 release readiness.

## Result

Task 28 is blocked. The release prebundle gate passed for revision `115f6e1c8ee737d92eebec6efe0ee983bac82e99`, but the release/locked build gate is not complete without a current exact release tag, signing key, signed Tauri bundle, bundle scan, install/upgrade matrix and live provider qualification. Stage 7 Gate does not pass.
