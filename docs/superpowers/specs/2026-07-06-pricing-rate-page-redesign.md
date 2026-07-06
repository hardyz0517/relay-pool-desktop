# Pricing / Rate Page Redesign

Date: 2026-07-06

## Goal

Redesign the `价格 / 倍率` page around concrete model-by-model price comparison. The page must help the user answer one question quickly:

> For this exact model, which station and station group is cheapest after accounting for group multiplier and recharge multiplier?

The page must not compare different models against each other, must not use wildcard rows such as `gpt-*`, and must not depend on station model discovery as the source of truth for which models exist.

## Product Decisions

- A model is a concrete official model, for example `gpt-5.5`, `claude-sonnet-4`, or `gemini-2.5-pro`.
- The default model set comes from a curated official model catalog, not from station discovery.
- The curated catalog is not the full provider catalog. It should include the models the user cares about for Relay Pool comparison.
- Station discovery is evidence only. It can mark a station/model as discovered, unverified, or unavailable, but it does not create wildcard model rows.
- Each model renders as an independent comparison section.
- Rows inside a model section represent `station + station group`.
- The same station may appear multiple times for the same model when it has multiple relevant groups with different multipliers.

## Pricing Model

Each comparison row combines three values:

- `officialInputPrice` and `officialOutputPrice`: official API price for the concrete model.
- `groupMultiplier`: the station group multiplier collected from Sub2API/NewAPI group facts.
- `creditPerCny`: the station recharge multiplier already stored on `Station.creditPerCny`.

Computed fields:

```text
effectiveMultiplier = groupMultiplier / creditPerCny
estimatedInputCny = officialInputPrice * effectiveMultiplier
estimatedOutputCny = officialOutputPrice * effectiveMultiplier
```

Pricing assumes the user's current business rule:

```text
1 RMB of station credit is treated as equivalent to 1 USD of official API credit.
```

The UI must show the raw values and the computed value:

- group multiplier, for example `0.8x`
- recharge multiplier, for example `10 额度/元`
- effective multiplier, for example `0.08x`
- estimated input price
- estimated output price

The default sort is applied inside each model section only:

```text
estimatedOutputCny ascending
```

## Data Design

### Official Model Catalog

Add a small frontend catalog module for the first implementation slice.

Suggested path:

```text
src/features/pricing/officialModelCatalog.ts
```

Catalog entry shape:

```ts
type OfficialModelCatalogEntry = {
  provider: "openai" | "anthropic" | "google";
  modelId: string;
  displayName: string;
  officialInputPrice: number;
  officialOutputPrice: number;
  currency: "USD";
  unit: "per_1m_tokens";
  aliases: string[];
  groupMatchers: string[];
  enabledByDefault: boolean;
};
```

`groupMatchers` maps model catalog entries to station groups. Examples:

- OpenAI/GPT models match OpenAI/GPT/default-style groups.
- Anthropic/Claude models match Anthropic/Claude groups.
- Google/Gemini models match Google/Gemini groups.

The matcher should support multiple groups per provider family. It must not collapse all GPT groups into one group.

### Comparison View Model

Create or replace a focused helper for pricing comparison.

Suggested path:

```text
src/features/pricing/pricingComparisonViewModel.ts
```

Input:

- official model catalog entries
- stations
- station group bindings
- group rate records
- pricing rules if they still exist for manually entered complete prices
- optional model discovery evidence from collector snapshots when available

Output:

```ts
type PricingModelSection = {
  modelId: string;
  displayName: string;
  provider: "openai" | "anthropic" | "google";
  officialInputPrice: number;
  officialOutputPrice: number;
  rows: PricingComparisonRow[];
};

type PricingComparisonRow = {
  stationId: string;
  stationName: string;
  groupBindingId: string | null;
  groupName: string;
  groupMultiplier: number | null;
  creditPerCny: number;
  effectiveMultiplier: number | null;
  estimatedInputCny: number | null;
  estimatedOutputCny: number | null;
  evidenceStatus: "discovered" | "unverified" | "unavailable";
  evidenceLabel: string;
  source: string;
  updatedAt: string;
};
```

Rules:

- If `groupMultiplier` is missing, the row can appear only when useful for diagnostics and must not be sorted as a priced row.
- If `creditPerCny` is missing or invalid, treat it as `1` and label the row as using default recharge multiplier.
- If a station has multiple matching groups for a model, show all matching groups.
- If a station has no matching group for a model, do not create a priced row for that station.
- If model discovery does not confirm availability, keep the row but label evidence as `未验证`.

## UI Layout

The page uses a dense desktop-tool layout, not a marketing/dashboard hero layout.

### Top Area

Keep the metric cards only if they answer operational questions. Recommended metrics:

- `覆盖模型`: number of enabled catalog models with at least one comparable row.
- `可比价分组`: number of priced `station + group` rows.
- `最低折算倍率`: lowest effective multiplier across visible rows, with the model name.

Avoid broad cards that repeat "暂无数据" when the table has useful partial data.

### Toolbar

Use one compact toolbar:

- provider segmented filter: `全部 / OpenAI / Anthropic / Google`
- model search input with accessible label
- station filter select with accessible label
- `只看已验证可用` toggle

Sorting is fixed to section-local estimated output price ascending for the first implementation. Do not add a global sort that mixes models.

### Model Sections

Each visible model renders as a section:

```text
GPT 5.5
官方价: 输入 $x.xx / 输出 $y.yy per 1M tokens
```

Then a dense table:

| Column | Purpose |
| --- | --- |
| 站点 | station name |
| 分组 | station group name |
| 分组倍率 | raw group multiplier |
| 充值倍率 | `creditPerCny` |
| 折算倍率 | `groupMultiplier / creditPerCny` |
| 输入价 | estimated input price |
| 输出价 | estimated output price |
| 证据 | discovered, unverified, or unavailable |
| 更新时间 | latest group/rate timestamp |

Rows are sorted inside the current model section only. The cheapest row may receive a restrained green background or left accent, but text values must remain the primary signal.

### Empty States

Use specific empty states:

- No catalog models enabled: `未配置默认模型目录`
- No station groups collected: `尚未采集分组倍率`
- A specific model has no matching station groups: `暂无可比价分组`
- Filters hide all rows: `当前筛选无结果`

Do not show generic "暂无对比数据" when the reason is known.

## UX Review Requirements

The design was checked against `ui-ux-pro-max` guidance for dense comparison tables. The implementation must follow these constraints:

- Do not communicate cheap/expensive state by color alone.
- Keep high contrast text in light mode.
- Use horizontal overflow only within a model table if the viewport is too narrow.
- Keep keyboard focus visible for toolbar controls.
- Use real labels or `aria-label` for search, selects, toggles, and icon buttons.
- Use stable hover states that do not shift layout.
- Memoize filtering, grouping, and sorting calculations with `useMemo`.
- Keep data shaping in helper/view-model modules rather than expanding `PricingPage.tsx`.

## Architecture

Recommended frontend modules:

- `officialModelCatalog.ts`: curated model catalog and matching metadata.
- `pricingComparisonViewModel.ts`: joins stations, group facts, catalog entries, and evidence into model sections.
- `PricingPage.tsx`: loads data, owns toolbar state, renders model sections.
- `pricingFormatters.ts` or existing local helpers: formats money, multipliers, time, evidence labels.

Backend changes are not required for the first redesign slice if existing station, group binding, and group rate APIs provide enough data. Backend work may be added later for persistent official catalog management or stronger model discovery evidence.

## Data Flow

1. `PricingPage` loads stations, station group bindings, group rate records, and existing pricing rules.
2. It passes data plus toolbar state to the comparison view model.
3. The view model:
   - filters enabled official catalog models
   - matches each model to relevant station groups
   - computes effective multiplier and estimated prices
   - attaches discovery evidence when available
   - sorts rows inside each model section by estimated output price
4. `PricingPage` renders sections and tables from the view model.

## Testing Plan

Add focused tests before implementation:

- Official catalog never emits wildcard model rows.
- One concrete model can produce multiple rows for the same station when multiple groups match.
- Estimated prices include `creditPerCny` using `officialPrice * groupMultiplier / creditPerCny`.
- Rows sort inside each model section by estimated output price.
- Unknown model discovery does not hide a catalog model; it marks rows `未验证`.
- Filter tests for provider, model query, station filter, and verified-only toggle.
- Page/source test that `PricingPage.tsx` delegates comparison construction to the helper instead of rebuilding joins inline.

Run at minimum:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
pnpm.cmd build
```

If Rust/backend files are changed in a later implementation plan, also run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

## Migration From Current Interim State

The current interim approach that creates `gpt-*` and `claude-*` rate-only rows should be removed or replaced. It is acceptable as a debugging proof, but it is not acceptable product behavior.

Replace it with concrete catalog-driven model sections. If an implementation branch already contains `buildGroupRateOnlyPricingRules` or wildcard fallback rows, the implementation plan must either delete those paths or narrow them so they cannot render wildcard model names.

## Out Of Scope

- Full official provider price auto-sync.
- Live web fetching of official model prices.
- Cross-model price ranking.
- Token usage analytics.
- Latency or health monitoring UI redesign.
- Dark theme redesign.
- Station management UI changes outside the data needed by this page.

## Acceptance Criteria

- The page never displays `gpt-*`, `claude-*`, or `gemini-*` as model names.
- Each visible section represents exactly one concrete model.
- Each section compares only rows for that model.
- Same-station multi-group rows are visible when they have distinct matching group multipliers.
- Estimated prices include both group multiplier and `Station.creditPerCny`.
- The UI shows group multiplier, recharge multiplier, effective multiplier, input price, and output price.
- The default order inside each model section is estimated output price ascending.
- Filtering is compact and accessible.
- The implementation keeps comparison logic out of the page component.
