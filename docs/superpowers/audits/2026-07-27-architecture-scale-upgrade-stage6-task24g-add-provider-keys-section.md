# Stage 6 Task 24g - Add Provider Keys Section

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting the AddProvider key editor and remote discovery section from `src/features/stations/AddProviderPage.tsx`.
- Move key toolbar, `StationKeyRowsEditor`, and remote discovery list composition into `ProviderKeysSection`.
- Keep key row state mutation, scan/create/bind/local-toggle handlers and backend operation flow in `AddProviderPage.tsx`.

## View Ownership

- `src/features/stations/pages/add-provider/AddProviderSections.tsx`
  - owns `ProviderKeysSection`
  - continues to own preset, connection, groups, options and shared field sections
- `src/features/stations/AddProviderPage.tsx`
  - remains the page state/controller owner
  - passes rows, remote state and callbacks into the keys section

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior or persistence path changed.
- AddProvider page size is reduced to 927 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs AddProvider remaining state/controller closeout plus StationsPage and KeyPoolPage splits, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/add-provider/AddProviderSections.test.tsx src/features/stations/pages/add-provider/formModel.test.ts src/features/stations/pages/add-provider/keyGroupModel.test.ts src/features/stations/pages/add-provider/saveController.test.ts` - 15 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 970 resolved edges
- `pnpm lint` - passed with existing 90 warnings only
