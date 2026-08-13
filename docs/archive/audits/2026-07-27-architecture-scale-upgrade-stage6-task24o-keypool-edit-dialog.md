# Stage 6 Task 24o - KeyPool Edit Dialog

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting KeyPoolPage edit/create dialog JSX from `src/features/key-pool/KeyPoolPage.tsx`.
- Move `KeyEditDialog`, dialog-local field wrappers, checkbox wrappers and dialog input styling into `src/features/key-pool/KeyEditDialog.tsx`.
- Keep data loading, save handlers, mutations, toast handling, query invalidation and temporary station group label compatibility helpers in `KeyPoolPage.tsx`.

## View Ownership

- `src/features/key-pool/KeyEditDialog.tsx`
  - owns the create/edit dialog layout, labels, form fields and capability checkboxes
  - delegates form changes, station changes, close and submit actions through page-owned callbacks
  - receives current group option rendering helpers from the page instead of importing station feature-private view helpers
- `src/features/key-pool/KeyPoolPage.tsx`
  - remains the owner of query state, group fact reads, dialog open state, form state and backend save calls
  - retains the existing page-level temporary station group label helper imports covered by the current boundary manifest

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior, storage behavior or persistence path changed.
- `KeyPoolPage.tsx` size is reduced to 712 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs closeout review, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/key-pool/KeyEditDialog.test.tsx src/features/key-pool/KeyPoolFormModel.test.ts src/features/key-pool/KeyPoolRows.test.tsx src/features/key-pool/KeyConnectivityTestDialog.test.tsx src/features/key-pool/connectivityOperationController.test.ts` - 10 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1018 resolved edges
- `pnpm lint` - passed with existing 82 warnings only
- `git diff --check` - passed
