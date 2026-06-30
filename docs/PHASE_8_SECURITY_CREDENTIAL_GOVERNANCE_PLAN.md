# P8 Security and Credential Governance Plan

## Goal

P8 makes Relay Pool Desktop safe enough for long-term use with real Station Keys, station login credentials, captured session metadata, collector snapshots, request logs, route logs, and proxy errors.

## Scope

P8 introduces one SecretManager and one redaction boundary. Business modules keep owning their product behavior, but they no longer store, display, log, import, or export raw secrets directly.

Import/export follows `docs/SECURITY_EXPORT_IMPORT.md`. P8 default exports never include raw secrets or encrypted secret payloads.

## Sensitive Data

- Station Key API keys
- legacy station API keys
- station login passwords
- token, cookie, session, and authorization values
- collector snapshot raw payloads
- request log error details
- route details and rejected candidates
- import/export backup payloads

## Storage Strategy

Secrets are encrypted before SQLite persistence. The app data encryption key is stored in the host OS keychain. SQLite stores ciphertext, nonce, masked value, hash, and metadata.

## Non-Goals

- no new route strategy
- no new pricing adapter
- no cloud sync
- no team permissions
- no public LAN proxy mode
- no full enterprise audit system

## Completion Standard

- raw SQLite does not contain full keys, passwords, tokens, cookies, prompts, or responses
- UI defaults to masked values
- request logs and collector snapshots are redacted before persistence
- existing plaintext credentials migrate without data loss
- local proxy remains bound to 127.0.0.1
- build, check, and library tests pass
