# Relay Pool Desktop 外观基础统一实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Relay Pool Desktop 的外观收敛成统一壳层、统一布局模板和统一组件语言，保证后续页面长期可维护。

**Architecture:** 先统一全局壳层和视觉 token，再收拢基础表面原语，最后把各页面迁移到固定模板。整个过程不碰业务逻辑，只调整外观骨架与内容承载方式，确保新增页面时能直接复用同一套布局契约。

**Tech Stack:** React 18, TypeScript, Vite, Tailwind CSS, Lucide React, 现有的 `SectionCard` / `MetricCard` / `StatusBadge` / `DataTableLite` / `PageScaffold` / `WorkspaceLayout` 组件。

---

## 文件结构

- `src/styles.css`：放全局视觉 token、壳层几何变量、基础背景和边框语义。
- `src/components/shell/AppShell.tsx`：收口全局壳层、侧边栏、顶部条和底部状态区的几何契约。
- `src/components/shell/PageScaffold.tsx`：统一页面标题、描述、右侧操作区的结构。
- `src/components/ui/WorkspaceLayout.tsx`：统一列表/详情型或双栏型页面的列宽和间距。
- `src/components/ui/Toolbar.tsx`：统一分组操作条的高度、边界和按钮布局。
- `src/components/ui/InspectorPanel.tsx`：统一详情面板的标题区、内容区和边框语言。
- `src/components/ui/SectionCard.tsx`、`MetricCard.tsx`、`StatusBadge.tsx`、`DataTableLite.tsx`、`EmptyState.tsx`：统一基础表面原语。
- `src/features/dashboard/DashboardPage.tsx`、`stations/StationsPage.tsx`、`key-pool/KeyPoolPage.tsx`、`pricing/PricingPage.tsx`、`logs/LogsPage.tsx`、`routing/RoutingPage.tsx`、`settings/SettingsPage.tsx`、`collectors/CollectorsPage.tsx`、`channels/ChannelStatusPage.tsx`：迁移到统一页面模板。
- `docs/superpowers/specs/2026-06-27-relay-pool-ui-foundation-design.md`：本次实现对应的设计规格。

---

### Task 1: 收口壳层 token 和几何契约

**Files:**
- Modify: `src/styles.css`
- Modify: `src/components/shell/AppShell.tsx`
- Modify: `src/components/shell/PageScaffold.tsx`
- Modify: `src/components/ui/WorkspaceLayout.tsx`
- Modify: `src/components/ui/Toolbar.tsx`
- Modify: `src/components/ui/InspectorPanel.tsx`
- Create: `src/components/ui/layout.ts`

- [ ] **Step 1: 定义统一的壳层常量和视觉 token**

```ts
export const shellLayout = {
  sidebarExpandedWidth: 196,
  sidebarCollapsedWidth: 72,
  headerHeight: 44,
  footerHeight: 81,
  pageGap: 12,
} as const;
```

```css
:root {
  --shell-sidebar-expanded: 196px;
  --shell-sidebar-collapsed: 72px;
  --shell-header-height: 44px;
  --shell-footer-height: 81px;
  --surface-radius: 14px;
  --surface-shadow: 0 12px 30px rgba(33, 79, 88, 0.07);
}
```

- [ ] **Step 2: 把 `AppShell` 改成只依赖壳层常量**

`AppShell` 的侧边栏宽度、顶部条高度、底部状态区高度都要改为读取共享常量，不再散落硬编码值。收起态仍然保留 Local Proxy 行和折叠按钮的 DOM 顺序，避免几何跳变。

- [ ] **Step 3: 把 `PageScaffold` 变成统一的页面标题壳**

页面标题、描述、动作区保持固定高度和固定对齐，不让每个页面自己写不同的 header 规则。描述文字保持截断但不改变整体布局高度。

- [ ] **Step 4: 让 `WorkspaceLayout`、`Toolbar`、`InspectorPanel` 共用同一套间距语义**

双栏布局、工具条、详情面板都使用同一套间距档位，避免左列表/右详情、顶部操作条、辅助说明区各自长出不同的边距和圆角。

- [ ] **Step 5: 跑一次基线验证**

Run: `pnpm.cmd build`

Expected: `tsc --noEmit` 和 `vite build` 都通过，说明壳层 token 没有引入类型错误或构建错误。

---

### Task 2: 统一基础表面原语

**Files:**
- Modify: `src/components/ui/SectionCard.tsx`
- Modify: `src/components/ui/MetricCard.tsx`
- Modify: `src/components/ui/StatusBadge.tsx`
- Modify: `src/components/ui/DataTableLite.tsx`
- Modify: `src/components/ui/EmptyState.tsx`
- Modify: `src/components/ui/index.ts`

- [ ] **Step 1: 收拢卡片表面语言**

`SectionCard`、`MetricCard`、`EmptyState` 要共用同一套圆角、边框、阴影和标题层级。不要让每个组件看起来像来自不同设计稿。

```tsx
<section className="rounded-[14px] border border-border bg-white shadow-[var(--surface-shadow)]">
  ...
</section>
```

- [ ] **Step 2: 统一指标卡、状态徽标和空状态**

`MetricCard` 的 label/value/detail 层级固定下来，`StatusBadge` 只保留有限的语义色，`EmptyState` 保持更轻的边界和更克制的提示样式。

- [ ] **Step 3: 统一表格的行高、悬停和选中态**

`DataTableLite` 的表头高度、行高、hover、selected 状态都要统一，避免不同页面的表格看起来像不同组件。空数据时也要维持同一套容器边界。

- [ ] **Step 4: 统一组件导出面**

把基础组件集中从 `src/components/ui/index.ts` 导出，减少页面层直接拼接局部风格的机会。

- [ ] **Step 5: 跑一次构建验证**

Run: `pnpm.cmd build`

Expected: 构建通过，基础原语重构没有破坏现有页面的类型与依赖。

---

### Task 3: 把页面迁移到固定模板

**Files:**
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/pricing/PricingPage.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/features/channels/ChannelStatusPage.tsx`

- [ ] **Step 1: 把总览页固定成“指标带 + 活动区 + 状态汇总”**

总览页只保留一套阅读方向，继续使用 `PageScaffold`，下面按统一间距组织指标卡、活动列表和健康状态区，不再自己发明新层级。

- [ ] **Step 2: 把列表/详情页统一成“左列表 + 右详情”**

站点、Key 池等管理页统一用 `WorkspaceLayout` 组织列表和详情面板，左边只负责扫描，右边只负责解释和编辑。

```tsx
<WorkspaceLayout>
  <SectionCard>列表</SectionCard>
  <InspectorPanel>详情</InspectorPanel>
</WorkspaceLayout>
```

- [ ] **Step 3: 把表格分析页统一成“表格主体 + 详情解释”**

价格页和日志页改成同一套表格模板：表格负责密度，详情面板负责上下文，避免每页各自拼接一个“半表格半卡片”的形态。

- [ ] **Step 4: 把设置和规则页统一成“分组表单 + 说明区”**

设置页、路由页、采集页要共享同一类表单布局和说明层，不再出现每页都独立定义输入区块的情况。

- [ ] **Step 5: 跑一次整站构建**

Run: `pnpm.cmd build`

Expected: 页面迁移后仍可构建通过，说明模板替换没有引入断裂。

---

### Task 4: 收紧控制条和交互状态的一致性

**Files:**
- Modify: `src/components/ui/Toolbar.tsx`
- Modify: `src/components/ui/StatusBadge.tsx`
- Modify: `src/components/ui/button.tsx`
- Modify: `src/components/ui/SegmentedControl.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`

- [ ] **Step 1: 统一工具条和分段控件的高度语法**

工具条、分段控件、图标按钮、状态徽标都要遵守同一套控件高度和内边距，不让页面里出现一组“紧一点”、一组“松一点”的控制条。

- [ ] **Step 2: 把状态语义压成一套词汇**

健康、警告、错误、禁用这些状态在按钮、徽标、行状态里保持同样的颜色和边框逻辑，避免页面间重复定义 tone。

- [ ] **Step 3: 把设置页和路由页的控件重新对齐**

确保选择器、切换器、按钮、说明文字在这两类页面里呈现同样的节奏，避免同一套控件在不同页面显得像不同系统。

- [ ] **Step 4: 跑一次构建验证**

Run: `pnpm.cmd build`

Expected: 控件层统一后没有造成导出或属性不匹配。

---

### Task 5: 做完整视觉验收和回归确认

**Files:**
- None

- [ ] **Step 1: 启动本地开发服务**

Run: `pnpm.cmd dev`

Expected: Vite 在本地启动成功，可打开应用主界面。

- [ ] **Step 2: 检查壳层几何**

在浏览器里确认：
- 侧边栏展开和收起只变化宽度，不抖动、不左上漂移
- 底部 Local Proxy 行始终占位，不会突然消失
- 顶部条高度固定
- 页面标题和内容区不会相互挤压

- [ ] **Step 3: 检查三类页面模板**

依次查看总览页、列表/详情页、表格分析页，确认它们都使用同一套骨架语言，卡片、表格、详情区的视觉节奏一致。

- [ ] **Step 4: 做最终构建确认**

Run: `pnpm.cmd build`

Expected: 最终构建通过，说明外观基础统一可以进入后续增量迭代。

---

## 规格覆盖检查

- 壳层统一：Task 1
- 页面模板统一：Task 3
- 基础原语统一：Task 2
- 状态系统统一：Task 2 和 Task 4
- 信息密度与阅读节奏：Task 3 和 Task 5
- 维护约束和长期可维护性：Task 1 到 Task 4 都在收敛职责边界

## 风险检查

- 没有把业务逻辑和外观层混在同一个任务里
- 没有把页面重构写成“大杂烩”
- 没有引入新的深色主题或营销化视觉方向
- 没有把验证停留在静态检查，最后保留了真实浏览器验收
