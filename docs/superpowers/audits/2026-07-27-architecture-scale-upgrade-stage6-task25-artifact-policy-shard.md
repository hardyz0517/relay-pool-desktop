# Stage 6 Task 25.D Shard - Artifact Policy Cleanup

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Remove tracked legacy `output/` image artifacts that were registered with `delete_task: 25`.
- Strengthen the architecture artifact policy gate so registered artifact paths fail closed when they are absolute, unresolved, root-shaped, or escaping the workspace.
- Do not delete directories recursively and do not touch Persistence V2 work.

## Changes

- Deleted six tracked legacy UI comparison images from `output/`.
  - `output/relay-pool-icon-size-preview.png`
  - `output/taskbar-current-enlarged.png`
  - `output/taskbar-icon-bounds-th32.png`
  - `output/taskbar-qq-vs-relay-enlarged.png`
  - `output/taskbar-relay-current-plus-new.png`
  - `output/taskbar-relay-size-after-core-boost.png`
- Removed the corresponding Task 25 artifact exceptions from `architecture-scale-upgrade-inventory.json`.
- Updated `scripts/architecture/check-artifact-policy.mjs`.
  - Registered artifact paths must be normalized repository-relative paths.
  - The gate rejects absolute paths, parent traversal, workspace root, home-relative tokens, and unresolved environment variables.
  - Fixture mode now covers the new path rejection cases.

## Deletion Safety

- Each deleted file was resolved with `Resolve-Path`.
- Each absolute path was verified to start with the current worktree root and the current worktree `output` root before deletion.
- Deletion used explicit file paths only; no recursive deletion was performed.
- `rg` found no references to the deleted PNG file names outside the inventory registration removed in this shard.

## Evidence

- `node scripts/architecture/check-artifact-policy.mjs --fixtures` - passed
- `pnpm.cmd run architecture:artifacts` - passed, 0 registered legacy roots
- `pnpm.cmd run architecture:fixtures` - passed
- `git ls-files output` - returned no tracked output files
- `git diff --cached --check` - passed

## Verification Notes

- The formal `architecture:artifacts` gate was run after the deleted tracked files were staged, because the gate intentionally uses `git ls-files` as its tracked artifact source.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/superpowers/audits/persistence-v2-boundary-manifest.json`
