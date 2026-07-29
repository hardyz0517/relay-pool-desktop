# Security Export and Import Policy

This policy separates four data movement modes that are easy to confuse but have different security properties:

1. Default export: non-secret configuration and metadata only.
2. Local backup: database backup for the same Windows user and machine profile.
3. Same-device relocation: moving the application data directory while keeping the same protected local key material.
4. Portable migration: an explicit, password-protected package intended for a different Windows user or computer.

Portable migration is implemented only behind a disabled security gate in the current source tree. It must not be exposed in a release until a formal security policy review approves the capability and the release qualification checklist is completed.

## Default Export

Default exports do not include raw API keys, station login passwords, cookies, sessions, tokens, authorization headers, prompts, responses, or encrypted ciphertext.

Default exports may include:

- station display name
- station type
- base URL
- masked key value
- key enabled state
- routing policy metadata
- pricing and balance metadata
- request log metadata without prompt or response text

Default imports may create stations, key metadata, pricing rules, aliases, and routing settings. They do not silently overwrite existing secrets. A user must paste new secret values through the normal credential forms.

## Local Backups

SQLite database backups include encrypted secret ciphertext. A backup remains tied to the original Windows user credential store and the original device key material unless a later approved portable migration flow is used.

Persistence V2 upgrade backups can contain the complete local business database and encrypted credential ciphertext. They depend on the original Windows user credential store, are not portable exports, and must not be uploaded, attached to issues, or included in screenshots. The verified generation-1 backup is retained after a successful v0.3.1 to v0.3.2 upgrade until an explicit cleanup policy is shipped; the application does not silently delete it.

Older security-format backups may contain a plaintext Local Key from before the encrypted-secret baseline conversion. Treat those backups as sensitive secret material.

## Same-Device Relocation

Same-device relocation is allowed only when the Windows user profile and protected credential store remain available. It is not a cross-device recovery mechanism. If the old Windows credential entry is missing, copied SQLite files and old backups cannot be decrypted by design.

## Portable Migration

Portable migration, when approved, must be an explicit user action that creates a `.rpd-move` package protected by a migration password. The package must not export the source device key. The importer rebuilds data under the target device key after authenticated decryption and validation.

Current release state:

- `SECURITY_POLICY_APPROVED` remains `false`.
- Capability discovery must report the portable migration feature as disabled with `security_policy_not_approved`.
- Export/import start commands must fail closed with the stable public error `feature_unavailable`.
- No release may claim portable migration support until the security approval record and smoke checklist evidence exist for the same source revision.

User-facing constraints:

- A lost migration password is unrecoverable. The project must not add a backdoor, escrow key, or support-only recovery path.
- Old local backups remain bound to the old Windows user/device key material.
- The application does not promise physical erasure from SSDs, filesystem journals, antivirus caches, sync clients, or prior backups.
- JavaScript strings cannot be guaranteed to be zeroized; UI code must avoid retaining migration passwords longer than needed and must not persist them.
- Real packages, real keys, real cookies, screenshots with secrets, and local databases must never be committed to the repository.
