# Relay Pool Desktop Documentation

This directory contains the current product, architecture, security, and release documents for Relay Pool Desktop. Historical files under `docs/archive/` are for traceability only and must not be used as the sole implementation source.

## Current entry points

- [Project plan](PROJECT_PLAN.md): product scope, current phase, and domain terminology.
- [Product model](PRODUCT_MODEL.md): core concepts and user-facing mental model.
- [Security export/import policy](SECURITY_EXPORT_IMPORT.md): default export, local backup, same-device relocation, and portable migration boundaries.
- [Cross-device encrypted migration spec](proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md): implementation contract for password-protected `.rpd-move` packages.
- [Portable migration crypto ADR](superpowers/specs/2026-07-29-portable-migration-crypto-format-adr.md): cryptographic format and dependency decision.
- [Portable migration smoke checklist](release/PORTABLE_MIGRATION_SMOKE_CHECKLIST.md): required two-machine and release qualification evidence before enabling the feature.

## Portable migration release status

Portable migration is not currently approved for release. The code path must remain behind the disabled security gate until the security policy is formally approved and the smoke checklist is completed for the exact release revision.

Default exports and local backups are not portable migration packages. Do not put real `.rpd-move` packages, SQLite databases, secrets, logs, or screenshots containing secrets into the repository.
