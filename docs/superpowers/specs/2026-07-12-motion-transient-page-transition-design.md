# Relay Pool Motion 子页面过渡设计

## 状态

- 设计方向已由用户确认。
- 本文档等待用户复核后进入实施计划。
- 目标是提升内部子页面的打开和关闭手感，不改业务数据逻辑。

## 目标

在保留现有页面保活、路由分类、数据激活隔离和可访问性约束的前提下，使用 `framer-motion` 统一管理内部子页面的进入与退出生命周期。

最终手感参考 CC Switch 的全屏编辑面板：父页面保持在原位，子页面以纯透明度过渡覆盖；关闭时子页面淡出并露出原父页面。过渡应有明确连续性，同时不能重新引入卡顿、闪烁、列表抖动、重复挂载或隐藏页面刷新。

## 参考边界

参考项目：`farion1231/cc-switch`，MIT License，源码提交 `f39d463c442e705727531b85f2db98e00ccaf11e`。

参考点：

- `src/App.tsx` 使用 `AnimatePresence mode="wait"` 管理视图切换。
- `src/components/common/FullScreenPanel.tsx` 使用全屏 Portal 和 `opacity 0 <-> 1`、`200ms` 过渡管理编辑面板。
- CC Switch 使用 `framer-motion ^12.23.25`。

本项目只采用其成熟的生命周期管理和纯淡入淡出手感，不复制组件实现、页面结构或业务代码。

## 当前架构

当前页面切换由以下模块组成：

- `src/app/pageTransitionPolicy.ts` 集中分类 shell 页面和 transient 子页面。
- `src/app/App.tsx` 保存当前路由、上一条路由、已挂载 shell 页面和 transient 页面实例。
- `PageActivityProvider` 区分真正活跃页面与隐藏页面，防止隐藏页面触发刷新。
- `src/styles.css` 负责 shell/transient 的显示、隐藏、焦点隔离和 CSS keyframes。

当前可靠性基础应保留：

- shell 页面访问后保持 React 挂载。
- transient 页面映射回父级 shell 路由，侧栏高亮不跳变。
- 非活跃层使用 `inert`、`aria-hidden` 和 pointer isolation。
- 中转站详情首帧使用已有 station seed，数据读取在首帧之后静默执行。
- 实体型子页面使用实体 key，避免快速 A -> 返回 -> B 时复用错误状态。

当前复杂度来自手工管理退出生命周期：

- `lastActiveTransientPageRef`
- `exitingTransientPage`
- `transientExitTimeoutRef`
- `useLayoutEffect` 中的 outgoing page 保存
- `animationend` 过滤和超时兜底
- transient enter/exit CSS keyframes

这些机制不能与 Motion 并存。实施必须由 Motion 完整替代，而不是在旧状态机外再包一层动画。

## 目标架构

### 1. 路由和业务状态

`App.tsx` 继续负责：

- `activeRouteId` 和必要的上一条路由信息。
- `mountedRouteIds` shell 页面保活集合。
- transient page descriptor 的构造。
- 业务页面所需的 station/key 实体 ID 和 seed。

`App.tsx` 不再负责：

- 保存 outgoing transient ReactNode。
- 动画结束事件。
- 动画清理计时器。
- transient 页面退出阶段状态。

### 2. TransientPageHost

新增 `src/app/TransientPageHost.tsx`，作为唯一允许直接导入 `framer-motion` 的页面过渡模块。

接口合同：

```ts
type TransientPageDescriptor = {
  pageId: AppPageId;
  instanceKey: string;
  node: ReactNode;
};

type TransientPageHostProps = {
  page: TransientPageDescriptor | null;
};
```

宿主内部使用：

- `MotionConfig reducedMotion="user"`
- `AnimatePresence initial={false} mode="wait"`
- 单个 keyed `motion.div`
- `useIsPresent()` 区分 active 和 exiting 阶段

`instanceKey` 必须包含实体身份，例如 `stationDetail:<stationId>`，确保不同实体不共享内部 state；同一实例从 active 进入 exiting 时 key 不变，确保退出期间不重新挂载。

### 3. 父 shell 层

shell 页面需要拆分三个视觉/交互状态：

- `active`：当前就是 shell 页面，可见、可交互、`PageActivityProvider active=true`。
- `background`：当前 transient 的父页面，可见但不可交互，`inert`、`aria-hidden`，`PageActivityProvider active=false`。
- `inactive`：不是当前父页面，保持挂载但 `display:none`。

这会修正当前架构中“进入 transient 时父页面立即 display:none”的断层。父页面在子页面淡入时仍然可见，子页面完全不透明后自然被覆盖；关闭时同一父页面从淡出的子页面下方露出。

退出动画期间，motion overlay 必须继续拦截 main 区域点击，避免用户穿透点击已经恢复的父页面。侧栏仍可用于快速导航。

### 4. 页面激活隔离

`TransientPageHost` 内的 transient layer 使用 `useIsPresent()` 驱动：

- active/entering：`PageActivityProvider active=true`，可交互，`aria-hidden=false`。
- exiting：`PageActivityProvider active=false`，内容 `inert`、`aria-hidden=true`，overlay 保持 pointer shield。

父 shell 在 transient 打开期间保持 `background`，不会触发 `usePageActivation()`。返回 shell 后恢复 active；即使 transient 正在淡出，pointer shield 仍防止误操作。

## 动效规范

内部子页面统一采用：

- `initial: { opacity: 0 }`
- `animate: { opacity: 1 }`
- `exit: { opacity: 0 }`
- `transition: { duration: 0.2 }`
- 不使用 `translateX`、`translateY`、`scale`、blur 或背景位移动画。
- overlay 使用不透明的应用背景色；透明度由整个 motion layer 控制。
- `will-change: opacity` 仅加在动画层，不扩散到业务页面。

主导航 shell 动画不在本轮改写。当前任务只替换内部 transient 页面的进入/退出实现，控制风险和验证范围。

## 生命周期

### 打开子页面

1. 用户在父 shell 页面点击详情、编辑或新增。
2. `activeRouteId` 切换到 transient page。
3. 父 shell 进入 `background`，保持原 DOM 和几何位置。
4. `TransientPageHost` 挂载 keyed motion layer。
5. seed 内容首帧立即渲染，motion layer 在 200ms 内从透明变为不透明。
6. 子页面静默数据读取继续在普通 effect 中执行。

### 关闭子页面

1. `activeRouteId` 切换回父 shell。
2. 父 shell 恢复 active，但 main 区域仍被 exiting overlay 遮挡。
3. `AnimatePresence` 保留原 transient 实例并执行 200ms 淡出。
4. `useIsPresent()` 将退出页面设为 inactive/inert。
5. Motion 完成退出后卸载 overlay，不需要手工 timer 或 animation event。
6. 父 shell 已在原位置，无 shell 二次入场动画和列表位移。

### transient 切换到 transient

- `mode="wait"` 先完成旧实例退出，再进入新实例。
- 不同实体使用不同 `instanceKey`，不复用表单或详情 state。
- 快速导航以最后一次 `activeRouteId` 为准，AnimatePresence 负责清理中间实例。

### 减少动态效果

- `MotionConfig reducedMotion="user"` 遵守系统偏好。
- reduced motion 下不引入位移；纯透明度由 Motion 按用户偏好降级。
- 页面可用性不能依赖动画完成回调。

## 文件边界

计划新增：

- `src/app/TransientPageHost.tsx`
- `scripts/motion-page-transition.test.mjs`

计划修改：

- `package.json`
- `pnpm-lock.yaml`
- `src/app/App.tsx`
- `src/styles.css`
- `scripts/page-transition-container.test.mjs`
- `scripts/page-transition-styles.test.mjs`
- `scripts/station-detail-transition-performance.test.mjs`

禁止修改：

- 业务 API、Tauri 命令和数据库。
- StationDetailContent、StationsPage 列表结构和业务数据选择器。
- 各业务页面内部加入独立 Motion 代码。
- 主导航 shell 的页面切换策略。

## 迁移和删除清单

实现 Motion 宿主后必须删除：

- `TRANSIENT_EXIT_TIMEOUT_MS`
- `RenderedTransientPage` 中仅服务于手工退出保存的字段或旧结构
- `lastActiveTransientPageRef`
- `transientExitTimeoutRef`
- `exitingTransientPage`
- outgoing transient 的 `useLayoutEffect`
- `handleTransientExitComplete`
- `handleTransientExitAnimationEnd`
- `relayTransientEnter` 和 `relayTransientExit` keyframes
- transient CSS `animation` 选择器

静态回归测试应明确禁止这些旧标识重新出现，防止两套状态机共存。

## 可靠性

- Presence 生命周期由成熟库管理，避免 React state 与 CSS animation event 不同步。
- 父 shell 始终保留同一个实例和几何位置，避免关闭后的刷新感和滚动跳变。
- transient descriptor 使用稳定实体 key，避免跨实体 state 泄漏。
- active、background、exiting、inactive 各自有明确的交互和可访问性状态。
- exiting overlay 作为 pointer shield，防止关闭动画期间穿透点击。
- 数据加载时序与动画解耦，动画不能等待 API 或数据库读取。
- `prefers-reduced-motion` 由 MotionConfig 统一处理。

## 可扩展性

- 新增 transient page 时，只需在 `pageTransitionPolicy.ts` 注册父 shell，并构造 descriptor。
- 所有 transient page 自动获得相同的动画、清理、focus isolation 和 reduced-motion 行为。
- `instanceKey` 是实体隔离的统一扩展点，不在业务页面中复制生命周期代码。
- Motion 实现被限制在单个宿主文件，未来替换动画库不会影响业务页面。

## 可维护性

- App 负责导航，Host 负责动画，PageActivityProvider 负责数据激活，CSS 负责几何和静态可见性。
- 删除的手工退出代码应多于新增到 App 的动画胶水代码。
- `framer-motion` import 只能出现在 `TransientPageHost.tsx`；测试对该边界做静态约束。
- 动效参数只定义一次，不允许业务页覆盖时长或曲线。
- 不保留兼容旧 CSS keyframes 的双轨路径。

## 测试和验收

### 自动化合同

- Motion 依赖和唯一 import 边界存在。
- `AnimatePresence initial={false} mode="wait"` 存在。
- transient layer 使用稳定 `instanceKey`。
- exiting layer 使用 `useIsPresent()`、`inert` 和 `aria-hidden`。
- 父 shell 的 `background` 状态保持 `display:block`，但 pointer/focus/activity 均被隔离。
- 旧手工退出 state、ref、timer、animationend 和 transient keyframes 均不存在。
- station detail 继续 seed-first、effect-refresh。
- A -> 返回 -> B 使用不同实体 key。

### 浏览器验收

- 中转站列表 -> 详情：首帧能看到父列表，详情在 200ms 内平滑覆盖，没有空白帧。
- 详情 -> 列表：详情淡出并露出同一列表，列表首行位置在动画前后保持一致。
- 列表在打开详情期间不响应点击或 Tab，不触发页面激活刷新。
- 退出动画期间 main 区域不能穿透点击。
- 快速执行详情 A -> 返回 -> 详情 B，不显示 A 的旧内容，不残留 overlay。
- 编辑供应商、添加供应商、添加 Key、编辑 Key、模型基础价格使用同一过渡。
- reduced-motion 下页面正常切换且没有位移。
- 浏览器控制台无 error/warning，始终只有一个可交互页面层。

### 构建验收

- 聚焦 transition scripts 全部通过。
- `pnpm exec vite build` 通过。
- `pnpm exec tsc --noEmit` 若被现有工作区错误阻塞，必须确认没有新增来自本次文件的错误并如实记录。
- `git diff --check` 对任务文件通过。

## 非目标

- 不修改业务页面布局或文案。
- 不增加共享元素过渡、卡片飞入、3D、弹簧位移或模糊背景。
- 不替换 React 路由模型。
- 不复制 Cockpit iframe/frame 架构。
- 不在本轮重做主导航 shell 动画。

## 风险控制

- 若 Motion 与 retained shell 的 DOM 行为不符合预期，优先修正 Host 边界，不把 presence state 下放到业务页。
- 若 `mode="wait"` 在 transient-to-transient 场景产生明显等待，只允许在 Host 内评估 `mode="sync"`，不得让页面自行选择模式。
- 若浏览器验证发现父 shell 在 background 状态触发刷新，修正 PageActivityProvider 的 active 判定，不隐藏父 shell DOM。
- 若退出期间发生点击穿透，保留 exiting overlay 的 pointer shield，不能通过延长动画掩盖问题。

## 自审结果

- 文档不含未决实现项或模糊占位。
- Motion 和旧手工退出状态机不能共存，删除清单明确。
- 导航、动画、数据激活和 CSS 几何职责边界互不重叠。
- 可靠性、可扩展性、可维护性均有对应代码边界和自动化验收。
- 范围限定为 transient 子页面，不包含业务逻辑和主导航重构。
