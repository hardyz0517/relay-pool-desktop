# Stage 6 Task 24f - Add Provider Groups Section

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting the AddProvider groups editor section from `src/features/stations/AddProviderPage.tsx`.
- Move group toolbar and `StationGroupRowsEditor` composition into `ProviderGroupsSection`.
- Keep group row state mutation, dedupe, sync operation and remote capability state in `AddProviderPage.tsx`.

## View Ownership

- `src/features/stations/pages/add-provider/AddProviderSections.tsx`
  - owns `ProviderGroupsSection`
  - continues to own preset, connection, options and shared field sections
- `src/features/stations/AddProviderPage.tsx`
  - passes rows and callbacks into the groups section
  - still owns page effects, state, and backend operation handlers

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior or persistence path changed.
- AddProvider page size is reduced to 977 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs the remaining AddProvider key/remote section split plus StationsPage and KeyPoolPage splits, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/add-provider/AddProviderSections.test.tsx src/features/stations/pages/add-provider/formModel.test.ts src/features/stations/pages/add-provider/keyGroupModel.test.ts src/features/stations/pages/add-provider/saveController.test.ts` - 14 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 969 resolved edges
- `pnpm lint` - passed with existing 90 warnings only
