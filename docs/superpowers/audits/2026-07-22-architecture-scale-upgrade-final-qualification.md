# 2026-07-22 Architecture Scale Upgrade Final Qualification

Date: 2026-07-29

## Scope

- Stage 7 Task 28 release/locked build, artifact and final snapshot qualification.
- Release candidate version: `v0.3.3`.
- `v0.3.3` is used because `v0.3.2` already points to the earlier released commit `db51a12b7b783661fd946952600a7a78595ddb0f`.
- Final release source revision under test: `f1dc30009f543e76a50134331b35cecf10d42280`.
- Latest live provider source revision under test: `4217aa9420e4e5e6c0221d5f7038392c199fcf33`.
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
- Full release shared entrypoint on exact local tag `v0.3.3`:
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify.ps1 -Profile release`
  - result: exit code 0
  - started at: `2026-07-29T02:28:58.1684357Z`
  - finished at: `2026-07-29T02:37:29.7464155Z`
  - duration: 511.58s
  - revision: `f1dc30009f543e76a50134331b35cecf10d42280`
  - raw evidence:
    `output/architecture-scale/qualification/release/release-all-v0.3.3-signed-f1dc300-2026-07-29.txt`
  - provenance:
    `output/architecture-scale/qualification/release/provenance.json`
  - covered all deterministic release gates, release version contract, locked Rust release build, signed Tauri bundle and final release bundle scan
  - release executable:
    `src-tauri/target/x86_64-pc-windows-msvc/release/relay-pool-desktop.exe`
  - executable bytes: 36454912
  - executable sha256: `69dfb46cef5ebd192299e6543d3754b832d6670704cbb277a2b56d418448e902`
  - signed NSIS bundle:
    `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/Relay Pool Desktop_0.3.3_x64-setup.exe`
  - signed NSIS bundle bytes: 8287015
  - signed NSIS bundle sha256: `7ee8f67eacd96797986075c6c46c00ec52d7accda7f48acb2a7480ba8659ae8b`
  - updater signature:
    `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/Relay Pool Desktop_0.3.3_x64-setup.exe.sig`
  - updater signature bytes: 432
  - updater signature sha256: `3f0958b2dd8605f66b668b3a4bee6d4dc9791d353d6475eccc9d4ed8c026fea6`
- Authenticated OpenAI-compatible live provider probe:
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run-openai-compatible-live-qualification.ps1 -BaseUrl <approved endpoint> -Model codex-auto-review -OutputPath output\architecture-scale\qualification\live-provider\station-key-connectivity-live-probe-4217aa9-2026-07-28-summary.json`
  - result: exit code 0
  - revision: `4217aa9420e4e5e6c0221d5f7038392c199fcf33`
  - final protocol: `responses`
  - final response mode: `stream`
  - final status: 200
  - final success: true
  - summary:
    `output/architecture-scale/qualification/live-provider/station-key-connectivity-live-probe-4217aa9-2026-07-28-summary.json`

## Entrypoint Repair

- `scripts\verify.ps1` originally invoked the release version script as `pnpm verify:release-version -- --require-tag`.
- On this pnpm/Windows path, the literal `--` was forwarded to `scripts/verify-release-version.mjs`, causing `unknown argument: --`.
- The entrypoint now invokes `pnpm verify:release-version --require-tag`.
- `scripts\verify.ps1` also originally invoked the Tauri build script as `pnpm tauri:build -- --target x86_64-pc-windows-msvc`.
- On this pnpm/Windows path, the literal `--` was forwarded to Tauri, causing the target release binary path to resolve incorrectly.
- The entrypoint now invokes `pnpm tauri:build --target x86_64-pc-windows-msvc`.
- The project updater key was generated as a passwordless Tauri key stored outside the repository at `C:\Users\cpp_s\.tauri\relay-pool-desktop.key`; `verify.ps1` now supports loading that key through `TAURI_SIGNING_PRIVATE_KEY_PATH` and keeps the private key out of command-line arguments and tracked files.
- Focused verification passed:
  - `pnpm verify:release-version --require-tag` with `RELAY_POOL_RELEASE_TAG=v0.3.2`
  - `node scripts\release-verification-entrypoint.test.mjs`

## Superseded Blocking Evidence

- Full release shared entrypoint:
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify.ps1 -Profile release`
  - result: failed after passing deterministic/release prebundle steps
  - started at: `2026-07-28T04:21:32.0250869Z`
  - finished at: `2026-07-28T04:25:20.8904961Z`
  - duration: 228.87s
  - root cause: `TAURI_SIGNING_PRIVATE_KEY is required for release bundling`
  - raw evidence:
    `output/architecture-scale/qualification/release/release-all-v0.3.3-signing-blocker-2026-07-28.txt`
- This blocker is superseded by the successful signed release run on `2026-07-29`.

## Remaining Manual Matrix

- Existing `v0.3.2` tag points to `db51a12b7b783661fd946952600a7a78595ddb0f`; it was not moved.
- A local exact `v0.3.3` tag now points to `f1dc30009f543e76a50134331b35cecf10d42280`; it has not been pushed.
- Signed Tauri bundle and final bundle scan passed.
- Fresh install, supported upgrade, update relaunch, offline startup, old asset/new binary mismatch, single-instance launch and tray/exit matrix were not run in this pass.
- No desktop app launch, screenshot, or direct visual desktop inspection was used.

## Result

Task 28 passes for deterministic release build, signing, bundle scan and artifact provenance on exact local tag `v0.3.3` at revision `f1dc30009f543e76a50134331b35cecf10d42280`. Authenticated OpenAI-compatible live provider qualification passed on revision `4217aa9420e4e5e6c0221d5f7038392c199fcf33`. The remaining unverified release surface is the destructive/manual install, upgrade, relaunch, offline startup, old asset/new binary mismatch, single-instance launch and tray/exit matrix, which was not run because this pass avoided launching or visually inspecting the desktop app.
