# Request Log Meta Tags Design

## Goal

Render the request-log group, request type, and billing mode as compact light-blue metadata tags matching the supplied reference.

## Scope

- Change only the presentation in `src/features/logs/RequestLogTable.tsx` and its focused regression test.
- Keep reasoning effort, endpoint, Token, cost, latency, status, and time presentation unchanged.
- Keep all existing data mapping and fallback text unchanged.

## Component

Add a local `LogMetaTag` component inside `RequestLogTable.tsx`. It is metadata, not a health status, so it will not reuse `StatusBadge`.

The tag uses:

- light blue background and blue text;
- no border;
- 4px corner radius rather than a pill shape;
- compact fixed-height typography consistent with the table;
- `max-w-full` and truncation so long group names do not resize the row;
- a `title` containing the complete value for truncated group names.

The group, type, and billing columns will wrap their existing display values in `LogMetaTag`.

## Verification

- Extend `scripts/request-log-observability-table.test.mjs` first and prove RED.
- Run the focused request-log scripts and `pnpm.cmd build` after implementation.
- Inspect the live desktop request-log page to confirm tag sizing, truncation, and row stability.
