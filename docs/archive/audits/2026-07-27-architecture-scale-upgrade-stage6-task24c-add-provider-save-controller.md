# Stage 6 Task 24c - Add Provider Save Controller

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting AddProvider key/group save controllers from `src/features/stations/AddProviderPage.tsx`.
- Move row delete/update/create and group disable/upsert orchestration into `src/features/stations/pages/add-provider/saveController.ts`.
- Keep React state, page effects, remote-key actions, dialog state and cache invalidation in `AddProviderPage.tsx` for later shards.

## Controller Ownership

- `src/features/stations/pages/add-provider/saveController.ts`
  - owns `saveKeyRows` key-row persistence orchestration
  - owns `saveGroupRows` group-row persistence orchestration
  - owns save-controller-only group matching and group-key hash construction
  - uses explicit dependency types so tests can validate calls without live Tauri bridge access
- `src/features/stations/AddProviderPage.tsx`
  - calls `saveKeyRows` and `saveGroupRows`
  - still owns page submit/remote action flow and user-visible toasts

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No persistence files or migrations changed.
- AddProvider page size is reduced to 1221 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs the remaining AddProvider page composition extraction plus StationsPage and KeyPoolPage splits, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/add-provider/formModel.test.ts src/features/stations/pages/add-provider/keyGroupModel.test.ts src/features/stations/pages/add-provider/saveController.test.ts` - 10 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 960 resolved edges
- `pnpm lint` - passed with existing 90 warnings only
