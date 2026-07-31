# Schema Upgrade Authoring Contract

Status: active

This contract defines how Relay Pool Desktop changes the local generation-2 SQLite schema after schema `15`.

The rule is intentionally small: schema `15` is the minimum automatic upgrade baseline, and every future release must keep one clear route from `15` to the latest schema. Do not add per-version startup branches, runtime repair shortcuts, or duplicate schema constants.

## Runtime Shape

Supported startup follows one path:

```text
read-only probe -> pure planner -> executor(plan) -> postconditions -> ready | typed recovery
```

Ownership boundaries:

- `schema_registry.rs` owns the minimum baseline, latest schema derived from embedded migrations, current secret format, and binary compatibility values.
- `startup_probe.rs` only observes database facts. It must not create keys, open writable runtime sessions, repair settings, or decide upgrade routes.
- `startup_upgrade_plan.rs` maps probe facts to ordered steps or typed recovery. It is the only policy owner.
- `startup_upgrade_executor.rs` executes supplied steps. It must not probe, plan, or add schema-specific route decisions.
- Durable step implementations own their local transaction, backup, journal phase, and postconditions.
- Recovery UI consumes typed recovery reasons. It must not classify backend failures by matching error strings.

## Normal Schema N To N+1

For an ordinary schema change:

1. Add exactly one append-only SQL migration file.
2. Update compatibility metadata inside that migration.
3. Add or update the postcondition that proves the schema change.
4. Add focused coverage for `N -> N+1`.
5. Keep the frozen schema `15 -> latest` fixture route passing.
6. Update release documentation to declare the new latest schema.
7. Run architecture and release gates.

Expected production change budget for a normal schema release:

| Required | Usually Forbidden |
|---|---|
| `00NN_*.sql` | `src-tauri/src/lib.rs` startup orchestration |
| Postcondition and tests | `startup_probe.rs` control flow |
| Frozen schema15 fixture expectation | `startup_upgrade_executor.rs` route logic |
| Release declaration | Recovery UI routing policy |

If a normal schema migration requires editing startup orchestration, stop and update the design first. That is no longer a normal schema change.

## Secret Format Changes

Secret-format changes are not ordinary SQL-only migrations. They require a typed transition with:

- key identity precondition;
- verified backup or equivalent durable safety boundary;
- journal phase for interruption handling;
- decryptability postcondition;
- typed recovery reason for every known failure;
- schema15 fixture coverage that proves ordering from structural schema to secret transition.

Do not hide secret conversion inside a generic schema migration or normal service startup.

## Legacy Data Cleanup

Legacy plaintext or alias repair may exist only inside an explicit upgrade/import transition. It must not remain in normal startup after the transition is complete.

For local access keys:

- schema baseline conversion owns legacy plaintext import;
- empty or insecure placeholder values must be replaced with a freshly generated `sk-local-*` key;
- normal settings service may create a key only for a proven fresh/empty state;
- normal settings service must reject non-empty unmigrated plaintext.

## Review Rejection List

Reject a change when any of these appear without a design update and explicit tests:

- `if schema == N` in startup coordinator or executor;
- executor calls probe or planner;
- existing database path creates or replaces the active device key;
- normal startup calls legacy repair/import code;
- migration succeeds without a postcondition;
- historical migration checksum drift is accepted by only updating a manifest;
- known recovery failure crosses module boundaries as an untyped string;
- frontend recovery logic parses backend error messages.

## Required Gates

Before merging a schema-upgrade change, run at minimum:

```powershell
pnpm verify:fast
cargo test --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade -- --nocapture
```

Before release qualification, run the release gate documented in `docs/release/SCHEMA15_UPGRADE_RECOVERY.md`.
