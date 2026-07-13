# Relay Pool Desktop 应用主题设计

## 状态

- 设计方向已由用户确认。
- 本文定义日间、夜间、跟随系统三态主题的架构、交互、迁移、失败处理和验收合同。
- 本文不直接授权修改业务数据模型、数据库或代理、采集、路由逻辑。
- 实施前仍需单独编写可执行计划，并按测试驱动方式推进。

## 背景与现状

Relay Pool Desktop 当前是固定浅色桌面工具。主题基础并非从零开始：

- `tailwind.config.ts` 已使用 `darkMode: ["class"]`。
- `src/styles.css` 已定义 `background`、`foreground`、`muted`、`accent`、`border` 等少量 CSS 变量。
- `AppShell`、通用 UI 组件和业务页面大量直接使用 `bg-white`、`text-slate-*`、`border-slate-*` 等浅色工具类。
- 设计调查时，启发式扫描在 61 个 TypeScript/TSX 文件中发现 637 处浅色硬编码匹配；该数字是迁移基线，不是最终验收条件。
- 当前 `AppSettings` 经 React Query、Tauri command、Rust DTO 和 SQLite `settings` 表形成业务运行设置链路。主题是设备级 UI 偏好，不应扩大这条业务链路。

如果只在现有类名旁追加 `dark:*`，会把同一种表面规则复制到数百处，并让后续组件继续遗漏暗色状态。如果用全局 CSS 强制覆盖 `.bg-white` 或 `.text-slate-*`，又会把面板、输入框、滑块和强调前景错误地映射成同一种颜色。因此本设计采用集中主题运行时和语义色令牌迁移。

## 参考实现与许可证边界

参考项目为 `farion1231/cc-switch`，MIT License，审阅提交：

`c6197ae32450cd70e2bf03b35e3f5f53ac12044c`

参考文件：

- `src/components/theme-provider.tsx`
- `src/components/settings/ThemeSettings.tsx`
- `src/hooks/useDarkMode.ts`
- `src/index.css`
- `tailwind.config.cjs`

可借鉴的方向：

- `light | dark | system` 三态偏好。
- 在根元素上应用有效主题类。
- 使用 `matchMedia("(prefers-color-scheme: dark)")` 响应系统变化。
- 同步 Tauri 原生窗口主题。
- 在设置页使用日间、夜间、系统三个明确选项。

本项目不复制 CCSwitch 组件、样式值或业务代码。Relay Pool 使用自身的 TypeScript 模块边界、Tailwind 语义令牌、Tauri 2 权限和可靠性合同独立实现。

## 已确认的产品决策

1. 支持“日间”“夜间”“跟随系统”三种模式。
2. 默认模式为“跟随系统”。
3. 入口仅位于设置页，不在侧栏增加快捷按钮。
4. 主题偏好只属于当前设备，不随数据目录、备份、导入导出或数据库迁移。
5. 本次交付必须完整覆盖全应用，不接受长期存在的“壳已变暗、业务页仍是浅色”状态。
6. 切换即时生效，不需要额外保存按钮。

## 目标

1. 日间、夜间和系统模式在主窗口中行为确定，重载后偏好保持一致。
2. 系统模式可以在应用运行期间实时响应操作系统主题变化。
3. Web 内容、原生标题栏和系统控件的主题尽可能保持一致。
4. 首个 React 内容帧使用正确主题，不先渲染一帧错误的浅色页面。
5. 所有主页面、瞬态页面、弹窗、表单、表格、Toast 和状态组件在两种有效主题下均完整可用。
6. 主题失败不能阻止应用启动、设置页加载或业务功能运行。
7. 新组件通过语义令牌自然获得主题支持，不重复编写主题判断。
8. 主题基础设施、存储、DOM、副作用和原生桥接可以独立理解和测试。

## 非目标

- 不提供自定义主题颜色、主题市场或用户 CSS。
- 不提供按时间、日出日落或页面自动切换。
- 不增加云同步、账号同步或跨设备同步。
- 不把主题偏好加入 `AppSettings`、Rust 设置 DTO 或 SQLite。
- 不改造现有页面信息架构、业务布局或交互流程。
- 不给远程授权捕获窗口中的第三方网站强制注入 Relay Pool 主题。
- 不在本轮增加高对比主题；架构只需避免阻塞未来扩展。
- 不为主题切换增加全局颜色过渡动画，避免大面积重绘和短暂不可读状态。

## 主题模型

主题偏好和有效主题必须分开建模：

```ts
export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

export type ThemeSnapshot = {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
};

export type ThemeUpdateResult = {
  persisted: boolean;
};
```

- `preference` 表示用户选择，允许为 `system`。
- `resolvedTheme` 表示当前真正渲染的颜色，只允许为 `light` 或 `dark`。
- 业务组件不得把 `system` 当作一种颜色。
- 本地存储键固定为 `relay-pool.theme-preference.v1`。
- 读取到空值、旧值或非法值时回退为 `system`，但启动阶段不强制写回；下次成功选择时自然覆盖。

## 模块边界

新增 `src/theme/`：

### `theme.ts`

只包含类型、常量和纯函数：

- `parseThemePreference(value): ThemePreference`
- `resolveTheme(preference, systemPrefersDark): ResolvedTheme`
- `nativeThemeFor(preference): "light" | "dark" | null`

该文件不得访问 `window`、`document`、Tauri 或 React。

### `themeStorage.ts`

只负责安全读取和写入设备级偏好：

- `readThemePreference(): ThemePreference`
- `writeThemePreference(preference): boolean`

任何存储异常都在本层被捕获。`false` 表示本次无法持久化，不表示主题不能在当前会话应用。业务页面不得直接访问主题存储键。

### `themeDom.ts`

负责 DOM 和系统媒体查询：

- `systemPrefersDark(): boolean`
- `applyResolvedTheme(theme): void`
- `subscribeToSystemTheme(listener): () => void`

`applyResolvedTheme` 必须幂等：从 `<html>` 移除旧的 `light/dark` 类，只添加一个有效类，并同步 `color-scheme`。监听函数在 `matchMedia` 不存在时返回空清理函数，系统主题稳定回退为浅色。

### `themeBootstrap.ts`

在 React 挂载前执行：

1. 读取偏好。
2. 同步采样系统主题。
3. 解析有效主题。
4. 立即更新根元素。
5. 返回同一个 `ThemeSnapshot` 给 Provider 使用。

启动逻辑只读取一次偏好，Provider 不再重复初始化，避免启动状态分叉。该合同保证首个 React 内容帧正确；原生窗口标题栏因为 Tauri 调用是异步的，允许在挂载后完成同步。

### `nativeTheme.ts`

只负责主窗口原生主题：

- 使用 `@tauri-apps/api/window` 的当前窗口 `setTheme`。
- `light`、`dark` 传明确主题；`system` 映射为 `null`，交还操作系统管理。
- 浏览器预览、权限缺失或 Tauri 调用失败时返回失败结果，不抛到 React 渲染链。
- 使用模块级单例的串行、末值优先队列合并重复请求，保证快速连续切换后最后一个偏好最后落地。
- 单例队列的 generation 在 Provider 生命周期之外单调递增；Strict Mode 卸载旧 Provider 后，旧请求仍不能越过新实例的请求。
- 每次请求携带 generation；过期请求完成后不得刷新当前系统主题状态。

不新增自定义 Rust command，也不扩展 `src-tauri/permissions/main-window.toml` 中的应用 command 集合。主窗口由 `src-tauri/capabilities/default.json` 聚合权限，因此只在该 capability 中增加 Tauri 2 的 `core:window:allow-set-theme`。

### `ThemeProvider.tsx`

Provider 是唯一 React 状态源，接收 bootstrap 返回的初始快照，提供：

```ts
type ThemeContextValue = ThemeSnapshot & {
  setPreference: (preference: ThemePreference) => ThemeUpdateResult;
};
```

职责：

- 更新偏好和有效主题。
- 在 layout effect 中应用 DOM 主题，保证 React 提交后、浏览器绘制前完成颜色切换。
- 只在 `system` 模式订阅系统媒体查询；切出该模式立即清理。
- 切入 `system` 前同步重新采样系统主题，避免使用手动模式期间的陈旧值。
- 偏好变化时请求原生主题同步。
- `system` 原生同步完成后，如果 generation 仍是最新值，再采样一次媒体查询，弥补 WebView 在 `setTheme(null)` 完成后才更新 `prefers-color-scheme` 的时序差。
- 所有 effect 在 React Strict Mode 重复挂载下均可清理、可重放。

第三方组件如果不能消费 CSS 变量，可以读取 `resolvedTheme`。不得新增 `MutationObserver` 或各自的系统主题监听器。

## 启动与切换时序

### 启动

```text
main module evaluation
  -> bootstrap reads local preference
  -> bootstrap samples prefers-color-scheme
  -> bootstrap applies html class and color-scheme
  -> React mounts with the same snapshot
  -> Provider subscribes when preference=system
  -> native main-window theme sync runs asynchronously
```

### 手动切换

```text
Settings segmented control
  -> setPreference(light|dark)
  -> attempt local persistence
  -> update React snapshot
  -> layout effect applies effective html theme
  -> native queue applies explicit window theme
```

### 切换到系统模式

```text
Settings segmented control
  -> synchronously sample system theme
  -> persist preference=system
  -> apply sampled effective theme
  -> subscribe to media changes
  -> native queue calls setTheme(null)
  -> latest generation re-samples system theme after native completion
```

### 系统主题变化

```text
matchMedia change
  -> ignore unless preference=system
  -> update resolvedTheme only
  -> layout effect updates html class
```

系统变化不重复写入本地偏好，也不需要再次调用原生 `setTheme(null)`。

## 设置页交互

设置页顶部增加独立“外观”区，不等待后端 `getSettings()` 完成，也不受业务设置的 `loading/saving` 状态控制。

使用现有 `SegmentedControl`，提供三个固定宽度选项。当前 option 接口只接受字符串 label，因此共享组件需要增加向后兼容的可选 `icon` 字段；现有调用方不传 icon 时行为和布局保持不变。图标由 `SegmentedControl` 统一渲染并设置为装饰性内容，可访问名称仍只来自文字 label。

| 值 | 文案 | 图标 |
|---|---|---|
| `light` | 日间 | `Sun` |
| `dark` | 夜间 | `Moon` |
| `system` | 跟随系统 | `Monitor` |

交互合同：

- 控件使用 `radiogroup` / `radio` 语义，并暴露当前选中状态。
- 三个选项的图标、文字间距和最小宽度由共享组件稳定约束，切换选项不改变控件尺寸。
- 支持左右、上下方向键和点击切换。
- 焦点环在两个主题下均清晰可见。
- 选中态不能只依赖颜色；图标、前景和选中表面共同表达状态。
- 切换即时生效，不显示额外保存按钮，不发送 `SETTINGS_UPDATED_EVENT`，不触发 React Query 设置失效。
- `writeThemePreference` 失败时当前会话继续使用新主题，并通过现有 Toast 显示一次“主题已切换，但偏好无法保存；重启后可能恢复上次设置”。
- 原生标题栏同步失败不弹阻断性 Toast；Web UI 已成功切换，失败只在开发日志中以不含敏感信息的方式记录一次。

## 语义颜色系统

`src/styles.css` 在 `:root, .light` 和 `.dark` 中定义同一组变量；`tailwind.config.ts` 只把变量映射成语义类。组件不关心变量的具体颜色值。

### 中性与表面令牌

| 令牌 | 用途 |
|---|---|
| `background` | 应用窗口和页面画布 |
| `foreground` | 默认正文和高强调文字 |
| `surface` | 卡片、侧栏、工具栏、对话框主体 |
| `surface-subtle` | 次级面板、表头、悬停前的弱表面 |
| `surface-inset` | 输入框只读区、代码字段、内嵌区域 |
| `popover` | 菜单、选择器、浮层 |
| `muted` | 禁用或低强调背景 |
| `muted-foreground` | 辅助说明、元数据 |
| `border` | 常规细边框 |
| `border-strong` | 强分隔、选中边界 |
| `input` | 可编辑控件边界 |
| `ring` | 键盘焦点环 |
| `hover` | 中性悬停表面 |
| `selected` | 中性选中表面 |
| `selected-foreground` | 选中表面的前景 |
| `scrim` | 模态遮罩 |
| `control-thumb` | 开关滑块等独立控件前景 |
| `on-solid` | 主按钮、强状态底色上的文字和图标 |

### 品牌与状态令牌

- `primary`、`primary-foreground`：主操作和必要强调，保持现有蓝色方向。
- `success-{surface,foreground,border}`：运行、健康、成功。
- `warning-{surface,foreground,border}`：待处理、风险、降级。
- `danger-{surface,foreground,border}`：错误、失败、危险操作。
- `info-{surface,foreground,border}`：一般信息和中性提示。

状态色只承担语义，不作为大面积主题背景。深色模式使用低饱和状态表面和足够明亮的文字，不直接复用浅色状态底。

### 阴影与层级

- `--surface-shadow` 和 `--surface-shadow-hover` 在两个主题中分别定义。
- 深色主题主要依靠表面明度和边框分层，阴影更轻，不制造发光边缘。
- 页面画布、普通表面和内嵌表面必须有可辨识但克制的明度差。
- 深色背景不使用纯黑，正文不使用纯白；避免高反差疲劳和蓝灰单色化。

### 浅色兼容

浅色变量应尽量保持当前视觉结果，不借主题迁移顺带重做布局、品牌色或密度。必要的变化只用于把现有 `white/slate` 语义归并到稳定令牌。

## 组件迁移规则

迁移完成后，React/TypeScript 源码原则上不得继续使用原始中性色工具类：

- 禁止普通布局使用 `white`、`black`、`slate`、`gray`、`zinc`、`neutral`、`stone` 色族。
- 禁止在业务页面成对堆叠 `dark:*` 来实现常规表面、正文、边框和状态色。
- 允许特殊图片或第三方组件需要的固定颜色，但必须集中在适配层，并有明确原因。
- 开关滑块、强色背景前景等原本需要固定白色的场景改用 `control-thumb` 或 `on-solid`，不保留匿名 `bg-white/text-white`。
- 任意 `rgba()` 阴影和遮罩迁移为 CSS 变量或语义令牌。
- 动态 view model 不再返回 `text-slate-*` 等类名；返回 `tone` 或稳定语义类。
- `StatusBadge` 等共享组件统一拥有状态色，业务页面只传语义 tone。
- Portal 渲染的 Dialog、Select、Toast 必须继承根变量，不创建独立主题状态。

新增 `scripts/theme-audit.mjs`，扫描 `src/**/*.{ts,tsx}` 中禁止的原始 Tailwind 调色板颜色工具类，包括中性色和 red/orange/amber/yellow/lime/green/emerald/teal/cyan/sky/blue/indigo/violet/purple/fuchsia/pink/rose 状态与强调色族。审计同时禁止组件类名中的直接 `rgba()`、十六进制颜色和 `hsl(var(--...))` 任意值表达式；这些颜色必须先在 Tailwind 配置中获得稳定语义名称。`package.json` 增加 `theme:audit`，并将其纳入常规构建验证。审计目标为零例外；确实需要的固定颜色应先获得语义名称，而不是维护文件行号白名单。

## 全应用迁移范围

内部按以下批次实施，但只有全部通过后才视为交付完成：

1. 主题模型、bootstrap、Provider、原生桥接、CSS 令牌和 Tailwind 映射。
2. `AppShell`、`PageScaffold`、页面过渡层和错误边界。
3. `components/ui` 下全部共享组件。
4. 设置页外观入口。
5. Dashboard、Stations、Key Pool、Routing、Pricing、Channels、Changes、Logs、Collectors、Settings 全部 shell 页面。
6. Add/Edit Provider、Station Detail、Add/Edit Key、Model Base Prices 等 transient 页面。
7. Dialog、ConfirmDialog、Select 浮层、Toast、Inspector、拖拽、加载、空状态、错误、禁用、只读和焦点状态。
8. 动态颜色元数据、状态 tone 和所有内联颜色。

实现可以按批次形成可审查提交，但在最终验收前不得把“部分页面主题化”标记为完成。

## 失败处理与并发合同

| 场景 | 行为 |
|---|---|
| 存储键缺失或非法 | 使用 `system`，应用正常启动 |
| `localStorage.getItem` 抛错 | 使用 `system`，不阻断挂载 |
| `localStorage.setItem` 抛错 | 当前会话应用新主题，返回 `persisted=false` 并提示一次 |
| `matchMedia` 不存在 | 系统模式解析为浅色，仍可手动切换 |
| 系统主题连续变化 | 只提交最新媒体查询状态，DOM 更新幂等 |
| 快速连续点击三种模式 | React 最终偏好、存储值和原生队列最终值均等于最后一次选择 |
| 旧原生请求晚完成 | generation 过期，不得重新采样或覆盖当前 React 状态 |
| Tauri API 不存在 | 浏览器预览正常工作 |
| Tauri 权限或调用失败 | Web 主题保持成功状态，业务功能不受影响 |
| Strict Mode 重复 effect | 最多保留一个有效系统监听器和一个原生队列 |
| 切出系统模式同时收到媒体事件 | 当前偏好检查拒绝过期系统事件 |

主题模块不得记录本地路径、API key、Cookie、token、请求正文或业务数据。

## 测试策略

### 最小测试基础

前端当前没有主题行为测试脚本。为保证状态机和副作用合同可回归，计划引入 `vitest` 和 `jsdom`，在 `package.json` 增加可重复执行的 `test` 脚本，并使用 `pnpm test` 运行。不为主题测试引入完整 UI 测试框架；React Provider 测试使用现有 React DOM 测试能力和受控 mock。

### 纯函数与存储测试

- 三个合法偏好正确解析。
- 空值、旧值和任意非法值回退为 `system`。
- `system` 在系统浅色和深色下解析正确。
- 手动偏好不受系统值影响。
- 存储读取、写入异常被捕获并返回确定结果。
- 原生主题映射中 `system` 严格映射为 `null`。

### DOM 与 Provider 测试

- bootstrap 在 React 挂载前应用且只保留一个有效主题类。
- `color-scheme` 与有效主题一致。
- 初始快照和 Provider 首次状态一致。
- 切换三种模式更新偏好和有效主题。
- 系统监听只在 `system` 模式有效并正确清理。
- 从手动模式切回系统时立即重新采样。
- Strict Mode 重挂载不泄漏监听器。
- 存储失败时会话主题仍切换，调用方收到 `persisted=false`。
- 原生调用失败不会形成未处理 Promise rejection。

### 原生同步队列测试

- 相同连续请求被合并。
- `light -> dark -> system` 快速请求最终以 `system/null` 收尾。
- 中间调用失败后队列仍能处理最后请求。
- 过期 generation 完成后不能触发系统状态刷新。
- 最新 `system` 调用完成后执行一次系统主题重采样。

### 设置控件测试

- 三个选项具有正确 radio 语义和选中状态。
- 可选图标不改变文字可访问名称，未传图标的现有调用方保持原行为。
- 点击和方向键均能切换。
- 控件不依赖后端 settings loading 状态。
- 不发送 `SETTINGS_UPDATED_EVENT`，不调用 `updateSettings`。
- 存储失败只提示一次，不产生 Toast 风暴。

### 静态审计

- `pnpm theme:audit` 对禁止的原始中性色类零命中。
- `pnpm test` 执行全部主题单元和 Provider 测试并通过。
- `pnpm build` 包含 TypeScript、主题审计和 Vite 构建。
- `cargo check` 验证 Tauri 权限和桌面端依赖没有破坏 Rust 构建。

## 视觉与真实运行验收

### 浏览器验收

使用当前源码启动 Vite，在 1180x760 默认桌面窗口尺寸至少覆盖：

- 全部 shell 路由。
- 全部 transient 页面。
- Dialog、ConfirmDialog、Select、Toast、Inspector 和拖拽状态。
- loading、empty、error、disabled、read-only、hover、focus、selected 状态。
- `light`、`dark`、`system + light media`、`system + dark media`。
- 选择后重载、非法存储值重载、存储不可用降级。
- 系统媒体查询运行中变化。

截图和 DOM 检查必须确认无刺眼浅色残块、无不可读文字、无边界消失、无布局位移和无重叠。主题切换只能改变视觉令牌，不改变组件尺寸、表格列宽或页面滚动位置。

### 对比度与可访问性

- 常规正文与背景目标至少 4.5:1。
- 大号文字、图标边界、焦点环和交互控件目标至少 3:1。
- success/warning/danger/info 在两种主题下均不能只靠颜色传达状态。
- 键盘焦点在页面、表单、分段控件、菜单和对话框中连续可见。
- `color-scheme` 必须让原生表单控件和浏览器绘制表面匹配当前有效主题。

### 真实 Tauri 验收

浏览器验证不能替代 Windows Tauri 验收。使用 `pnpm tauri:dev` 验证：

- 主窗口标题栏在日间、夜间模式下与 Web 内容一致。
- 系统模式交还 OS 管理，并在 Windows 主题变化后同步更新标题栏和 WebView。
- 手动模式不被 OS 主题变化覆盖。
- 快速连续切换后最终主题与最后一次选择一致。
- 重启应用后恢复偏好。
- 远程授权捕获窗口不被应用主题注入或改写第三方页面。

## 计划文件边界

计划新增：

- `src/theme/theme.ts`
- `src/theme/themeStorage.ts`
- `src/theme/themeDom.ts`
- `src/theme/themeBootstrap.ts`
- `src/theme/nativeTheme.ts`
- `src/theme/ThemeProvider.tsx`
- 对应主题测试文件
- `scripts/theme-audit.mjs`

计划修改：

- `src/main.tsx`
- `src/styles.css`
- `tailwind.config.ts`
- `src/features/settings/SettingsPage.tsx`
- `src/components/shell/**`
- `src/components/ui/**`
- `src/app` 中含视觉类名的宿主和错误边界
- `src/features/**` 中全部含视觉类名的页面、子组件和动态颜色元数据
- `src-tauri/capabilities/default.json`
- `package.json`
- `pnpm-lock.yaml`

禁止修改：

- `src-tauri/src/models/settings.rs`
- `src-tauri/src/services/database.rs` 的 settings schema 和读写逻辑
- `src-tauri/permissions/main-window.toml` 的应用 command 集合
- 数据库 migration
- 代理、采集、路由、价格、凭据和日志业务语义
- 与主题无关的页面布局、文案和导航结构

## 实施顺序

1. 添加纯函数、存储、DOM 和原生队列的失败测试，证明当前缺失行为。
2. 实现主题模型、bootstrap 和 Provider，使测试通过。
3. 添加 Tauri 权限并验证浏览器降级与真实主窗口同步。
4. 建立完整语义令牌和 Tailwind 映射。
5. 先迁移 Shell 和共享 UI，建立可复用主题表面。
6. 添加设置页三态控件和交互测试。
7. 按页面迁移全部 shell、transient 和浮层状态。
8. 将动态颜色 view model 收敛为语义 tone。
9. 添加并启用 `theme:audit`，清除全部命中。
10. 运行 `pnpm test`、`pnpm build`、`cargo check`、浏览器全路由验收和真实 Tauri 验收。

每个迁移批次只改变颜色语义，不顺带重构业务流程。最终完成条件按全应用验收判断，不按已迁移文件数量判断。

## 可靠性、可维护性与可扩展性审查

### 可靠性

- bootstrap 与 Provider 共享同一初始快照，首帧状态不分叉。
- 偏好、有效主题、系统主题和原生主题的所有转换都有明确类型和单向数据流。
- 存储、媒体查询和 Tauri 三类外部失败均有非阻断降级。
- 原生调用使用串行末值优先队列和 generation，避免快速切换竞态。
- 原生队列位于 Provider 生命周期之外，Strict Mode 重挂载不会重置竞态保护。
- 系统模式在原生 `setTheme(null)` 后重新采样，覆盖 WebView 媒体查询更新延迟。
- Strict Mode、切换边界和过期媒体事件均有测试合同。
- 浏览器和真实 Tauri 分层验收，避免用 Web 预览推断原生标题栏行为。

### 可维护性

- 类型与纯函数、存储、DOM、原生桥接、React 状态和设置 UI 职责不重叠。
- 业务设置 DTO 和数据库保持不变，主题故障不会污染运行配置保存。
- 页面只使用语义令牌，状态颜色由共享组件和 tone 管理。
- 静态主题审计进入构建，防止新增浅色硬编码静默回归。
- 浅色主题尽量保持现状，迁移评审可以聚焦主题语义而非混合布局重做。

### 可扩展性

- `ThemePreference` 与 `ResolvedTheme` 分离，未来增加高对比偏好时不破坏当前颜色消费者。
- CSS 令牌覆盖画布、表面、交互、状态和特殊控件，新增页面无需新增主题状态。
- 第三方图表和编辑器统一消费 `resolvedTheme`，不建立第二套监听系统。
- 原生同步封装不暴露 Tauri 细节，未来多平台差异只在适配层处理。
- 版本化存储键为未来偏好迁移保留清晰边界。

## 自审结论

- 文档没有占位符、未完成章节或待用户决定项。
- 三态产品决策、设备级持久化、设置入口和全应用覆盖与已确认需求一致。
- 启动、手动切换、系统切换、原生同步和失败降级形成闭环，没有相互等待的循环依赖。
- `system` 模式下 WebView 媒体查询可能晚于原生主题复位的问题，已通过最新 generation 完成后重新采样处理。
- 本地存储失败既不会丢失当前会话选择，也不会伪装成已持久化成功。
- 主题偏好没有进入业务设置事件、React Query 或数据库，避免跨层耦合。
- 迁移范围包含共享组件、全部页面、Portal、动态颜色、交互状态和原生标题栏，不存在已知断链。
- 范围可以由一份实施计划完成，内部允许分批审查，但最终只按全应用验收完成。
