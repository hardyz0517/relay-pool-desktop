# Stage 6 Task 25.B/C Shard - PageActivity Legacy Deletion

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Delete the frontend `PageActivity` compatibility module after page query ownership had already moved to canonical `PageVisibility`.
- Replace brittle PageActivity source-contract expectations with parser-backed and jsdom behavior evidence.
- Keep production behavior unchanged for shell/transient page visibility, inert state, and query execution boundaries.

## Changes

- Updated shell and transient page hosts.
  - `ShellPageHost` now wraps retained shell slots directly in `PageVisibilityProvider`.
  - `TransientPageHost` now wraps transient content directly in `PageVisibilityProvider`.
  - Deleted `src/components/shell/PageActivity.tsx`.
- Expanded `PageVisibility.test.ts`.
  - Added jsdom coverage proving canonical visibility is visible to interaction consumers.
  - Added query behavior coverage proving hidden pages do not start `useActivityQuery` work until foreground visibility is restored.
- Updated frontend contract scripts.
  - `page-activity-query-contract.test.mjs` now parses `PageVisibility` and `useActivityQuery` structure instead of requiring the legacy module.
  - `page-activation-refresh.test.mjs` now checks current query-owner modules for Stations and Key Pool.
  - Page transition and motion scripts now assert `PageVisibilityProvider` wiring instead of `PageActivityProvider` wiring.

## Boundary Notes

- Production `src` scan is clear for:
  - `PageActivityProvider`
  - `usePageActivity`
  - `usePageRefreshEnabled`
  - `from "@/components/shell/PageActivity"`
- This closes the frontend PageActivity legacy path for this Task 25 shard.
- Task 25 remains open for the remaining legacy categories: runner/stop flag, outbound builder/provider dispatcher, artifact policy cleanup, and final graph regeneration.

## Evidence

- `pnpm.cmd exec tsc --noEmit` - passed
- `pnpm.cmd exec vitest run src/app/navigation/PageVisibility.test.ts` - 6 tests passed
- `node scripts/page-activity-query-contract.test.mjs` - passed
- `node scripts/page-activation-refresh.test.mjs` - passed
- `node scripts/page-transition-container.test.mjs` - passed
- `node scripts/motion-page-transition.test.mjs` - passed
- `pnpm.cmd run architecture:typescript` - passed, 1041 resolved edges
- `pnpm.cmd run build` - passed; Vite reported the existing large chunk warning
- `git diff --check` - passed

## Verification Notes

- This shard did not run the desktop app or inspect screenshots.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/audits/persistence-v2-boundary-manifest.json`
