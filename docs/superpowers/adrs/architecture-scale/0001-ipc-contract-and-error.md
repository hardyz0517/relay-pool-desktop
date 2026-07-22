# ADR 0001: IPC contract and public error

## Status

Accepted. The `specta`/`tauri-specta` spike did not pass. Task 3 will use the narrow repository-owned build-time generator defined below; generator selection is no longer a Stage 0 blocker.

## Context

The current frontend calls Tauri commands through handwritten strings and duplicated TypeScript types. Rust commands also expose inconsistent string errors. This permits command registration, capability authorization and frontend bindings to drift independently.

The 2026-07-23 evidence snapshot is:

- `src-tauri/Cargo.toml` uses Tauri 2 and Rust edition 2021 with `rust-version = "1.89"`.
- neither `specta` nor `tauri-specta` is present in `Cargo.toml` or `Cargo.lock`;
- `cargo info` resolves both candidates to `2.0.0-rc.25`; their published metadata reports no Rust MSRV;
- downloaded `tauri-specta 2.0.0-rc.25` source contains a TypeScript `Channel<T>` mapping path, but this is implementation evidence only, not a repository compatibility result;
- no repository spike yet proves Tauri command export, serde rename, enums, nullable values, `tauri::ipc::Channel`, Windows CI or byte-for-byte deterministic output.

Consequently the candidate generator maturity requirement did not pass. A release-candidate version, source inspection and documentation examples are not sufficient evidence for a long-lived contract boundary. Neither candidate will be added by this upgrade.

## Decision

Rust command input, output, event and public error types are the only authority. A compiled command registry owns command identity and drives or verifies all three projections: Tauri registration, capability/ACL authorization and generated TypeScript bindings.

All public failures use a versioned `CommandError { code, message, details, correlation_id, retryability }`. `code` and the schema of `details` are stable machine contracts. UI code may display `message` but must never branch on it. Internal errors and secrets are mapped and redacted at the command boundary.

Generated files are build-time artifacts committed to the repository. Their header contains the repository generator format version, IPC contract version and a canonical SHA-256 contract hash. Two consecutive generations from the same locked revision must be byte-identical. CI regenerates and requires zero diff.

Task 3 implements a narrow repository-owned generator with these fixed boundaries:

- `scripts/generate-bindings.mjs` is only an orchestration wrapper; the contract extractor is Rust build/test tooling and never runs in the application runtime.
- The extractor reads only the explicitly registered public IPC DTO/error modules and command descriptors. It does not scan the whole repository or infer commands from arbitrary source text.
- Its accepted Rust grammar is deliberately closed: named structs, explicitly represented unit/newtype variants, tagged enums, `Option`, scalar primitives, strings, bounded vectors/maps and references to other registered public IPC types. It implements only the serde rename/tag/rename-all rules used by those public contracts.
- Unsupported type shapes, attributes, generics, cfg-dependent ambiguity, duplicate exported names or unregistered referenced types are hard generation errors. The generator must be extended with a failing fixture before such a shape can enter a public contract.
- Ordinary command names, inputs, outputs and public errors come from the compiled command registry. Generated TypeScript contains their types and typed invoke wrappers; feature code cannot handwrite these strings.
- Tauri `Channel` is one explicit transport descriptor in the registry. The generator emits `Channel<RegisteredEvent>` at the bridge boundary, while one repository-owned adapter handles subscription/cancellation mechanics. It cannot be used to bypass generated event payload types.
- The generator supports only Relay Pool Desktop's public command DTO/error/Channel contract. It is not a general Rust-to-TypeScript package, runtime schema system or reusable framework.

Each supported construct has a golden pair: Rust `serde_json` serialization/deserialization fixtures covering every struct field, rename, nullable state and enum variant, plus the expected generated TypeScript declaration/invoke snapshot. Generation is stable-sorted by canonical type and command identity, uses normalized LF output and contains no timestamps or absolute paths. CI generates twice into clean temporary roots, compares SHA-256, then requires zero diff against `src/lib/bridge/generated.ts`.

Breaking changes require a versioned command/event or an IPC contract version increase. A rename, removal, semantic reinterpretation, required field addition or error-detail schema change is breaking. Additive optional output fields are non-breaking. Documentation-only edits must not change the hash.

Streaming is a distinct boundary. If `Channel` cannot be generated safely, one typed handwritten streaming adapter may map generated payload/event DTOs to Tauri transport. It may not reintroduce handwritten ordinary command names or DTOs.

The rejected `specta`/`tauri-specta` path cannot be reconsidered inside Task 3 or an unrelated migration shard. A future replacement requires a separate ADR, pinned stable versions, maintenance/MSRV/Windows/Tauri evidence, full Channel coverage, two-run determinism and dual-generator golden equivalence before cutover. No generator is a runtime dependency or domain abstraction.

IPC command deadlines are defined in `architecture-scale-capacity-budgets.json`: 10 seconds for reads, 15 seconds for mutations, and 2 seconds for operation admission/status/cancel commands. Long work returns an operation id and is never kept alive by extending an IPC call indefinitely.

## Alternatives

- Handwritten TypeScript DTOs and command strings: rejected because drift is already the primary failure mode.
- Runtime reflection or runtime schema negotiation: rejected because it adds startup failure modes and makes packaged assets non-reproducible.
- Use `specta`/`tauri-specta 2.0.0-rc.25`: rejected for this upgrade because the spike did not establish a mature stable/MSRV/Windows/deterministic contract.
- Keep generator selection open until Task 3: rejected because Stage 0 must freeze the implementation boundary before feature migration.
- Generate all code from TypeScript: rejected because Rust is the execution and serialization authority.

## Consequences

Command changes become more deliberate and CI becomes stricter. Ordinary IPC receives compile-time client types and stable error handling. Streaming retains one small, typed transport adapter. The repository owns a deliberately limited generator and must reject unsupported constructs instead of gradually becoming a general code-generation framework.

## Rollback

A migrated command group may roll back to its versioned legacy adapter, but generated clients must not silently fall back to old business behavior. Rollback restores the previous registry, generated artifact and contract hash together. Published contract versions are not reused for incompatible schemas.

## Verification

- serialization fixtures cover every public DTO and error variant;
- compiled registry, Tauri registration and capability manifests have no missing or extra command;
- two clean generations have identical SHA-256 output and the checked-in file has zero diff;
- unknown error codes map to a safe generic frontend state without parsing text;
- a contract mismatch at startup fails closed and cannot enter DemoBackend;
- every accepted generator grammar construct has golden serde and TypeScript fixtures, and every unsupported construct fails closed;
- `specta`/`tauri-specta` is absent from the Task 3 dependency graph unless a later independent ADR supersedes this decision.
