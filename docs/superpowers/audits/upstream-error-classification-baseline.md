# Upstream Error Classification Baseline

Status: Task 0 complete; RED evidence captured, production cutover not started by this task

Captured: 2026-08-12

Workspace: `E:\Dev\Projects\relay-pool-desktop`

Start branch: `master...origin/master [ahead 8]`

Start HEAD: `5ae9abad` (`docs: record routing plan audit blocker`)

This audit is the start-of-work evidence for
[`2026-08-12-upstream-error-classification-retry-upgrade.md`](../plans/2026-08-12-upstream-error-classification-retry-upgrade.md).
It describes current behavior, not the desired architecture. Task 0 intentionally
does not implement a GREEN path.

## Worktree preservation record

At capture time `git status --short` was empty. There were no uncommitted user
files to edit around. The five newest commits were:

```text
5ae9abad docs: record routing plan audit blocker
26483776 fix: keep log pagination controls visible
46aa0b8a feat: expand pricing and alerting activity contracts
8af73f71 docs: plan upstream error retry upgrade
779e5db6 feat: align pricing and station management contracts
```

Task 0 only adds audit/test artifacts and updates the intelligent-routing
ledger/manifest. It does not change production proxy code.

## In-scope ingress and external-response boundary

The public inference surface confirmed in
`src-tauri/src/services/proxy/ingress.rs` is:

| Public endpoint | Method | Upstream response forms in scope |
|---|---|---|
| `/v1/models` | `GET` | buffered HTTP JSON/non-JSON error |
| `/v1/chat/completions` | `POST` | buffered HTTP and OpenAI-compatible SSE |
| `/v1/responses` | `POST` | buffered HTTP, Responses SSE, and chat fallback |
| `/v1/embeddings` | `POST` | buffered HTTP JSON/non-JSON error |

`/usage` and `/v1/usage` are local reads and do not consume a provider error
envelope. Images, Videos, Voice, Anthropic `/v1/messages`, Gemini-native routes,
Realtime and WebSocket cases from the Sub2API research table are explicitly
excluded from the first production cutover. Authentication, scheduling,
billing, upstream and protocol failures from Sub2API remain in scope when they
can be returned while serving one of the four endpoints above.

## Current production call graph

```text
Hyper ingress
  -> ProxyExecutionEngine
  -> V2RoutingRepository / RouteAdmissionCoordinator
  -> resolve execution target + acquire capacity lease
  -> ReqwestUpstreamClient.execute
       -> non-success: response.bytes() (unbounded)
       -> build ProxyFailure from status/body
  -> openai_error_semantic_signal / responses_error_semantic_signal
  -> failure_from_provider_signal -> CanonicalOutcome
  -> ProxyFailure::from_public_error (drops target/retry/health/capability)
  -> RetryPolicy::decide(status/code)
  -> classified_attempt_failure
       -> attempt_failure_kind(ProxyFailure)
       -> health_effect(ProxyFailure/status)
  -> lifecycle/finalization writer for current station key
  -> ProxyFailure::into_response (`relay_pool_error` public envelope)

Streaming branch
  -> bootstrap_stream
  -> first non-empty TCP chunk is accepted as successful bootstrap
  -> response body wrapper forwards/parses later stream events
```

The current flow therefore has a canonical classifier, but its result is not the
single execution fact. Public projection becomes an intermediate control-plane
input and multiple consumers independently reconstruct semantics.

## Confirmed RED behavior matrix

| ID | Current evidence | Current behavior | Required GREEN owner |
|---|---|---|---|
| RED-01 | `adapters/openai.rs`: `400 | 409 | 422 => BadRequest` | HTTP 400 capacity is a stopped request | versioned provider rule set + canonical classifier |
| RED-02 | `execution.rs::bootstrap_stream` | any first non-empty byte chunk, including `event:error`, commits bootstrap | protocol event decoder + downstream commit tracker |
| RED-03 | `ProxyFailure::from_public_error(canonical.public)` | canonical target/retry/health/capability are discarded | lossless effect plan consumed by execution |
| RED-04 | `adapters/openai.rs`: every 429 is `RateLimited` | quota, usage-window, concurrency, queue-full and ordinary rate limit share one effect | provider rules + typed canonical class |
| RED-05 | every HTTP 401 is `ConfirmedAuthentication` | `USER_INACTIVE` is attributed to the current credential | station-account scoped rule/effect |
| RED-06 | `ProxyFailure::into_response` | capacity/overload errors expose `relay_pool_error` and internal codes | OpenAI-compatible public adapter |
| RED-07 | successful HTTP status bypasses provider error classification | a `2xx` error envelope can enter the success path | bounded envelope parser before success finalization |
| RED-08 | status switches have no closed cases for 3xx/407/413/499/conflicts | outcomes fall through or are inferred inconsistently | canonical closed fallback matrix |
| RED-09 | `upstream.rs`: `response.bytes().await` | non-success response bodies have no decompressed-size bound | shared bounded diagnostic-memory reader |
| RED-10 | retry is expressed as next-candidate status logic | no same-target capacity retry/domain suppression contract exists | retry controller + provider-capacity domain |
| RED-11 | `health_effect` and finalization bind failure to current attempt/key | scoped effects can collapse to Station Key writeback | scoped verdict projector/read model |
| RED-12 | retry wait/request-body ownership is not an explicit invariant | lease release and immutable body reuse are not contractually proven | attempt resource lifecycle + fault tests |

The isolated long-term contract gate is `node scripts/upstream-error-contract.test.mjs`.
It is expected to exit non-zero until the production cutover. Catalog-only
validation is independently green:

```powershell
node scripts/upstream-error-contract.test.mjs --catalog-only
node scripts/upstream-error-contract.test.mjs
```

## Fixture catalog

The machine-readable catalog is
[`src-tauri/tests/fixtures/upstream_errors/catalog.v1.json`](../../../src-tauri/tests/fixtures/upstream_errors/catalog.v1.json).
It contains only cases reachable through the four public endpoints and records:

- transport (`http` or `sse`) and compatible endpoint set;
- status, `type`, `code`, message/signature and envelope shape;
- expected semantic family, scope, retry disposition and evidence confidence;
- dynamic/missing-field variants;
- Sub2API and a second generic OpenAI-compatible gateway profile.

The catalog is classification input, not executable provider code. Free-form
message text is fixture evidence and must never become a durable key, metric
label or scope identity. Adding a provider code after cutover is permitted only
through a versioned rule set plus catalog/conformance fixture; execution,
health and public consumers must remain provider-code agnostic.

## Deletion and boundary contract

The rows added to
[`intelligent-routing-deletion-ledger.md`](intelligent-routing-deletion-ledger.md)
are open cutover obligations. The matching
`upstream_error_classification_cutover` section in
[`intelligent-routing-boundary-manifest.json`](intelligent-routing-boundary-manifest.json)
names the one intended owner chain and forbidden duplicate owners. Task 9 may
close those entries only after production references are zero and architecture
gates prove the new composition.

## Task 0 exit result

- public endpoint scope is frozen without Images/Videos/Anthropic/Gemini/WS;
- current behavior and call graph are recorded;
- the fixture catalog covers every required family and both HTTP/SSE envelopes;
- a second gateway profile and missing/dynamic/conflicting cases are present;
- deletion owners and boundary obligations are machine-readable;
- RED evidence is isolated and reproducible;
- no GREEN production behavior was implemented.
