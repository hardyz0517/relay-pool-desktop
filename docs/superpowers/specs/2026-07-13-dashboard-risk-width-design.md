# Dashboard Current Risk Width Design

## Goal

Keep the four current-risk metric cards on one row at every supported desktop window width without allowing the risk section or its detail rows to extend beyond the visible content area.

## Layout

- Render the current-risk summary as an explicit four-column grid at all widths.
- Allow each metric card and its text column to shrink below their intrinsic content width.
- Keep the current-risk section and risk-detail list shrinkable inside the dashboard's single-column grid.
- Preserve one-line truncation for metric labels, values, details, and long risk messages.
- Do not add horizontal scrolling or change the surrounding application shell.

## Scope

Only the dashboard current-risk section and its focused regression test are in scope. Other dashboard metric panels and the shared `ObjectRow` component keep their existing behavior.

## Verification

- A source-level regression test requires an explicit four-column risk grid and shrinkable containers.
- TypeScript and Vite build checks must pass.
- Browser measurements at narrow desktop widths must show no horizontal overflow, with all four cards inside the content viewport.
