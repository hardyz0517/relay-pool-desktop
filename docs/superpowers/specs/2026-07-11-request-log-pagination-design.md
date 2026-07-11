# Request Log Pagination Design

## Goal

Add compact pagination below the request-log table so long usage histories remain easy to scan without changing the request-log persistence or query contract.

## Scope

- Paginate the request logs already loaded by `LogsPage`.
- Keep the existing backend limit of the newest 500 records.
- Default to 20 rows per page and offer 20, 50, and 100 rows per page.
- Do not add database pagination, cursor APIs, or infinite scrolling.

## Interaction

- The footer shows `第 X-Y 条 / 共 N 条` on the left.
- A compact page-size selector appears beside the count.
- Previous and next icon buttons plus the highlighted current page appear on the right.
- Previous is disabled on the first page; next is disabled on the last page.
- Changing the filter, refreshing, clearing records, or changing page size resets to page 1.
- If filtering reduces the result count, the current page is clamped to the last valid page.
- Row selection and the developer inspector operate on records from the current page.

## Layout

- Render pagination as a separate light surface below the table rather than inside the table border.
- Use approximately 16px of vertical separation from the records above.
- Keep the footer compact, flat, and consistent with the existing light desktop-tool UI.
- Use Lucide chevrons for previous and next controls, visible focus states, and disabled styling.
- Wrap controls on narrow widths without overlapping or forcing the table itself to compress.

## Architecture

- Add a pure pagination helper to `requestLogViewModels.ts` that clamps page and page size and returns visible records plus range metadata.
- Keep page and page-size state in `LogsPage`, where filtering and refresh lifecycle are already owned.
- Add a request-log-specific pagination footer component near `RequestLogTable`; pass it derived metadata and callbacks rather than raw fetching concerns.
- Reuse existing `Button` and native `select` patterns instead of adding a shared abstraction for one surface.

## Verification

- Add a focused script covering default page size, valid ranges, final-page clamping, empty results, filter reset, page-size reset, and separated footer styling.
- Run the focused request-log scripts and the TypeScript/Vite build.
- Open the local app surface and inspect desktop and narrow viewport layouts when the runtime can expose request-log rows.
