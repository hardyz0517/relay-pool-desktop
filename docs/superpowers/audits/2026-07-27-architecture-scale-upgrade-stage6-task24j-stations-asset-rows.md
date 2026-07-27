# Stage 6 Task 24j - Stations Asset Rows

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting StationsPage station asset row rendering from `src/features/stations/StationsPage.tsx`.
- Move `SortableStationAssetListRow`, `StationAssetListRow`, row-only manual authorization helpers and issue-tag badge rendering into `src/features/stations/pages/stations/StationAssetRows.tsx`.
- Keep DnD context ownership, page state, queries, actions and API calls in `StationsPage.tsx`.

## View Ownership

- `src/features/stations/pages/stations/StationAssetRows.tsx`
  - owns sortable row wrapper and station row rendering
  - owns row-local authorization visibility and issue tag badge rendering
  - receives website opening as a callback so the view does not own station API calls
- `src/features/stations/StationsPage.tsx`
  - remains the page controller and DnD context owner
  - passes station row callbacks and action state into row components

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior or persistence path changed.
- StationsPage size is reduced to 1379 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs remaining StationsPage split plus KeyPoolPage split, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/stations/formModel.test.ts src/features/stations/pages/stations/displayModel.test.ts src/features/stations/pages/stations/StationAssetRows.test.tsx` - 7 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 984 resolved edges
- `pnpm lint` - passed with existing 90 warnings only
