# ADR 0007: CI, security gates and artifact policy

## Status

Accepted with Stage 0 blockers. The repository currently has only a tag-triggered `release.yml`, production `csp: null`, a broad capture URL shell and no shared architecture verification entrypoint. Stage 0 does not pass until Task 2 installs the declared gates.

## Context

Many current contract tests inspect source text. The release workflow is the first comprehensive automation point, so structural and security drift may survive until tagging. Generated, build and test outputs are also spread across several roots and can pollute watchers or indexes.

## Decision

CI is fail closed and repository-owned. `scripts/verify.ps1` is the only orchestration entry, with `fast`, `full` and `release` profiles. PR and release workflows call it rather than duplicating command lists. Node installs use `pnpm install --frozen-lockfile`; Cargo commands use `--locked`. GitHub Actions remain pinned by commit SHA.

Use standard tools first:

- TypeScript direct-import rules: ESLint flat config with exact locked dependency versions;
- resolved TS graph/barrel/dynamic-import/fan-out rules: TypeScript Compiler API using the repository's locked TypeScript;
- Rust source module rules: repository `syn` visitor compiled as tests, with real target cfg from Cargo metadata;
- macro command truth: compiled registry/serialization fixtures, not source guessing;
- Rust advisories/licenses/sources: `cargo-deny` installed at an exact version with checksum/version verification;
- format/lint/build: locked pnpm, rustfmt, clippy, Cargo test/check/build and Tauri build.

The Task 2 tool baseline is frozen as follows. Existing resolved versions come from the current lockfiles; new tool versions were queried from their registries on 2026-07-23 and must be written into the lockfile/installer before the Stage 0 gate runs.

| Tool | Exact version | Installation/identity check |
|---|---:|---|
| Node in CI | `22.13.0` | `node --version`; GitHub setup action remains commit-SHA pinned |
| pnpm | `11.7.0` | `packageManager` plus `pnpm --version` and frozen lockfile |
| Rust release toolchain | `1.95.0-x86_64-pc-windows-msvc` | pinned rust-toolchain action input plus `rustc --version` and `cargo --version`; run an additional MSRV check at `1.89.0` |
| TypeScript Compiler API | `5.9.3` | resolved `pnpm-lock.yaml` plus `pnpm exec tsc --version` |
| Vite | `6.4.3` | resolved lockfile plus `pnpm exec vite --version` |
| Vitest | `3.2.7` | resolved lockfile plus `pnpm exec vitest --version` |
| `syn` architecture visitor | `2.0.118` | Cargo.lock checksum plus locked Cargo test build |
| ESLint | `10.7.0` | exact devDependency, frozen pnpm lock and `pnpm exec eslint --version` |
| `@eslint/js` | `10.0.1` | exact devDependency and frozen pnpm lock |
| `typescript-eslint` | `8.65.0` | exact devDependency and frozen pnpm lock |
| `cargo-deny` | `0.20.2` | `cargo install cargo-deny --version 0.20.2 --locked`; crates.io checksum is verified by Cargo; require `cargo deny --version` exact match |

ESLint and `cargo-deny` are not installed in the current repository/toolchain snapshot. Their registry metadata is compatible with the declared baselines (`eslint 10.7.0` supports Node `^22.13.0`; `cargo-deny 0.20.2` declares Rust `1.88.0`, below the crate's `rust-version = 1.89`). The current workflow also asks for floating Rust `stable` and Node `22`, so it does not yet satisfy the exact versions above. These are **Stage 0 blockers until Task 2 locks, installs and executes them**; the table records approved versions, not a claim that the gate already passes. Changing any listed major/minor tool version requires fixture compatibility evidence and an ADR amendment.

No CI script may run unversioned `cargo install`, fetch `latest`, or execute an unchecked downloaded binary. Tool versions and installation checks live in one manifest/script. Advisory exceptions require ecosystem, package, advisory id, applicability, owner, approval date, expiry date and rationale; global ignores and expired entries fail.

Custom architecture parsers are supplementary fitness functions. Fixtures must cover alias, glob, re-export, path mapping, cfg/cfg_attr, same-name symbol, dynamic import, macro registry and stale allowlist bypasses. Unknown syntax fails or needs an exact owner/expiry exception. Existing source-regex tests may remain only for literal UI/log string contracts; they cannot assert ownership, call graphs or import boundaries.

All generated/build/benchmark artifacts live below `output/<purpose>/`, except committed generated source contracts. Git, Vite watcher, CodeGraph and test discovery share exclusion parity. Every release artifact records source revision, dirty flag, toolchain, profile, target triple and SHA-256. Verifiers receive an explicit artifact path.

Tauri security is a hard gate: production CSP is non-null and forbids remote script, `unsafe-eval` and arbitrary main-window navigation; main, capture and preview capabilities are separate. The current capture `http://*`/`https://*` capability is only a shell and every capture invoke must also validate window label, station id, endpoint revision and exact origin. A release fails if demo entry is reachable, capture can invoke main commands, ACL differs from the compiled registry, CSP is null or an exception lacks owner/expiry.

Current security/build debt has fixed owners: Task 8 removes production `csp: null` and physically separates production/demo entry graphs; Task 17 narrows capture application authorization and exit lifecycle; Task 2 installs fail-closed detection; Task 28 refuses release until all three are resolved. These entries cannot be extended beyond their owner task or accepted as permanent baseline.

## Alternatives

- Keep tag-only release checks: rejected because feedback is too late and does not protect mainline.
- Use regex tests for architecture: rejected because aliases, cfg, macro expansion and re-exports bypass them.
- Depend on CodeGraph as CI truth: rejected because it is an excellent development index but not the locked build/compiler authority.
- Let workflows contain bespoke command lists: rejected because PR/release behavior drifts.

## Consequences

PRs become stricter and may take longer. Parser maintenance is bounded by using standard compiler/lint facilities first. Security and provenance become reviewable release evidence. Existing debt is allowed only through expiring manifests and cannot grow.

## Rollback

A faulty supplemental parser may be reverted to the last pinned version with its fixtures, but required boundary coverage must remain fail closed through a replacement gate. PR/release cannot bypass the shared entrypoint. Artifact or security gates cannot be disabled to publish; release waits for a corrected tool or explicit ADR revision.

## Verification

- PR workflow runs frozen frontend install, lint/test/build, Rust fmt/clippy/check/test, architecture, advisory and generation checks;
- release calls the same entry and adds locked release build, Tauri bundle, signing, explicit artifact scan and provenance;
- fixture suites demonstrate every listed bypass and stale exceptions fail;
- repeated builds from the same staged revision identify provenance and generated-contract drift;
- source tree scan finds no unapproved output roots and ignore policies agree;
- parsed production bundle/config proves non-null CSP, capability isolation, exact-origin application owner and no demo reachability.
