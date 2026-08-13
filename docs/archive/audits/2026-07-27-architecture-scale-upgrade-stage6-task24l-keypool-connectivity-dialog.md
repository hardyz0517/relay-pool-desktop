# Stage 6 Task 24l - KeyPool Connectivity Dialog

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting KeyPoolPage station-key connectivity test dialog rendering from `src/features/key-pool/KeyPoolPage.tsx`.
- Move `KeyConnectivityTestDialog`, connectivity console line construction, model option construction and the default connectivity test model constant into `src/features/key-pool/KeyConnectivityTestDialog.tsx`.
- Keep connectivity operation ownership, cancellation, event handling, backend test calls, query invalidation and monitoring mutations in `KeyPoolPage.tsx`.

## View Ownership

- `src/features/key-pool/KeyConnectivityTestDialog.tsx`
  - owns dialog JSX, model select state and connectivity console view model
  - derives model options from passed station-key capabilities
  - delegates test requests, close handling and displayed response text updates through page-owned callbacks
- `src/features/key-pool/KeyPoolPage.tsx`
  - remains the owner of station-key connectivity operation lifecycle
  - remains the owner of `testStationKeyConnectivity`, monitor creation/update and key-pool cache invalidation

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior, storage behavior or persistence path changed.
- `KeyPoolPage.tsx` size is reduced to 1367 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs remaining KeyPoolPage split work, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/key-pool/KeyConnectivityTestDialog.test.tsx src/features/key-pool/connectivityOperationController.test.ts` - 4 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1000 resolved edges
- `pnpm lint` - passed with existing 82 warnings only
- `git diff --check` - passed
