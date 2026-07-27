# Stage 6 Task 24r - KeyPool Controller

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting KeyPool page query, mutation, form, operation and dialog state ownership from `src/features/key-pool/KeyPoolPage.tsx`.
- Add `src/features/key-pool/useKeyPoolPageController.ts` as the page controller hook.
- Keep `KeyPoolPage.tsx` focused on page scaffold, filters toolbar, DnD composition, row/dialog composition and current group label rendering helpers.

## Controller Ownership

- `src/features/key-pool/useKeyPoolPageController.ts`
  - owns key-pool, stations and channel-monitor query reads
  - owns filter state, edit/create form state, delete state, connectivity dialog state and monitor action state
  - owns reorder, enabled toggle, delete, create/edit save, connectivity test and monitor toggle actions
  - owns query invalidation and optimistic reorder cache updates
- `src/features/key-pool/KeyPoolPage.tsx`
  - owns DnD shell composition, toolbar composition, row/dialog mounting and group option label rendering
  - no longer imports backend APIs, `QueryClient`, `useActivityQuery`, `useToast` or connectivity operation internals

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added to the page component.
- Existing key-pool data and mutation owners moved into a single colocated hook.
- No listener ownership changed.
- No UI, API, storage behavior or persistence path changed.
- `KeyPoolPage.tsx` size is reduced to 262 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs a closeout review across AddProviderPage, StationsPage and KeyPoolPage before Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1038 resolved edges
- `pnpm exec vitest run src/features/key-pool/KeyEditDialog.test.tsx src/features/key-pool/KeyPoolFormModel.test.ts src/features/key-pool/KeyPoolRows.test.tsx src/features/key-pool/KeyConnectivityTestDialog.test.tsx src/features/key-pool/connectivityOperationController.test.ts` - 5 files, 10 tests passed
- `pnpm lint` - passed with existing 73 warnings only
- `git diff --check` - passed
