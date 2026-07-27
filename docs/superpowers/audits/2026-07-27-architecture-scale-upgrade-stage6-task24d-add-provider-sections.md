# Stage 6 Task 24d - Add Provider Sections

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting stateless AddProvider render sections from `src/features/stations/AddProviderPage.tsx`.
- Move preset selection, sidebar options and shared `Field` label layout into `src/features/stations/pages/add-provider/AddProviderSections.tsx`.
- Keep connection info, group/key editors, remote-key discovery, page effects and remote actions in `AddProviderPage.tsx` for later shards.

## View Ownership

- `src/features/stations/pages/add-provider/AddProviderSections.tsx`
  - owns `ProviderPresetSection`
  - owns `ProviderOptionsSection`
  - owns shared `Field`
- `src/features/stations/AddProviderPage.tsx`
  - remains the page composition owner
  - passes form state and handlers into the extracted view sections

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior or persistence path changed.
- AddProvider page size is reduced to 1100 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs the remaining AddProvider connection/key/remote section split plus StationsPage and KeyPoolPage splits, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/add-provider/AddProviderSections.test.tsx src/features/stations/pages/add-provider/formModel.test.ts src/features/stations/pages/add-provider/keyGroupModel.test.ts src/features/stations/pages/add-provider/saveController.test.ts` - 12 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 968 resolved edges
- `pnpm lint` - passed with existing 90 warnings only
