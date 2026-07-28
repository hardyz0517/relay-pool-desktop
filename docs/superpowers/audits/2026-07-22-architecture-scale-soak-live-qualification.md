# 2026-07-22 Architecture Scale Soak And Live Qualification

Date: 2026-07-28

## Scope

- Stage 7 Task 27 soak/live qualification for the architecture scale upgrade.
- Source revision under test: `eb1fbea419afffe0c0b0c664bad98ffd2509d579`.
- Worktree: `D:\Dev\Projects\relay-pool-desktop-architecture-scale-upgrade`.
- Branch: `codex/architecture-scale-upgrade`.
- No desktop app launch, screenshot, or direct visual desktop inspection was used.
- Persistence V2 protected source and migrations were not modified.

## Passed Evidence

- Command:
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run-proxy-lifecycle-soak.ps1 -DurationMinutes 60`
- Result: exit code 0.
- Raw evidence:
  `output/architecture-scale/qualification/soak/proxy-lifecycle-soak-2026-07-28.txt`
- Summary:
  `output/architecture-scale/qualification/soak/proxy-lifecycle-soak-2026-07-28-summary.json`
- Proxy lifecycle soak result:
  - passes: 1492
  - samples: 1492
  - p95: 2694.56 ms
  - min: 2248.1176 ms
  - max: 5040.8589 ms
  - final pass assertion: `services::proxy::soak_tests::v2_soak_returns_all_resource_counters_to_zero`

## Fixture Coverage Already Included In Task 26 Full Verification

- Provider conformance fixtures are covered by the Task 26 full verification run.
- The provider capability matrix declares redacted fixture coverage for Sub2API, NewAPI and OpenAI-compatible capabilities.
- This fixture coverage is deterministic evidence only. It is not authenticated live provider qualification.

## Live Qualification Blockers

- No authenticated live provider endpoint or credential was available from the local environment.
- No local environment variables matching Relay Pool live provider qualification were set.
- No repository-owned live provider harness was found that can run authenticated Sub2API/NewAPI/OpenAI-compatible live qualification without externally supplied endpoint credentials.
- Because live provider qualification is unavailable, Task 27 does not pass its release gate.

## Result

Task 27 is partially complete. The 60-minute deterministic proxy lifecycle soak passed on revision `eb1fbea419afffe0c0b0c664bad98ffd2509d579`, but authenticated live provider qualification remains blocked. Release readiness is not claimed.
