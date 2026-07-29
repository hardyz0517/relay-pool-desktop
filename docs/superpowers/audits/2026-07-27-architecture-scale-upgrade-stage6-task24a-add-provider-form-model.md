# Stage 6 Task 24a - Add Provider Form Model

Date: 2026-07-27

## Scope

- Start Task 24 by extracting AddProvider page form/draft state from `src/features/stations/AddProviderPage.tsx`.
- Keep this shard limited to pure form state, dirty snapshot serialization and remote capability defaults.
- Leave provider operations, subscriptions, command calls, listeners and page composition in their existing owners.

## State Ownership

- `src/features/stations/pages/add-provider/formModel.ts`
  - owns `AddProviderFormState`, `ConnectionTestState` and `RemoteCreateInput`
  - owns create/edit form hydration through `createDefaultProviderForm` and `formFromStation`
  - owns dirty snapshot serialization through `serializeProviderDraft`
  - owns draft remote capability defaults through `draftRemoteCapability`
- `src/features/stations/AddProviderPage.tsx`
  - remains the composition owner for this shard
  - continues to own query usage, mutation calls, event handlers, dialogs and layout
  - imports the form model rather than defining it inline

## Boundary Notes

- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No route, operation or persistence boundary changed.
- Stage 6 Gate is not claimed; Task 24 still needs the remaining AddProvider split, StationsPage split, KeyPoolPage split and Task 25.

## Evidence

- `pnpm exec tsc --noEmit` - passed
- `pnpm exec vitest run src/features/stations/pages/add-provider/formModel.test.ts` - 4 tests passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 945 resolved edges
- `pnpm lint` - passed with existing warnings only
