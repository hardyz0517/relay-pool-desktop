# Stage 6 Task 24h - Stations Form Model

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting StationsPage station/key form model logic from `src/features/stations/StationsPage.tsx`.
- Move form state types, default states, station input serialization, key input serialization, key edit hydration and endpoint-origin warnings into `src/features/stations/pages/stations/formModel.ts`.
- Keep page state, queries, effects, DnD, dialogs, detail drawer and save handlers in `StationsPage.tsx` for later shards.

## Model Ownership

- `src/features/stations/pages/stations/formModel.ts`
  - owns `StationFormState` and `StationKeyFormState`
  - owns `emptyForm` and `emptyKeyForm`
  - owns `formToInput`, `toCreateKeyInput`, `toUpdateKeyInput` and `keyToForm`
  - owns endpoint-origin warning derivation
- `src/features/stations/StationsPage.tsx`
  - imports model helpers and remains the page state/controller owner

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior or persistence path changed.
- StationsPage size is reduced to 1712 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs remaining StationsPage split plus KeyPoolPage split, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/stations/formModel.test.ts` - 3 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 973 resolved edges
- `pnpm lint` - passed with existing 90 warnings only
