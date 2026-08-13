# Automatic Web Authorization Completion Design

## Problem

The existing Web authorization implementation can read HttpOnly cookies from a
Tauri capture window, verify a NewAPI session through `/api/user/self`, and
persist the encrypted cookie plus the NewAPI user id. However, the station-list
authorization action only starts the capture window. It never invokes the
finish command that performs native cookie recovery and persistence.

This differs from the existing Sub2API experience. Sub2API login responses
usually expose an access token in JSON, so the injected capture script can send
that token to `record_capture_event` and persist the session immediately. A
KamiAPI-style login establishes its reusable session with an HttpOnly cookie,
which page JavaScript cannot read. The missing piece is automatic handoff from
the observed successful login to the native cookie verifier.

## Goals

- Complete Web authorization automatically after the user logs in.
- Keep normal NewAPI/KamiAPI authorization as simple as the Sub2API flow.
- Treat native verification, not a page-side signal, as the source of truth.
- Make repeated or concurrent completion signals safe and idempotent.
- Keep provider-specific recognition behind an authorization strategy boundary.
- Preserve manual completion as a fallback, not a required step.
- Never expose cookies, tokens, passwords, or full response bodies in UI events,
  logs, snapshots, or errors.

## Non-Goals

- Replacing the existing capture-window implementation.
- Adding a KamiAPI-only command or component.
- Automatically submitting login or MFA challenges.
- Supporting arbitrary OAuth providers without a station verification strategy.
- Changing Sub2API password or access-token authentication behavior.

## Considered Approaches

### 1. Required manual completion

After opening the login window, show a main-window button that invokes the
existing finish command. This is simple but creates an unnecessary NewAPI-only
step and is easy to miss. It remains useful only as a fallback.

### 2. Page-event completion only

When injected `fetch` or `XMLHttpRequest` capture observes a successful identity
endpoint, invoke the native finish command. This avoids polling and matches the
existing capture architecture, but it can miss requests performed outside the
patched APIs or pages that do not load identity data immediately after login.

### 3. Event-driven completion with an idempotent native coordinator

Use successful captured identity traffic as the primary trigger. Route all
completion attempts through a native coordinator that deduplicates attempts,
reads the WebView cookie jar, verifies the session, persists it, and emits a
sanitized result event. Keep the existing explicit finish action as a fallback.

This is the selected approach. It keeps the common path automatic without
introducing continuous network polling, while the native coordinator provides
the reliability and extension boundary that page-side code cannot provide.

## Architecture

### Authorization strategy

An authorization strategy describes provider-family behavior without owning
window or persistence logic. The initial NewAPI cookie strategy defines:

- which captured request paths can indicate an authenticated session;
- which HTTP statuses and response shape form a completion candidate;
- the native verification endpoint;
- how to extract the stable user identity needed by later collector requests;
- the persisted session source.

KamiAPI uses the generic NewAPI cookie strategy. It must not introduce hostname
checks in the capture script or coordinator.

### Native completion coordinator

The coordinator owns one state machine per station:

```text
idle -> waiting -> verifying -> authorized
                  |             |
                  v             v
                waiting        failed
```

`failed` represents a terminal implementation or persistence error. An
unverified candidate, such as a stale or unauthenticated cookie, returns to
`waiting` so a later login event can retry.

The coordinator guarantees:

- at most one verification request per station at a time;
- duplicate completion candidates join or ignore the active attempt;
- persistence happens only after native verification succeeds;
- repeated completion after success returns the existing successful state;
- cancellation or window closure prevents new attempts;
- state events contain station id, state, error category, and timestamps only.

The coordinator does not store raw cookies in its public status. Cookie values
move directly from the WebView cookie reader to verification and encrypted
credential persistence.

### Capture-window bridge

The injected capture script continues to wrap both `fetch` and
`XMLHttpRequest`. After `record_capture_event` accepts a captured response, the
script evaluates only sanitized completion-candidate metadata: request path,
status, and whether the expected identity envelope exists.

For a candidate, it invokes an idempotent native `try_complete` command. A
page-local in-flight guard avoids redundant calls, but correctness does not rely
on that guard because navigation creates a new JavaScript context and duplicate
requests are normal.

### Main-window integration

The native coordinator emits a sanitized completion event after persistence.
The main window listens for events belonging to the active station and then:

1. invalidates station credential, snapshot, and collector-run queries;
2. shows a success or actionable failure toast;
3. closes the authorization window after successful persistence;
4. starts one fresh collection when authorization originated from a station-row
   collect/re-authorize workflow.

The event listener must not infer success from the capture window closing.
Success means native verification and persistence completed.

## Data Flow

```text
User completes login in WebView
  -> site requests authenticated identity endpoint
  -> capture bridge records the response
  -> strategy recognizes a completion candidate
  -> native coordinator deduplicates the attempt
  -> Tauri reads HttpOnly cookies for the management origin
  -> verifier calls /api/user/self with Cookie
  -> verifier extracts NewAPI user id
  -> encrypted credential store persists cookie + user id + session source
  -> coordinator emits sanitized success
  -> main window refreshes state and runs collection
```

## Error Handling

- No cookies yet: remain in `waiting`; do not show a terminal error.
- Identity verification returns 401/403: remain in `waiting` because the user may
  still be completing login.
- Verification timeout or transient 5xx: report a retryable state and allow the
  next candidate or manual fallback to retry.
- Invalid identity payload: report a strategy mismatch without persisting.
- Persistence failure: report a terminal local error and leave the login window
  open so the user does not lose the authenticated WebView session.
- Window closed before success: cancel the coordinator and preserve existing
  stored credentials.
- Existing valid session: treat completion as idempotent success.

Errors shown to the UI are short, categorized, and redacted. Raw response bodies
and cookie values remain unavailable to frontend code.

## Extensibility

New provider families add a strategy implementation with candidate recognition,
verification, and identity extraction. They reuse the same coordinator, cookie
reader, encrypted persistence, status events, and UI lifecycle.

The strategy interface should be introduced only around behavior that already
differs. Window creation, event capture, state management, persistence, and UI
refresh remain shared. This avoids both hostname conditionals and premature
provider-specific command duplication.

## Compatibility

- Sub2API token extraction remains unchanged.
- Existing explicit `finish_web_authorization_session` behavior remains
  available and delegates to the same coordinator path.
- Existing NewAPI username/password login remains available for stations that
  support it.
- Sessions with source `web_authorization` never fall back to password login
  after authentication rejection; they request reauthorization.

## Testing

### Rust unit tests

- Candidate recognition accepts a successful NewAPI self response.
- Candidate recognition rejects 401, unrelated endpoints, and missing identity.
- Duplicate candidates start only one verification attempt.
- A transient verification failure can be retried.
- Successful verification persists cookie presence, user id, and stable source.
- Public status and diagnostics never contain cookie values.
- Explicit finish and automatic finish share the same completion function.

### Frontend/source regression tests

- The station-list NewAPI authorization path has an automatic completion
  listener, not only a window-start action.
- Successful completion invalidates station state and triggers one collection.
- Manual completion remains visible only as fallback behavior.
- Existing Sub2API authorization capability remains present.

### Integration verification

- A local fixture returns 401 before the session cookie and a user envelope after
  it; automatic completion reaches `authorized` and persists once.
- Focused capture and NewAPI test suites pass.
- TypeScript/Vite build, `cargo fmt --check`, and `cargo check` pass.
- Manual KamiAPI QA confirms login automatically closes the authorization window
  and the subsequent balance/groups/models collection uses the saved session.

## Acceptance Criteria

- Logging into KamiAPI in the authorization window is sufficient; no second
  button is required in the normal flow.
- The database contains an encrypted cookie reference, NewAPI user id, and
  `session_source = "web_authorization"` after native verification.
- Collection does not attempt local username/password login after successful Web
  authorization.
- Repeated identity requests cannot create duplicate persistence or collection
  runs.
- No secret value appears in logs, snapshots, UI events, or errors.
- The design supports another NewAPI-style HttpOnly-cookie provider without
  adding provider-hostname branches to shared code.
