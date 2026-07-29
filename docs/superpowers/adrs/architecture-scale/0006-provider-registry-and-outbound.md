# ADR 0006: Provider capability registry and async outbound

## Status

Accepted. Migration cannot begin until the shared async outbound policy tests are green; synchronous `ureq` remains tracked legacy debt and may not appear in new production code.

## Context

Provider behavior is spread across collectors, connectivity, authorization and remote-key services. String dispatch and synchronous HTTP make a new provider modify unrelated orchestrators and duplicate timeout, proxy, redirect, retry and credential policy.

## Decision

Use a compile-time closed `ProviderRegistry` keyed by `ProviderKind`. An entry contains a descriptor and only the capability implementations it supports, such as collector, connectivity, remote-key and authorization. A missing capability returns typed `Unsupported`. The registry never performs network calls, retries, persistence or task scheduling.

Each provider module may share private auth/client/parser/mapping helpers. Capability drivers build typed requests, call the neutral `AsyncOutboundClient`, parse provider payloads and return domain evidence. Orchestrators depend only on the capability they need and do not match provider names or error strings.

`AsyncOutboundClient` uses pooled `reqwest::Client` instances keyed by stable proxy route and transport policy. It owns connect/request timeout, parent deadline, redirect validation, HTTPS downgrade rejection, response-size limits, retry classification and redaction. It strips authorization, cookie and provider-secret headers on every cross-origin redirect. Clients are not built per request.

Credentials are opaque secret wrappers acquired through short-lived provider accessors. Debug/serialize/display are redacted; raw secrets never enter DTOs, metrics, logs, fixtures or operation progress. Refresh is single-flight per credential identity. Cancellation or redirect cannot duplicate a refresh or leak headers.

Limits are: 8 total provider endpoint requests, 4 per station/provider operation, 8 MiB successful response body, 64 KiB error body and at most 2 attempts within one 30-second parent operation budget. Queue wait, connect, execution and retry all consume remaining time; no nested layer resets the budget. These limits exclude the already-owned local OpenAI proxy lifecycle.

## Alternatives

- One universal `Provider` trait: rejected because capability growth couples unrelated implementations.
- Runtime plugin ABI/dynamic loading: rejected because the desktop product has no independent plugin deployment requirement.
- Provider-specific HTTP clients and retry stacks: rejected because policy and credential safety drift.
- Put synchronous HTTP in the blocking pool: rejected because it consumes scarce blocking capacity and still lacks async cancellation.

## Consequences

Provider modules become locally extensible and transport behavior becomes consistent. Some provider-specific fallback behavior must be represented explicitly as typed evidence. The central registry remains a small composition list, not a business service.

## Rollback

Rollback is per capability driver. Restore the old orchestrator adapter without adding new `ureq` use or string dispatch. Shared outbound policy is not bypassed by a migrated provider. Credential and redirect protections cannot be rolled back independently.

## Verification

- conformance suites run every registered capability against success, auth failure, unsupported, malformed payload, timeout, cancellation and retry fixtures;
- tests prove per-provider and global fan-out, body limits, client reuse and shared parent deadlines;
- cross-origin redirect, HTTPS downgrade and secret-canary tests fail closed;
- registry gate detects duplicate kind, unregistered driver, string dispatcher and capability dependency leakage;
- production dependency/import gate reaches zero `ureq` after Task 22;
- adding a reference provider changes only its module, registry entry and fixtures.
