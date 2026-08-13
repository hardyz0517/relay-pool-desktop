# Schema 15 Upgrade Debt Closeout

Date: 2026-07-31

Status: cleanup complete, source-qualified, and production-architecture-qualified. Full release signing and bundling is a publish-time gate, not a blocker for this cleanup closeout.

## Summary

The schema 15 upgrade debt cleanup route is implemented around the fixed startup protocol:

```text
read-only probe -> pure planner -> executor(plan) -> postconditions -> ready | typed recovery
```

D-01 through D-09 are marked closed in `docs/audits/2026-07-31-schema15-upgrade-debt-manifest.json`. The RustSec advisory gate passes when PowerShell is run through the local proxy at `http://127.0.0.1:7890`. The release version contract also passes when `RELAY_POOL_RELEASE_TAG=v0.3.3` is set. Because this cleanup is not being published as a release in this turn, Tauri release signing credentials are recorded as a publish-time requirement rather than a cleanup blocker.

## Additional Fix Applied

During release qualification, the full Rust test suite exposed an inconsistent secret masking contract:

- NewAPI conformance expected `sk-p********7890`.
- Production `mask_secret()` still emitted `sk-...7890`.

The production mask contract now keeps four leading characters, eight fixed `*` characters, and four trailing characters. This keeps the output non-secret while aligning NewAPI, provider conformance, vault, and station-key DTO behavior.

Modified files:

- `src-tauri/src/models/secrets.rs`
- `src-tauri/src/services/secrets/vault.rs`
- `src-tauri/src/ipc/dto/station_keys.rs`

## Verification Evidence

Passed:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test provider_conformance -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
pnpm verify:fast
cargo deny --manifest-path src-tauri/Cargo.toml --config output/architecture-scale/generated/deny.toml --target x86_64-pc-windows-msvc --offline check advisories bans licenses sources
```

Focused checks that passed while diagnosing full-suite failures:

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test provider_conformance -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml standard_observation_gate_rejects_memory_or_queue_contract_violations -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml schema_16_generation_two_runs_secret_baseline_before_opening_runtime -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml mask_secret_keeps_prefix_and_suffix_only -- --nocapture
```

`cargo test --locked --manifest-path src-tauri/Cargo.toml` was also rerun with output redirected to a local temp log and returned `EXIT_CODE=0`; the log showed all Rust unit, integration, and doc-test result lines as `ok`.

## Release Gate Notes

Previously failed until the PowerShell proxy was set:

```powershell
pnpm verify:release
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-advisories.ps1
```

Failure details:

```text
cargo deny failed while fetching https://github.com/RustSec/advisory-db
fatal: unable to access 'https://github.com/RustSec/advisory-db/': Recv failure: Connection was reset
fatal: unable to access 'https://github.com/RustSec/advisory-db/': Could not resolve host: github.com
```

Resolved by running release verification with:

```powershell
$env:HTTP_PROXY='http://127.0.0.1:7890'
$env:HTTPS_PROXY='http://127.0.0.1:7890'
$env:ALL_PROXY='http://127.0.0.1:7890'
```

The cached offline advisory/license/source check passes, so the observed blocker is the online advisory database refresh required by the release gate, not a known local dependency policy violation.

Resolved release tag blocker:

```text
RELAY_POOL_RELEASE_TAG is required for a tagged release
```

The source version is `0.3.3`, so the release gate expects:

```powershell
$env:RELAY_POOL_RELEASE_TAG='v0.3.3'
```

Publish-time signing requirement:

```text
TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH is required for release bundling
```

## Publish-Time Work

1. Provide the release signing key through one of Tauri's expected environment variables. Prefer `TAURI_SIGNING_PRIVATE_KEY_PATH` so the private key is not pasted into shell history.
2. Rerun with the local proxy, release tag, and signing key:

```powershell
$env:HTTP_PROXY='http://127.0.0.1:7890'
$env:HTTPS_PROXY='http://127.0.0.1:7890'
$env:ALL_PROXY='http://127.0.0.1:7890'
$env:RELAY_POOL_RELEASE_TAG='v0.3.3'
$env:TAURI_SIGNING_PRIVATE_KEY_PATH='<absolute-path-to-release-private-key>'
pnpm verify:release
```

3. Only if that command passes, mark the actual release record as full-release-qualified.

## Current Conclusion

The upgrade direction is mature and intentionally not heavy: future ordinary schemas should add an append-only migration, postcondition, fixture coverage, and release evidence without modifying top-level startup policy. The schema15 cleanup itself is closed; Tauri signing remains a normal publish-time requirement.
