# Change Center Exchange Ratio Design

## Problem

Change events persist raw upstream group multipliers. The Change Center currently formats those raw values directly, while other multiplier surfaces convert them to the user-facing effective multiplier with `rawMultiplier / creditPerCny`. This makes Change Center values disagree with Pricing and Station Detail whenever a station exchange ratio differs from `1`.

## Boundary

Raw multiplier storage remains unchanged. `creditPerCny` is a display-time normalization concern and must not be written into collector facts or historical change-event JSON. The Change Center projection will receive the current station-to-exchange-ratio mapping and apply the existing `effectiveRateMultiplierForCredit` helper exactly once before formatting.

The current station ratio is intentionally used for historical events so Change Center remains consistent with Pricing and Station Detail. Missing stations, non-finite ratios, zero, and negative ratios retain the shared helper's safe fallback of `1`.

## Scope

- Convert multiplier values for `rate_changed`, `group_added`, and `group_missing` rows.
- Reuse the station list already loaded by `ChangeCenterPage`; do not add another query or backend join.
- Preserve event JSON and collector/database semantics.
- Preserve all non-multiplier change rendering.

## Regression Contract

- A raw multiplier of `1.8` with `creditPerCny = 2` renders as `0.9`.
- Rebuilding the same row remains `0.9`, proving the projection is pure and does not cumulatively convert to `0.45`.
- A ratio of `1`, an absent ratio, or an invalid ratio preserves the raw value.
- Added and missing group rows follow the same conversion boundary as changed-rate rows.

## Verification

Run the focused Change Center view-model regression, TypeScript checking, the Vite build, and `git diff --check`. No Rust verification is required because the backend and persisted event contract do not change.
