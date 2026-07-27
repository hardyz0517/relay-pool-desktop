# Stage 6 Task 24k - Stations Dialogs

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting StationsPage dialog and detail body rendering from `src/features/stations/StationsPage.tsx`.
- Move `StationDialogs`, `DetailBody`, `SectionBlock`, `KeyDialog`, `Field`, dialog-local status tone mapping and form input class into `src/features/stations/pages/stations/StationDialogs.tsx`.
- Keep page state, query orchestration, station/key mutation handlers, drawer state and delete confirmations in `StationsPage.tsx`.

## View Ownership

- `src/features/stations/pages/stations/StationDialogs.tsx`
  - owns dialog/detail JSX composition only
  - receives station form, key form, credentials, snapshots and detail lists as props
  - delegates form changes, submit, key edits and key deletes back to page-owned callbacks
- `src/features/stations/StationsPage.tsx`
  - remains the controller for station/key state and all async handlers
  - remains the owner of query invalidation, collector refreshes and confirmation dialogs

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior, storage behavior or persistence path changed.
- `StationsPage.tsx` size is reduced to 840 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs KeyPoolPage split work, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/stations/StationDialogs.test.tsx src/features/stations/pages/stations/StationAssetRows.test.tsx src/features/stations/pages/stations/formModel.test.ts src/features/stations/pages/stations/displayModel.test.ts` - 9 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 993 resolved edges
- `pnpm lint` - passed with existing 82 warnings only
- `git diff --check` - passed
