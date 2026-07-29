# Portable Migration Smoke Checklist

Portable migration must remain disabled until this checklist is completed for the same source revision that will be released. Do not attach real `.rpd-move` packages, real Station Keys, cookies, local databases, or unredacted screenshots to the repository.

## 1. Preconditions

- Source revision:
- Build artifact revision:
- Reviewer:
- Date:
- Security policy approval record:
- Confirmation that `SECURITY_POLICY_APPROVED` is intentionally set for the reviewed release:
- Confirmation that default export and local backup semantics remain unchanged:

If the security policy approval record is absent, stop here. The feature must remain disabled and release notes must not advertise portable migration support.

## 2. Two-machine matrix

Run the matrix with disposable Windows 10/11 virtual machines or isolated Windows user profiles. Record only fixture IDs, redacted screenshots, command outputs, and pass/fail notes.

| Case | Source | Target | Data directory | Transport | Required evidence | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Win10 to Win11 default path | Windows 10 VM/profile | Windows 11 VM/profile | default | USB drive | export, copy, import, target app starts | |
| Win11 to Win10 default path | Windows 11 VM/profile | Windows 10 VM/profile | default | LAN share | export, copy, import, target app starts | |
| Custom data directory | Windows 11 VM/profile | Windows 11 VM/profile | custom absolute path | USB drive | import honors target directory choice | |
| Non-ASCII path | Windows 11 VM/profile | Windows 11 VM/profile | path containing non-ASCII characters | LAN share | package and activation succeed | |
| Long path | Windows 11 VM/profile | Windows 11 VM/profile | long nested path near Windows path limits | local removable drive | package and activation succeed | |
| Cloud-synced transport | Windows 11 VM/profile | Windows 11 VM/profile | default | cloud drive file copy | complete file only, no partial import | |

## 3. Functional checks

- Real Station Key request succeeds on the source before export.
- Target import rebuilds secrets under the target device key; the source device key is not present in the package or target database.
- Target Station Key request succeeds after import.
- Website login sessions that are not portable require user re-authorization on the target.
- CCSwitch or the connected local-client configuration is updated to the new target Local Key or local endpoint as applicable.
- Wrong migration password returns the approved public error without revealing whether keys, cookies, or rows exist.
- Truncated, renamed, and partially copied packages fail closed.
- Re-import idempotency and duplicate-package behavior are documented.

## 4. Activation fault checks

For each phase, force-kill the app process and then restart it. The app must recover deterministically without serving from a partially replaced database.

| Phase | Required result | Result |
| --- | --- | --- |
| Prepare before verified backup | original data remains active | |
| Prepare after verified backup | original data remains active or recovery resumes safely | |
| Replace before commit barrier | original data remains active or import rolls back | |
| Replace after commit barrier | recovery finishes or reports result-unknown with no partial service | |
| Rollback after failure | original data is restored | |

## 5. Automated release gates

Run the shared release gates on the same revision:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm.cmd run generate:bindings
pnpm.cmd run architecture:commands
pnpm.cmd run architecture:security
pnpm.cmd run architecture:dependencies
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-advisories.ps1
pnpm.cmd run test
pnpm.cmd run build
pnpm.cmd run verify:full
pnpm.cmd run verify:release
pnpm.cmd run tauri:build -- --target x86_64-pc-windows-msvc
```

The portable migration integration gates included in `verify:full` and `verify:release` are:

- `portable_migration_e2e`
- `portable_migration_faults`
- `portable_migration_malicious`

The 1 GiB streaming performance harness is release qualification evidence, not a default shared verifier step:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-portable-migration-performance.ps1
```

Attach the generated JSON report path and summary, not the temporary dataset or package.

## 6. Artifact and canary audit

Run:

```powershell
git status --short
git diff --check
rg -n "RPD_TEST_|sk-|Bearer |refresh_token|access_token|cookie" docs src src-tauri scripts -g "!src-tauri/tests/fixtures/portable-migration/**"
```

Every hit must be manually classified as a field name, redaction rule, fixed test canary, or documentation warning. Any real secret, local database, real package, or unredacted screenshot blocks release.
