# 2026-07-22 Architecture Scale Soak And Live Qualification

Date: 2026-07-28

## Scope

- Stage 7 Task 27 soak/live qualification for the architecture scale upgrade.
- Soak source revision under test: `eb1fbea419afffe0c0b0c664bad98ffd2509d579`.
- Live provider source revision under test: `4217aa9420e4e5e6c0221d5f7038392c199fcf33`.
- Final release candidate requalification revision: `5f8467ffcf1bea2fbc2436710d9141b267ea2a20`.
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
- Authenticated OpenAI-compatible live provider probe:
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\run-openai-compatible-live-qualification.ps1 -BaseUrl <approved endpoint> -Model codex-auto-review -OutputPath output\architecture-scale\qualification\live-provider\station-key-connectivity-live-probe-4217aa9-2026-07-28-summary.json`
  - result: exit code 0
  - raw/sanitized summary:
    `output/architecture-scale/qualification/live-provider/station-key-connectivity-live-probe-4217aa9-2026-07-28-summary.json`
  - endpoint role: approved temporary OpenAI-compatible live provider
  - auth: bearer credential supplied out-of-band and recorded only as `redacted`
  - models endpoint: `/v1/models`, success true, HTTP 200 equivalent, 7 model IDs discovered
  - selected model: `codex-auto-review`
  - probe path: product-shaped station key connectivity probe
  - final protocol: `responses`
  - final response mode: `stream`
  - final status: 200
  - final success: true
  - secret scan over the summary found no key, account, password, authorization header, or bearer token

## Fixture Coverage Already Included In Task 26 Full Verification

- Provider conformance fixtures are covered by the Task 26 full verification run.
- The provider capability matrix declares redacted fixture coverage for Sub2API, NewAPI and OpenAI-compatible capabilities.
- This fixture coverage is deterministic evidence only. It is not authenticated live provider qualification.

## Live Qualification Notes

- A repository-owned live provider harness now exists at:
  `scripts/run-openai-compatible-live-qualification.ps1`
- The harness requires `RELAY_POOL_LIVE_API_KEY` and writes only a redacted JSON summary.
- The first manual smoke proved `/v1/models` and `/v1/chat/completions` non-stream availability, but the final qualification evidence above is the cleaner product-shaped harness run on revision `4217aa9420e4e5e6c0221d5f7038392c199fcf33`.
- The supplied account/password were not used; only the temporary bearer key was used.

## Final Candidate Requalification

- The authenticated OpenAI-compatible probe was rerun on revision `5f8467ffcf1bea2fbc2436710d9141b267ea2a20`.
- Summary:
  `output/architecture-scale/qualification/live-provider/station-key-connectivity-live-probe-5f8467f-2026-07-29-summary.json`
- Result: `responses` protocol, streaming response mode, HTTP 200, success true.
- The summary records the bearer credential only as `redacted`; account/password were not used.
- The 60-minute proxy lifecycle soak was rerun on the same revision.
- Raw evidence:
  `output/architecture-scale/qualification/soak/proxy-lifecycle-soak-5f8467f-2026-07-29.txt`
- Summary:
  `output/architecture-scale/qualification/soak/proxy-lifecycle-soak-5f8467f-2026-07-29-summary.json`
- Result: 1373 passes/samples, p95 2853.27 ms, zero failures.
- Final assertion:
  `services::proxy::soak_tests::v2_soak_returns_all_resource_counters_to_zero`

## Result

Task 27 passes on final candidate revision `5f8467ffcf1bea2fbc2436710d9141b267ea2a20`. The authenticated OpenAI-compatible live probe passed with a streaming Responses API result, and the 60-minute deterministic proxy lifecycle soak completed 1373 successful samples with p95 2853.27 ms and the final resource counters returned to zero. The earlier `eb1fbea`/`4217aa9` evidence remains as history; the final Gate uses this same-revision requalification.
