# Routing Operational Field Ownership

Status: Task 0 frozen ledger
Date: 2026-07-30

This ledger defines one intended semantic owner for each routing operational field family. It is not a schema design; later tasks may refine names, but must not create a second authoritative resolver.

| Field family | Current duplicated/unsafe sources | Target owner | Target consumers | Delete or adapt by |
|---|---|---|---|---|
| Station identity | station catalog, routing DTO, frontend rows | Station catalog / operational identity newtypes | collector, monitoring, routing read models | Task 2/3 |
| Station Key identity | `station_keys`, `RuntimeRoutingCandidate`, frontend Key Pool | Station Key identity newtypes | routing planner, monitoring target, UI detail | Task 2/3 |
| Endpoint reference | `upstream_base_url`, station endpoint revision, endpoint ping target | `EndpointRef` plus sanitized origin and revision | target resolver, monitoring transport, UI label | Task 3/17/25 |
| Credential availability | candidate `api_key`, `api_key_secret`, credential store | credential availability fact and late `ExecutionTargetResolver` | executor only after selected route | Task 3/17 |
| Group binding | group facts, pricing rules, candidate economics, frontend projection | Group/multiplier projector | planner, pricing, simulator, UI detail | Task 4/8/23 |
| Multiplier | scheduler multiplier module, pricing rule fallback, frontend group matcher | Group/multiplier projector with provenance/freshness | eligibility, CostFirst multiplier proxy, UI explanation | Task 4/7/12 |
| Pricing | `PricingService`, `RouteCandidateEconomics`, frontend `pricingFacts.ts` | PricingProjector and CostCalculator | route economics assessment, attempt settlement, pricing UI | Task 7/19/23 |
| Balance | balance snapshots, candidate economics, dashboard summaries | Balance projector | eligibility, route workspace, dashboard/key summaries | Task 4/9 |
| Capability | station_key_capabilities, future collector model inventory evidence, HTTP failures | Capability projector and scoped capability evidence writer | planner, monitoring, operational detail | Task 5/18/19 |
| Key health | station_key_health, proxy failure classifier, monitoring writeback | HealthTransitionService/Store | HealthProjector, UI status, routing eligibility | Task 6/18/19 |
| Endpoint health | endpoint ping target, monitor execution, request failure target | Endpoint health facts and HealthProjector | routing eligibility, monitoring status, station detail | Task 6/18/19 |
| Account health | provider account observations | AccountStateReducer or explicit evidence gap | UI evidence and future capacity constraints | Task 6/18 |
| Runtime metrics | scheduler `RuntimeMetricsRegistry` by key | scoped runtime metrics registry | planner runtime overlay, outlier, affinity escape | Task 14 |
| Capacity | scheduler capacity snapshot and simulated acquire | CapacityRegistry with RAII leases | selected route, attempt lifecycle, runtime gauges | Task 15/16 |
| Retry/wait | proxy retry loop, precommit budget | retry budget and bounded wait plan | planner/controller/execution | Task 15/16 |
| Affinity | scheduler affinity store and test-only bind facade | success-only affinity consumer | planner overlay and route trace | Task 14/19/20 |
| Decision trace | route explanations, request log rejected JSON | routing decision store | routing workspace, request decision timeline | Task 13/23 |
| Attempt outcome | request attempts, proxy finalization, health observation | AttemptOutcome and effect planner | journal, health/capability/cost/runtime feedback | Task 19/20 |
| Request outcome | request log finalization, response body delivery | RequestOutcome and lifecycle journal | request log, cost aggregate, affinity binding | Task 19/20 |
| Public failure code | `ProxyFailureCode`, routing string errors, `InternalProxyError` | sealed planning/execution failure taxonomy | HTTP mapping, UI messages, trace redaction | Task 18/22 |
| UI truth | frontend matchers and independent page state | backend read models plus runtime overlay | Routing Workspace, pricing/rate, channel status, request logs | Task 9/23/24 |

Rules:

- A page may format, sort, and filter read-model fields, but cannot re-derive authoritative group, pricing, capability, health, or route eligibility semantics.
- Monitoring and routing may share typed facts and narrow ports; neither may import the other's candidate/read DTO as its truth source.
- Raw collector JSON, credentials, full endpoint URLs, request headers, and prompt/response payloads are never route candidate fields.
- Historical request facts stay historical; current pricing or health must not rewrite old decisions.

Task 5 clarification:

- The current codebase does not yet have a dedicated collector model evidence writer; it has manual `station_key_capabilities` rows plus legacy runtime capability reads. Task 5 therefore freezes the canonical evidence/projector contract and provider-neutral adapter signals without inventing a second persistence owner.
- The scoped capability evidence writer is introduced when production request/monitoring outcomes are converted into typed `CapabilityEffect` in Task 18/19. New writer code must consume the Task 5 evidence contract instead of writing route allow booleans.
