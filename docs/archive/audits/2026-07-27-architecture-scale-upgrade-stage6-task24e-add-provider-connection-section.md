# Stage 6 Task 24e - Add Provider Connection Section

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting the AddProvider connection information section from `src/features/stations/AddProviderPage.tsx`.
- Move station name/type, website/API URL, login credentials, connection-test status and login authorization buttons into `ProviderConnectionSection`.
- Keep page state, form handlers, remote capability updates and backend calls in `AddProviderPage.tsx`.

## View Ownership

- `src/features/stations/pages/add-provider/AddProviderSections.tsx`
  - owns `ProviderConnectionSection`
  - owns the existing `ProviderPresetSection`, `ProviderOptionsSection` and shared `Field`
- `src/features/stations/AddProviderPage.tsx`
  - remains the page composition owner
  - passes scalar state and callbacks into `ProviderConnectionSection`

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior or persistence path changed.
- AddProvider page size is reduced to 999 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs the remaining AddProvider group/key/remote section split plus StationsPage and KeyPoolPage splits, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/add-provider/AddProviderSections.test.tsx src/features/stations/pages/add-provider/formModel.test.ts src/features/stations/pages/add-provider/keyGroupModel.test.ts src/features/stations/pages/add-provider/saveController.test.ts` - 13 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 968 resolved edges
- `pnpm lint` - passed with existing 90 warnings only
