# Phase 1 UI Plan

Phase 1 的目标是把当前 7 个 placeholder 页面升级为接近真实产品的假数据界面。所有数据都来自本地 mock，不接后端，不保存设置，不调用 Tauri commands。UI 必须继续保持 CCSwitch 风格的浅色、简约、克制、紧凑桌面工具感，并为后续真实数据接入预留清晰结构。

## Phase 1 Boundary

Phase 1 要做：

- 将总览、中转池、Sub2API 采集、价格表、路由规则、请求日志、设置页面从 placeholder 升级为真实感 mock UI。
- 使用本地 mock data 驱动页面，不直接把假数据散落在 JSX 中。
- 建立可复用的小型 UI 组件，优先服务紧凑卡片、表格、状态、键值信息和工具按钮。
- 保留后续真实数据接入口，让 Phase 2 以后可以替换数据来源，而不是重写页面结构。

Phase 1 不做：

- 不接数据库。
- 不写本地 proxy。
- 不写 Sub2API 真实采集。
- 不写真实路由。
- 不写真实健康检测。
- 不写真实请求转发。
- 不接 Tauri commands。
- 不引入复杂状态管理。
- 不做深色主题。
- 不做网站后台风。
- 不做营销页风格。

拖拽范围：Phase 1 只做拖拽手柄视觉占位，不强制实现 dnd-kit 真实拖拽。真实拖拽可以放到 Phase 1B 的后续增强任务，或作为 Phase 2 持久化排序前置任务。

## Visual Guidelines

- 默认浅色主题。
- 窗口背景使用浅灰，例如 `#f6f7f9` 或当前 `--background` 接近色。
- 卡片和面板使用白色或近白色。
- 边框使用浅灰细线。
- 文字使用深灰，不使用大面积纯黑。
- 主色只使用一种克制蓝色或蓝紫色，用于导航选中态、主按钮和轻量焦点态。
- 状态色低饱和：绿色代表正常，黄色代表警告，红色代表错误，灰色代表禁用。
- 控件紧凑，高信息密度，小圆角，轻阴影或无阴影。
- 不做大圆角、大阴影、大色块、大图标、大留白。
- 不做营销页 hero，不做企业后台模板感。

尺寸参考：

- 顶栏高度：48px。
- 侧栏宽度：240px。
- 页面 padding：16px。
- 卡片圆角：8px 以内。
- 表格行高：36-40px。
- 按钮高度：28-32px。
- 正文字号：13-14px。
- 辅助文字：12px。

## Page Plans

### Dashboard

页面目标：一屏概览本地代理、站点、余额、请求和价格变化，让用户打开应用后立即知道本地入口是否可用、哪些站点有风险、今天请求大致情况如何。

信息架构：

- 顶部紧凑指标区。
- 本地代理状态卡片。
- 最近请求列表。
- 最近价格变化列表。
- 站点健康概览。

主要区块：

- 本地代理状态卡片。
- Base URL。
- Local Key 脱敏展示。
- 当前路由策略。
- 可用站点数量。
- 余额告警数量。
- 今日请求数。
- 今日估算花费。
- 最近请求。
- 最近价格变化。
- 站点健康概览。

假数据字段：

- `proxyRunning`
- `baseUrl`
- `maskedLocalKey`
- `strategy`
- `enabledStationCount`
- `balanceAlertCount`
- `todayRequests`
- `todayCostCny`
- `recentRequests`
- `priceChanges`
- `healthSummary`

交互占位：

- 复制 Base URL。
- 复制 Local Key。
- 当前策略展示或轻量切换视觉。
- 查看最近请求详情入口。

后续真实数据接入口：

- Local proxy service。
- Settings。
- Request logs。
- Pricing snapshots。
- Health checker summary。

实现建议文件位置：

- `src/features/dashboard/DashboardPage.tsx`
- 必要时拆出 `src/features/dashboard/components/DashboardMetrics.tsx`

### Stations

页面目标：使用左侧站点列表 + 右侧站点详情模拟真实站点管理体验，提前确定站点信息密度、状态表达和操作入口。

信息架构：

- 左侧站点列表。
- 右侧站点详情。
- 操作按钮区域。
- 模型与健康摘要。

站点列表展示：

- 站点名。
- 站点类型：Sub2API / NewAPI / OpenAI-compatible / Custom。
- 余额。
- 状态。
- 延迟。
- 上次刷新时间。
- 启用状态。
- 拖拽手柄视觉占位。

右侧详情展示：

- 基础信息。
- 连接配置摘要。
- 余额状态。
- 采集状态。
- 健康状态。
- 支持模型摘要。
- 最近错误。
- 操作按钮占位：测试连接、刷新余额、刷新倍率、编辑、禁用。

假数据字段：

- `id`
- `name`
- `type`
- `baseUrlHost`
- `enabled`
- `status`
- `balanceCny`
- `latencyMs`
- `lastCheckedAt`
- `lastPricingFetchedAt`
- `supportedModels`
- `recentError`

交互占位：

- 站点选中态。
- 启用状态展示。
- 拖拽手柄视觉。
- 操作按钮 hover / disabled 视觉。

后续真实数据接入口：

- Station CRUD。
- Collector snapshots。
- Health checks。
- Pricing snapshots。
- Settings。

实现建议文件位置：

- `src/features/stations/StationsPage.tsx`
- `src/features/stations/components/StationListItem.tsx`
- `src/features/stations/components/StationDetailPanel.tsx`

### Collectors

页面目标：突出 Sub2API 采集是项目特色，但 Phase 1 只展示假数据 UI，不打开 WebView，不捕获真实 XHR，不访问真实站点。

信息架构：

- 当前选择的 Sub2API 站点。
- 登录与采集摘要。
- 捕获接口列表。
- 字段识别结果。
- 最近采集快照。
- 失败原因与手动校正入口。

主要区块：

- 当前选择的 Sub2API 站点。
- 登录状态。
- 最近采集时间。
- 采集来源：frontend-api / webview-capture / html / manual。
- 捕获到的接口列表。
- 识别出的余额字段。
- 识别出的 group 字段。
- 识别出的 rate_multiplier 字段。
- 最近采集快照。
- 采集失败原因。
- 手动校正入口占位。

假数据字段：

- `stationId`
- `stationName`
- `loginStatus`
- `source`
- `fetchedAt`
- `capturedEndpoints`
- `detectedBalanceField`
- `detectedGroupFields`
- `detectedRateFields`
- `snapshotSummary`
- `failureReason`

交互占位：

- 选择站点下拉视觉。
- 重新采集按钮占位。
- 手动校正按钮占位。
- 捕获接口展开视觉。

后续真实数据接入口：

- Collector service。
- Pricing snapshot。
- Manual correction storage。
- WebView capture result。

实现建议文件位置：

- `src/features/collectors/CollectorsPage.tsx`
- `src/features/collectors/components/CollectorSnapshotPanel.tsx`

### Pricing

页面目标：展示模型级价格归一化 mock UI，让用户能比较不同站点的真实人民币成本。

信息架构：

- 模型价格表。
- 选中模型详情区。
- 各站点价格对比。
- 原始倍率折叠区。
- 推荐原因。

表格字段：

- 模型。
- 推荐站点。
- 输入价格：`¥ / 1M tokens`。
- 输出价格：`¥ / 1M tokens`。
- 可用站点数。
- 更新时间。
- 价格变化。
- 状态。

模型详情区：

- 同一模型在不同站点的价格对比。
- 原始倍率折叠展示。
- 推荐原因，例如“最低输出价”“余额充足”“健康状态正常”。

假数据字段：

- `model`
- `recommendedStationId`
- `recommendedStationName`
- `inputCnyPer1M`
- `outputCnyPer1M`
- `stationCount`
- `updatedAt`
- `deltaPercent`
- `status`
- `rawRatios`
- `recommendReasons`

交互占位：

- 选中模型行。
- 原始倍率展开/折叠视觉。
- 站点价格对比排序视觉。

后续真实数据接入口：

- Pricing snapshots。
- Price normalizer。
- Station health。
- Model mapping。

实现建议文件位置：

- `src/features/pricing/PricingPage.tsx`
- `src/features/pricing/components/PricingTable.tsx`
- `src/features/pricing/components/ModelPriceDetail.tsx`

表格策略：Phase 1 先使用普通 HTML table 或 `DataTableLite`，不强制引入 TanStack Table。如果后续排序、虚拟滚动、列配置复杂度上升，再引入 TanStack Table。

### Routing

页面目标：先做静态规则表单 UI，帮助确定路由设置的信息结构，不保存任何设置。

信息架构：

- 默认策略设置。
- 失败切换设置。
- 余额与熔断阈值。
- 健康检测缓存。
- 模型固定路由列表。

展示字段：

- 默认策略：手动排序优先 / 最低价优先 / 稳定性优先。
- 失败自动切换开关。
- 余额低于多少不再使用。
- 熔断时间。
- 健康检测缓存时间。
- 模型固定路由列表。

假数据字段：

- `defaultStrategy`
- `fallbackEnabled`
- `lowBalanceThresholdCny`
- `circuitBreakerMinutes`
- `healthCacheSeconds`
- `modelOverrides`

交互占位：

- 策略 segmented control 视觉。
- 开关视觉。
- 数字输入视觉。
- 模型固定路由列表增删按钮占位。

后续真实数据接入口：

- Settings。
- Routing service。
- Station manager。
- Model mapping。

实现建议文件位置：

- `src/features/routing/RoutingPage.tsx`
- `src/features/routing/components/RouteRulesForm.tsx`

### Logs

页面目标：展示请求日志列表和详情面板 mock UI，为后续真实日志、fallback trace 和脱敏摘要打结构基础。

信息架构：

- 请求日志表格。
- 静态选中日志详情面板。
- fallback trace。
- 上游错误与脱敏摘要。

表格字段：

- 时间。
- 模型。
- 实际站点。
- 状态。
- 是否 fallback。
- 耗时。
- input tokens。
- output tokens。
- 估算成本。
- 错误原因。

详情面板：

- 请求模型。
- 标准模型名。
- 上游模型名。
- 候选站点排序。
- fallback trace。
- 上游错误信息。
- 脱敏请求摘要。

假数据字段：

- `id`
- `createdAt`
- `model`
- `canonicalModel`
- `upstreamModel`
- `stationName`
- `status`
- `fallback`
- `latencyMs`
- `inputTokens`
- `outputTokens`
- `estimatedCostCny`
- `errorReason`
- `candidateStations`
- `fallbackTrace`
- `redactedRequestSummary`

交互占位：

- 选中日志行。
- 详情面板固定展示。
- 状态筛选按钮视觉。

后续真实数据接入口：

- Request log service。
- Local proxy。
- Router。
- Health checker。

实现建议文件位置：

- `src/features/logs/LogsPage.tsx`
- `src/features/logs/components/RequestLogTable.tsx`
- `src/features/logs/components/RequestLogDetail.tsx`

Phase 1 可以使用静态选中日志，不要求实现真实抽屉状态。

### Settings

页面目标：展示本地设置表单 UI，但不保存、不调用 Tauri commands。

信息架构：

- 本地代理设置。
- 采集设置。
- 数据与安全。
- 外观与托盘行为。
- 导入 / 导出配置。

展示字段：

- 本地代理端口。
- Local Key 脱敏。
- 采集频率。
- 低余额阈值。
- 数据目录。
- 托盘行为。
- 导入 / 导出配置。
- 主题说明：第一版默认浅色，深色后续预留。

假数据字段：

- `proxyPort`
- `maskedLocalKey`
- `collectionIntervalMinutes`
- `lowBalanceThresholdCny`
- `dataDir`
- `trayBehavior`
- `themeNote`

交互占位：

- 输入框视觉。
- 复制 Local Key。
- 重新生成 Local Key 按钮占位。
- 导入 / 导出按钮占位。

后续真实数据接入口：

- Settings persistence。
- Local key storage。
- App data directory。
- Tray integration。

实现建议文件位置：

- `src/features/settings/SettingsPage.tsx`
- `src/features/settings/components/SettingsFormSection.tsx`

## Reusable Components

- `SectionCard`：白色面板容器；Phase 1 必须实现。建议位置：`src/components/ui/SectionCard.tsx`。
- `MetricCard`：总览指标卡；Phase 1 必须实现。建议位置：`src/components/ui/MetricCard.tsx`。
- `StatusBadge`：状态徽标；Phase 1 必须实现。建议位置：`src/components/ui/StatusBadge.tsx`。
- `StationStatusDot`：站点状态点；Phase 1 必须实现。建议位置：`src/features/stations/components/StationStatusDot.tsx`。
- `DataTableLite`：轻量表格封装；Phase 1 必须实现，不引入 TanStack Table。建议位置：`src/components/ui/DataTableLite.tsx`。
- `KeyValueRow`：详情面板键值行；Phase 1 必须实现。建议位置：`src/components/ui/KeyValueRow.tsx`。
- `MaskedSecret`：Local Key / API key 脱敏展示；Phase 1 必须实现。建议位置：`src/components/ui/MaskedSecret.tsx`。
- `ToolbarButton`：紧凑工具按钮；Phase 1 可复用现有 `Button`，必要时轻量封装。建议位置：`src/components/ui/ToolbarButton.tsx`。
- `StationListItem`：中转池左侧列表项；Phase 1 必须实现。建议位置：`src/features/stations/components/StationListItem.tsx`。
- `PriceCell`：价格与涨跌展示；Phase 1 必须实现。建议位置：`src/features/pricing/components/PriceCell.tsx`。
- `RouteStrategyBadge`：路由策略标识；Phase 1 可选。建议位置：`src/features/routing/components/RouteStrategyBadge.tsx`。
- `EmptyState`：空状态；Phase 1E 统一补齐。建议位置：`src/components/ui/EmptyState.tsx`。

组件拆分原则：

- 通用展示组件放在 `src/components/ui` 或 `src/components/shell`。
- 领域组件放在对应 `src/features/<feature>/components`。
- 不为了 Phase 1 引入大型组件库或后台模板。
- 组件 API 保持简单，优先接受 mock data 对象或少量展示字段。

## Mock Data Plan

新增轻量 mock data，不建立过重业务模型，不把 Phase 2 数据库 schema 提前固化。

建议文件：

- `src/lib/mock/stations.ts`
- `src/lib/mock/pricing.ts`
- `src/lib/mock/logs.ts`
- `src/lib/mock/collector.ts`
- `src/lib/mock/settings.ts`

类型建议：

```ts
type MockStation = {
  id: string
  name: string
  type: 'sub2api' | 'newapi' | 'openai-compatible' | 'custom'
  status: 'healthy' | 'warning' | 'error' | 'disabled'
  enabled: boolean
  balanceCny: number
  latencyMs: number
  lastCheckedAt: string
  supportedModels: string[]
  recentError?: string
}

type MockPricingRow = {
  model: string
  recommendedStationId: string
  inputCnyPer1M: number
  outputCnyPer1M: number
  stationCount: number
  updatedAt: string
  deltaPercent: number
  status: 'fresh' | 'stale' | 'unavailable'
}

type MockRequestLog = {
  id: string
  createdAt: string
  model: string
  stationName: string
  status: 'success' | 'failed' | 'fallback'
  fallback: boolean
  latencyMs: number
  inputTokens: number
  outputTokens: number
  estimatedCostCny: number
  errorReason?: string
}

type MockCollectorSnapshot = {
  stationId: string
  loginStatus: 'logged-in' | 'expired' | 'unknown'
  source: 'frontend-api' | 'webview-capture' | 'html' | 'manual'
  fetchedAt: string
  capturedEndpoints: string[]
  detectedFields: string[]
  failureReason?: string
}

type MockSettings = {
  proxyPort: number
  maskedLocalKey: string
  collectionIntervalMinutes: number
  lowBalanceThresholdCny: number
  dataDir: string
  trayBehavior: 'minimize-to-tray' | 'close-to-tray' | 'disabled'
}
```

Mock 数据要求：

- 不包含真实 API key、cookie、token、站点账号或用户本地路径。
- Local Key 和 API key 一律使用脱敏假值。
- 时间字段可用固定字符串，避免测试和截图随时间漂移。
- 数据量保持适中，足够展示表格和状态即可。

## Implementation Breakdown

### Phase 1A: 基础 UI 组件与 mock data

目标：

- 建立 Phase 1 需要的基础展示组件。
- 建立集中 mock data 文件。
- 保持现有 AppShell，不接业务。

修改文件：

- 新增 `src/components/ui/*` 小组件。
- 新增 `src/lib/mock/*`。
- 必要时小幅调整现有 `Button` 以支持紧凑工具按钮。

验收标准：

- 组件可被多个页面复用。
- mock 数据集中管理。
- `pnpm build` 通过。

不该做：

- 不实现完整页面业务。
- 不接后端。
- 不引入 TanStack Table 或复杂状态管理。

### Phase 1B: 总览页和中转池页

目标：

- 完成最核心的 Dashboard 与 Stations mock UI。
- 建立总览指标、最近活动、站点健康概览和左列表右详情布局。

修改文件：

- `src/features/dashboard/DashboardPage.tsx`
- `src/features/stations/StationsPage.tsx`
- 相关 `components` 子目录。

验收标准：

- 总览页包含代理状态、Base URL、Local Key、策略、站点、余额、请求、价格变化和健康概览。
- 中转池页包含左侧站点列表、右侧详情、操作按钮占位和拖拽手柄视觉。

不该做：

- 不做真实拖拽。
- 不做 CRUD。
- 不测试真实连接。
- 不刷新真实余额或倍率。

### Phase 1C: Sub2API 采集页

目标：

- 完成项目特色的 Sub2API 采集 mock UI。
- 展示登录、采集来源、捕获接口、字段识别、快照、失败原因和手动校正入口。

修改文件：

- `src/features/collectors/CollectorsPage.tsx`
- `src/features/collectors/components/*`

验收标准：

- 页面能清楚表达 frontend-api / webview-capture / html / manual 四类来源。
- 能展示捕获接口列表和字段识别结果。
- 能展示失败原因和手动校正入口占位。

不该做：

- 不打开 WebView。
- 不捕获真实 XHR。
- 不访问真实 Sub2API 站点。
- 不保存手动校正。

### Phase 1D: 价格表、路由规则、请求日志、设置页

目标：

- 补齐 Pricing、Routing、Logs、Settings 四个页面。
- 保持轻量表格和表单，不引入大型表格库。

修改文件：

- `src/features/pricing/PricingPage.tsx`
- `src/features/routing/RoutingPage.tsx`
- `src/features/logs/LogsPage.tsx`
- `src/features/settings/SettingsPage.tsx`
- 对应页面局部组件。

验收标准：

- 价格表有模型价格、推荐站点、价格变化和详情区。
- 路由规则有静态策略表单、失败切换、余额阈值、熔断时间、模型固定路由。
- 请求日志有列表和详情面板。
- 设置页有代理端口、Local Key、采集频率、低余额阈值、数据目录、托盘行为、导入导出和主题说明。

不该做：

- 不保存设置。
- 不写真实日志。
- 不引入 TanStack Table。
- 不调用 Tauri commands。

### Phase 1E: 视觉统一、空状态、响应式微调、验证

目标：

- 统一间距、状态色、表格密度、空状态和小屏表现。
- 清理重复 class，保证页面像同一个桌面工具。

修改文件：

- 共享 UI 组件。
- 各页面少量 class 和布局微调。

验收标准：

- 所有页面保持浅色简约桌面工具风。
- 文本不溢出按钮、卡片和表格单元格。
- 小屏宽度下主内容可滚动，布局不互相覆盖。
- `pnpm build` 和 `cargo check --manifest-path .\src-tauri\Cargo.toml` 通过。

不该做：

- 不新增业务功能。
- 不扩大 Phase 1 范围。
- 不改 Rust 业务代码。

## Acceptance Criteria

Phase 1 完成后必须满足：

- `pnpm build` 通过。
- `cargo check --manifest-path .\src-tauri\Cargo.toml` 通过。
- 所有 7 个页面都有真实感假数据。
- UI 保持浅色、简约、克制、紧凑的桌面工具风格。
- 没有接真实后端。
- 没有接数据库。
- 没有调用 Tauri commands。
- 没有引入账号、支付、云同步。
- 没有提交 key、cookie、日志、本地数据库。
- 工作区改动清晰，提交时只按任务范围精确 add，禁止 `git add .`。

## Notes for Future Codex Runs

- 每次只实现一个 Phase 1 子阶段，不要一次性做完整业务。
- 开始前必须阅读 `AGENTS.md`、`docs/PROJECT_PLAN.md`、`docs/PHASE_1_UI_PLAN.md` 和相关页面文件。
- UI 改动保持 CCSwitch 风格：浅色、克制、紧凑、高信息密度。
- 不要引入大型后台模板。
- 不要把页面做成网站或营销页。
- 不要默认做深色主题。
- 不要用 `git add .`。
- 完成后必须运行可用验证，并汇总改了什么、如何验证、还有哪些未完成。
