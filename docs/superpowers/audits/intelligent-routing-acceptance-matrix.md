# Intelligent Routing Acceptance Matrix

Status: planned. Each row receives a concrete command, commit, and artifact link before Task 20 may mark it complete.

| ID | Primary tasks | Required evidence |
|---:|---:|---|
| 1 | 12, 13 | eligibility/tier/factor/dispatch unit tests |
| 2 | 11, 14 | immutable policy and capacity guard tests |
| 3 | 7, 11 | factor evidence and contribution trace fixtures |
| 4 | 7, 13 | Unknown/exploration starvation qualification |
| 5 | 6, 7 | request/monitor observation parity tests |
| 6 | 6 | anonymous probe non-elevation test |
| 7 | 6, 12 | scoped observation and failure-domain tests |
| 8 | 5, 11 | cost comparability and unknown-cost tests |
| 9 | 13, 14 | capacity and near-optimal dispatch tests |
| 10 | 12, 15 | affinity and escape fixtures |
| 11 | 15 | latest-overlay replan tests |
| 12 | 10, 15 | policy-to-trace effective-config tests |
| 13 | 11, 15 | simulation/production planner parity |
| 14 | 13, 19 | deterministic replay artifacts |
| 15 | 11 | factor extension boundary gate |
| 16 | 3, 4, 17 | PlanningSnapshot-only planner gate |
| 17 | 5, 8, 16 | shared-projector consumer parity |
| 18 | 8, 17 | frontend truth removal gate |
| 19 | 6, 7 | source/equivalence-preserving quality tests |
| 20 | 4, 14 | durable/runtime identity and revision tests |
| 21 | 15 | late target revision-fence test |
| 22 | 9 | revision-notice drop/reorder test |
| 23 | 4, 15 | hot-path query budget and trace query tests |
| 24 | 1, 17 | manifest/no-permanent-allowlist gate |
| 25 | 5, 8 | source-ref/projector-version parity fixture |
| 26 | 17 | production source absence scan |
| 27 | 17, 18 | generated-binding and fixture absence scan |
| 28 | 20 | zero-open-entry deletion ledger |
| 29 | 1, 17 | dead-code and test-only-facade gate |
| 30 | 10 | policy migration classification tests |
| 31 | 10, 16 | unique routing-policy write port tests |
| 32 | 10, 18 | legacy parser source-allowlist gate |
| 33 | 15, 17 | opaque historical trace test |
| 34 | 15, 16, 17 | unique workspace identity/revision tests |
| 35 | 8, 9, 17 | service/store ownership gate |
| 36 | 4, 15 | no-secret snapshot and late-resolution test |
| 37 | 6 | CanonicalOutcome consumer parity |
| 38 | 14 | runtime restart ABA test |
| 39 | 10 | policy CAS stale-editor test |
| 40 | 10, 15 | draft simulation purity test |
| 41 | 8, 16 | IPC/domain type separation gate |
| 42 | 4, 10, 15 | required-port fail-closed tests |
| 43 | 6, 15 | execution-shell responsibility gate |
| 44 | 17 | V2/speculative/dead-code absence scan |
| 45 | 17 | wrapper ownership inventory |
| 46 | 4 | one ReadSession snapshot test |
| 47 | 15 | direct-ID/cursor trace query test |
| 48 | 4 | typed-row/no-timestamp-revision test |
| 49 | 1, 18 | legacy-positive-gate absence test |
| 50 | 3, 4 | distinct snapshot type test |
| 51 | 3, 11 | fixed-point posterior golden/property tests |
| 52 | 6, 18 | multi-axis health/no-writeback test |
| 53 | 6, 7 | idempotency/order/rebuild tests |
| 54 | 12, 15 | failure-domain fallback test |
| 55 | 13, 14 | shared admission registry concurrency test |
| 56 | 19 | fail-closed fault matrix |
| 57 | 14, 19 | monotonic-clock protection test |
| 58 | 19 | replay/distribution/concurrency/fault/performance report |
| 59 | 4, 14 | durable-vs-runtime overlay contract test |
| 60 | 14 | production capacity contract test |
| 61 | 14, 17 | runtime-owner/composition-root gate |
| 62 | 13 | Unknown exploration-lane test |
| 63 | 13 | seed derivation/commitment test |
| 64 | 7, 11 | minimum-sample posterior safety test |
| 65 | 11, 13 | complete profile deterministic-rank test |
| 66 | 13 | independent exploration starvation proof |
| 67 | 13, 15 | redacted seed commitment test |
| 68 | 6, 7 | probe equivalence-scope test |
| 69 | 1, 16, 18 | atomic gate/manifest cutover evidence |
| 70 | 5, 8 | station/key consumer read-model test |
| 71 | 8, 17 | server group identity/hash join test |
| 72 | 5, 6 | collector atomic fact/projection test |
| 73 | 6, 18 | independent status-axis schema test |
| 74 | 9 | query-purity/concurrent-dashboard test |
| 75 | 6, 7, 8 | one-append/multi-projector test |
| 76 | 9 | MutationReceipt/notice mapping test |
| 77 | 8, 17 | wrapper/normalizer/shared-container absence scan |
| 78 | 5, 8, 19 | cross-page verdict parity qualification |
