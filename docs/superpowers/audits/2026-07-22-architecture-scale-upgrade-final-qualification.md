# 2026-07-22 Architecture Scale Upgrade Final Qualification

Date: 2026-07-28

## Scope

- Stage 7 Task 28 release/locked build, artifact and final snapshot qualification.
- Release candidate version: `v0.3.3`.
- `v0.3.3` is used because `v0.3.2` already points to the earlier released commit `db51a12b7b783661fd946952600a7a78595ddb0f`.
- Source revision under test: `f74326b5e9ebfe808a8a534feb4c1aa262458ed8`.
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
  - started at: `2026-07-28T04:13:21.6427226Z`
  - finished at: `2026-07-28T04:21:20.7040191Z`
  - duration: 479.06s
  - revision: `f74326b5e9ebfe808a8a534feb4c1aa262458ed8`
  - raw evidence:
    `output/architecture-scale/qualification/release/release-prebundle-v0.3.3-complete-2026-07-28.txt`
  - summary:
    `output/architecture-scale/qualification/release/release-v0.3.3-2026-07-28-summary.json`
  - verified source version: `v0.3.3`
  - covered all deterministic `full` profile gates plus release version contract and locked Rust release build
  - artifact:
    `src-tauri/target/x86_64-pc-windows-msvc/release/relay-pool-desktop.exe`
  - bytes: 36344320
  - sha256: `b21b6eb242a9807df7da2887739eff52f1f69758a3f62b17ec44f6bd8d5c78a2`
- Version/tag metadata after preparing the next release candidate:
  `RELAY_POOL_RELEASE_TAG=v0.3.3 pnpm verify:release-version --require-tag`
  - result: exit code 0
  - verified package/Cargo/Tauri version contract for `v0.3.3`

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
  - started at: `2026-07-28T04:21:32.0250869Z`
  - finished at: `2026-07-28T04:25:20.8904961Z`
  - duration: 228.87s
  - root cause: `TAURI_SIGNING_PRIVATE_KEY is required for release bundling`
  - raw evidence:
    `output/architecture-scale/qualification/release/release-all-v0.3.3-signing-blocker-2026-07-28.txt`
- Current `HEAD` is not exactly tagged.
- Existing `v0.3.2` tag points to `db51a12b7b783661fd946952600a7a78595ddb0f`; it was not moved.
- A current exact `v0.3.3` release tag still needs to be created on the final release snapshot before rerunning release qualification.
- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are not present in the environment.
- No Tauri bundle directory exists for this revision:
  `src-tauri/target/x86_64-pc-windows-msvc/release/bundle`
- Signed Tauri bundle, final bundle scan, fresh install, supported upgrade, update relaunch, offline startup, old asset/new binary mismatch, single-instance launch and tray/exit matrix were not run.
- Task 27 authenticated live provider qualification is blocked, so Task 28 cannot claim Stage 7 release readiness.

## Result

Task 28 is blocked. The release prebundle gate passed for revision `f74326b5e9ebfe808a8a534feb4c1aa262458ed8`, and the candidate version metadata verifies as `v0.3.3`. The release/locked build gate is not complete without a current exact `v0.3.3` release tag, signing key, signed Tauri bundle, bundle scan, install/upgrade matrix and live provider qualification. Stage 7 Gate does not pass.
