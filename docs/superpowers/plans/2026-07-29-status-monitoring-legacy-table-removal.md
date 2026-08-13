# Status Monitoring Legacy Table Removal Follow-up

Status: completed on 2026-08-12 by schema 34 and the command/DTO/reader cutover.

Scope: remove the read-only compatibility surface for `channel_monitor_runs` after Status Monitoring V2 has shipped and one observation cycle confirms no production dependency remains.

## Preconditions

- Status Monitoring V2 has shipped in a formal release.
- `scripts/verify-monitoring-db.ps1` has been run against upgraded user/dev databases and shows no new production writes to `channel_monitor_runs`.
- `docs/superpowers/audits/status-monitoring-boundary-manifest.json` has no unexpired production exceptions except this follow-up.
- Users no longer need old run rows for support diagnostics.

## Work items

1. Add a migration that archives or deletes `channel_monitor_runs` according to the selected release policy.
2. Remove the dedicated legacy reader in `src-tauri/src/application/monitoring/legacy.rs`.
3. Remove `MonitoringStore::summary_runs` and any remaining `ChannelMonitorRun` compatibility DTOs that are only needed by the legacy reader.
4. Remove architecture allowlist entries for `channel_monitor_runs` and `list_channel_monitor_runs`.
5. Regenerate Tauri command bindings and permissions after command/DTO removal.
6. Update `docs/proposals/STATUS_MONITORING_REFACTOR_SPEC.md`, `docs/PROJECT_PLAN.md`, and release notes to mark the legacy observation cycle closed.

## Validation

```powershell
rg -n "channel_monitor_runs|list_channel_monitor_runs|ChannelMonitorRun|summary_runs" src-tauri/src src scripts
node scripts/monitoring-architecture.test.mjs
pnpm.cmd generate:bindings
pnpm.cmd test:contracts
pnpm.cmd build
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_migration -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_read_model -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

Exit condition: no production or compatibility reader code references `channel_monitor_runs`; historical status is available only through V2 execution/target/attempt facts and rollups.

## Completion

- schema `34` drops `channel_monitor_runs` after schema `10` backfilled its history;
- the Tauri command, registry contract, generated binding source, DTO, model, facade, service method, and persistence reader were removed;
- generation-1 import no longer copies the obsolete table;
- portable migration accepts the table only as ignored legacy package input and never exports or restores it;
- architecture checks forbid reintroducing the production command or reader chain.
