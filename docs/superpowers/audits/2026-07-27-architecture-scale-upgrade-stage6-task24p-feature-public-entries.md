# Stage 6 Task 24p - Feature Public Entries

Date: 2026-07-27

## Scope

- Continue Task 24 by adding feature public `index.ts` entries for shell pages whose page exports do not create new transitive cross-feature private edges.
- Route `src/app/shellPageRegistry.tsx` through public entries for channels, changes, collectors, dashboard, logs, routing, settings and stations.
- Route `src/app/App.tsx` through the stations public entry for transient station pages.
- Shrink the `legacy-composition-imports-feature-descendants` TypeScript temporary edge list from fifteen identities to the five key-pool/pricing identities that still depend on the remaining shared visual/helper ownership cleanup.

## Public Entry Ownership

- Added public entries:
  - `src/features/channels/index.ts`
  - `src/features/changes/index.ts`
  - `src/features/collectors/index.ts`
  - `src/features/dashboard/index.ts`
  - `src/features/logs/index.ts`
  - `src/features/routing/index.ts`
  - `src/features/settings/index.ts`
  - `src/features/stations/index.ts`
- Deliberately left key-pool and pricing page imports private for this shard because their public barrels would expose existing cross-feature private helper edges as new transitive violations.

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No UI, API, storage behavior or persistence path changed.
- Stage 6 Gate is not claimed; Task 24 still needs the remaining key-pool/pricing cross-feature helper cleanup before its temporary TypeScript edges can be deleted.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1027 resolved edges
- `pnpm lint` - passed with existing 82 warnings only
- `git diff --check` - passed
