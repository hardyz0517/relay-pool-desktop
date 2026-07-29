# Stage 6 Task 24 Closeout - Giant Page Decomposition

Date: 2026-07-27

## Scope

- Close Task 24 after decomposing the three named giant pages:
  - `src/features/stations/AddProviderPage.tsx`
  - `src/features/stations/StationsPage.tsx`
  - `src/features/key-pool/KeyPoolPage.tsx`
- Verify that page components now own UI composition while query, mutation, form, operation, dialog/list row and pure view-model owners are separated into colocated modules.
- Verify that Task 24 TypeScript temporary edges were deleted after moving true shared helpers into shared ownership.

## Final Ownership Map

### AddProvider

- `src/features/stations/AddProviderPage.tsx` - page scaffold, form shell, section composition, remote-key dialog composition and discard confirmation composition.
- `src/features/stations/useAddProviderPageController.ts` - edit/create loading, settings read, form draft state, group/key row state, remote-key discovery state, provider save, connection test, manual authorization, remote group sync, remote key scan/create/bind/unbind and cache invalidation.
- `src/features/stations/pages/add-provider/formModel.ts` - provider form DTO/defaults/serialization.
- `src/features/stations/pages/add-provider/keyGroupModel.ts` - group/key row view-model and validation helpers.
- `src/features/stations/pages/add-provider/saveController.ts` - group/key persistence sequencing.
- `src/features/stations/pages/add-provider/AddProviderSections.tsx` - section-level JSX.

### Stations

- `src/features/stations/StationsPage.tsx` - page scaffold, status/filter toolbar, DnD list composition, drawer composition, station dialogs and delete confirmations.
- `src/features/stations/useStationsPageController.ts` - station queries, balance/change/station-asset reads, detail extra reads, selected station state, drawer animation state, create/edit/detail dialog state, key dialog state, station create/update/delete/reorder, credentials removal, collection/balance/manual authorization and station-key create/update/delete actions.
- `src/features/stations/pages/stations/formModel.ts` - station and station-key form DTO conversion.
- `src/features/stations/pages/stations/displayModel.ts` - list/detail display derivations.
- `src/features/stations/pages/stations/StationAssetRows.tsx` - asset row and sortable row JSX.
- `src/features/stations/pages/stations/StationDialogs.tsx` - station detail/create/edit/key dialog JSX.

### KeyPool

- `src/features/key-pool/KeyPoolPage.tsx` - page scaffold, filters toolbar, DnD composition, row/dialog mounting and group option label rendering.
- `src/features/key-pool/useKeyPoolPageController.ts` - key-pool/stations/channel-monitor query reads, filter state, edit/create form state, delete state, connectivity dialog state, monitor action state, reorder, enabled toggle, delete, create/edit save, connectivity test, monitor toggle and cache invalidation.
- `src/features/key-pool/KeyPoolFormModel.tsx` - key-pool form DTO/defaults/group selection/capability conversion.
- `src/features/key-pool/KeyPoolRows.tsx` - sortable key-pool row JSX.
- `src/features/key-pool/KeyEditDialog.tsx` - create/edit key dialog JSX.
- `src/features/key-pool/KeyConnectivityTestDialog.tsx` - connectivity test dialog JSX.
- `src/features/key-pool/connectivityOperationController.ts` and `src/features/key-pool/useConnectivityOperation.ts` - connectivity operation lifecycle.

### Shared Helpers

- `src/lib/groupOptionViewModels.ts`, `src/lib/groupVisualMeta.ts`, `src/lib/groupVisualStyles.ts`, `src/components/group/StationGroupChip.tsx`, `src/components/group/Sub2ApiPlatformIcon.tsx` - shared station-group option/visual ownership.
- `src/lib/channelMonitorViewModel.ts` - shared channel monitor draft/label/station-key monitor helper ownership.

## Exit Checks

- Page source ownership:
  - `AddProviderPage.tsx` - 179 lines
  - `StationsPage.tsx` - 304 lines
  - `KeyPoolPage.tsx` - 262 lines
  - `Select-String -Path src\features\stations\AddProviderPage.tsx,src\features\stations\StationsPage.tsx,src\features\key-pool\KeyPoolPage.tsx -Pattern 'useQueryClient','useToast','@/lib/api','useActivityQuery','useEffect','useMemo','useState','FormEvent'` - no matches
- TypeScript boundary manifest:
  - `temporary_edges` is empty.
  - TypeScript architecture gate rejects stale temporary edges and passed after Task 24 closeout.
- Composition roots:
  - Feature shell pages are imported through public feature entries.
  - KeyPool and Pricing public entries were restored after shared helper ownership moved out of feature-private stations/channels paths.
- Behavior ownership:
  - Dialog, row/list, form model, save controller, operation controller and view-model modules have focused tests.
  - Page components no longer receive full `BackendClient`, `QueryClient` or unbounded context locators.
- No Persistence V2 files changed during this closeout shard.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1045 resolved edges
- `pnpm exec vitest run src/features/stations/pages/add-provider/formModel.test.ts src/features/stations/pages/add-provider/keyGroupModel.test.ts src/features/stations/pages/add-provider/saveController.test.ts src/features/stations/pages/add-provider/AddProviderSections.test.tsx src/features/stations/pages/stations/formModel.test.ts src/features/stations/pages/stations/displayModel.test.ts src/features/stations/pages/stations/StationAssetRows.test.tsx src/features/stations/pages/stations/StationDialogs.test.tsx src/features/key-pool/KeyEditDialog.test.tsx src/features/key-pool/KeyPoolFormModel.test.ts src/features/key-pool/KeyPoolRows.test.tsx src/features/key-pool/KeyConnectivityTestDialog.test.tsx src/features/key-pool/connectivityOperationController.test.ts src/lib/groupVisualMeta.test.ts` - 14 files, 40 tests passed
- `node scripts\tests\channelMonitorViewModel.test.mjs` - passed
- `pnpm lint` - passed with existing 73 warnings only
- `git diff --check` - passed
- `git status --short --branch` - clean on `codex/architecture-scale-upgrade` before writing this closeout document

## Gate Note

Task 24 is closed by this evidence. Stage 6 Gate is not claimed because Task 25 has not been completed.
