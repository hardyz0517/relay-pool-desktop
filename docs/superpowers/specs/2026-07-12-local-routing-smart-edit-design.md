# Local Routing Smart Edit Page Design

Date: 2026-07-12

## Goal

Upgrade the Local Routing `编辑` tab from a mostly read-only/reorder surface into a real automatic-routing configuration panel.

The page should feel like an editor, not an introduction page: compact labels, editable fields, short status values, and minimal explanatory copy.

## Scope

In scope:

- Add editable global automatic-routing controls to `LocalRoutingEditTab`.
- Reuse existing settings fields:
  - `maxRateMultiplier`
  - `defaultRoutingGroupFilter`
  - `schedulerAdvancedSettings`
- Preserve the existing draggable candidate ordering as a lower-priority preview/manual correction area.
- Keep the UI aligned with Relay Pool Desktop's light, compact desktop-tool style.

Out of scope for this slice:

- New Rust database fields unless implementation proves an existing required field is missing.
- Changing the routing algorithm itself.
- Adding a marketing/onboarding explanation block.
- Reworking the `状态` tab beyond any small consistency fixes needed by shared formatting helpers.

## Page Structure

### 1. Hard-boundary controls

The top card is a compact form, not an explainer.

Fields:

- Strategy: fixed display value `automatic_balanced`.
- Multiplier limit: editable numeric field; empty means no configured ceiling.
- Default group filter: select control with:
  - All groups
  - GPT
  - Claude
  - Gemini
  - Grok
  - Image generation
  - Ungrouped only
- No-candidate policy: fixed display value `严格拒绝`.
- Evidence threshold: editable `multiplierMinConfidence`.

Copy rule:

- Use labels and short values only.
- Avoid long descriptions such as “运行时会综合...” or “当所有健康 Key...”.

### 2. Sub2API-style scheduler parameters

The middle card exposes scheduler settings in a dense grid.

First visible group:

- `topK`
- `multiplier`
- `priority`
- `load`
- `queue`
- `errorRate`
- `ttft`
- `quotaHeadroom`

Second visible group:

- `previousResponse`
- `sessionSticky`
- `stickyEscapeTtftMs`
- `stickyEscapeErrorRate`
- `stickySessionTtlSeconds`
- `stickyResponseTtlSeconds`
- `stickyMaxWaiting`
- `stickyWaitTimeoutSeconds`
- `fallbackMaxWaiting`
- `fallbackWaitTimeoutSeconds`

Controls:

- Numeric inputs for numeric fields.
- `stickyWeighted` is the only user-facing scheduler boolean. Sub2API keeps sticky escape enabled by default as an internal gateway safeguard rather than exposing it as an admin UI switch.
- `stickyWeighted` is promoted to a standalone full-width row above all scheduler parameter groups, matching the Sub2API gateway layout.
- The promoted `stickyWeighted` switch shows only the switch track and thumb; it keeps an accessible label but does not show `开启` / `关闭` text or an outer button surface.
- `stickyEscape` remains an internal persisted compatibility field with a default of `true`; the editor does not render or mutate it. The TTFT and error-rate thresholds remain editable advanced parameters.
- Reset-to-default action for scheduler settings.
- Save state visible as a compact badge: idle/saving/saved/error.

Validation:

- Numeric fields must reject non-finite values.
- `topK`, TTL, waiting, and timeout fields must be positive safe integers; `topK` must also fit the backend `u16` range.
- Weight fields must be finite and non-negative.
- Confidence and error-rate thresholds must stay within `0..=1`.
- At least one base score weight must be greater than zero, matching the Rust scheduler validator.
- Invalid local input should block save and show a focused inline error near the field group.

All fields already present in `SchedulerAdvancedSettings` must be reachable from this editor. Adding a new backend field must produce a TypeScript compile-time gap until its default, field kind, form metadata, and UI control are defined.

### 3. Candidate preview and manual order

Keep the existing candidate row list and drag sorting, but title it as preview/manual correction.

Candidate rows continue to show:

- Key/station identity.
- Enabled/health/group-match badges.
- Order.
- Effective multiplier.
- Source/confidence.
- Balance.
- Cooldown.

The list should not be the only editable area.

## Data Flow

Read:

- `LocalRoutingEditTab` receives workspace data for preview and candidate order.
- The edit tab also reads app settings through `getSettings()`.

Write:

- Settings form writes through `updateSettings()`.
- The form derives a complete `UpdateSettingsInput` from the latest `AppSettings` snapshot before applying routing overrides, so unrelated settings are preserved.
- Candidate order continues to write through `reorderLocalRoutingKeys()`.

Event sync:

- After `updateSettings()` succeeds, dispatch `SETTINGS_UPDATED_EVENT` so other surfaces can refresh consistently.
- If the parent routing page already refreshes workspace after settings changes, reuse that mechanism. Otherwise, add a narrow refresh hook.

## UX Rules

- This is an editor, not an intro page.
- No long explanation cards.
- Use concise labels and compact inline status.
- Scheduler group titles use normal document flow with 12px space above and below; titles must not sit on divider lines.
- Do not hide required routing controls in Settings only; the routing edit page must be the main place to tune automatic routing.
- Keep keyboard accessibility: labels must map to inputs, focus states remain visible, and disabled/saving states must be explicit.

Runtime parity requirement:

- Scheduler metrics, capacity, and scoped affinity live for the lifetime of the local proxy server rather than being recreated for each selection.
- A valid sticky binding is ignored for the current selection when TTFT EWMA exceeds the configured threshold, error-rate EWMA exceeds the configured threshold, or its concurrency capacity is full.
- Soft escape does not delete the binding. Hard eligibility failures continue to prevent affinity from bypassing group, multiplier, model, health, or capability gates.

## Test Plan

Add or update focused tests/scripts to verify:

- The edit tab renders multiplier limit and group filter controls.
- The edit tab renders scheduler parameter controls.
- Old explanatory copy is not present in the edit tab.
- Saving settings calls the typed settings API path, not direct `invoke` from UI.
- Scheduler validation mirrors the Rust `SchedulerAdvancedSettings::validate` boundary.
- Older or partial scheduler settings are merged with typed defaults when read.
- Existing candidate reorder test still passes.

Manual verification:

- `pnpm.cmd build`
- Relevant local routing UI contract scripts.
- If Rust code changes become necessary, run `cargo check` and focused Rust tests.
