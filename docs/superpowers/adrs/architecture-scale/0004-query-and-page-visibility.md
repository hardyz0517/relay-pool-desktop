# ADR 0004: Query ownership and page visibility

## Status

Accepted. Existing per-resource stale times may remain temporarily, but every migrated query must conform to the policy and budget ledger.

## Context

TanStack Query is present, yet pages also keep server-state copies, activation loaders, DOM events and per-row `useQueries`. Multiple activity signals make it difficult to prove that hidden pages stop polling or that a mutation updates one authoritative state.

## Decision

TanStack Query is the sole owner of frontend server state. Components may own form drafts, selection and ephemeral presentation state, but may not keep a synchronized copy of query results. Mutations use a canonical cache transition or invalidation owned by the matching feature.

Every resource has one feature-owned query-key factory and query-options factory. Aggregate list read models replace per-row IPC. They use stable cursor pagination and a consistent backend snapshot/revision. Default page size is 100 rows, maximum page size is 500 rows and a decoded aggregate response is capped at 1 MiB. Detail payloads are queried separately.

`PageVisibility` is the only page-lifecycle signal. It combines mounted/visible/interactive state and directly controls query `enabled`/subscription/polling. Hidden pages issue zero active refreshes and do not react to presentation-level DOM events. The shell retains at most current, previous and one explicitly transient page. Other pages unmount.

Default migrated-query policy is `staleTime = 5 s`, `gcTime = 10 min`, one in-flight request per canonical key and no background interval while hidden. Explicit classes are: fast status `staleTime = 1 s`, history/change `2 s`, stable settings `60 s`. Exceptions require an owner and focused test. Query cache decoded-data estimate is capped at 64 MiB; crossing 80% emits a bounded diagnostic and crossing 100% evicts inactive least-recently-used queries before new prefetches.

The authoritative numeric values and qualification rules are in `architecture-scale-capacity-budgets.json`.

## Alternatives

- Add Redux/Zustand as a second server-state store: rejected because it duplicates cache lifecycle.
- Preserve all pages permanently for navigation speed: rejected because hidden effects and memory scale with navigation history.
- Keep per-row `useQueries` behind one component: rejected because IPC/backend work still scales with row count.
- Broadcast DOM events to refresh pages: rejected because cache mutation/invalidation is the owner.

## Consequences

Some pages will refetch after their inactive cache expires, but lifecycle and memory become bounded. Aggregate APIs require backend read models and payload metrics. Forms must explicitly separate draft values from resource snapshots.

## Rollback

Rollback is per resource/page and restores the previous query implementation as one unit. It may not introduce a second synchronized server-state owner or per-row IPC into an already migrated aggregate. Cache keys and mutation semantics roll back together.

## Verification

- component/runtime metrics show zero active queries for hidden pages;
- fixed 10/100/500 datasets use O(1) IPC calls and bounded backend read-port calls;
- concurrent mutation pagination has no duplicate, missing or mixed-revision row;
- decoded payload, cache estimate, retained pages and in-flight counts remain within ledger limits;
- parser tests reject direct invoke, cross-feature query-key ownership and server-result-to-state synchronization patterns;
- mutation tests prove the canonical cache transition and rollback behavior.
