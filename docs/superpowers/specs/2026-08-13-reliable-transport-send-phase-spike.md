# Reliable Transport Send-Phase Spike

Status: accepted compatibility boundary. This document records the formal
three-state transport conclusion for the current scope of
`2026-08-13-upstream-error-classification-retry-closure.md`.

## Current Evidence

The production upstream owner is `services/proxy/upstream.rs`. It uses
`reqwest::RequestBuilder::send()` because the current product contract requires
direct, system, HTTP-proxy, SOCKS-proxy, Rustls, HTTP/2, connection timeout,
and streaming-response support from one client pool.

At this abstraction, `reqwest` provides two authoritative outcomes:

- an `is_connect()` failure is recorded as `NotConnected`;
- receiving `Response` headers is recorded as `ResponseStarted`.

Every other send failure is `Unknown`. In particular, polling a request body,
or observing a peer close after it accepts a TCP connection, does not prove
whether the peer accepted headers or any body bytes. The regression
`upstream_disconnect_after_receiving_request_stays_unknown` uses a local TCP
fixture that reads request bytes and closes before a response; production still
records `Unknown`.

`Unknown` is intentionally non-replayable for a non-idempotent operation. The
execution replay gate consumes the phase through
`RequestSendPhase::definitely_no_request_bytes_sent`; it does not infer a
pre-write state from a downstream precommit result or a body-stream poll.

## Why A Direct Hyper Replacement Is Not Yet Accepted

The workspace lockfile has transitive `hyper-rustls` and `tokio-rustls`, but
the manifest only declares the server-oriented `hyper` / `hyper-util` features.
There is no existing client transport owner for HTTP CONNECT, SOCKS5/SOCKS5h,
system proxy discovery, TLS configuration, pooling, or HTTP/2. Replacing
`reqwest` solely to expose body polling would therefore regress supported
routes, and body polling still would not establish kernel/socket acceptance.

No new dependency or transport path is added by this spike. The current
fallback remains fail closed rather than presenting test-only intermediate
phases as production facts. This is an accepted product and engineering
decision for the current closure scope: future transport replacement requires
a separately approved plan, rather than being an unfinished item here.

## Required Adapter Contract Before Production Cutover

A lower-level adapter may move `ConnectedNoHeaders`, `HeadersSent`,
`BodyPartiallySent`, and `BodyFullySent` out of `cfg(test)` only when it:

1. owns connection, TLS and request serialization writes, and advances a
   request-local phase monotonically only after the relevant transport write
   future has completed;
2. distinguishes body polling from successful transport write completion;
3. preserves direct, system, HTTP, SOCKS, Rustls, HTTP/2, timeout, pooling,
   buffered response and streaming response behavior on Windows;
4. keeps unsupported proxy/protocol paths at `Unknown`, never at
   `NotConnected`; and
5. has deterministic local TCP tests for connect/TLS, headers, partial body,
   complete body, response headers and mid-stream failure. Each test must
   drive the real `ExecutionEngine` retry path and show non-idempotent replay
   stops after an uncertain or possibly accepted send.

The adapter selection must document crate license compatibility and the
resulting `Cargo.lock` change before dependency addition. Until these gates are
met, `reqwest` remains the single production transport owner and its narrow
fact set is the correct safety boundary.
