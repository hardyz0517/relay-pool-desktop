# Collector Failure Transition Design

## Problem

Collector failures use a stable dedupe key for each station and task type, but the generic change-event upsert marks every conflict as `unread` and refreshes its timestamps. A station that remains unavailable therefore recreates the sidebar unread signal on every scheduled collection.

## Desired Behavior

- The first failure for a station and collector task creates an unread `collector_failed` event.
- Repeated failures while that failure is active do not change its read status or occurrence timestamps.
- A successful or partial run after a failure resolves the matching `collector_failed` event and keeps the existing `collector_recovered` event behavior.
- A later failure for the same station and task reactivates the resolved failure event as unread with fresh occurrence timestamps.
- Failure state is scoped by both station ID and collector task type.

## Design

Keep the transition policy at the database change-event boundary, where deduplication and event status already live.

For `collector_failed` conflicts, preserve the stored status, `detected_at`, `resolved_at`, and `updated_at` while the event is not resolved. If the stored event is resolved, treat the conflict as a new failure episode: set it to unread, clear `resolved_at`, and use the new timestamps.

When a collector run transitions from failed to success or partial, resolve the matching `collector_failed` event before upserting the existing `collector_recovered` event. Derive the failure dedupe key with the same station and task normalization helper used by failure creation.

Other change-event types retain their current upsert behavior.

## Verification

Add database-level regression tests covering:

1. A failure event is marked read, then a repeated failure preserves the read status and original occurrence timestamps.
2. A failure is followed by recovery, then another failure becomes unread with new occurrence timestamps.
3. Recovery and failure transitions remain isolated by collector task type.

Run the focused Rust tests, the full Rust test suite if practical, `cargo fmt --check`, and `cargo check`.
