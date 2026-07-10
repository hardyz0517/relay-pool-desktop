# Channel Monitor Stream Observability Design

## Goal

Make channel-monitor request logs truthful and useful: record the monitored Key's group, actual stream mode, reasoning effort, cache-aware token usage, first-token latency, and total duration. Upgrade the request-log Token and latency cells to the compact visual treatment in the supplied references.

## Root Causes

- `run_monitor_for_key` constructs every monitor context with `stream: false`.
- The built-in Chat and Responses templates persist literal `stream: false` values.
- `RenderedMonitorRequest` retains only method, path, headers, and bytes, so request metadata is discarded after rendering.
- `run_monitor_probe` buffers a bounded response body and records only elapsed completion time; it does not observe SSE chunks or first-token latency.
- `insert_monitor_request_log` hardcodes `stream: false`, `reasoning_effort: None`, and `first_token_ms: None`.
- Monitor log group identity is obtained indirectly through route economics. A Key can have a valid `group_binding_id` while that economics projection returns no group.
- The Token and latency columns both use a generic two-line text renderer, which cannot express the requested hierarchy.

## Request Metadata

`render_monitor_request` will derive observable request metadata from the rendered JSON body, not from assumptions about the template name:

- `stream` is the rendered body's boolean `stream` value, defaulting to `false` when absent or invalid.
- `reasoning_effort` uses the existing `RequestObservation` parser, supporting both `reasoning.effort` and `reasoning_effort`.

`RenderedMonitorRequest` will carry these values beside the serialized body. This makes custom templates truthful: a custom non-streaming template remains non-streaming even though built-in templates become streaming.

## Built-In Templates

All four built-in monitor templates will render `stream: true` through the existing `{{stream}}` context placeholder. `run_monitor_for_key` will supply `stream: true`.

The two Responses templates will add:

```json
"reasoning": { "effort": "minimal" }
```

Chat templates will not force a reasoning parameter because generic OpenAI-compatible Chat endpoints frequently reject unsupported reasoning fields. A custom Chat template can still supply flat `reasoning_effort`, which the metadata parser will record.

The existing built-in-template upsert updates persisted built-ins on the next application initialization, so existing installations receive the corrected request bodies without a separate migration.

## Streaming Probe

The monitor continues to call the selected upstream Key directly. It must not route through the local proxy because doing so could select a different Key and invalidate the health check.

For streaming requests, the probe will:

1. Open the upstream response and read it incrementally.
2. Record `first_token_ms` when the first non-empty response chunk is received, matching the local proxy's current timing semantics.
3. Feed SSE bytes into the existing usage observer so the final Responses or Chat completion event can supply input, output, total, cache-read, and cache-creation Token values.
4. Retain only bounded, redacted response material for diagnostics.
5. Record total elapsed time after the stream ends.

For non-streaming custom templates, the existing buffered JSON path remains available and `first_token_ms` remains absent.

`MonitorProbeResult` will expose `first_token_ms` and cache-aware usage. Failed probes preserve any timing already observed while continuing to redact response excerpts.

## Request Log Persistence

`insert_monitor_request_log` will persist metadata from the actual rendered request and probe result:

- `stream` from `RenderedMonitorRequest.stream`.
- `reasoning_effort` from `RenderedMonitorRequest.reasoning_effort`.
- `first_token_ms` from `MonitorProbeResult.first_token_ms`.
- Token and cache fields from the observed final usage event.
- `group_binding_id` directly from `target.group_binding_id`.

Total duration remains the completed probe duration. The existing pricing calculation continues to consume normalized input/output/total Token counts.

## Request Log Presentation

### Group

The group column will prefer the current Key's human-readable `groupName`. If the Key is unavailable, it falls back to the persisted `groupBindingId`, then `未分组`. This repairs historical monitor rows visually while new rows also persist the correct binding ID.

### Token

The Token cell becomes a purpose-built compact component:

- First row: green downward arrow plus input count, purple upward arrow plus output count.
- Second row: blue database/cache icon plus cache-read count; cache-creation is shown beside it when present.
- Missing values remain `-`; counts use compact notation when large.
- Icons are fixed-size and do not change the table row geometry.

### Latency

The latency cell becomes a purpose-built compact component:

- A stable teal vertical line anchors the left side.
- Two aligned rows show muted labels `首字` and `总耗时` with teal values.
- Missing first-token data remains `-`, which is expected for historical and non-streaming custom rows.

The existing row density, right-side columns, selection state, and horizontal scrolling behavior remain unchanged.

## Tests

- Template tests prove built-ins seed streaming bodies and Responses reasoning effort.
- Renderer tests prove stream and reasoning metadata come from the rendered JSON body.
- Probe tests use a local staged SSE response to prove first-token timing, total completion, and usage/cache extraction.
- Monitor integration tests prove the selected Key's group binding, stream flag, reasoning effort, first-token latency, and usage fields reach `request_logs`.
- Frontend regression tests prove the group-name fallback and the dedicated Token/latency visual components.
- Verification runs focused Rust tests, the request-log Node scripts, `cargo check`, and `pnpm.cmd build`.

## Out Of Scope

- Backfilling or rewriting historical request-log rows in SQLite.
- Routing monitor traffic through the local proxy.
- Forcing reasoning parameters into generic Chat-compatible templates.
- Changing monitor scheduling, failure thresholds, model selection, or pricing rules.
