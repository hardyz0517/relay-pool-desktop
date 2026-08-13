# Page Switch Freshness and Performance Design

## Goal

Keep Change Center, Channel Status, Usage Records, and Pricing Comparison responsive during navigation without weakening data freshness or changing displayed results.

## Evidence

- The local database is about 97 MB, but the relevant indexed SQLite reads complete in roughly 0.5-6.3 ms.
- Channel Status starts the same workspace load through both React Query and a manual activation refresh.
- Pricing Comparison expands 18 stations into 54 station-scoped IPC calls in addition to its base reads.
- Usage Records loads 500 rows and resolves missing pricing context once per legacy row even when many rows share the same station key and model.
- Change Center persists unread state with one IPC command per event.
- These operations serialize on the application's shared SQLite connection and become intermittently visible when proxy and collector writes overlap navigation reads.

## Design

### Cached data remains visible

Retained pages continue to show their last complete snapshot. React Query performs background revalidation only after the page handoff allows refresh. A refetch must not clear usable data or replace the page with an initial loading state.

### One authoritative query per page

Channel Status consumes the React Query workspace result directly. It does not run a second manual workspace load on activation. While active, a five-second safety interval keeps status current; manual refresh and monitor-completion refresh still force an immediate read.

Pricing Comparison receives one backend workspace containing stations, station keys, group bindings, group rates, pricing rules, and the developer-mode flag. The backend builds the response while holding one database guard, eliminating station-by-station IPC fan-out and mixed-version cross-call snapshots. The query is stale on activation so revisiting the page triggers background revalidation while cached content remains visible.

### Preserve calculation semantics

Usage Records keeps the same 500-row result and the same legacy cost-estimation rules. Within one list operation, pricing economics are memoized by the exact `(station_key_id, trimmed_model)` input. Rows with the same lookup input reuse the same immutable context from that database snapshot.

### Atomic unread persistence

Change Center sends the unread IDs captured from the current snapshot in one command. The backend updates only rows that are still unread, inside one transaction, and returns current rows for those IDs. Events created after the snapshot are not touched, and rows concurrently resolved or dismissed are not changed back to read.

## Failure Behavior

- Background refetch failures leave the last complete snapshot visible and use the existing query error notification path.
- Workspace commands fail as a unit; the UI never combines a new station list with old pricing facts.
- Browser-only development fallbacks retain the existing multi-call loaders when Tauri invoke is unavailable.
- Batch read persistence falls back to the existing single-event API only when the new command is unavailable.

## Verification

- RED/GREEN regression tests for channel single-source loading, pricing workspace shape, request-log lookup reuse, and transactional batch read semantics.
- Existing focused page-activation, query-boundary, change-center, pricing, and request-log tests.
- `pnpm build` and `cargo check`.
- Browser navigation benchmark with realistic list sizes, plus a source-launched desktop smoke test across all four pages.
