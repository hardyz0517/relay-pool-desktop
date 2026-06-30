# Security Export and Import Policy

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

## Secret Export

Encrypted secret export is not part of P8. If added in a later phase, it must require explicit user confirmation and password-based encryption.

## Import

Imports may create stations, key metadata, pricing rules, aliases, and routing settings. Imports do not silently overwrite existing secrets. A user must paste new secret values through the normal credential forms.

## Backups

SQLite database backups include encrypted secret ciphertext. A backup remains tied to the system keychain entry unless a later encrypted-export flow is implemented.
