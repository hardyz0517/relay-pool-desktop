# ADR 0003: Production and demo backend modes

## Status

Accepted with current security/build debt. The present `src/main.tsx` and `vite.config.ts` form one startup graph; the required physical production/demo split does not yet exist and is a Stage 2 hard gate.

## Context

Frontend API modules currently contain invoke-unavailable fallback behavior. A desktop transport failure can therefore look like successful mock data. Runtime flags alone cannot prove that demo code and credentials are absent from a production bundle.

## Decision

Production desktop and browser demo/test are separate build entries and separate composition roots:

- production: `index.html` -> `src/main.tsx` -> desktop bootstrap -> `DesktopBackend`;
- demo: `demo.html` -> `src/demo.tsx` through `vite.demo.config.ts` -> `DemoBackend`.

The production Vite/Tauri module graph must not import, dynamically import or package `DemoBackend`, demo datasets or demo reset code. The demo entry cannot import Tauri APIs, credentials, persistence, local proxy, updater, filesystem or real network adapters. Build-mode selection is compile-time entry selection, not a query parameter, local-storage switch or invoke-error fallback.

Desktop bootstrap performs the runtime contract handshake before rendering the normal application. Missing Tauri transport, version/hash mismatch or capability mismatch enters a fail-closed recovery/error state. It never switches to demo.

Demo data and time are deterministic: a fixed epoch (`2026-01-01T00:00:00Z`), seeded identifiers and explicit clock advancement. Reset replaces the complete in-memory demo dataset with its original immutable snapshot. Unsupported capabilities return typed `Unsupported`; they do not fabricate success. The demo UI always has a persistent, testable `Demo` mode marker.

Production Tauri uses a dedicated non-null CSP and the main-window least-privilege capability. Preview/dev relaxations, if any, live in separate config files and cannot be inputs to a release build. Current `tauri.conf.json` has `csp: null`; this is not accepted as a baseline and must be removed by Task 8 before Stage 2 exits.

## Alternatives

- Runtime `isTauri` checks with fallback data: rejected because transport outages become false success.
- One entry with tree-shaken conditional imports: rejected because reachability and release provenance are harder to prove.
- Mock Service Worker or live remote demo APIs: rejected for this deterministic local preview boundary.

## Consequences

There are two explicit frontend build commands and contract suites, but only one production behavior. Preview remains useful without weakening desktop reliability or security. Shared view components may be reused; backend composition and datasets are not shared implicitly.

## Rollback

The production entry can roll back to the last known `DesktopBackend` revision. It cannot restore fallback-to-demo behavior. The demo entry may be disabled independently without changing the production bundle.

## Verification

- production bundle graph contains no `DemoBackend`, demo fixture or demo entry module;
- demo build contains no `@tauri-apps`, credential, updater, proxy or persistence adapter import;
- handshake failure and transport absence render recovery/error state and never mock data;
- demo reset is deterministic and unsupported commands return typed `Unsupported`;
- parsed release config has a non-null CSP and no preview/dev relaxation;
- smoke tests assert the visible mode marker in demo and its absence in production.
