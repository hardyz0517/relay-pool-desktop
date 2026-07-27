# Stage 6 Task 24i - Stations Display Model

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting StationsPage pure display helpers from `src/features/stations/StationsPage.tsx`.
- Move station avatar label, URL display, balance formatting, relative time formatting, nullable time formatting, multiplier formatting and collector/group status labels into `src/features/stations/pages/stations/displayModel.ts`.
- Keep row components, dialogs, detail drawer, DnD and page state in `StationsPage.tsx`.

## Model Ownership

- `src/features/stations/pages/stations/displayModel.ts`
  - owns scalar display formatting for StationsPage
  - owns issue-tag tone class selection
  - owns collector task/run and group binding status labels
- `src/features/stations/StationsPage.tsx`
  - imports display helpers and remains the view composition owner

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior or persistence path changed.
- StationsPage size is reduced to 1622 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs remaining StationsPage split plus KeyPoolPage split, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/stations/formModel.test.ts src/features/stations/pages/stations/displayModel.test.ts` - 6 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 975 resolved edges
- `pnpm lint` - passed with existing 90 warnings only
