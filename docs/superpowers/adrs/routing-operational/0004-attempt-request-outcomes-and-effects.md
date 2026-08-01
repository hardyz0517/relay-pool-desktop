# ADR 0004: Attempt And Request Outcomes

Status: Accepted for Task 0
Date: 2026-07-30

## Context

The current request lifecycle has useful foundations: request admission, attempt reservations, response-body finalization leases, writer fail-closed behavior, and idempotent persistence. Production consumers are incomplete. Attempt terminal mainly drives journal and Key health; pricing settlement, decision trace, runtime feedback, capability evidence, endpoint/account health, and success-only affinity do not share one fixed ordering contract.

A generic event bus or distributed outbox is not appropriate for the local desktop product at this stage.

## Decision

Use fixed outcome types and fixed consumers:

- `AttemptOutcome`: selected target, attempt ordinal, protocol terminal, failure taxonomy, scoped effect targets, frozen pricing context, observed usage, latency, and sanitized diagnostics.
- `RequestOutcome`: downstream delivery terminal, aggregate status, attempt count, fallback count, multi-currency cost aggregate, trace completeness, and success contract.

`RequestFinalizationService` becomes a module with an effect planner and explicit consumer order. The writer keeps bounded permits and fail-closed admission.

Ordering:

1. Request admission reserves request lifecycle capacity before upstream work.
2. Attempt start/finish writer permit is reserved before sending upstream when the endpoint requires an attempt journal.
3. Attempt terminal commits journal, scoped health/capability effects, per-attempt cost, and decision outcome atomically where possible.
4. Runtime feedback happens once after durable attempt ack or records a result-unknown state if the writer fails before ack.
5. Request terminal waits for all started AttemptOutcome acks that can still arrive.
6. Request cost aggregate and success-only affinity bind only after RequestOutcome durable success.

Crash gap:

- If the process dies after observing upstream/delivery but before durable commit, startup reconciliation marks the request `interrupted` or `trace_incomplete`.
- It must not synthesize missing attempts, usage, cost, or success.
- If field data later proves this gap unacceptable, a separate ADR may evaluate a lightweight local WAL. This upgrade does not introduce Redis, distributed outbox, or generic event sourcing.

## Consequences

- Failure classification must produce typed `FailureTarget`, `FailureClass`, and effect plans.
- Public HTTP/UI errors come from one exhaustive mapping.
- Historical legacy rows may project as unavailable/trace-incomplete, not as invented compatibility details.
