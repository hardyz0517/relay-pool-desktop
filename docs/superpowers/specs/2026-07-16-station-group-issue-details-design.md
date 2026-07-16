# Station Group Issue Details Design

## Goal

Stop reporting harmless historical group records as current station anomalies, while making every remaining `分组异常` tag explain exactly which active dependency is affected.

## Scope

This change is limited to the station-list read model and its tag presentation. It does not delete or rewrite historical group bindings, change collector persistence, change routing behavior, or redesign the station list.

## Current Problem

`countGroupIssues` currently counts every station-level binding whose status is `missing` or `disabled`. That conflates three cases:

1. A historical name-based binding shadowed by a current ID-based binding with the same group name.
2. A group that disappeared upstream but is no longer used locally.
3. A missing or disabled group that is still referenced by an enabled station key and can affect routing.

Only the third case is a current operational anomaly. Manual collection reloads all bindings immediately, so it makes the stale statuses visible even when the latest collection succeeded.

## Decision

Derive group issues in the station-list projection instead of mutating historical storage.

For each `station_group` binding with status `missing` or `disabled`:

1. Normalize its group name by trimming, lowercasing, and collapsing surrounding identity differences.
2. Ignore it when a current `available` station-group binding has the same normalized group name. This treats old name-based or old-ID records as shadow history.
3. Otherwise, find enabled station keys that still reference it. Match in this order:
   - exact `groupBindingId`;
   - matching non-empty `groupIdHash`;
   - matching normalized `groupName`.
4. Report an issue only when at least one enabled key matches. Disabled keys do not keep a station-level anomaly active because they do not participate in routing.

This keeps historical bindings and change events intact while making the list tag reflect current operational impact.

## Read-Model Shape

Replace the opaque count-only projection with a small structured reason list owned by `stationAssetViewModels.ts`. Each reason contains enough display-ready information to build the tag details without additional queries:

- binding id and group name;
- binding status (`missing` or `disabled`);
- affected enabled-key count;
- affected enabled-key names, bounded for compact display;
- a complete Chinese reason sentence.

`StationAssetRow` keeps `groupIssueCount` for existing filters and summary behavior, but derives it from `groupIssueReasons.length`. It also exposes the reason strings used by the tag.

## Tag Copy

The visible tag remains `分组异常`. Its detail text is assembled from the projected reasons, for example:

- `分组「ccmax-限制客户端-暂停供应」已下架，但仍被 2 个启用 Key 使用：生产 Key、备用 Key。`
- `分组「pro」已禁用，但仍被启用 Key「生产 Key」使用。`

When several groups are affected, each reason is shown on its own line. The wording must distinguish `missing` as `已下架` and `disabled` as `已禁用`. No API keys, masked key values, cookies, tokens, raw collector payloads, or secret identifiers may appear.

## Tooltip Interaction

The station row renders the existing tag inside a small tooltip trigger:

- mouse hover opens the detail layer;
- keyboard focus opens the same detail layer;
- the trigger has a visible focus state and `tabIndex=0` only when detailed text exists;
- the tooltip uses `role="tooltip"` and is associated through `aria-describedby`;
- native `title` remains as a fallback;
- the layer uses existing light-theme surface, border, shadow, and text tokens;
- positioning and opacity changes must not move the row layout.

Other issue tags retain their current title-based details unless they already supply richer text. This change does not introduce a global tooltip system.

## Error and Edge Cases

- A historical `missing` binding shadowed by an `available` binding of the same normalized name produces no issue.
- A missing group referenced only by disabled keys produces no issue.
- A missing group referenced by one or more enabled keys produces one group reason, not one reason per key.
- Duplicate historical bindings for the same normalized name are coalesced into one displayed reason when they affect the same current dependency.
- Keys with no group reference do not match a missing group.
- Empty or whitespace-only group names are not matched by name.
- Long key-name lists are bounded in the tooltip while preserving the total affected count.

## Testing

Extend `scripts/station-list-risk-tags.test.mjs` through test-first development. Required regressions:

1. Same-name `available` binding suppresses historical `missing` and `disabled` shadows.
2. Unreferenced missing groups do not create a tag.
3. Disabled-key-only references do not create a tag.
4. Enabled-key references by binding id, group id hash, and normalized name each create a reason.
5. Duplicate historical bindings produce one reason.
6. The tag title contains the concrete Chinese reason.
7. The station-row source exposes hover and focus tooltip behavior with accessible association.

After focused regressions pass, run TypeScript checking and the Vite build. Because this is a frontend-only projection and presentation change, no Rust production files are expected to change.

## Non-Goals

- Deleting or migrating historical group-binding rows.
- Changing collector group identity or missing-marking rules.
- Creating new change-center events.
- Treating pricing rules without an active key as routing-impact evidence.
- Generalizing a project-wide tooltip component.
