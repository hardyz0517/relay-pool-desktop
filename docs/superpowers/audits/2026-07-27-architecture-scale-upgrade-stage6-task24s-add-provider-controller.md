# Stage 6 Task 24s - AddProvider Controller

Date: 2026-07-27

## Scope

- Continue Task 24 by extracting AddProvider page query, mutation, form, remote-key and discard-confirm state ownership from `src/features/stations/AddProviderPage.tsx`.
- Add `src/features/stations/useAddProviderPageController.ts` as the page controller hook.
- Keep `AddProviderPage.tsx` focused on page scaffold, form shell, section composition, remote-key dialog composition and discard confirmation composition.

## Controller Ownership

- `src/features/stations/useAddProviderPageController.ts`
  - owns edit/create loading, settings read, form draft state, group/key row state and remote-key discovery state
  - owns provider save, connection test, manual authorization, remote group sync, remote key scan/create/bind/unbind, local-key toggle and discard-confirm actions
  - owns query invalidation for stations/key-pool caches
- `src/features/stations/AddProviderPage.tsx`
  - owns only UI composition and delegates state/actions through the controller return value
  - no longer imports backend APIs, `QueryClient`, `useToast`, React state/effect hooks or `FormEvent`

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added to the page component.
- Existing AddProvider form/key group/save owners remain in their colocated modules.
- No listener ownership changed.
- No UI, API, storage behavior or persistence path changed.
- `AddProviderPage.tsx` size is reduced to 179 lines after this shard.
- Stage 6 Gate is not claimed; Task 24 still needs StationsPage controller work and closeout review before Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1040 resolved edges
- `pnpm exec vitest run src/features/stations/pages/add-provider/formModel.test.ts src/features/stations/pages/add-provider/keyGroupModel.test.ts src/features/stations/pages/add-provider/saveController.test.ts src/features/stations/pages/add-provider/AddProviderSections.test.tsx` - 4 files, 15 tests passed
- `pnpm lint` - passed with existing 73 warnings only
- `git diff --check` - passed
- `Select-String -Path src\features\stations\AddProviderPage.tsx -Pattern 'useQueryClient','useToast','@/lib/api','useEffect','useMemo','useState','FormEvent'` - no matches
