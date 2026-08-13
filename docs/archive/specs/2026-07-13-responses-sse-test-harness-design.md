# Responses SSE Test Harness Design

## Goal

Make `forward_responses_request_streams_with_sse_accept_header` deterministic on Windows without changing production proxy behavior.

## Root Cause

The test upstream stops reading as soon as it sees the HTTP header terminator. It can therefore close the socket while part of the POST body declared by `Content-Length` remains unread.

On Windows, closing a socket with unread inbound data can reset the connection. The upstream client reports that reset as a transport failure, and `forward_responses_with_fallback` correctly converts the exhausted candidate list into HTTP 502. Whether the headers and body arrive in one read makes the test timing-dependent.

The passing Chat SSE test does not stop at the header terminator, which explains the behavioral difference. The tray behavior changes do not participate in this request path.

## Design

The fake upstream inside the failing test will reuse the existing `read_http_request` parser. That parser reads the header, parses `Content-Length`, and consumes the complete request body before returning a `ParsedRequest`.

The test server will inspect the parsed lowercase `accept` header:

- Requests without `text/event-stream` receive the existing buffered JSON probe response.
- Requests with `text/event-stream` receive the existing SSE response and finish the server thread.

The existing two-request upper bound remains in place so the fixture can support a probe followed by the real streaming request. No production forwarding, routing, retry, or socket code changes.

## Testing

The currently failing test is the regression RED. After changing the fake upstream reader:

- Run the focused test repeatedly to confirm it no longer depends on packet timing.
- Run the complete Rust test suite with no skipped tests.
- Re-run `cargo check`, `cargo fmt --check`, and the existing frontend build before branch integration.

## Out of Scope

- Changing `forward_responses_request` or fallback behavior.
- Changing application proxy configuration.
- Changing tray or window lifecycle behavior.
- Refactoring other ad hoc HTTP test servers.
