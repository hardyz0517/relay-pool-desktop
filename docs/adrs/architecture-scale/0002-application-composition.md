# ADR 0002: Application composition and command facades

## Status

Accepted. Integration with the final Persistence V2 composition API remains an external precondition; this ADR does not authorize changes under `src-tauri/src/persistence/**`.

## Context

Commands currently receive broad `State<AppServices>` access. This hides dependencies, increases fan-out and lets transport code perform orchestration. The upgrade needs clearer ownership without introducing a DI container, service locator or microservices.

## Decision

The application remains a single-process modular monolith. Construction occurs in one Rust composition root. `AppServices` may exist only as a private construction bundle while objects are assembled; it must not be Tauri-managed runtime state passed to production commands.

Each command group receives a narrow application facade or read/write port containing only the use cases that group exposes. A command performs transport validation, calls exactly one facade use case, maps the result to a DTO/public error and returns. Facades may coordinate multiple domain services only for a real application use case; they must not mirror every service method or expose internal service fields.

Business dependencies belong to constructed task bodies, operation bodies and provider drivers. `TaskSupervisor`, `OperationRegistry`, `BackendClient` and provider registries cannot perform arbitrary service lookup.

Dependency direction is:

`command/runner -> application facade or port -> domain/service -> infrastructure adapter`.

Infrastructure and domain modules cannot import command modules. Cross-domain use cases are explicit application coordinators. Cycles, wildcard re-export escape hatches and broad context bags are architecture-gate failures.

Persistence V2 is consumed only through its committed public composition/read/write ports. If its final API differs from this expectation, Stage 0 must add a narrow adapter plan; the persistence implementation itself stays out of scope.

## Alternatives

- Keep `State<AppServices>` and add conventions: rejected because conventions cannot expose or bound dependency radius.
- Introduce a generic DI container/service locator: rejected because it preserves runtime lookup and weakens compile-time ownership.
- Split into microservices or a sidecar: rejected because a local desktop tool gains operational complexity without an independent deployment need.
- One facade per service with one-to-one forwarding: rejected because it merely renames the god object.

## Consequences

Composition code becomes more explicit and some adapters are temporarily required during migration. Commands and tests gain small dependency surfaces. New use cases have predictable ownership, and physical file decomposition can follow stable edges instead of moving the existing coupling.

## Rollback

Rollback is per command-group cutover. Restore the previous command registration and facade wiring together. Temporary adapters must name their caller, owner and deletion task and cannot gain new callers. No rollback may add a new `State<AppServices>` command.

## Verification

- parser-backed tests find zero production command signatures containing `State<AppServices>` after Task 7;
- command facade fan-out and public exports match the boundary manifest;
- every command calls one facade use case and has no direct persistence, HTTP or task-registry access;
- composition tests construct each facade with fakes and without a global container;
- dependency graph tests reject cycles, forbidden re-exports and locator-style generic accessors.
