# ADR 0002: Snapshot-Consistent Operational Facts

Status: Accepted for Task 0
Date: 2026-07-30

## Context

Routing currently loads runtime candidates through service methods that flatten station, endpoint, key, health, balance, capability, proxy, and credential data into `RuntimeRoutingCandidate`. Monitoring also uses that DTO. Pricing and frontend projections perform separate matching. This makes it possible for one request or one UI view to combine facts from different logical moments and different semantic owners.

SQLite is local and sufficient; introducing Redis, distributed snapshots, or a general event bus would be more complexity than the product needs.

## Decision

Create `OperationalFactReader` and `OperationalFactBundle` backed by a real SQLite read transaction or an existing `ReadSession` proven to provide snapshot isolation.

The reader loads canonical facts in bounded batch queries:

- enabled station keys and station/account identities;
- endpoint refs and endpoint revisions;
- credential availability refs, not secret values;
- group binding and multiplier facts;
- pricing inputs and model base prices needed by the request;
- balance snapshots;
- key, endpoint, account, and model capability/health evidence;
- routing config and alias inputs.

The read transaction closes after raw bundle assembly. Pure projectors continue outside the transaction and produce request-specific projections.

Each bundle carries a request-local `snapshot_id`, `assembled_at_ms`, and a `FactVersionVector` with per-domain revisions/record IDs. No global cross-table `snapshot_revision` is invented.

Runtime state is separate:

- capacity registry;
- runtime metrics/outlier/cooldown;
- half-open probes;
- retry/wait queues;
- affinity candidates.

Planner receives only an immutable runtime overlay snapshot, never live registries or locks.

## Consequences

- Stage 1-4 can build and test readers/projectors without production data-plane cutover.
- Snapshot rebuild is allowed only for execution fence invalidation or explicit wait/revision wake, with a request-level rebuild budget.
- Single-model inference queries must not load full model inventory/history. `/v1/models` uses a separate catalog query shape.
- Monitoring must consume shared endpoint/key facts or narrow ports, not routing candidate DTO.
