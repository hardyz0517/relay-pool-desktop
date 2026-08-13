# Key Connectivity Streaming Test Design

## Goal

Change the Station Key connectivity test from a non-streaming-only probe into a stream-first probe that displays real upstream deltas. If streaming is not successful, retry the same model and endpoint once with a non-streaming request before continuing through the existing protocol and model fallback order.

The test must report whether the final successful response used streaming or the non-streaming fallback. It must not simulate streaming with a frontend timer.

## Scope

This change is limited to the Key Pool connectivity test and its existing Tauri command. It preserves:

- model discovery and candidate ordering;
- Responses-first protocol selection;
- the existing conditional Responses-to-Chat-Completions fallback;
- station health result persistence;
- the current short `"hi"` prompt and output-token limit.

It does not change proxy streaming, channel monitor probes, routing, request logs, or collector behavior.

## Architecture

### Request-scoped IPC channel

The TypeScript API creates a Tauri 2 `Channel<StationKeyConnectivityTestEvent>` for each invocation and passes it to `test_station_key_connectivity`. Rust sends typed progress events through that channel from the blocking probe task.

A request-scoped channel is preferred over global Tauri events because it avoids global listener registration, event-name coordination, and cross-request filtering. The frontend also assigns each run a monotonically increasing token and ignores callbacks from stale runs after the dialog closes, reopens, or starts a newer test.

Channel delivery is observational. If the frontend closes and channel delivery fails, the backend still finishes the probe and records its result without exposing credentials or full response bodies in logs.

### Event contract

The progress event is a tagged payload with these variants:

- `attemptStarted`: identifies the model and protocol for a new streaming attempt; the frontend resets response text from any earlier protocol or model attempt.
- `delta`: contains only the newly decoded assistant text for the current streaming attempt.
- `fallback`: states that the current streaming attempt failed and that the same endpoint is being retried without streaming; it includes a short redacted reason.

The command still returns one `StationKeyConnectivityTestResult`. The result adds:

- `responseMode`: `stream` or `non_stream_fallback`, identifying the transport used by the final attempt even when that attempt fails;
- `streamFallbackReason`: the redacted reason when the final successful protocol used non-streaming fallback, otherwise `null`.

The returned `message` remains the final assistant text or the existing redacted failure summary.

## Backend Flow

For each model candidate, preserve the existing protocol decision order. Each protocol probe uses this state machine:

1. Send an `attemptStarted` event for streaming mode.
2. Send the request with `stream: true` and `Accept: text/event-stream`.
3. Require a 2xx response and a `text/event-stream` content type.
4. Decode SSE incrementally from the response reader and emit assistant-text deltas immediately.
5. Mark streaming successful only after a valid terminal signal is observed.
6. If any streaming requirement fails, emit `fallback`, then send one request to the same model and endpoint with `stream: false` and `Accept: application/json`.
7. If the non-streaming retry succeeds, return it as `non_stream_fallback`.
8. If both attempts fail, pass the combined redacted failure into the existing protocol or model fallback logic.

This means an auto/custom station may perform Responses stream, Responses non-stream, Chat stream, and Chat non-stream for one model only when each preceding attempt fails and the existing protocol fallback policy permits the next step.

Each protocol probe's duration covers its stream attempt and, when used, its non-stream attempt, including response-body reads and SSE parsing. Timing stops only after the relevant body has been consumed or the attempt has failed. Existing aggregation across protocol and model candidates remains unchanged.

## SSE Decoder

The decoder is a small, testable Rust state machine independent of HTTP and Tauri:

- accepts arbitrary byte chunks;
- supports LF and CRLF event boundaries;
- combines multiple `data:` lines in one event;
- ignores blank events, comments, keep-alives, and unknown well-formed JSON events;
- recognizes `[DONE]` for Chat Completions;
- recognizes `response.completed` for Responses;
- extracts Responses `response.output_text.delta` text;
- extracts Chat Completions `choices[0].delta.content` text;
- rejects malformed non-empty JSON data and incomplete streams;
- bounds pending undecoded bytes to prevent unbounded memory growth.

The decoder records whether it observed valid SSE data and a protocol-appropriate terminal signal. A connection close without that terminal signal is a stream failure even if partial text was received. Partial text from a failed attempt is discarded when the next `attemptStarted` or `fallback` event reaches the frontend.

## Frontend Behavior

While streaming, the console appends each `delta` directly to `displayedResponseText`. There is no interval or artificial typewriter effect.

When streaming falls back, the console clears partial output and displays a restrained status line indicating that a non-streaming retry is running. When the command resolves:

- a streaming success is labeled `流式响应`;
- a fallback success is labeled `非流式回退`, with the short fallback reason available in the console;
- a final failure uses the existing error presentation.

The test button remains disabled while the current test is active. Closing the dialog invalidates the frontend run token so late channel events cannot modify a later dialog session.

## Error Handling And Security

- API keys remain backend-only and are never included in channel events.
- Error messages pass through the existing redaction helper before reaching TypeScript.
- Raw SSE payloads and full upstream bodies are not logged or emitted.
- HTTP errors, wrong content type, malformed SSE, missing terminal events, read failures, and timeouts all trigger exactly one same-endpoint non-streaming retry.
- A failed channel send does not change the connectivity result.
- The SSE pending buffer has a fixed maximum; exceeding it fails the stream and activates fallback.

## Testing Strategy

Implementation follows RED-GREEN TDD.

Rust unit tests cover:

- request bodies and Accept modes for stream and non-stream attempts;
- Responses deltas split across arbitrary chunks;
- Chat deltas, CRLF boundaries, comments, and `[DONE]`;
- malformed JSON, oversized pending data, and missing terminal signals;
- stream success without a non-stream retry;
- stream failure followed by exactly one non-stream retry;
- preservation of Responses-to-Chat and model fallback ordering;
- duration accumulation through body consumption.

The existing Node contract test is changed first so it fails until the TypeScript API uses a Tauri `Channel`, the dialog consumes typed delta/fallback events, and the old immediate-only contract is removed.

Final verification includes the focused Node contract, focused Rust tests, the full frontend build, and the available Cargo test/check suite. A controlled local HTTP fixture verifies delayed SSE chunks and fallback request counts without depending on a third-party station. The running Tauri app is then visually checked to confirm that channel deltas update the dialog without layout regressions.

## Non-Goals

- Cancelling an in-flight backend HTTP request when the dialog closes.
- Sharing the decoder with the production proxy in this change.
- Persisting individual connectivity-test deltas.
- Adding configurable prompts, token limits, or stream policies.
