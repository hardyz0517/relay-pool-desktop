# 2026-07-22 Architecture Scale Upgrade Final Qualification

Date: 2026-07-29

## Scope

- Stage 7 Task 28 release/locked build, artifact, install/upgrade and final snapshot qualification.
- Release candidate version: `v0.3.3`.
- Exact release candidate revision: `5f8467ffcf1bea2fbc2436710d9141b267ea2a20`.
- Local tag `v0.3.3` points to that revision and has not been pushed.
- Rollback revision: released `v0.3.2` at `db51a12b7b783661fd946952600a7a78595ddb0f`.
- Worktree: isolated local `codex/architecture-scale-upgrade` checkout.
- No screenshot or direct visual desktop inspection was used.
- Persistence V2 protected source and migrations were not modified.

## Prior Qualification Inputs

- Task 26 deterministic qualification:
  `docs/archive/audits/2026-07-22-architecture-scale-deterministic-qualification.md`
- Task 27 soak/live qualification:
  `docs/archive/audits/2026-07-22-architecture-scale-soak-live-qualification.md`
- The release profile below reran the full deterministic profile on the final candidate. The Task 27 live probe was also rerun on the final candidate. A final-candidate lifecycle soak rerun is recorded below before merge.

## Signed Release Gate

- Command:
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify.ps1 -Profile release`
- Result: exit code 0.
- Started: `2026-07-29T03:52:58.8754788Z`.
- Finished: `2026-07-29T04:07:57.6019284Z`.
- Duration: 898.73s.
- Revision: `5f8467ffcf1bea2fbc2436710d9141b267ea2a20`.
- Raw evidence:
  `output/architecture-scale/qualification/release/release-all-v0.3.3-signed-5f8467f-2026-07-29.txt`
- Provenance:
  `output/architecture-scale/qualification/release/provenance.json`
- Passed the shared deterministic release gates, release version contract, locked Rust release build, signed Tauri bundle and final release bundle scan.
- The same run included updater drain-aware preparation contracts, updater timeout recovery, production runtime-contract fail-closed tests, Tauri security checks and final bundle scanning.

## Artifacts

- Release executable:
  `src-tauri/target/x86_64-pc-windows-msvc/release/relay-pool-desktop.exe`
  - bytes: 36603392
  - sha256: `0f767ba17f8796b141a43d155ac00480067f4508823f3bc9ec4ad9743ef19cf1`
- Signed NSIS installer:
  `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/Relay Pool Desktop_0.3.3_x64-setup.exe`
  - bytes: 8357567
  - sha256: `b1a6ab739b59f3388746a574a1356fe00759fc8f2a8049eb3218aa670f5c3c96`
- Updater signature:
  `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/Relay Pool Desktop_0.3.3_x64-setup.exe.sig`
  - bytes: 432
  - sha256: `e7e87bc1fc0db9151ff27cd2d9634ad9c070eafa0693cc56353188b38fd80378`

## Install And Upgrade Matrix

- Harness:
  `scripts/run-install-upgrade-matrix.ps1`
- Summary:
  `output/architecture-scale/qualification/release/install-upgrade-matrix-v0.3.3-2026-07-29-summary.json`
- Overall result: `pass`.
- Passed steps:
  - preserve existing Local/Roaming application data
  - remove prior install or orphan state
  - fresh install signed `v0.3.3`
  - fresh startup stability, no established startup TCP connections, single-instance second launch and close-to-tray probe
  - uninstall fresh `v0.3.3`
  - install and start supported baseline `v0.3.2`
  - upgrade `v0.3.2 -> v0.3.3`
  - post-upgrade startup, single-instance second launch and close-to-tray probe
  - restore preserved application data
- The final installed snapshot reports registry/file/product version `0.3.3` and the expected signed installer hash.
- The harness used process, registry, version-resource, hash and TCP evidence only. It did not capture screenshots or inspect the desktop visually.

## Final-Candidate Live And Soak Evidence

- Authenticated OpenAI-compatible live provider probe:
  `output/architecture-scale/qualification/live-provider/station-key-connectivity-live-probe-5f8467f-2026-07-29-summary.json`
  - revision: `5f8467ffcf1bea2fbc2436710d9141b267ea2a20`
  - protocol: `responses`
  - response mode: `stream`
  - status: 200
  - success: true
  - credential: recorded only as `redacted`
- 60-minute proxy lifecycle soak:
  `output/architecture-scale/qualification/soak/proxy-lifecycle-soak-5f8467f-2026-07-29-summary.json`
  - revision: `5f8467ffcf1bea2fbc2436710d9141b267ea2a20`
  - passes/samples: 1373
  - p95: 2853.27 ms
  - failures: 0
  - final resource-counter assertion: pass

## Qualification Boundaries

- Offline startup was checked by ten-second launch stability and an empty established-TCP snapshot; the host network adapter was not disabled.
- Closing the main window was verified to return successfully while the process remained alive, matching close-to-tray behavior. The harness then terminated only the exact test executable processes to isolate the next matrix step.
- The tray menu was not clicked through the desktop shell. Tray Quit routing through `ExitCoordinator`, updater preparation/relaunch ownership and stale runtime-contract mismatch fail-closed behavior are covered by the same-revision release test suite rather than screenshot-driven desktop interaction.
- The supported upgrade used the signed NSIS installer directly. It validates installed-package replacement and post-upgrade startup; it does not depend on a published updater manifest for this unpushed local tag.

## Superseded Evidence

- The earlier signing-key blocker and release runs on `f1dc300` are superseded by the successful signed release gate on `5f8467f`.
- Earlier Task 26/27 reports retain their original evidence history. The final release profile, live probe and lifecycle soak above are the same-revision requalification used by this Gate.

## Result

Stage 7 Gate passes for the authorized non-visual qualification scope on exact local candidate tag `v0.3.3` at revision `5f8467ffcf1bea2fbc2436710d9141b267ea2a20`. The same revision passed the full deterministic/release profile, signed bundle and bundle scan, authenticated streaming live-provider probe, 60-minute lifecycle soak, fresh install, supported `v0.3.2 -> v0.3.3` upgrade, startup, single-instance and close-to-tray process probes. The tray menu itself was not clicked and the unpushed local tag could not exercise a published updater manifest; those boundaries are explicitly covered by same-revision automated contracts and are not represented as direct desktop interaction. The branch is qualified for local merge; no remote push or release publication is authorized by this report.
