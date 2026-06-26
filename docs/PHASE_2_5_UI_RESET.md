# Phase 2.5 UI Reset

Phase 2.5 is a visual and interaction reset. It does not add new backend behavior. The goal is to keep Relay Pool Desktop as a local desktop tool while moving the content area away from a rigid enterprise dashboard and toward a softer Sub2API-style control console.

## Why Reset

The Phase 1 / Phase 2 UI had the right functional coverage, but the visual hierarchy was weak:

- Too many equally weighted cards.
- Dashboard-like KPI blocks felt like a SaaS backend.
- Stations mixed a permanent add/edit form with details.
- Buttons and badges did not have one consistent visual language.
- Fullscreen layouts could feel sparse or awkward.
- The Sub2API collector page did not feel like a diagnostic tool.

## Corrected Direction

The target is:

- Sub2API-style soft card console for the content area.
- CCSwitch-style local desktop navigation and shell.
- Light cyan-gray / blue-green background.
- White or near-white rounded cards.
- Soft shadows and comfortable radius.
- Small status badges and icon tiles.
- Clear numeric cards where metrics matter.
- Inspector panels for details.
- Dialog-based create/edit flows instead of permanent large forms.

This is not a website, SaaS console, marketing page, or enterprise admin template.

## Component Language

The reset introduces and uses small UI primitives:

- `WorkspaceLayout`: workspace-style page grid.
- `InspectorPanel`: right-side detail panel.
- `Toolbar`: compact page or panel actions.
- `PropertyList` / `PropertyRow`: desktop settings and inspector rows.
- `SegmentedControl`: compact strategy or filter control.
- `ActivityList` / `ActivityItem`: desktop activity feed.

Existing components were softened:

- `Button`: primary / secondary / outline / ghost / danger variants.
- `SectionCard`: softer radius, light shadow, lower header weight.
- `DataTableLite`: softer table wrapper, weaker header, compact rows.
- `StatusBadge`: rounded pill badges for status only.
- `MetricCard` and `EmptyState`: softer card tone.

## Page Changes

### Overview

The overview now follows a Sub2API-like dashboard console:

- Soft metric cards with icon tiles.
- Local proxy entry card.
- Station health summary.
- Recent requests and price changes as activity feed.

### Stations

Stations is treated as the core provider management page:

- Main area uses station status cards inspired by Sub2API channel cards.
- Each station shows name, type, status, balance, latency, availability placeholder, refresh time, enable state, and request-state bar.
- Right side is an inspector panel.
- Create/edit moved to a dialog.
- P2 persistence remains: create, edit, delete, enable/disable, reorder.

### Sub2API Collectors

Collectors now behaves like a diagnostic console:

- Top metrics for login state, captured endpoints, recognized fields, and recent errors.
- Capture table resembles a compact Network panel.
- Field recognition highlights balance / group / rate_multiplier / key.
- Snapshot and error details live in an inspector panel.

### Pricing

Pricing keeps the table-first structure but softens the layout:

- Top metric cards.
- Main price table as the focal area.
- Right inspector for recommended station, station comparison, and raw ratio note.

### Routing

Routing is now a compact settings page:

- Segmented control for default strategy.
- Setting rows for fallback, balance threshold, circuit breaker, and health cache.
- Lightweight fixed-route list.

### Logs

Logs now behaves more like a log tool:

- Filter toolbar.
- Main request table.
- Right log inspector.
- Compact fallback trace.

### Settings

Settings now uses desktop setting rows:

- Left side label and description.
- Right side control.
- Low-key safety warnings for data path and plaintext key limitation.

## Business Scope Not Included

Phase 2.5 does not implement:

- Real local proxy.
- Real Sub2API collection.
- Real NewAPI collection.
- Real routing.
- Real health checks.
- Request forwarding.
- New Rust data-layer behavior.
- New dependencies.
- Dark theme.

## Phase 3 Notes

When Phase 3 starts, Sub2API collection can plug into the existing diagnostic console:

- Captured endpoints can replace mock endpoint rows.
- Detected balance / group / rate fields can replace mock field matches.
- Collector errors can flow into the warning block.
- Manual correction can open a real correction dialog without changing the page structure.

## Phase 2.5B Notes

This pass tightened the provider-management side of the reset:

- Stations returned to a narrow provider row list instead of a large card/detail layout.
- Create, edit, and preview/details now all use Dialog, not Drawer.
- Detail no longer renders below the list.
- Drag sorting uses `@dnd-kit/core` and `@dnd-kit/sortable`, with a dedicated handle on the left.
- A new `渠道状态` page now carries latency, availability, recent request state bars, and health diagnostics.

### Boundary Update

- `中转池`: configuration, balance, ordering, enable/disable, edit.
- `渠道状态`: latency, availability, request-state bars, health diagnosis.
- `总览`: summary only, with a lightweight channel-health entry point.

## Phase 2.5C Notes

This pass corrected page responsibilities and tightened drag behavior:

- `中转池` now only shows the provider row list and its toolbar.
- Station summary, key-value detail panels, health explanation blocks, and responsibility notes were removed from the page body.
- Create, edit, and detail remain Dialog-only flows.
- Station details are read-only inside Dialog and never render below the list.
- `渠道状态` is now a health-card matrix instead of dashboard metrics plus a persistent inspector.

### Responsibility Boundary

- `中转池`: configuration, balance, ordering, enable/disable, edit, detail, delete.
- `渠道状态`: latency, PING, availability, recent 60-request bars, health diagnosis.

### Drag Sorting Constraints

- Persist ordering only once in `onDragEnd`.
- Do not call Tauri commands or write SQLite while dragging.
- Keep `DndContext` scoped to the station list.
- Use the left drag handle as the only drag activator.
- Use `DragOverlay` for a lightweight floating clone.
- Keep row transforms GPU-friendly and avoid layout-heavy changes during drag.
