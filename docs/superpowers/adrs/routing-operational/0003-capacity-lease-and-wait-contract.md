# ADR 0003: Capacity Lease And Wait Contract

Status: Accepted for Task 0
Date: 2026-07-30

## Context

Current scheduler capacity is explanatory: it attempts to acquire, releases immediately, and records `acquired_simulated`. The selected candidate can therefore be executed without owning capacity. Fallback and wait behavior are also mixed with static candidate order, making capacity miss, real attempt failure, and replan triggers indistinct.

## Decision

Introduce explicit RAII leases:

- `RequestLease`: downstream request admission and lifecycle writer capacity.
- `RetryPermit`: fallback ordinal budget beyond the first attempt.
- `HalfOpenPermit`: exactly one probe per runtime metric scope.
- `CapacityLease`: composite global/station/key/provider-scope admission.

Composite acquisition uses a fixed order and rollbacks all previously acquired constraints on any miss. A `SelectedRoute` cannot exist without a real `CapacityLease`. The lease transfers into attempt/protocol lifecycle and releases exactly once on success, failure, target resolve failure, timeout, cancel, panic unwind, or downstream drop after upstream terminal.

Capacity miss behavior:

- It marks `unavailable_this_pass` inside the current `RoutePlan`.
- It does not become request exclusion.
- It may enqueue a bounded waiter when request budget allows.
- Wait wake-up samples a new runtime overlay and may replan.

Actual attempt failure behavior:

- It appends a durable or pending attempt outcome.
- It adds actual-attempt exclusion to request progress.
- It may update runtime overlay immediately through the fixed outcome consumer.
- It can trigger fallback replan within remaining request budget.

## Consequences

- All queues, waiters, retry budgets, leases, body budgets, and registries require hard limits and shutdown/drain behavior.
- Tests must prove gauge-to-zero for normal terminal, precommit failures, target resolver failures, stream drop, slow downstream, panic unwind, and cancellation.
- Provider account concurrency is enabled only when scope and freshness are trustworthy; otherwise it is an evidence gap, not guessed capacity.
