# Stage 6 Task 24t - Stations Controller

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting Stations page query, mutation, dialog, drawer, DnD and per-station extra-data state ownership from `src/features/stations/StationsPage.tsx`.
- Add `src/features/stations/useStationsPageController.ts` as the page controller hook.
- Keep `StationsPage.tsx` focused on page scaffold, status/filter toolbar, DnD list composition, drawer composition, station dialogs and delete confirmations.

## Controller Ownership

- `src/features/stations/useStationsPageController.ts`
  - owns station queries, balance/change/station-asset reads and per-station detail extra reads
  - owns selected station, drawer open/close animation state, create/edit/detail dialog state and key dialog state
  - owns station create/update/delete/reorder, credentials removal, collection/balance/manual-authorization actions and station-key create/update/delete actions
  - owns query cancellation/invalidation for station shared query families
- `src/features/stations/StationsPage.tsx`
  - owns DnD shell composition, drawer markup, list/dialog mounting and filter toolbar composition
  - no longer imports backend APIs, `QueryClient`, `useToast`, React state/effect hooks, `FormEvent` or station form model helpers

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added to the page component.
- Existing stations form/display/dialog/row owners remain in their colocated modules.
- DnD sensors remain page-composition state; query/mutation/state ownership moved into the controller hook.
- No listener ownership changed.
- No UI, API, storage behavior or persistence path changed.
- `StationsPage.tsx` size is reduced to 304 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs closeout review before Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1045 resolved edges
- `pnpm exec vitest run src/features/stations/pages/stations/formModel.test.ts src/features/stations/pages/stations/displayModel.test.ts src/features/stations/pages/stations/StationAssetRows.test.tsx src/features/stations/pages/stations/StationDialogs.test.tsx src/lib/groupVisualMeta.test.ts` - 5 files, 15 tests passed
- `pnpm lint` - passed with existing 73 warnings only
- `git diff --check` - passed
- `Select-String -Path src\features\stations\StationsPage.tsx -Pattern 'useQueryClient','useToast','@/lib/api','useEffect','useMemo','useState','FormEvent','formModel'` - no matches
