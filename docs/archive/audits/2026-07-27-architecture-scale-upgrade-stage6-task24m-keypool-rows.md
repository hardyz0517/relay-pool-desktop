# Stage 6 Task 24m - KeyPool Rows

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting KeyPoolPage table row rendering from `src/features/key-pool/KeyPoolPage.tsx`.
- Move `SortableKeyRow`, `KeyRowContent`, `TableHeadCell`, row grid constants, compact badge derivation, station base URL formatting and cooldown detection into `src/features/key-pool/KeyPoolRows.tsx`.
- Keep list filtering, DnD context ownership, reorder mutation, toggle mutation, connectivity opening and delete confirmation in `KeyPoolPage.tsx`.

## View Ownership

- `src/features/key-pool/KeyPoolRows.tsx`
  - owns key-pool row JSX and sortable row wrapper
  - owns row-local status badge priority and base URL display formatting
  - delegates edit, delete, scheduling toggle, monitoring toggle and connectivity test actions through page-owned callbacks
- `src/features/key-pool/KeyPoolPage.tsx`
  - remains the owner of key-pool query state, filters, DnD context, cache updates and backend mutations
  - retains action handlers and passes row action state into row components

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior, storage behavior or persistence path changed.
- `KeyPoolPage.tsx` size is reduced to 1119 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs remaining KeyPoolPage split work, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/key-pool/KeyPoolRows.test.tsx src/features/key-pool/KeyConnectivityTestDialog.test.tsx src/features/key-pool/connectivityOperationController.test.ts` - 6 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1007 resolved edges
- `pnpm lint` - passed with existing 82 warnings only
- `git diff --check` - passed
