# 设置页信息架构收敛 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将设置页收敛为应用级控制，把路由安全策略、采集调度参数和模型基准价格归还给各自业务页面，同时保持现有配置数据与后端行为兼容。

**Architecture:** 这是一次纯前端职责迁移，继续复用 `AppSettings`、`getSettings()`、`updateSettings()` 和既有 Tauri `update_settings` 命令，不修改 Rust、数据库字段或配置格式。先扩展现有路由编辑器和采集中心，使它们完整承接原设置项；再删除设置页的重复入口、收窄页面宽度，并用源码契约测试和真实桌面运行验证信息架构。

**Tech Stack:** React 18, TypeScript 5.7, Vite 6, Tailwind CSS, Tauri 2, Node.js contract tests

---

## Locked Product Decisions

- 设置页保留：本地代理、默认网络出口、数据目录、显示高级工具、关于/更新。
- `developerModeEnabled` 内部字段保持不变；用户可见名称统一为“显示高级工具”，侧边栏效果文案为“在侧边栏显示采集中心”。
- 倍率限制、候选分组、默认低余额阈值、余额耗尽兜底全部在 `路由规则 > 编辑 > 路由边界` 修改。
- 倍率限制使用显式开关；关闭时保存 `maxRateMultiplier: null`，并明确显示“自动路由不可用”，不再用空输入框暗示这一副作用。
- 路由边界统一显式保存，不再通过 400ms 防抖自动保存。保存成功后沿用 `SETTINGS_UPDATED_EVENT` 刷新当前候选统计。
- 采集中心提供三个频率预设：
  - 及时：余额 2 分钟、分组/倍率 10 分钟、模型 30 分钟、价格 30 分钟。
  - 均衡：余额 5 分钟、分组/倍率 20 分钟、模型 60 分钟、价格 60 分钟。
  - 节省资源：余额 15 分钟、分组/倍率 60 分钟、模型 180 分钟、价格 180 分钟。
- 采集超时和并发数不随频率预设改变；“恢复推荐值”将频率恢复为“均衡”、超时恢复为 15 秒、并发恢复为 3。
- `collectorIntervalMinutes` 保持原值并继续随 `appSettingsToUpdateInput()` 透传，本轮不新增对应 UI。
- 模型基准价格只从 `价格 / 倍率` 页面进入；现有 `ModelBasePricesPage` 和 API 不改。
- 设置页主体最大宽度固定为 `1080px` 并居中，避免超宽窗口下标签和控件相距过远。
- 本轮不修改 Rust，因此最终要求 `pnpm build`，不要求 `cargo check`；若执行中实际触碰 `src-tauri/**`，必须追加 `cargo check --manifest-path src-tauri/Cargo.toml`。

## File Map

**Create**

- `src/features/collectors/collectorSettingsForm.ts`：采集频率预设、草稿转换和输入校验。
- `src/features/collectors/CollectorAdvancedSettings.tsx`：采集调度的独立加载、编辑、保存和恢复默认 UI。
- `scripts/local-routing-boundary-controls.test.mjs`：路由边界归位和显式保存契约。
- `scripts/collector-settings-form.test.mjs`：采集预设与校验行为测试。
- `scripts/collector-settings-layout.test.mjs`：采集中心承接调度参数的布局契约。
- `scripts/settings-information-architecture.test.mjs`：设置页最终职责边界契约。

**Modify**

- `src/features/routing/localRoutingSettingsForm.ts`：把低余额和耗尽兜底纳入路由边界草稿与解析结果。
- `src/features/routing/LocalRoutingSettingsFields.tsx`：增加显式倍率开关、单位、低余额和兜底控件。
- `src/features/routing/LocalRoutingSettingsEditor.tsx`：取消边界自动保存，增加显式保存和当前生效范围。
- `src/features/routing/LocalRoutingEditTab.tsx`：把 `workspace` 传给路由设置编辑器。
- `scripts/local-routing-settings-form.test.mjs`：覆盖新增边界字段和无效输入。
- `scripts/local-routing-automatic-settings.test.mjs`：锁定路由字段只出现在路由页面。
- `src/features/collectors/CollectorsPage.tsx`：挂载独立采集调度组件。
- `src/features/settings/SettingsPage.tsx`：删除错误归类的字段，重组为网络、数据、高级等应用级分区。
- `src/components/shell/PageScaffold.tsx`：设置页宽度约束。
- `src/app/App.tsx`：移除设置页打开基准价格的回调。
- `src/app/routes.tsx`：把用户可见“开发者模式”改为“高级工具”。
- `scripts/settings-autosave.test.mjs`：只验证仍留在设置页的单项保存行为。
- `scripts/settings-card-density.test.mjs`：增加设置页最大宽度契约。
- `scripts/settings-toggle-labels.test.mjs`：分别验证高级工具开关和路由兜底开关的归属。
- `docs/PROJECT_PLAN.md`：更新设置、路由、价格和高级采集页面职责。

**Deliberately Unchanged**

- `src/lib/types/settings.ts`
- `src/lib/api/settings.ts`
- `src-tauri/**`
- 模型基准价格 API、数据库结构和配置序列化

---

### Task 1: Establish A Clean Baseline

**Files:**

- Inspect only: all files listed in the File Map

- [ ] **Step 1: Confirm target paths are not already modified**

Run:

```powershell
git status --short -- `
  src/features/settings/SettingsPage.tsx `
  src/features/routing/localRoutingSettingsForm.ts `
  src/features/routing/LocalRoutingSettingsFields.tsx `
  src/features/routing/LocalRoutingSettingsEditor.tsx `
  src/features/routing/LocalRoutingEditTab.tsx `
  src/features/collectors/CollectorsPage.tsx `
  src/components/shell/PageScaffold.tsx `
  src/app/App.tsx `
  src/app/routes.tsx `
  docs/PROJECT_PLAN.md
```

Expected: no output. If any target path is modified, inspect that diff and preserve it; do not reset or overwrite it.

- [ ] **Step 2: Confirm nothing unrelated is staged**

Run:

```powershell
git diff --cached --name-only
```

Expected: no output. If output exists, do not unstage it; use exact-path staging for every commit in this plan.

- [ ] **Step 3: Run the existing focused baseline**

Run:

```powershell
node scripts/local-routing-settings-form.test.mjs
node scripts/local-routing-automatic-settings.test.mjs
node scripts/local-routing-page-layout.test.mjs
node scripts/settings-autosave.test.mjs
node scripts/settings-card-density.test.mjs
node scripts/settings-data-dir.test.mjs
node scripts/settings-toggle-labels.test.mjs
node scripts/manual-proxy-default.test.mjs
node scripts/model-base-prices-page.test.mjs
```

Expected: every command exits `0`; the routing tests print their `... contract ok` messages and the settings toggle test prints `Settings toggles render without inline state text.`

---

### Task 2: Move All Routing Safety Controls Into The Routing Page

**Files:**

- Create: `scripts/local-routing-boundary-controls.test.mjs`
- Modify: `scripts/local-routing-settings-form.test.mjs`
- Modify: `src/features/routing/localRoutingSettingsForm.ts`
- Modify: `src/features/routing/LocalRoutingSettingsFields.tsx`
- Modify: `src/features/routing/LocalRoutingSettingsEditor.tsx`
- Modify: `src/features/routing/LocalRoutingEditTab.tsx`

- [ ] **Step 1: Extend the form behavior test for the new boundary fields**

Add these assertions after `validResult` in `scripts/local-routing-settings-form.test.mjs`. Replace the existing `noCeilingResult` block with the explicit `disabledLimitResult` block below; leaving `maxRateLimitEnabled: true` with an empty value must now be invalid.

```javascript
assert.equal(validDraft.maxRateLimitEnabled, true);
assert.equal(validDraft.lowBalanceThresholdCny, "15");
assert.equal(validDraft.allowDepletedFallback, false);
assert.equal(validResult.value.lowBalanceThresholdCny, 15);
assert.equal(validResult.value.allowDepletedFallback, false);

const disabledLimitResult = parseLocalRoutingSettingsDraft({
  ...validDraft,
  maxRateLimitEnabled: false,
  maxRateMultiplier: "",
});
assert.equal(disabledLimitResult.ok, true);
assert.equal(disabledLimitResult.value.maxRateMultiplier, null);

for (const lowBalanceThresholdCny of ["", "-0.01", "not-a-number"]) {
  const result = parseLocalRoutingSettingsDraft({
    ...validDraft,
    lowBalanceThresholdCny,
  });
  assert.equal(result.ok, false);
  assert.match(result.errors.lowBalanceThresholdCny, /大于或等于 0/);
}

const depletedFallbackResult = parseLocalRoutingSettingsDraft({
  ...validDraft,
  allowDepletedFallback: true,
});
assert.equal(depletedFallbackResult.ok, true);
assert.equal(depletedFallbackResult.value.allowDepletedFallback, true);
```

- [ ] **Step 2: Add a source contract for placement and explicit saving**

Create `scripts/local-routing-boundary-controls.test.mjs`:

```javascript
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const editor = await readFile("src/features/routing/LocalRoutingSettingsEditor.tsx", "utf8");
const fields = await readFile("src/features/routing/LocalRoutingSettingsFields.tsx", "utf8");
const editTab = await readFile("src/features/routing/LocalRoutingEditTab.tsx", "utf8");

for (const label of [
  "倍率限制",
  "倍率上限",
  "候选分组",
  "默认低余额阈值",
  "余额耗尽兜底",
]) {
  assert.ok(fields.includes(label), `routing boundary should render ${label}`);
}

assert.match(fields, /suffix="×"/);
assert.match(fields, /suffix="CNY"/);
assert.match(fields, /关闭时自动路由不可用/);
assert.match(fields, /站点未单独设置时使用/);

assert.match(editor, /handleBoundarySave/);
assert.match(editor, /保存路由边界/);
assert.match(editor, /eligibleUnderMultiplierLimitCount/);
assert.match(editor, /enabledCandidateCount/);
assert.doesNotMatch(editor, /queueBoundaryAutoSave/);
assert.doesNotMatch(editor, /boundarySaveTimeoutRef/);

assert.match(editTab, /<LocalRoutingSettingsEditor workspace={workspace} \/>/);

console.log("local routing boundary controls contract ok");
```

- [ ] **Step 3: Run both tests and verify the intended RED**

Run:

```powershell
node scripts/local-routing-settings-form.test.mjs
node scripts/local-routing-boundary-controls.test.mjs
```

Expected: both fail. The behavior test fails because `maxRateLimitEnabled` is missing; the source contract fails because the routing fields do not yet contain “默认低余额阈值”.

- [ ] **Step 4: Extend the routing draft and parser**

In `src/features/routing/localRoutingSettingsForm.ts`, change the boundary types to this shape:

```typescript
export type LocalRoutingSettingsDraft = {
  maxRateLimitEnabled: boolean;
  maxRateMultiplier: string;
  defaultRoutingGroupPreset: RoutingGroupPreset;
  currentRoutingGroupFilter: RoutingGroupFilter;
  lowBalanceThresholdCny: string;
  allowDepletedFallback: boolean;
  scheduler: SchedulerDraft;
};

export type LocalRoutingSettingsErrorKey =
  | "maxRateMultiplier"
  | "lowBalanceThresholdCny"
  | "baseWeights"
  | keyof SchedulerAdvancedSettings;

export type LocalRoutingSettingsValue = {
  maxRateMultiplier: number | null;
  defaultRoutingGroupFilter: RoutingGroupFilter;
  lowBalanceThresholdCny: number;
  allowDepletedFallback: boolean;
  schedulerAdvancedSettings: SchedulerAdvancedSettings;
};

export type LocalRoutingBoundarySettingsValue = Pick<
  LocalRoutingSettingsValue,
  | "maxRateMultiplier"
  | "defaultRoutingGroupFilter"
  | "lowBalanceThresholdCny"
  | "allowDepletedFallback"
> & {
  schedulerAdvancedPatch: Pick<SchedulerAdvancedSettings, "multiplierMinConfidence">;
};
```

Return the new draft fields from `createLocalRoutingSettingsDraft()`:

```typescript
return {
  maxRateLimitEnabled: settings.maxRateMultiplier != null,
  maxRateMultiplier:
    settings.maxRateMultiplier == null ? "" : String(settings.maxRateMultiplier),
  defaultRoutingGroupPreset: routingGroupFilterToPreset(settings.defaultRoutingGroupFilter),
  currentRoutingGroupFilter: settings.defaultRoutingGroupFilter,
  lowBalanceThresholdCny: String(settings.lowBalanceThresholdCny),
  allowDepletedFallback: settings.allowDepletedFallback,
  scheduler,
};
```

Parse the new boundary values in `parseLocalRoutingBoundaryDraft()`:

```typescript
const maxRateMultiplier = draft.maxRateLimitEnabled
  ? parseRequiredNonNegativeNumber(
      draft.maxRateMultiplier,
      "倍率上限必须是大于或等于 0 的数字",
      (message) => {
        errors.maxRateMultiplier = message;
      },
    )
  : null;
const lowBalanceThresholdCny = parseRequiredNonNegativeNumber(
  draft.lowBalanceThresholdCny,
  "默认低余额阈值必须是大于或等于 0 的数字",
  (message) => {
    errors.lowBalanceThresholdCny = message;
  },
);
```

Return these fields in the parsed boundary value and in `parseLocalRoutingSettingsDraft()`:

```typescript
lowBalanceThresholdCny: boundary.value.lowBalanceThresholdCny,
allowDepletedFallback: boundary.value.allowDepletedFallback,
```

Add this helper beside `parseNullableNonNegativeNumber()` and remove `parseNullableNonNegativeNumber()` when it has no remaining callers:

```typescript
function parseRequiredNonNegativeNumber(
  rawValue: string,
  invalidMessage: string,
  reportError: (message: string) => void,
) {
  const trimmed = rawValue.trim();
  const value = Number(trimmed);
  if (!trimmed || !Number.isFinite(value) || value < 0) {
    reportError(invalidMessage);
    return 0;
  }
  return value;
}
```

- [ ] **Step 5: Render product-language boundary controls with visible units**

In `src/features/routing/LocalRoutingSettingsFields.tsx`, extend `LocalRoutingBoundaryFields` with callbacks for the three new draft fields:

```typescript
onMaxRateLimitEnabledChange: () => void;
onLowBalanceThresholdChange: (value: string) => void;
onAllowDepletedFallbackChange: () => void;
```

Replace the first two boundary rows with this ordering, then keep the existing `multiplierMinConfidence` boundary row after them:

```tsx
<CompactSettingRow
  label="倍率限制"
  description={
    draft.maxRateLimitEnabled
      ? "超过上限的 Key 不参与自动路由。"
      : "关闭时自动路由不可用。"
  }
>
  <SwitchControl
    ariaLabel="倍率限制"
    checked={draft.maxRateLimitEnabled}
    disabled={disabled}
    onCheckedChange={onMaxRateLimitEnabledChange}
    showLabel={false}
  />
</CompactSettingRow>
{draft.maxRateLimitEnabled ? (
  <CompactSettingRow label="倍率上限">
    <LabeledNumberInput
      hideLabel
      id="routing-max-rate-multiplier"
      label="倍率上限"
      value={draft.maxRateMultiplier}
      error={errors.maxRateMultiplier}
      disabled={disabled}
      min="0"
      step="0.01"
      placeholder="例如 1.00"
      suffix="×"
      onChange={onMaxRateMultiplierChange}
    />
  </CompactSettingRow>
) : null}
<CompactSettingRow label="候选分组">
  <SelectControl<RoutingGroupPreset>
    ariaLabel="候选分组"
    className="w-full sm:w-[220px]"
    disabled={disabled}
    value={draft.defaultRoutingGroupPreset}
    options={groupOptions}
    onChange={onGroupPresetChange}
  />
</CompactSettingRow>
<CompactSettingRow
  label="默认低余额阈值"
  description="站点未单独设置时使用。"
>
  <LabeledNumberInput
    hideLabel
    id="routing-low-balance-threshold"
    label="默认低余额阈值"
    value={draft.lowBalanceThresholdCny}
    error={errors.lowBalanceThresholdCny}
    disabled={disabled}
    min="0"
    step="0.01"
    suffix="CNY"
    onChange={onLowBalanceThresholdChange}
  />
</CompactSettingRow>
<CompactSettingRow
  label="余额耗尽兜底"
  description="开启后，余额耗尽的 Key 仅在没有其他候选时参与兜底。"
>
  <SwitchControl
    ariaLabel="余额耗尽兜底"
    checked={draft.allowDepletedFallback}
    disabled={disabled}
    onCheckedChange={onAllowDepletedFallbackChange}
    showLabel={false}
  />
</CompactSettingRow>
```

Extend `CompactSettingRow` so descriptions appear only when they carry effect, risk, or override information:

```tsx
function CompactSettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="grid min-h-12 grid-cols-1 items-center gap-2 border-b border-border px-3 py-2 last:border-b-0 sm:grid-cols-[minmax(0,1fr)_minmax(180px,260px)] sm:gap-4">
      <div className="min-w-0">
        <div className="text-sm font-medium text-slate-800">{label}</div>
        {description ? (
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</p>
        ) : null}
      </div>
      <div className="min-w-0 w-full justify-self-stretch sm:w-auto sm:justify-self-end">
        {children}
      </div>
    </div>
  );
}
```

Add `suffix?: string` to `LabeledNumberInput` and wrap its input like this:

```tsx
<div className="relative min-w-0">
  <input
    id={id}
    aria-describedby={error ? errorId : undefined}
    aria-invalid={Boolean(error)}
    className={cn(
      inputClassName,
      suffix && "pr-12",
      error && "border-rose-300 focus:border-rose-400 focus:ring-rose-100",
    )}
    disabled={disabled}
    max={max}
    min={min}
    placeholder={placeholder}
    step={step}
    type="number"
    value={value}
    onChange={(event) => onChange(event.target.value)}
  />
  {suffix ? (
    <span className="pointer-events-none absolute inset-y-0 right-2.5 flex items-center text-xs text-muted-foreground">
      {suffix}
    </span>
  ) : null}
</div>
```

- [ ] **Step 6: Replace boundary autosave with explicit save**

In `src/features/routing/LocalRoutingSettingsEditor.tsx`:

1. Accept `workspace`:

```typescript
export function LocalRoutingSettingsEditor({
  workspace,
}: {
  workspace: LocalRoutingWorkspace | null;
}) {
```

2. Import `LocalRoutingWorkspace`, remove `boundarySavePending`, `boundaryDraftVersionRef`, `boundarySaveTimeoutRef`, `queueBoundaryAutoSave()` and their cleanup code.

3. Calculate dirty state from the five boundary values:

```typescript
const boundaryDirty = useMemo(() => {
  if (!draft || !savedDraft) {
    return false;
  }
  return (
    draft.maxRateLimitEnabled !== savedDraft.maxRateLimitEnabled ||
    draft.maxRateMultiplier !== savedDraft.maxRateMultiplier ||
    draft.defaultRoutingGroupPreset !== savedDraft.defaultRoutingGroupPreset ||
    draft.lowBalanceThresholdCny !== savedDraft.lowBalanceThresholdCny ||
    draft.allowDepletedFallback !== savedDraft.allowDepletedFallback ||
    draft.scheduler.multiplierMinConfidence !==
      savedDraft.scheduler.multiplierMinConfidence
  );
}, [draft, savedDraft]);

const visibleBoundarySaveState: VisibleSaveState =
  boundarySaveState === "saving" || boundarySaveState === "error"
    ? boundarySaveState
    : boundaryDirty
      ? "dirty"
      : boundarySaveState;
```

4. Replace `handleBoundaryAutoSave()` with `handleBoundarySave()` using the existing operation-id stale-response guard:

```typescript
async function handleBoundarySave() {
  const currentSettings = settingsRef.current ?? settings;
  if (!currentSettings || !draft || boundarySaveState === "saving") {
    return;
  }
  const parsed = parseLocalRoutingBoundaryDraft(draft);
  if (!parsed.ok) {
    setFieldErrors((current) => ({ ...current, ...parsed.errors }));
    setBoundarySaveState("error");
    setBoundarySaveError("请修正标记的边界参数");
    return;
  }

  const operationId = boundarySaveOperationRef.current + 1;
  boundarySaveOperationRef.current = operationId;
  setBoundarySaveState("saving");
  setBoundarySaveError(null);
  try {
    const nextSettings = await updateSettings({
      ...appSettingsToUpdateInput(currentSettings),
      defaultRoutingStrategy: "automatic_balanced",
      maxRateMultiplier: parsed.value.maxRateMultiplier,
      defaultRoutingGroupFilter: parsed.value.defaultRoutingGroupFilter,
      lowBalanceThresholdCny: parsed.value.lowBalanceThresholdCny,
      allowDepletedFallback: parsed.value.allowDepletedFallback,
      schedulerAdvancedSettings: {
        ...currentSettings.schedulerAdvancedSettings,
        ...parsed.value.schedulerAdvancedPatch,
      },
    });
    if (operationId !== boundarySaveOperationRef.current) {
      return;
    }
    const nextDraft = createLocalRoutingSettingsDraft(nextSettings);
    applySettings(nextSettings);
    setDraft((current) => (current ? mergeBoundaryDraft(current, nextDraft) : nextDraft));
    setSavedDraft((current) =>
      current ? mergeBoundaryDraft(current, nextDraft) : nextDraft,
    );
    setBoundarySaveState("saved");
    setFieldErrors({});
    window.dispatchEvent(new Event(SETTINGS_UPDATED_EVENT));
    toast.success("路由边界已保存");
  } catch (requestError) {
    if (operationId !== boundarySaveOperationRef.current) {
      return;
    }
    const message = readError(requestError);
    setBoundarySaveState("error");
    setBoundarySaveError(message);
    toast.error("保存路由边界失败", message);
  }
}
```

Add this helper so saving a boundary does not discard unsaved scheduler edits:

```typescript
function mergeBoundaryDraft(
  current: LocalRoutingSettingsDraft,
  saved: LocalRoutingSettingsDraft,
): LocalRoutingSettingsDraft {
  return {
    ...current,
    maxRateLimitEnabled: saved.maxRateLimitEnabled,
    maxRateMultiplier: saved.maxRateMultiplier,
    defaultRoutingGroupPreset: saved.defaultRoutingGroupPreset,
    currentRoutingGroupFilter: saved.currentRoutingGroupFilter,
    lowBalanceThresholdCny: saved.lowBalanceThresholdCny,
    allowDepletedFallback: saved.allowDepletedFallback,
    scheduler: {
      ...current.scheduler,
      multiplierMinConfidence: saved.scheduler.multiplierMinConfidence,
    },
  };
}
```

5. Make all boundary changes local-only until the button is pressed:

```typescript
function updateBoundaryDraft(
  update: (current: LocalRoutingSettingsDraft) => LocalRoutingSettingsDraft,
) {
  setDraft((current) => (current ? update(current) : current));
  setBoundarySaveState((current) => (current === "saving" ? current : "idle"));
  setBoundarySaveError(null);
}
```

Use this helper for max-limit enablement, max value, group preset, low-balance value, depleted fallback, and `multiplierMinConfidence`. Clear the matching field error before updating.

Use these concrete callbacks:

```typescript
function updateMaxRateLimitEnabled() {
  updateBoundaryDraft((current) => ({
    ...current,
    maxRateLimitEnabled: !current.maxRateLimitEnabled,
  }));
  clearFieldError("maxRateMultiplier");
}

function updateMaxRateMultiplier(maxRateMultiplier: string) {
  clearFieldError("maxRateMultiplier");
  updateBoundaryDraft((current) => ({ ...current, maxRateMultiplier }));
}

function updateRoutingGroupPreset(defaultRoutingGroupPreset: RoutingGroupPreset) {
  updateBoundaryDraft((current) => ({ ...current, defaultRoutingGroupPreset }));
}

function updateLowBalanceThreshold(lowBalanceThresholdCny: string) {
  clearFieldError("lowBalanceThresholdCny");
  updateBoundaryDraft((current) => ({ ...current, lowBalanceThresholdCny }));
}

function updateAllowDepletedFallback() {
  updateBoundaryDraft((current) => ({
    ...current,
    allowDepletedFallback: !current.allowDepletedFallback,
  }));
}

function updateBoundaryNumericField(field: SchedulerNumericField, value: string) {
  if (field !== "multiplierMinConfidence") {
    updateNumericField(field, value);
    return;
  }
  clearFieldError(field);
  updateBoundaryDraft((current) => ({
    ...current,
    scheduler: { ...current.scheduler, [field]: value },
  }));
}
```

Remove `boundarySavePending` from the guard in `handleSubmit()`. Define disabled states as:

```typescript
const schedulerDisabled =
  loading || schedulerSaveState === "saving" || boundarySaveState === "saving";
const boundaryDisabled =
  loading || schedulerSaveState === "saving" || boundarySaveState === "saving";
```

6. Replace the `路由边界` card action with explicit status and save controls:

```tsx
action={
  <div className="flex flex-wrap items-center justify-end gap-2">
    <StatusBadge tone={saveStateTones[visibleBoundarySaveState]}>
      {saveStateLabels[visibleBoundarySaveState]}
    </StatusBadge>
    <Button
      disabled={boundarySaveState === "saving" || !boundaryDirty}
      type="button"
      onClick={() => void handleBoundarySave()}
    >
      <Save className="h-4 w-4" />
      保存路由边界
    </Button>
  </div>
}
```

7. Add the current-effect summary below `LocalRoutingBoundaryFields`:

```tsx
<div className="border-t border-border bg-slate-50/70 px-4 py-2 text-xs text-muted-foreground">
  当前生效：倍率范围内 {workspace?.summary.eligibleUnderMultiplierLimitCount ?? 0} / {workspace?.summary.enabledCandidateCount ?? 0} 把已启用 Key
  {boundaryDirty ? "；待保存设置尚未生效。" : "。"}
</div>
```

- [ ] **Step 7: Pass the workspace into the editor**

In `src/features/routing/LocalRoutingEditTab.tsx`, replace:

```tsx
<LocalRoutingSettingsEditor />
```

with:

```tsx
<LocalRoutingSettingsEditor workspace={workspace} />
```

- [ ] **Step 8: Run focused routing verification**

Run:

```powershell
node scripts/local-routing-settings-form.test.mjs
node scripts/local-routing-boundary-controls.test.mjs
node scripts/local-routing-page-layout.test.mjs
```

Expected: all three exit `0` and print their success messages.

- [ ] **Step 9: Commit the routing slice with exact paths**

```powershell
git add -- `
  scripts/local-routing-boundary-controls.test.mjs `
  scripts/local-routing-settings-form.test.mjs `
  src/features/routing/localRoutingSettingsForm.ts `
  src/features/routing/LocalRoutingSettingsFields.tsx `
  src/features/routing/LocalRoutingSettingsEditor.tsx `
  src/features/routing/LocalRoutingEditTab.tsx
git diff --cached --name-only
git commit -m "feat: move routing safety controls to routing page"
```

Expected staged paths: exactly the six paths above.

---

### Task 3: Move Collector Scheduling Into The Collection Center

**Files:**

- Create: `src/features/collectors/collectorSettingsForm.ts`
- Create: `src/features/collectors/CollectorAdvancedSettings.tsx`
- Create: `scripts/collector-settings-form.test.mjs`
- Create: `scripts/collector-settings-layout.test.mjs`
- Modify: `src/features/collectors/CollectorsPage.tsx`

- [ ] **Step 1: Write the preset and validation behavior test**

Create `scripts/collector-settings-form.test.mjs`:

```javascript
import assert from "node:assert/strict";
import { createServer } from "vite";

const vite = await createServer({
  appType: "custom",
  logLevel: "silent",
  server: { middlewareMode: true },
});

try {
  const formModule = await vite.ssrLoadModule(
    "/src/features/collectors/collectorSettingsForm.ts",
  );
  const {
    applyCollectorFrequencyPreset,
    createCollectorSettingsDraft,
    detectCollectorFrequencyPreset,
    parseCollectorSettingsDraft,
  } = formModule;

  const settings = {
    balanceIntervalMinutes: 5,
    groupRateIntervalMinutes: 20,
    modelListIntervalMinutes: 60,
    pricingRefreshIntervalMinutes: 60,
    collectorTimeoutSeconds: 15,
    collectorMaxConcurrency: 3,
  };

  const draft = createCollectorSettingsDraft(settings);
  assert.equal(detectCollectorFrequencyPreset(draft), "balanced");

  const timely = applyCollectorFrequencyPreset(draft, "timely");
  assert.deepEqual(
    {
      balance: timely.balanceIntervalMinutes,
      groupRate: timely.groupRateIntervalMinutes,
      models: timely.modelListIntervalMinutes,
      pricing: timely.pricingRefreshIntervalMinutes,
    },
    { balance: "2", groupRate: "10", models: "30", pricing: "30" },
  );
  assert.equal(timely.collectorTimeoutSeconds, "15");
  assert.equal(timely.collectorMaxConcurrency, "3");

  const resourceSaver = applyCollectorFrequencyPreset(draft, "resource_saver");
  assert.equal(resourceSaver.balanceIntervalMinutes, "15");
  assert.equal(resourceSaver.groupRateIntervalMinutes, "60");
  assert.equal(resourceSaver.modelListIntervalMinutes, "180");
  assert.equal(resourceSaver.pricingRefreshIntervalMinutes, "180");

  const valid = parseCollectorSettingsDraft(draft);
  assert.equal(valid.ok, true);
  assert.equal(valid.value.collectorMaxConcurrency, 3);

  for (const [field, value] of [
    ["balanceIntervalMinutes", "0"],
    ["groupRateIntervalMinutes", "1.5"],
    ["collectorTimeoutSeconds", "2"],
    ["collectorMaxConcurrency", "9"],
  ]) {
    const result = parseCollectorSettingsDraft({ ...draft, [field]: value });
    assert.equal(result.ok, false);
    assert.ok(result.errors[field]);
  }

  console.log("collector settings form behavior ok");
} finally {
  await vite.close();
}
```

- [ ] **Step 2: Write the collection-center layout contract**

Create `scripts/collector-settings-layout.test.mjs`:

```javascript
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const page = await readFile("src/features/collectors/CollectorsPage.tsx", "utf8");
const panel = await readFile(
  "src/features/collectors/CollectorAdvancedSettings.tsx",
  "utf8",
);

assert.match(page, /<CollectorAdvancedSettings \/>/);
assert.match(panel, /title="采集调度"/);
assert.match(panel, /及时/);
assert.match(panel, /均衡/);
assert.match(panel, /节省资源/);
assert.match(panel, /自定义周期与执行参数/);
assert.match(panel, /余额周期/);
assert.match(panel, /分组 \/ 倍率周期/);
assert.match(panel, /模型周期/);
assert.match(panel, /价格周期/);
assert.match(panel, /采集超时/);
assert.match(panel, /采集并发数/);
assert.match(panel, /保存采集设置/);
assert.match(panel, /恢复推荐值/);

console.log("collector settings layout contract ok");
```

- [ ] **Step 3: Run both tests and verify RED**

Run:

```powershell
node scripts/collector-settings-form.test.mjs
node scripts/collector-settings-layout.test.mjs
```

Expected: the form test fails because `collectorSettingsForm.ts` does not exist; the layout test fails because `CollectorAdvancedSettings.tsx` does not exist.

- [ ] **Step 4: Implement the pure collector settings form module**

Create `src/features/collectors/collectorSettingsForm.ts`:

```typescript
import type { AppSettings } from "@/lib/types/settings";

export type CollectorFrequencyPreset =
  | "timely"
  | "balanced"
  | "resource_saver"
  | "custom";

export type CollectorSettingsDraft = {
  balanceIntervalMinutes: string;
  groupRateIntervalMinutes: string;
  modelListIntervalMinutes: string;
  pricingRefreshIntervalMinutes: string;
  collectorTimeoutSeconds: string;
  collectorMaxConcurrency: string;
};

export type CollectorSettingsField = keyof CollectorSettingsDraft;
export type CollectorSettingsErrors = Partial<Record<CollectorSettingsField, string>>;

export type CollectorSettingsValue = {
  [Key in CollectorSettingsField]: number;
};

type ParsedCollectorSettingsDraft =
  | { ok: true; value: CollectorSettingsValue }
  | { ok: false; errors: CollectorSettingsErrors };

const FREQUENCY_PRESETS = {
  timely: {
    balanceIntervalMinutes: "2",
    groupRateIntervalMinutes: "10",
    modelListIntervalMinutes: "30",
    pricingRefreshIntervalMinutes: "30",
  },
  balanced: {
    balanceIntervalMinutes: "5",
    groupRateIntervalMinutes: "20",
    modelListIntervalMinutes: "60",
    pricingRefreshIntervalMinutes: "60",
  },
  resource_saver: {
    balanceIntervalMinutes: "15",
    groupRateIntervalMinutes: "60",
    modelListIntervalMinutes: "180",
    pricingRefreshIntervalMinutes: "180",
  },
} as const;

export function createCollectorSettingsDraft(
  settings: Pick<AppSettings, CollectorSettingsField>,
): CollectorSettingsDraft {
  return {
    balanceIntervalMinutes: String(settings.balanceIntervalMinutes),
    groupRateIntervalMinutes: String(settings.groupRateIntervalMinutes),
    modelListIntervalMinutes: String(settings.modelListIntervalMinutes),
    pricingRefreshIntervalMinutes: String(settings.pricingRefreshIntervalMinutes),
    collectorTimeoutSeconds: String(settings.collectorTimeoutSeconds),
    collectorMaxConcurrency: String(settings.collectorMaxConcurrency),
  };
}

export function detectCollectorFrequencyPreset(
  draft: CollectorSettingsDraft,
): CollectorFrequencyPreset {
  for (const [preset, values] of Object.entries(FREQUENCY_PRESETS)) {
    if (
      Object.entries(values).every(
        ([field, value]) => draft[field as keyof typeof values] === value,
      )
    ) {
      return preset as Exclude<CollectorFrequencyPreset, "custom">;
    }
  }
  return "custom";
}

export function applyCollectorFrequencyPreset(
  draft: CollectorSettingsDraft,
  preset: Exclude<CollectorFrequencyPreset, "custom">,
): CollectorSettingsDraft {
  return { ...draft, ...FREQUENCY_PRESETS[preset] };
}

export function createRecommendedCollectorSettingsDraft(): CollectorSettingsDraft {
  return {
    ...FREQUENCY_PRESETS.balanced,
    collectorTimeoutSeconds: "15",
    collectorMaxConcurrency: "3",
  };
}

export function parseCollectorSettingsDraft(
  draft: CollectorSettingsDraft,
): ParsedCollectorSettingsDraft {
  const errors: CollectorSettingsErrors = {};
  const value = {} as CollectorSettingsValue;
  const intervalFields: CollectorSettingsField[] = [
    "balanceIntervalMinutes",
    "groupRateIntervalMinutes",
    "modelListIntervalMinutes",
    "pricingRefreshIntervalMinutes",
  ];

  for (const field of intervalFields) {
    value[field] = parseInteger(draft[field], 1, Number.MAX_SAFE_INTEGER, errors, field);
  }
  value.collectorTimeoutSeconds = parseInteger(
    draft.collectorTimeoutSeconds,
    3,
    Number.MAX_SAFE_INTEGER,
    errors,
    "collectorTimeoutSeconds",
  );
  value.collectorMaxConcurrency = parseInteger(
    draft.collectorMaxConcurrency,
    1,
    8,
    errors,
    "collectorMaxConcurrency",
  );

  return Object.keys(errors).length > 0
    ? { ok: false, errors }
    : { ok: true, value };
}

function parseInteger(
  rawValue: string,
  min: number,
  max: number,
  errors: CollectorSettingsErrors,
  field: CollectorSettingsField,
) {
  const value = Number(rawValue.trim());
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    errors[field] = max === Number.MAX_SAFE_INTEGER
      ? `请输入大于或等于 ${min} 的整数`
      : `请输入 ${min} 到 ${max} 的整数`;
    return min;
  }
  return value;
}
```

- [ ] **Step 5: Implement the self-contained collector settings component**

Create `src/features/collectors/CollectorAdvancedSettings.tsx` with these responsibilities:

```tsx
import { useEffect, useMemo, useState } from "react";
import { ChevronDown, RotateCcw, Save } from "lucide-react";
import { Button, SectionCard, SelectControl, StatusBadge, useToast } from "@/components/ui";
import { getSettings, SETTINGS_UPDATED_EVENT, updateSettings } from "@/lib/api/settings";
import { readError } from "@/lib/errors";
import { appSettingsToUpdateInput, type AppSettings } from "@/lib/types/settings";
import { cn } from "@/lib/utils";
import {
  applyCollectorFrequencyPreset,
  createCollectorSettingsDraft,
  createRecommendedCollectorSettingsDraft,
  detectCollectorFrequencyPreset,
  parseCollectorSettingsDraft,
  type CollectorFrequencyPreset,
  type CollectorSettingsDraft,
  type CollectorSettingsErrors,
  type CollectorSettingsField,
} from "./collectorSettingsForm";

const presetOptions = [
  { value: "timely", label: "及时" },
  { value: "balanced", label: "均衡" },
  { value: "resource_saver", label: "节省资源" },
  { value: "custom", label: "自定义" },
] satisfies Array<{ value: CollectorFrequencyPreset; label: string }>;

export function CollectorAdvancedSettings() {
  const toast = useToast();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [draft, setDraft] = useState<CollectorSettingsDraft | null>(null);
  const [savedDraft, setSavedDraft] = useState<CollectorSettingsDraft | null>(null);
  const [errors, setErrors] = useState<CollectorSettingsErrors>({});
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, []);

  const dirty = useMemo(
    () => Boolean(draft && savedDraft && JSON.stringify(draft) !== JSON.stringify(savedDraft)),
    [draft, savedDraft],
  );
  const preset = draft ? detectCollectorFrequencyPreset(draft) : "custom";

  async function load() {
    setLoadError(null);
    try {
      const nextSettings = await getSettings();
      const nextDraft = createCollectorSettingsDraft(nextSettings);
      setSettings(nextSettings);
      setDraft(nextDraft);
      setSavedDraft(nextDraft);
      setErrors({});
    } catch (requestError) {
      setLoadError(readError(requestError));
    }
  }

  function updateField(field: CollectorSettingsField, value: string) {
    setErrors((current) => {
      const next = { ...current };
      delete next[field];
      return next;
    });
    setDraft((current) => (current ? { ...current, [field]: value } : current));
  }

  function selectPreset(value: CollectorFrequencyPreset) {
    if (!draft || value === "custom") {
      return;
    }
    setDraft(applyCollectorFrequencyPreset(draft, value));
    setErrors({});
  }

  async function save() {
    if (!settings || !draft || saving) {
      return;
    }
    const parsed = parseCollectorSettingsDraft(draft);
    if (!parsed.ok) {
      setErrors(parsed.errors);
      toast.error("保存采集设置失败", "请修正标记的参数");
      return;
    }
    setSaving(true);
    try {
      const nextSettings = await updateSettings({
        ...appSettingsToUpdateInput(settings),
        ...parsed.value,
      });
      const nextDraft = createCollectorSettingsDraft(nextSettings);
      setSettings(nextSettings);
      setDraft(nextDraft);
      setSavedDraft(nextDraft);
      setErrors({});
      window.dispatchEvent(new Event(SETTINGS_UPDATED_EVENT));
      toast.success("采集设置已保存");
    } catch (requestError) {
      toast.error("保存采集设置失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  if (!draft) {
    return (
      <SectionCard title="采集调度">
        <div className="flex items-center justify-between gap-3 text-sm text-muted-foreground">
          <span>{loadError ?? "正在读取采集设置..."}</span>
          {loadError ? <Button variant="outline" onClick={() => void load()}>重试</Button> : null}
        </div>
      </SectionCard>
    );
  }

  return (
    <SectionCard
      title="采集调度"
      contentClassName="p-0"
      action={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <StatusBadge tone={dirty ? "warning" : "healthy"}>
            {dirty ? "待保存" : "已同步"}
          </StatusBadge>
          <Button
            type="button"
            variant="outline"
            disabled={saving}
            onClick={() => {
              setDraft(createRecommendedCollectorSettingsDraft());
              setErrors({});
            }}
          >
            <RotateCcw className="h-4 w-4" />
            恢复推荐值
          </Button>
          <Button type="button" disabled={saving || !dirty} onClick={() => void save()}>
            <Save className="h-4 w-4" />
            保存采集设置
          </Button>
        </div>
      }
    >
      <SettingRow label="采集频率">
        <SelectControl<CollectorFrequencyPreset>
          ariaLabel="采集频率"
          className="w-full sm:w-[220px]"
          value={preset}
          options={presetOptions}
          onChange={selectPreset}
        />
      </SettingRow>
      <details className="group border-t border-border">
        <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-3 text-sm font-medium text-slate-700">
          自定义周期与执行参数
          <ChevronDown className="h-4 w-4 text-muted-foreground transition group-open:rotate-180" />
        </summary>
        <div className="grid gap-3 border-t border-border p-3 sm:grid-cols-2 lg:grid-cols-3">
          <NumberField label="余额周期" field="balanceIntervalMinutes" suffix="分钟" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="分组 / 倍率周期" field="groupRateIntervalMinutes" suffix="分钟" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="模型周期" field="modelListIntervalMinutes" suffix="分钟" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="价格周期" field="pricingRefreshIntervalMinutes" suffix="分钟" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="采集超时" field="collectorTimeoutSeconds" suffix="秒" draft={draft} errors={errors} onChange={updateField} />
          <NumberField label="采集并发数" field="collectorMaxConcurrency" draft={draft} errors={errors} onChange={updateField} />
        </div>
      </details>
    </SectionCard>
  );
}
```

Implement `SettingRow` and `NumberField` at the bottom of the same file:

```tsx
function SettingRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid min-h-12 items-center gap-2 px-3 py-2 sm:grid-cols-[minmax(0,1fr)_minmax(180px,260px)] sm:gap-4">
      <span className="text-sm font-medium text-slate-800">{label}</span>
      <div className="min-w-0 w-full justify-self-stretch sm:w-auto sm:justify-self-end">
        {children}
      </div>
    </div>
  );
}

function NumberField({
  label,
  field,
  suffix,
  draft,
  errors,
  onChange,
}: {
  label: string;
  field: CollectorSettingsField;
  suffix?: string;
  draft: CollectorSettingsDraft;
  errors: CollectorSettingsErrors;
  onChange: (field: CollectorSettingsField, value: string) => void;
}) {
  const id = `collector-setting-${field}`;
  const errorId = `${id}-error`;
  const min = field === "collectorTimeoutSeconds" ? 3 : 1;
  const max = field === "collectorMaxConcurrency" ? 8 : undefined;
  const error = errors[field];

  return (
    <label className="grid min-w-0 gap-1.5" htmlFor={id}>
      <span className="text-xs font-medium text-slate-700">{label}</span>
      <div className="relative min-w-0">
        <input
          id={id}
          aria-describedby={error ? errorId : undefined}
          aria-invalid={Boolean(error)}
          className={cn(
            "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-white px-2.5 text-sm text-slate-800 outline-none focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]",
            suffix && "pr-12",
            error && "border-rose-300 focus:border-rose-400 focus:ring-rose-100",
          )}
          min={min}
          max={max}
          step="1"
          type="number"
          value={draft[field]}
          onChange={(event) => onChange(field, event.target.value)}
        />
        {suffix ? (
          <span className="pointer-events-none absolute inset-y-0 right-2.5 flex items-center text-xs text-muted-foreground">
            {suffix}
          </span>
        ) : null}
      </div>
      {error ? <span id={errorId} className="text-xs text-rose-700">{error}</span> : null}
    </label>
  );
}
```

Import `type ReactNode` from React for the `SettingRow` signature.

- [ ] **Step 6: Mount the component in the collection center**

In `src/features/collectors/CollectorsPage.tsx`, import:

```typescript
import { CollectorAdvancedSettings } from "./CollectorAdvancedSettings";
```

In the right-hand `space-y-3` column, insert the component immediately before the existing `InspectorPanel title="高级选项"`:

```tsx
<CollectorAdvancedSettings />
```

- [ ] **Step 7: Run collector GREEN verification**

Run:

```powershell
node scripts/collector-settings-form.test.mjs
node scripts/collector-settings-layout.test.mjs
```

Expected: both exit `0` and print their success messages.

- [ ] **Step 8: Commit the collector slice with exact paths**

```powershell
git add -- `
  src/features/collectors/collectorSettingsForm.ts `
  src/features/collectors/CollectorAdvancedSettings.tsx `
  src/features/collectors/CollectorsPage.tsx `
  scripts/collector-settings-form.test.mjs `
  scripts/collector-settings-layout.test.mjs
git diff --cached --name-only
git commit -m "feat: move collector tuning into collection center"
```

Expected staged paths: exactly the five paths above.

---

### Task 4: Reduce Settings To Application-Level Controls

**Files:**

- Create: `scripts/settings-information-architecture.test.mjs`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/components/shell/PageScaffold.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/app/routes.tsx`
- Modify: `scripts/settings-autosave.test.mjs`
- Modify: `scripts/settings-card-density.test.mjs`
- Modify: `scripts/settings-toggle-labels.test.mjs`
- Modify: `scripts/local-routing-automatic-settings.test.mjs`

- [ ] **Step 1: Write the final settings ownership contract**

Create `scripts/settings-information-architecture.test.mjs`:

```javascript
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settings = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const scaffold = await readFile("src/components/shell/PageScaffold.tsx", "utf8");
const app = await readFile("src/app/App.tsx", "utf8");
const pricing = await readFile("src/features/pricing/PricingPage.tsx", "utf8");
const collectors = await readFile("src/features/collectors/CollectorsPage.tsx", "utf8");

for (const label of [
  "倍率上限",
  "默认路由分组",
  "低余额阈值",
  "余额采集周期（分钟）",
  "分组 / 倍率采集周期（分钟）",
  "模型采集周期（分钟）",
  "价格刷新周期（分钟）",
  "模型基准价格",
  "采集超时（秒）",
  "采集并发数",
  "允许余额耗尽兜底",
]) {
  assert.ok(!settings.includes(`label="${label}"`), `settings should not render ${label}`);
}

assert.match(settings, /title="网络与代理"/);
assert.match(settings, /label="默认网络出口"/);
assert.match(settings, /title="数据"/);
assert.match(settings, /title="高级"/);
assert.match(settings, /label="显示高级工具"/);
assert.match(settings, /在侧边栏显示采集中心/);
assert.doesNotMatch(settings, /onOpenModelBasePrices/);

assert.match(scaffold, /max-w-\[1080px\]/);
assert.doesNotMatch(app, /<SettingsPage onOpenModelBasePrices=/);
assert.match(app, /return <SettingsPage \/>/);
assert.match(pricing, /模型基准价格/);
assert.match(collectors, /<CollectorAdvancedSettings \/>/);

console.log("settings information architecture contract ok");
```

- [ ] **Step 2: Run the contract and verify RED**

Run:

```powershell
node scripts/settings-information-architecture.test.mjs
```

Expected: FAIL on the first misplaced row, `settings should not render 倍率上限`.

- [ ] **Step 3: Shrink SettingsFormState without changing persisted data**

In `src/features/settings/SettingsPage.tsx`, reduce the form state to:

```typescript
type SettingsFormState = {
  localProxyPort: string;
  collectorProxyMode: CollectorProxyMode;
  collectorProxyUrl: string;
  trayBehavior: TrayBehavior;
  developerModeEnabled: boolean;
};
```

Keep the full `fallbackSettings: AppSettings`; it is still required by `getSettings()` fallback and must retain every `AppSettings` field.

Remove these now-unused items:

- `Coins` icon import.
- `SettingsPageProps` and the `onOpenModelBasePrices` parameter.
- `PricingGroupType`, `RoutingGroupFilter`, `RoutingGroupPreset` and preset conversion helpers.
- `handleAllowDepletedFallbackToggle()`.
- `SettingsNumberInput`.
- All route, balance, collector schedule, timeout, concurrency and base-price fields from `settingsToForm()` and `formToInput()`.

Keep `formToInput()` safe by spreading the current settings first:

```typescript
function formToInput(form: SettingsFormState, settings: AppSettings): UpdateSettingsInput {
  return {
    ...appSettingsToUpdateInput(settings),
    localProxyPort: Number(form.localProxyPort),
    defaultRoutingStrategy: "automatic_balanced",
    collectorProxyMode: form.collectorProxyMode,
    collectorProxyUrl:
      form.collectorProxyMode === "manual" && form.collectorProxyUrl.trim()
        ? form.collectorProxyUrl.trim()
        : null,
    schedulerAdvancedSettings: settings.schedulerAdvancedSettings,
    trayBehavior: form.trayBehavior,
    developerModeEnabled: form.developerModeEnabled,
  };
}
```

This spread is the compatibility boundary: moving a field out of Settings must never reset its persisted value.

- [ ] **Step 4: Replace the mixed section with three focused sections**

Delete the full `SectionCard title="采集与路由"` block. Insert these sections after `本地代理`:

```tsx
<SectionCard contentClassName="p-0" title="网络与代理">
  <SettingRow
    control={
      <div className="grid w-full min-w-0 gap-2">
        <SelectControl
          ariaLabel="默认网络出口"
          className={inputClassName}
          value={form.collectorProxyMode}
          options={Object.entries(collectorProxyModeLabels).map(([value, label]) => ({
            value: value as CollectorProxyMode,
            label,
          }))}
          onChange={(collectorProxyMode) =>
            void handleCollectorProxyModeChange(collectorProxyMode)
          }
        />
        {form.collectorProxyMode === "manual" ? (
          <input
            className={inputClassName}
            placeholder={DEFAULT_MANUAL_PROXY_URL}
            value={form.collectorProxyUrl}
            onChange={(event) =>
              setForm({ ...form, collectorProxyUrl: event.target.value })
            }
            onBlur={() => void commitSettingsForm(form, "默认网络出口已更新")}
          />
        ) : null}
      </div>
    }
    description="采集与转发默认使用；站点可单独覆盖。"
    label="默认网络出口"
  />
</SectionCard>
```

Rename `数据与安全` to `数据` and keep its existing actionable directory row unchanged.

After the data section, add:

```tsx
<SectionCard contentClassName="p-0" title="高级">
  <SettingRow
    control={
      <SwitchControl
        ariaLabel="显示高级工具"
        checked={form.developerModeEnabled}
        disabled={saving || loading}
        onCheckedChange={() => void handleDeveloperModeToggle()}
        showLabel={false}
      />
    }
    description="在侧边栏显示采集中心。"
    label="显示高级工具"
  />
</SectionCard>
```

Update save messages:

```typescript
nextForm.developerModeEnabled ? "高级工具已显示" : "高级工具已隐藏"
```

and:

```typescript
await commitSettingsForm(nextForm, "默认网络出口已更新");
```

- [ ] **Step 5: Remove the settings-to-price navigation prop**

Change the settings page signature to:

```typescript
export function SettingsPage() {
```

In `src/app/App.tsx`, change:

```tsx
return <SettingsPage onOpenModelBasePrices={() => navigateTo("modelBasePrices")} />;
```

to:

```tsx
return <SettingsPage />;
```

Do not modify the existing `PricingPage onOpenModelBasePrices` path; it is now the only business entry.

- [ ] **Step 6: Constrain the settings layout width**

In `src/components/shell/PageScaffold.tsx`, change only the `width === "settings"` class branch:

```typescript
width === "settings"
  ? "relative mx-auto flex min-w-0 w-full max-w-[1080px] flex-col gap-[var(--shell-page-gap)]"
  : "relative flex min-h-full min-w-0 w-full flex-col gap-[var(--shell-page-gap)]"
```

- [ ] **Step 7: Update visible route terminology**

In `src/app/routes.tsx`, change the collectors description from:

```typescript
description: "开发者模式下调试采集、登录态和快照识别",
```

to:

```typescript
description: "高级工具中调试采集、登录态和快照识别",
```

Keep the internal `developerModeEnabled` field and navigation filter unchanged.

- [ ] **Step 8: Repair the focused settings regression tests**

In `scripts/settings-autosave.test.mjs`, replace the second assertion with:

```javascript
assert.ok(
  settingsPageSource.includes("commitSettingsForm") &&
    settingsPageSource.includes("handleCollectorProxyModeChange") &&
    settingsPageSource.includes("handleDeveloperModeToggle"),
  "settings page should autosave each remaining application-level setting",
);
```

In `scripts/settings-toggle-labels.test.mjs`, read the routing fields as well:

```javascript
const routingFieldsSource = await readFile(
  "src/features/routing/LocalRoutingSettingsFields.tsx",
  "utf8",
);
```

Verify only `显示高级工具` in `SettingsPage`, and verify `余额耗尽兜底` in `LocalRoutingSettingsFields`. Both switches must contain `showLabel={false}`.

In `scripts/settings-card-density.test.mjs`, read `PageScaffold.tsx` and add:

```javascript
assert.ok(
  pageScaffoldSource.includes("max-w-[1080px]"),
  "settings width should remain bounded on wide desktop windows",
);
```

Add the ownership assertions to `scripts/local-routing-automatic-settings.test.mjs` now that the duplicated rows are removed:

```javascript
for (const label of ["低余额阈值", "允许余额耗尽兜底"]) {
  assert.doesNotMatch(settingsPage, new RegExp(`label="${label}"`));
}
assert.match(settingsFields, /默认低余额阈值/);
assert.match(settingsFields, /余额耗尽兜底/);
```

- [ ] **Step 9: Run the complete focused UI contract set**

Run:

```powershell
node scripts/settings-information-architecture.test.mjs
node scripts/settings-autosave.test.mjs
node scripts/settings-card-density.test.mjs
node scripts/settings-data-dir.test.mjs
node scripts/settings-local-access-key.test.mjs
node scripts/settings-local-proxy-copy.test.mjs
node scripts/settings-toggle-labels.test.mjs
node scripts/manual-proxy-default.test.mjs
node scripts/local-routing-automatic-settings.test.mjs
node scripts/local-routing-boundary-controls.test.mjs
node scripts/collector-settings-layout.test.mjs
node scripts/model-base-prices-page.test.mjs
node scripts/model-base-prices-header.test.mjs
```

Expected: every command exits `0`; the new IA test prints `settings information architecture contract ok`.

- [ ] **Step 10: Commit the settings slice with exact paths**

```powershell
git add -- `
  src/features/settings/SettingsPage.tsx `
  src/components/shell/PageScaffold.tsx `
  src/app/App.tsx `
  src/app/routes.tsx `
  scripts/settings-information-architecture.test.mjs `
  scripts/settings-autosave.test.mjs `
  scripts/settings-card-density.test.mjs `
  scripts/settings-toggle-labels.test.mjs `
  scripts/local-routing-automatic-settings.test.mjs
git diff --cached --name-only
git commit -m "refactor: simplify settings information architecture"
```

Expected staged paths: exactly the nine paths above.

---

### Task 5: Align Product Documentation

**Files:**

- Modify: `docs/PROJECT_PLAN.md:39-46`

- [ ] **Step 1: Update the page-responsibility definitions**

Replace the relevant information-architecture bullets in `docs/PROJECT_PLAN.md` with:

```markdown
- 路由规则：回答“为什么请求会走这把 Key？”，管理自动调度、候选分组、倍率限制、低余额边界、耗尽兜底、模型映射和路由模拟解释。
- 价格 / 倍率：回答“哪个站点更便宜？”，展示模型价格、分组倍率和模型可用性的跨站点矩阵，并管理模型基准价格。
- 信息采集（高级工具）：回答“采集器为什么得到这些结果？”，运行站点采集任务，查看快照与任务记录，并在高级区域调整采集频率、超时和并发。
- 设置：回答“应用本身如何运行？”，管理本地代理、默认网络出口、数据目录和高级工具可见性。
```

Keep `信息采集` out of the normal first-level navigation; this documentation describes ownership, not default visibility.

- [ ] **Step 2: Verify documentation no longer assigns collection timing to Settings**

Run:

```powershell
rg -n "设置：|路由规则：|价格 / 倍率：|信息采集（高级工具）" docs/PROJECT_PLAN.md
```

Expected: exactly one current IA bullet for each of the four page names, with no remaining “设置……采集周期和阈值” wording.

- [ ] **Step 3: Commit the documentation with its exact path**

```powershell
git add -- docs/PROJECT_PLAN.md
git diff --cached --name-only
git commit -m "docs: align settings page responsibilities"
```

Expected staged path: only `docs/PROJECT_PLAN.md`.

---

### Task 6: Full Verification And Visual QA

**Files:**

- Verify only: all changed paths

- [ ] **Step 1: Run all new and directly affected contract tests**

Run:

```powershell
node scripts/local-routing-settings-form.test.mjs
node scripts/local-routing-boundary-controls.test.mjs
node scripts/local-routing-automatic-settings.test.mjs
node scripts/local-routing-page-layout.test.mjs
node scripts/collector-settings-form.test.mjs
node scripts/collector-settings-layout.test.mjs
node scripts/settings-information-architecture.test.mjs
node scripts/settings-autosave.test.mjs
node scripts/settings-card-density.test.mjs
node scripts/settings-data-dir.test.mjs
node scripts/settings-local-access-key.test.mjs
node scripts/settings-local-proxy-copy.test.mjs
node scripts/settings-toggle-labels.test.mjs
node scripts/manual-proxy-default.test.mjs
node scripts/model-base-prices-page.test.mjs
node scripts/model-base-prices-editable-table.test.mjs
node scripts/model-base-prices-header.test.mjs
```

Expected: every command exits `0`.

- [ ] **Step 2: Run the required TypeScript and Vite build**

Run:

```powershell
pnpm build
```

Expected: `tsc --noEmit` and `vite build` both complete successfully.

- [ ] **Step 3: Audit diff hygiene and scope**

Run:

```powershell
git diff --check
git status --short
git diff --name-only HEAD~4..HEAD
```

Expected:

- `git diff --check` exits `0` with no whitespace errors.
- No user-owned dashboard, collector backend, pricing backend, or Rust changes are included in these four commits.
- The implementation commit range contains only the files enumerated in this plan.

- [ ] **Step 4: Launch current source through Tauri dev**

First inspect the default frontend port:

```powershell
Get-NetTCPConnection -LocalPort 1430 -ErrorAction SilentlyContinue
```

Then run:

```powershell
pnpm tauri:dev
```

Expected: the Vite frontend responds at `http://127.0.0.1:1430/` and a fresh `relay-pool-desktop.exe` process opens. Do not launch `src-tauri/target/debug/relay-pool-desktop.exe` directly because it can display stale `dist` output.

- [ ] **Step 5: Visually verify Settings at desktop widths**

Use the running app and capture screenshots at `1440x900` and `2048x1080`. Verify:

- The settings content remains centered and no wider than 1080px.
- Visible sections are `本地代理`, `网络与代理`, `数据`, `高级`, `关于`.
- The settings page contains no routing constraints, collector timing, timeout, concurrency, or model-base-price entry.
- `默认网络出口` and its manual URL field align with their row at both widths.
- `显示高级工具` is the only switch in the new advanced section.
- No label, description, input, switch, or button overlaps or clips.

- [ ] **Step 6: Visually verify the migrated workflows**

In the same live app:

1. Open `路由规则 > 编辑`.
2. Confirm the `路由边界` section contains the explicit limit switch, `×` suffix, candidate group, `CNY` threshold, depleted fallback, current active candidate count, and `保存路由边界`.
3. Change a harmless draft value and confirm the page says `待保存` without changing the current-effect count until save.
4. Cancel the draft by navigating away and back; confirm persisted values reload.
5. Enable `显示高级工具`, open `信息采集`, and confirm `采集调度` is visible.
6. Select each frequency preset and confirm only the four interval values change; timeout and concurrency remain unchanged.
7. Expand `自定义周期与执行参数`, enter an invalid concurrency value `9`, and confirm an inline validation error appears on save.
8. Open `价格 / 倍率` and confirm `模型基准价格` remains available there and returns to the pricing page.

- [ ] **Step 7: Final exact-path status audit**

Run:

```powershell
git status --short -- `
  docs/PROJECT_PLAN.md `
  src/features/settings/SettingsPage.tsx `
  src/features/routing/localRoutingSettingsForm.ts `
  src/features/routing/LocalRoutingSettingsFields.tsx `
  src/features/routing/LocalRoutingSettingsEditor.tsx `
  src/features/routing/LocalRoutingEditTab.tsx `
  src/features/collectors/collectorSettingsForm.ts `
  src/features/collectors/CollectorAdvancedSettings.tsx `
  src/features/collectors/CollectorsPage.tsx `
  src/components/shell/PageScaffold.tsx `
  src/app/App.tsx `
  src/app/routes.tsx `
  scripts/local-routing-boundary-controls.test.mjs `
  scripts/collector-settings-form.test.mjs `
  scripts/collector-settings-layout.test.mjs `
  scripts/settings-information-architecture.test.mjs
```

Expected: no unstaged implementation changes remain in the plan scope. Unrelated pre-existing worktree changes may still exist and must not be reverted or staged.

---

## Acceptance Checklist

- Settings no longer acts as a route-policy, collector-engine, or price-data dumping ground.
- Every removed setting still has exactly one actionable owner page.
- Existing persisted values survive the relocation without schema or backend changes.
- High-risk routing boundaries require explicit save and expose their current effect.
- Collector presets are deterministic, custom values remain possible, and invalid engine parameters are rejected inline.
- Model base prices are reachable only from `价格 / 倍率`.
- Wide-window scanning distance is bounded by the 1080px settings layout.
- Focused tests, `pnpm build`, desktop runtime checks, and screenshots all provide current-source evidence.
