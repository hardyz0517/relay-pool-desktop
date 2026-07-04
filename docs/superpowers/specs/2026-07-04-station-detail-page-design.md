# Station Detail Page Design

## Context

Relay Pool Desktop currently opens station detail in a right-side drawer from the station asset list. The drawer has become too dense for the amount of station-level information it needs to show: balance facts, visible groups, rate multipliers, collector runs, snapshots, login state, and related change events.

The target is a dedicated **station detail information page**. This page is for the station object itself, not for Key Pool item details and not for usage-statistics dashboards. The provided visual reference should be interpreted as a style and layout reference: compact identity header, clear card rhythm, dense information panels, and restrained desktop-tool styling.

## Goals

- Replace the main station detail surface with a full page.
- Keep the page read-only except for refresh and collection actions.
- Put balance and station asset status at the top.
- Make visible groups and rate multipliers the primary body content.
- Move health, login state, collector runs, snapshots, and related changes below the primary facts.
- Keep configuration editing in the existing edit-provider page.
- Keep component boundaries friendly to a future setting that may choose page, dialog, or drawer display mode.

## Non-Goals

- Do not build a usage statistics page.
- Do not build a Key Pool detail page.
- Do not add account, password, Base URL, API key, threshold, or note editing inside station detail.
- Do not add Key creation, editing, deletion, or ordering inside station detail.
- Do not add a settings preference for drawer/dialog/page mode in this iteration.
- Do not add large charts or new analytics dashboards.
- Do not add a new backend aggregation API unless existing APIs cannot support the page.

## Recommended Approach

Implement a dedicated page-first detail flow with a reusable content component:

- `StationDetailPage` handles route/page state, data loading, refresh actions, and navigation.
- `StationDetailContent` renders station facts from a prepared view model and does not perform requests.

This gives the product a clear page-based UX now, while keeping the content portable enough for a future dialog or drawer surface if the app later exposes a display-mode preference.

## Page Entry And Navigation

The station asset list should have two distinct interactions:

- Clicking a station row opens `StationDetailPage`.
- Clicking the row edit icon opens the edit-provider page.

The detail page has a back action that returns to the station list. A weak "edit supplier" entry can be present on the detail page, but it navigates to the edit-provider page instead of exposing inline editing.

## Page Information Hierarchy

### 1. Station Identity Header

The header identifies the station and provides station-level refresh actions.

Content:

- Station name
- Station type
- Base URL
- Enabled/disabled state
- Last balance refresh or collection time

Allowed actions:

- Refresh balance
- Collect groups and rates
- Re-collect all station facts
- Weak navigation to edit supplier

Do not put destructive delete actions in the primary header. Delete can remain in the list or edit flow.

### 2. Balance And Asset Status

Balance is the highest-priority fact because it tells the user whether the station is currently usable.

Suggested cards:

- Current balance
- Low-balance threshold and risk state
- Last balance refresh time
- Balance source and confidence, when available

The balance section is read-only. Refresh actions update it without replacing the entire page state.

### 3. Groups And Rate Multipliers

This is the primary body of the page. It should be larger than the surrounding diagnostic sections and should support dense scanning.

Suggested columns:

- Group name
- Group identifier or hash, only when already available as a non-sensitive id/hash
- Effective multiplier
- Default or collected multiplier
- Binding status
- Source: collected, manual, missing, or unknown
- Last collected time
- Warning or anomaly marker

Warnings should be inline and low-saturation, not modal. Examples:

- Group disappeared
- Multiplier missing
- Multiplier is zero
- Binding is missing
- Rate source is stale

### 4. Secondary Diagnostics

These sections sit below balance and groups/rates:

- Login/session summary
- Recent collector runs
- Latest collector snapshot summary
- Related change events

They are read-only. If a user needs to change login credentials or station configuration, they should navigate to the edit-provider page.

## Data Flow

`StationDetailPage` loads existing data sources:

- `listStations()` to resolve the current station
- `getStationCredentials(stationId)` for read-only login/session summary
- `listStationKeys(stationId)` for key count and state summary only
- `listStationGroupBindings(stationId)`
- `listGroupRateRecords(stationId)`
- `listCollectorRuns(stationId)`
- `getLatestCollectorSnapshot(stationId)`
- `listChangeEvents()` filtered by `stationId`
- Existing balance/economics APIs for balance facts and snapshots

The page should keep old data visible during refresh operations. Refresh failures should show an error near the affected section and a toast, without clearing the page.

## Refresh Actions

Refresh actions are station-level operations:

- Refresh balance: runs the balance collection task and reloads balance-related data.
- Collect groups and rates: runs the groups/rates collection task and reloads group binding and rate data.
- Re-collect all facts: runs the full station collection and reloads all station detail data.

Each action has local loading state. The whole page should not flash or reset while one action is running.

## Component Boundary

Recommended components:

- `StationDetailPage`
- `StationDetailContent`
- `StationAssetHeader`
- `StationBalanceOverview`
- `StationGroupRatePanel`
- `StationDiagnosticsSection`

`StationDetailContent` should receive view-model data and callbacks for refresh actions. It should not know whether it is inside a page, dialog, or drawer.

## Error Handling

- Missing `stationId`: return to station list or show a small "station not found" page.
- Missing station after load: show an error state with a back action.
- Refresh failure: keep stale data visible and show section-level error plus toast.
- Empty groups/rates: show an empty state that clearly says groups and rate multipliers have not been collected yet.
- Authentication missing: show it as a diagnostic fact and provide navigation to edit supplier, not inline credential fields.

## Testing

Required checks:

- `pnpm.cmd build`
- Browser smoke with Vite:
  - station list row click opens detail page
  - station detail is not a drawer or modal
  - balance appears above groups/rates
  - groups/rates are the primary body section
  - edit entry navigates to edit supplier page
  - back returns to station list
  - refresh actions show loading and keep old data visible on failure

Rust checks are needed only if the implementation changes Tauri commands or service logic.

## Implementation Boundary

The first implementation should focus on replacing the station drawer as the primary detail flow. It may leave legacy drawer code in place temporarily if removing it would create unrelated churn, but the normal user path should be page based.

Do not move Key Pool management into station detail. Station detail may summarize station keys, but key operations remain outside this spec. A separate design must explicitly reopen that scope before adding station-specific key dialogs or key editing here.
