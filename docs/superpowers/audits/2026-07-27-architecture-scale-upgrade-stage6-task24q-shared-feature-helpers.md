# Stage 6 Task 24q - Shared Feature Helpers

Date: 2026-07-27

## Scope

- Continue Task 24 by moving true shared TypeScript feature helpers out of feature-private stations/channels paths.
- Move station group option helpers and visual metadata into `src/lib`.
- Move station group badge/icon components into `src/components/group`.
- Move channel monitor view-model helpers into `src/lib/channelMonitorViewModel.ts`.
- Add key-pool and pricing public `index.ts` entries and route composition roots through public feature entries.
- Delete the Task 24 TypeScript temporary edges for legacy composition-root imports and cross-feature private visual/view-model reuse.

## Shared Ownership

- `src/lib/groupOptionViewModels.ts`
  - owns group option identity, label formatting, matching, normalization and select-option construction
- `src/lib/groupVisualMeta.ts`
  - owns provider/platform visual classification for station groups
- `src/lib/groupVisualStyles.ts`
  - owns class-name mapping for group visual platforms
- `src/lib/channelMonitorViewModel.ts`
  - owns channel monitor draft conversion, validation, target/template labels and station-key monitor helper inputs
- `src/components/group/StationGroupChip.tsx`
  - owns shared group badges and option labels
- `src/components/group/Sub2ApiPlatformIcon.tsx`
  - owns shared provider platform icons

## Boundary Notes

- The TypeScript boundary manifest no longer carries the `legacy-composition-imports-feature-descendants` temporary edge group.
- The TypeScript boundary manifest no longer carries the `legacy-cross-feature-visual-and-view-model-edges` temporary edge group.
- No new `BackendClient`, `QueryClient`, context locator prop or subscription owner was added.
- No listener ownership changed.
- No UI, API, storage behavior or persistence path changed.
- Stage 6 Gate is not claimed; Task 24 still needs a closeout review before Task 25.

## Evidence

- `rg -n "features/(stations/(groupOptionViewModels|components/StationGroupChip|components/Sub2ApiPlatformIcon|groupVisualMeta|groupVisualStyles)|channels/channelMonitorViewModel)|\\.\\./groupOptionViewModels|\\.\\./groupVisualMeta|\\.\\./groupVisualStyles|\\./StationGroupChip" src docs\superpowers\audits\architecture-scale-boundary-manifest.json -g "*.ts" -g "*.tsx" -g "*.json"` - no matches
- `pnpm exec tsc --noEmit` - passed
- `node scripts\architecture\check-typescript-boundaries.mjs` - passed, 1031 resolved edges
- `pnpm exec vitest run src/lib/groupVisualMeta.test.ts src/features/key-pool/KeyEditDialog.test.tsx src/features/key-pool/KeyPoolFormModel.test.ts src/features/key-pool/KeyPoolRows.test.tsx src/features/key-pool/KeyConnectivityTestDialog.test.tsx` - 5 files, 14 tests passed
- `node scripts\tests\channelMonitorViewModel.test.mjs` - passed
- `pnpm lint` - passed with existing 73 warnings only
