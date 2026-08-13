# Change Center Legacy Deletion Ledger

Status: Task 12 cutover complete; schema 30 destructive migration implemented,
but Task 13 observation and post-deletion cleanup gates remain open.

## Current production inventory

The production graph no longer contains the legacy ChangeService, ChangeStore,
change-event IPC, generated binding, query key, or frontend view-model. The
architecture gate reports 21 historical hits only in migration, import,
portable-catalog, upgrade-recovery, and test-fixture paths (18 hits in the
latest contract run).

Migration `0030_remove_legacy_change_events.sql` now removes the legacy table
and indexes after the durable schema 29 upgrade. The migration is covered by
schema21 postcondition and generation-upgrade restart tests; it does not close
the required production observation, backup/restore, or catalog-removal gates.

## Retained historical paths

| Path | Owner | Allowed purpose | Removal condition |
|---|---|---|---|
| `src-tauri/src/services/data_store/alerting_upgrade.rs` | upgrade executor | read/backfill and current-facts rebuild | observation complete and adapter count is zero |
| `src-tauri/src/persistence/migrations.rs` | migration history | preserve historical schema migration | never rewrite applied migration history |
| `src-tauri/src/persistence/legacy_import/*` | legacy import | import/export compatibility | remove with the independent schema/catalog revision |
| `src-tauri/src/services/portable_migration/*` | portable catalog | inspect older packages | remove after catalog compatibility window |
| `src-tauri/src/persistence/upgrade_fault.rs` | recovery journal | classify old upgrade phases | remove after recovery matrix is retired |
| `src-tauri/src/application/provider_drafts.rs` | test fixture | assert legacy empty-table behavior | replace fixture after Task 13 |

## Prohibited

- No production writer may insert into `change_events`.
- No product IPC, UI, query, badge, policy, delivery, or recovery decision may
  read the legacy table.
- No compatibility alias may reintroduce the removed commands or DTOs.
- Task 13 still requires a completed observation period, backup/restore drill,
  schema15-to-latest evidence on the post-delete schema, and removal of the
  remaining migration-only adapter/catalog entries before this ledger can be
  marked closed.

## Evidence

- `pnpm.cmd architecture:alerting`
- `pnpm.cmd test:contracts`
- `pnpm.cmd generate:bindings --check`
- `pnpm.cmd architecture:security`
