# Portable Migration Crypto Format ADR

Status: Accepted for implementation; branch capability approved

Date: 2026-07-29

Related specification: [`../../proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md`](../../proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md)

## Context

Relay Pool Desktop needs a Windows-first cross-device migration package that can move encrypted local data to a different Windows user and machine without exporting the source device key. The implementation contract and dependency choices are qualified by this ADR. On 2026-07-30, the repository owner approved enabling the capability on the `codex/cross-device-encrypted-migration` branch; release promotion still requires the smoke checklist and signed bundle evidence for the exact release revision.

## Decision

`.rpd-move` v1 uses the standard binary age v1 format with passphrase encryption. The cleartext inside the age stream uses this deterministic framing:

```text
RPDMOVE1
manifest_length_be_u32
manifest_json_utf8
transport_key_32_bytes
portable_sqlite_length_be_u64
portable_sqlite_bytes
portable_sqlite_sha256_32_bytes
```

The manifest is strict JSON with closed top-level fields. Extension data is allowed only under bounded `extensions` objects declared by the compatibility registry.

Text encodings and identifiers are fixed as follows:

- Binary package: standard age binary, not ASCII armor.
- Base64 fields: RFC4648 standard Base64 with explicit padding rules per field.
- Versions: SemVer strings for application/format policy versions, integer schema/profile versions where specified.
- Resource and idempotency IDs: canonical UUIDv7 strings.
- Times: RFC3339 UTC strings with `Z`.

The reader compatibility registry must select readers by exact format, portable schema profile, database generation, schema range, export policy version, secret encryption version, and required features. V1 readers have a 24-month support obligation after the first release that enables the feature.

## Dependency Decision

The Task 0 spike qualifies these exact crate versions:

- `age = "=0.12.1"` for standard age passphrase encryption.
- `hmac = "=0.13.0"` for process-local idempotency digests.
- `static_assertions = "=1.1.0"` as a dev-only dependency for later negative trait checks.

The `age` crate is still pre-1.0. It is accepted here because the spike proved the required passphrase envelope behavior, including streaming authenticated EOF and `scrypt::Identity::set_max_work_factor` rejecting excessive work factors before KDF derivation. The project must not loosen these dependency requirements to wildcards or ranges without a new ADR update and spike run.

## KDF Limits

The implementation must create age scrypt recipients through `age::scrypt::Recipient`, not by hand-writing age headers. The importer must decrypt through an identity configured with an explicit maximum accepted work factor. The Task 0 spike uses `set_max_work_factor` and confirms `DecryptError::ExcessiveWork` is returned before scrypt derivation for malicious headers.

The production adapter will centralize the accepted maximum in `PortableMigrationLimitsV1`. UI progress must treat KDF as indeterminate start/end work and must not synthesize a misleading percent.

## Security Gate

Until `docs/SECURITY_EXPORT_IMPORT.md` is formally updated and approved, portable secret migration capability must report disabled with `security_policy_not_approved`, and all start commands must fail closed with a stable `feature_unavailable` public error. After approval, the capability may be enabled only when platform and local data-store preconditions are also satisfied.

Default export semantics remain unchanged: default exports do not include raw secrets or encrypted ciphertext.

The `codex/cross-device-encrypted-migration` branch has an explicit owner approval record dated 2026-07-30, so `SECURITY_POLICY_APPROVED` may be set to `true` on that branch. Approval enables the branch capability; it does not by itself satisfy release qualification, signed bundling, or two-machine smoke evidence.

## Release Qualification

Before the first release that enables portable migration, the same source revision must have:

- completed `docs/release/PORTABLE_MIGRATION_SMOKE_CHECKLIST.md`;
- passing full and release shared verifier runs, including the portable migration integration gates;
- a canary/artifact audit proving that no real package, local database, key, cookie, token, or unredacted screenshot was committed;
- an explicit decision that the 24-month v1 reader support window has started.

## Rollback

If `age 0.12.1` is later found unsuitable, roll back to this branch point by removing the direct `age`, `hmac`, and `static_assertions` additions, deleting the age envelope adapter and tests introduced after Task 0, and disabling the feature gate. The released `.rpd-move` reader obligation starts only after a release ships with the approved capability enabled.
