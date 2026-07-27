# Stage 6 Task 24n - KeyPool Form Model

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting KeyPoolPage edit/create form state and save-input model helpers from `src/features/key-pool/KeyPoolPage.tsx`.
- Move `KeyPoolEditForm`, `emptyEditForm`, group selection conversion, capability conversion, edit/create form construction and list normalization into `src/features/key-pool/KeyPoolFormModel.tsx`.
- Keep current group fact loading, dialog JSX, save handlers, mutation calls, query invalidation and existing page-level label helper compatibility edges in `KeyPoolPage.tsx`.

## Model Ownership

- `src/features/key-pool/KeyPoolFormModel.tsx`
  - owns form DTO shape and default values
  - owns explicit group selection semantics: keep, clear and set
  - owns conversion from form text fields into station-key capabilities
  - uses only public shared types plus local key-pool defaults; it does not import stations feature-private modules
- `src/features/key-pool/KeyPoolPage.tsx`
  - remains the owner of group facts reads and backend save calls
  - still owns the edit dialog rendering and page-level temporary station group label helpers

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No API behavior, storage behavior or persistence path changed.
- `KeyPoolPage.tsx` size is reduced to 910 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs remaining KeyPoolPage dialog split or closeout review, then Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/key-pool/KeyPoolFormModel.test.ts src/features/key-pool/KeyPoolRows.test.tsx src/features/key-pool/KeyConnectivityTestDialog.test.tsx src/features/key-pool/connectivityOperationController.test.ts` - 9 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1012 resolved edges
- `pnpm lint` - passed with existing 82 warnings only
- `git diff --check` - passed
