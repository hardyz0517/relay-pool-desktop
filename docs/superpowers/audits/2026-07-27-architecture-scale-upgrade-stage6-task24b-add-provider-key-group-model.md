# Stage 6 Task 24b - Add Provider Key/Group Model

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting AddProvider key/group draft helpers from `src/features/stations/AddProviderPage.tsx`.
- Move pure draft hydration, dedupe, validation, group option projection, remote group projection and auto-created local-key note matching into `src/features/stations/pages/add-provider/keyGroupModel.ts`.
- Keep API save controllers and page event handlers in `AddProviderPage.tsx` for a later controller split.

## State Ownership

- `src/features/stations/pages/add-provider/keyGroupModel.ts`
  - owns station key draft hydration through `keyToDraft`
  - owns station group draft hydration through `groupBindingsToDrafts`
  - owns key/group row validation through `validateKeyRows` and `validateGroupRows`
  - owns group row dedupe and saved-option merging
  - owns remote group option projection and remote-created local key note matching
  - owns small scalar coercions used by AddProvider form submit paths
- `src/features/stations/AddProviderPage.tsx`
  - remains the composition and API controller owner for this shard
  - continues to own React state, effects, query invalidation, save calls, remote-key actions, dialogs and layout

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior or persistence path changed.
- AddProvider page size is reduced to 1389 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs AddProvider controller/dialog/list extraction plus StationsPage and KeyPoolPage splits, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/add-provider/formModel.test.ts src/features/stations/pages/add-provider/keyGroupModel.test.ts` - 8 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 953 resolved edges
- `pnpm lint` - passed with existing 90 warnings only
