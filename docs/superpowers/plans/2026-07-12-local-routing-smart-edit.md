# Local Routing Smart Edit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the Local Routing edit tab into a compact, reliable editor for every automatic-routing setting while preserving candidate drag ordering.

**Architecture:** Keep `LocalRoutingEditTab` responsible for candidate ordering and extract settings loading/saving into `LocalRoutingSettingsEditor`. Put pure draft conversion and validation in `localRoutingSettingsForm.ts`, driven by an exhaustive field-kind schema so a future scheduler field cannot be silently omitted. All writes go through the typed settings API and start from the latest full `AppSettings` snapshot.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, Tauri settings API, Node contract scripts

---

### Task 1: Lock The Editor Contract With RED Tests

**Files:**
- Modify: `scripts/local-routing-automatic-settings.test.mjs`
- Create: `scripts/local-routing-smart-edit.test.mjs`

- [ ] **Step 1: Add source-contract assertions for the editor boundary**

The focused script must assert that the edit tab renders `LocalRoutingSettingsEditor`, that the editor imports `getSettings`, `updateSettings`, and `SETTINGS_UPDATED_EVENT`, and that it never imports Tauri `invoke` directly. It must also reject the old explanatory sentences and require every scheduler field name.

```js
assert.match(editTab, /LocalRoutingSettingsEditor/);
assert.match(editor, /getSettings/);
assert.match(editor, /updateSettings/);
assert.match(editor, /SETTINGS_UPDATED_EVENT/);
assert.doesNotMatch(editor, /@tauri-apps\/api|\binvoke\s*\(/);
assert.doesNotMatch(editTab + editor, /运行时会综合|分组筛选不会跨组兜底/);
```

- [ ] **Step 2: Add reliability assertions**

Require an exhaustive field schema, nested default normalization, full settings-update projection, explicit validation error keys, and protection against corrupted `????` routing copy in Settings.

```js
assert.match(settingsTypes, /SCHEDULER_ADVANCED_FIELD_KINDS/);
assert.match(settingsTypes, /appSettingsToUpdateInput/);
assert.match(settingsApi, /normalizeSchedulerAdvancedSettings/);
assert.doesNotMatch(settingsPage, /"\?\?\?+/);
```

- [ ] **Step 3: Run the new test and confirm expected failure**

Run: `node .\scripts\local-routing-smart-edit.test.mjs`

Expected: FAIL because `LocalRoutingSettingsEditor.tsx`, the exhaustive schema, and the form module do not exist yet.

### Task 2: Add Exhaustive Scheduler Metadata And Pure Validation

**Files:**
- Modify: `src/lib/types/settings.ts`
- Create: `src/features/routing/localRoutingSettingsForm.ts`
- Test: `scripts/local-routing-smart-edit.test.mjs`
- Test: `scripts/local-routing-settings-form.test.mjs`

- [ ] **Step 1: Define a compile-time exhaustive field-kind schema**

Add `SCHEDULER_ADVANCED_FIELD_KINDS` using `satisfies Record<keyof SchedulerAdvancedSettings, SchedulerAdvancedFieldKind>`. The schema distinguishes `positiveInteger`, `nonNegativeWeight`, `ratio`, and `boolean`, and includes all 21 fields.

```ts
export const SCHEDULER_ADVANCED_FIELD_KINDS = {
  topK: "positiveInteger",
  multiplier: "nonNegativeWeight",
  priority: "nonNegativeWeight",
  load: "nonNegativeWeight",
  queue: "nonNegativeWeight",
  errorRate: "nonNegativeWeight",
  ttft: "nonNegativeWeight",
  quotaHeadroom: "nonNegativeWeight",
  previousResponse: "nonNegativeWeight",
  sessionSticky: "nonNegativeWeight",
  multiplierMinConfidence: "ratio",
  stickyWeighted: "boolean",
  stickyEscape: "boolean",
  stickyEscapeTtftMs: "positiveInteger",
  stickyEscapeErrorRate: "ratio",
  stickySessionTtlSeconds: "positiveInteger",
  stickyResponseTtlSeconds: "positiveInteger",
  stickyMaxWaiting: "positiveInteger",
  stickyWaitTimeoutSeconds: "positiveInteger",
  fallbackMaxWaiting: "positiveInteger",
  fallbackWaitTimeoutSeconds: "positiveInteger",
} as const satisfies Record<keyof SchedulerAdvancedSettings, SchedulerAdvancedFieldKind>;
```

- [ ] **Step 2: Add a full settings projection helper**

`appSettingsToUpdateInput(settings)` returns only fields accepted by `UpdateSettingsInput`. Routing saves spread this projection and override only `defaultRoutingStrategy`, `maxRateMultiplier`, `defaultRoutingGroupFilter`, and `schedulerAdvancedSettings`.

- [ ] **Step 3: Implement draft conversion and validator**

`createLocalRoutingSettingsDraft` converts persisted numbers to strings. `parseLocalRoutingSettingsDraft` returns a discriminated union with field errors or typed routing values. Validation mirrors Rust: `topK > 0 && <= 65535`, integer counters are positive safe integers, weights are finite and non-negative, ratios are in `[0, 1]`, and the seven base weights cannot all be zero.

```ts
export type ParsedLocalRoutingSettingsDraft =
  | { ok: true; value: LocalRoutingSettingsValue }
  | { ok: false; errors: LocalRoutingSettingsErrors };
```

- [ ] **Step 4: Run the focused test**

Run:

```powershell
node .\scripts\local-routing-smart-edit.test.mjs
node .\scripts\local-routing-settings-form.test.mjs
```

Expected: the runtime form behavior test passes; the source contract still fails only on the not-yet-created React editor assertions.

### Task 3: Build The Settings Editor And Preserve Reordering

**Files:**
- Create: `src/features/routing/LocalRoutingSettingsEditor.tsx`
- Create: `src/features/routing/LocalRoutingSettingsFields.tsx`
- Modify: `src/features/routing/LocalRoutingEditTab.tsx`
- Test: `scripts/local-routing-smart-edit.test.mjs`
- Test: `scripts/local-routing-reorder.test.mjs`

- [ ] **Step 1: Load settings independently from workspace preview**

The editor calls `getSettings()` on mount, ignores stale/unmounted responses with an operation counter, and renders a compact retry state on failure. Candidate preview remains available even when settings loading fails.

- [ ] **Step 2: Render hard-boundary controls**

Render labeled controls for strategy, multiplier ceiling, group filter, strict rejection, and evidence threshold. Use `SelectControl`, native labeled numeric inputs, and short fixed values only.

- [ ] **Step 3: Render every scheduler field from metadata**

Promote `stickyWeighted` to a standalone row before the score section and render its `SwitchControl` with `showLabel={false}` plus borderless outer chrome. It is the only user-facing scheduler boolean. Exclude internal `stickyEscape` from render metadata so the editor preserves but never displays or mutates it. Group numeric fields into score, stickiness, and waiting sections. Render each group with `role="group"`, `aria-label={title}`, and a normal-flow `h3` using 12px top/bottom spacing instead of a divider-overlapping `legend`. Numeric fields use field-kind-derived `min`, `max`, and `step`. Invalid fields use `aria-invalid` and compact inline errors.

- [ ] **Step 4: Save atomically through the typed API**

On submit, block invalid drafts, call `updateSettings({ ...appSettingsToUpdateInput(settings), ...routingOverrides })`, replace local state with the normalized response, and dispatch `SETTINGS_UPDATED_EVENT`. Disable controls during save and ignore stale save completions.

- [ ] **Step 5: Keep reset explicit and local**

Reset copies `DEFAULT_SCHEDULER_ADVANCED_SETTINGS` into the draft but does not persist until the user presses Save. Show compact idle/dirty/saving/saved/error status.

- [ ] **Step 6: Remove the explanatory edit block and retain candidate DnD**

`LocalRoutingEditTab` renders the editor first, then the existing candidate preview/manual ordering section. Keep its stale-operation guards and optimistic rollback unchanged.

- [ ] **Step 7: Run RED-to-GREEN checks**

Run:

```powershell
node .\scripts\local-routing-smart-edit.test.mjs
node .\scripts\local-routing-reorder.test.mjs
```

Expected: both PASS.

### Task 4: Harden Settings Normalization And Repair Existing Copy

**Files:**
- Modify: `src/lib/api/settings.ts`
- Modify: `src/features/settings/SettingsPage.tsx`
- Test: `scripts/local-routing-smart-edit.test.mjs`

- [ ] **Step 1: Normalize partial nested scheduler settings**

Add `normalizeSchedulerAdvancedSettings(value)` that iterates the exhaustive field-kind schema. Missing or invalid numeric values fall back to `DEFAULT_SCHEDULER_ADVANCED_SETTINGS`; missing booleans preserve their typed defaults rather than collapsing to false.

- [ ] **Step 2: Repair corrupted routing labels**

Replace literal question-mark strings with concise Chinese labels for multiplier ceiling and group filter, including all group options. Do not add explanatory copy to the Local Routing editor.

- [ ] **Step 3: Re-run the focused contract**

Run: `node .\scripts\local-routing-smart-edit.test.mjs`

Expected: PASS.

### Task 5: Verify Behavior, Layout, And Integration

**Files:**
- Verify only

- [ ] **Step 1: Run all local-routing scripts**

```powershell
node .\scripts\local-routing-automatic-settings.test.mjs
node .\scripts\local-routing-page-layout.test.mjs
node .\scripts\local-routing-explanation.test.mjs
node .\scripts\local-routing-reorder.test.mjs
node .\scripts\local-routing-smart-edit.test.mjs
node .\scripts\local-routing-settings-form.test.mjs
```

Expected: every script exits 0.

- [ ] **Step 2: Run the TypeScript/Vite build**

Run: `pnpm.cmd build`

Expected: TypeScript and Vite exit 0. The existing chunk-size warning is allowed.

- [ ] **Step 3: Run visual browser checks**

Start Vite on an unused local port, open the Local Routing edit tab, and check desktop and narrow viewport screenshots for labeled controls, no overlaps, no horizontal scroll, visible focus states, and absence of explanatory paragraphs.

- [ ] **Step 4: Audit scope and diff quality**

Run:

```powershell
git diff --check
git status --short
git diff --cached --name-only
```

Expected: only planned files are modified, staged paths are empty before exact-path staging, and no secrets or generated screenshots are tracked.

### Task 5: Correct sticky escape to match Sub2API runtime behavior

**Files:**
- Modify: `src/features/routing/localRoutingSettingsForm.ts`
- Modify: `src/features/routing/LocalRoutingSettingsFields.tsx`
- Modify: `scripts/local-routing-smart-edit.test.mjs`
- Modify: `src-tauri/src/services/proxy/scheduler/mod.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/types.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Test: Rust unit tests colocated with the scheduler and runtime modules

- [ ] **Step 1: Prove the current UI and runtime gaps with failing tests**

Add a frontend contract assertion that `stickyEscape` is absent from render metadata while its numeric thresholds remain present. Add scheduler tests showing that a sticky candidate is not promoted when its TTFT EWMA, error-rate EWMA, or live concurrency exceeds the configured boundary, and that the binding remains stored after soft escape.

- [ ] **Step 2: Run focused tests and verify RED**

Run `node .\scripts\local-routing-smart-edit.test.mjs` and the focused scheduler Rust tests. Expected: the UI assertion finds the current visible switch and the scheduler still promotes the degraded sticky candidate.

- [ ] **Step 3: Implement persistent runtime scheduler state**

Keep runtime metrics, capacity, and group-scoped affinity on `ProxyServerContext`. Pass the saved advanced settings and those registries into automatic selection. Feed scheduler-relevant success/failure outcomes back into EWMA state and bind successful session/response affinity without weakening the existing forward-time hard-gate recheck.

- [ ] **Step 4: Implement fixed-on user behavior and soft escape**

Remove `stickyEscape` from frontend render metadata but preserve it in typed settings round trips. Resolve affinity only after eligibility, then ignore it for the current selection when TTFT, error rate, or concurrency meets Sub2API escape conditions. Do not clear the affinity entry on soft escape.

- [ ] **Step 5: Verify GREEN and regression coverage**

Run the focused Node and Cargo tests, all local-routing scripts, `pnpm.cmd build`, `cargo fmt --check`, `cargo check`, and browser checks at desktop and 375px widths. Expected: all pass; only `stickyWeighted` is visible as a boolean control, while escape thresholds remain editable.
