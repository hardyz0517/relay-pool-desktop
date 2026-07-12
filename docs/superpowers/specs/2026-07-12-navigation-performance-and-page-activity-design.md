# Relay Pool Desktop 导航性能与页面活性设计

## 状态

- 设计方向已由用户确认。
- 本文定义导航调度、页面保活、渲染隔离、查询缓存、刷新策略、异常恢复和性能验收合同。
- 本文不授权修改 Rust 命令、数据库、代理、采集器或业务数据语义。
- 实施前仍需单独编写可执行计划，并按测试驱动方式推进。

## 背景

当前页面切换已经具备集中式路由分类、shell 页面保活、transient 覆盖层、焦点隔离和 reduced-motion 支持，但用户在快速点击侧边栏时仍能观察到两类问题：

1. 部分点击虽然最终改变了导航状态，中间页面却没有获得可见帧，主观上像“吞点击”。
2. 页面偶发延迟约 0.5 秒后才出现，表现为主线程仍在工作，但内容迟迟不能提交和绘制。

2026-07-12 的浏览器采样显示：

- 首次进入中转站页面约 227ms。
- 首次进入变更中心约 257ms。
- 首次进入设置页约 329ms。
- 采样期间出现最长约 246ms 的主线程 long task。
- 页面全部访问后，回访多数降到约 60-85ms。
- 以约 12ms 间隔触发六次导航时，六次点击和六次导航状态变化均被接收，但浏览器只绘制了三个中间页面；最终页面仍等于最后一次点击。

这说明问题不是点击监听器直接丢失，而是首次挂载、React 协调和后台更新占用主线程，导致浏览器缺少绘制机会。

## 参考实现与边界

参考项目为 `farion1231/cc-switch`，MIT License，当前源码提交：

`f39d463c442e705727531b85f2db98e00ccaf11e`

CCSwitch 的相关做法：

- `src/App.tsx` 只渲染当前业务页面，不保留全部已访问页面的业务子树。
- `currentView` 变化后，导航标题和旧页面退出动画立即获得一次提交。
- `AnimatePresence mode="wait"` 管理 200ms 的页面淡出/进入生命周期。
- `@tanstack/react-query` 缓存服务器状态、合并相同 query key 的请求并在后台重新验证。
- provider 查询使用 `placeholderData: keepPreviousData`，避免切换时清空已有内容。
- Tauri 事件通过精确的 query invalidation 或 cache update 推动数据同步。

本项目只采用其成熟的调度与数据缓存原则，不复制组件、页面结构或业务实现。Relay Pool 需要保留自身已经存在的页面状态、滚动位置和 transient 父子页连续性，因此不能直接改成“每次切换都卸载旧页面”。

## 当前根因

### 1. 常驻页面没有形成真正的渲染隔离

`src/app/App.tsx` 使用 `mountedRouteIds` 保存已访问 shell 页面，但每次导航仍会重新执行 shell 页面映射并创建所有已访问页面的 React element。页面组件没有统一的 memoized slot 边界，因此 CSS 的 `display: none` 只能阻止布局和绘制，不能阻止 React 协调。

### 2. 导航回调不稳定

`navigateTo` 依赖 `activeRouteId`。每次导航都会生成新的函数引用，并继续向 `AppShell` 和业务页面传播，妨碍后续使用 `React.memo` 隔离无关页面。

### 3. 隐藏页面仍在刷新

当前活性上下文只约束 `usePageActivation()`，不能约束页面中独立创建的定时器：

- `DashboardPage` 每 2 秒刷新代理状态和请求日志，每 30 秒刷新余额。
- `StationsPage` 每 30 秒刷新站点资产。
- `AppShell` 同时每 2 秒刷新代理状态、每 10 秒刷新变更事件。

Dashboard 隐藏后仍会执行自己的 2 秒轮询，并与 Shell 的代理状态轮询重复。隐藏页刷新结果触发的 React 更新如果与导航相撞，就会造成偶发长任务和输入延迟。

### 4. 页面各自持有重复的服务器状态

Dashboard、Logs、Stations、AppShell 等组件分别保存代理状态、请求日志、设置、站点、余额和变更事件。相同资源缺少统一 query key、请求去重、共享缓存和失效策略。

### 5. 首次反馈依赖目标页面提交

当前 shell 页面动画在目标页面已经完成 React 提交后才开始。目标页面首次挂载较重时，侧栏状态、内容和动画都要等待同一个同步工作阶段，形成“呼之欲出”的停顿。

## 目标

1. 用户点击后立即看到明确导航反馈，数据读取不能阻塞反馈。
2. 保持页面活性：缓存数据可后台刷新，活动页面持续获得需要的实时更新，隐藏页面不浪费资源。
3. 保留已访问页面的重要本地状态、滚动位置和 transient 父子页连续性。
4. 快速连续点击时可靠地执行最后意图胜出，不排队渲染已经过期的目标。
5. 同一资源只有一个事实缓存和一个刷新策略。
6. 新增页面时通过注册策略接入导航、活性、缓存和过渡，不复制生命周期代码。

## 非目标

- 不把页面改成静态快照。
- 不降低数据正确性或隐藏确定性错误。
- 不复制 CCSwitch 页面结构或业务代码。
- 不修改 Rust API、数据库模型、代理调度、采集器或定价算法。
- 不在本轮引入复杂 LRU 页面淘汰、虚拟 iframe 或不稳定的 React Offscreen API。
- 不通过延长动画掩盖主线程阻塞。

## 性能与行为合同

在真实 Tauri 开发运行环境中，目标合同为：

- 点击到侧栏确认反馈：p95 不超过 32ms。
- 已挂载页面的内容提交：p95 不超过 100ms。
- 经过空闲预热的重页面首次内容提交：p95 不超过 200ms。
- 导航关键区间不出现超过 50ms 的主线程 long task。
- 约 80ms 间隔的人手快速点击中，每次点击均产生可见的侧栏反馈。
- 快于一帧的极端输入允许合并中间绘制，但最终 active route 必须严格等于最后一次点击。
- 隐藏页面主动发起的页面级请求数量为 0。
- 同一 query key 同时最多存在一个在途读取。
- 任意时刻最多只有一个页面层可交互、可聚焦和暴露给辅助技术。

如果机器性能或数据规模导致冷页面提交无法达到 200ms，必须保留 32ms 的确认反馈合同，并使用 profiler 证据定位具体页面热点；不能放宽最后意图、唯一交互层或隐藏页零刷新合同。

## 总体架构

系统拆分为四个明确边界：

1. `NavigationController`：管理用户意图、已提交路由、导航序号和并发切换。
2. `ShellPageHost`：管理页面插槽、活性、可访问性、保活和视觉层级。
3. Query 层：管理服务器状态缓存、请求去重、失效、轮询和错误状态。
4. Page view state：管理筛选、分页、滚动位置、选中项和未提交表单。

导航不读取业务数据，Query 层不决定当前路由，页面 view state 不复制服务器事实。

## 导航状态机

### 状态

```ts
type NavigationSnapshot = {
  intentRouteId: AppPageId;
  committedRouteId: AppPageId;
  previousCommittedRouteId: AppPageId | null;
  sequence: number;
  pending: boolean;
};
```

- `intentRouteId`：用户最后点击的目标，使用紧急优先级更新，并驱动侧栏高亮。
- `committedRouteId`：已经完成 React 提交且成为唯一交互层的页面。
- `previousCommittedRouteId`：仅服务于交接视觉层，不形成退出队列。
- `sequence`：单调递增的导航版本。
- `pending`：目标页面正在低优先级准备。

### 转移

```text
active
  -> click(route, sequence + 1)
  -> intent acknowledged
  -> concurrent preparation
  -> commit only if sequence is still latest
  -> target active, previous inactive
```

点击处理必须先紧急更新 `intentRouteId`，再通过 `startTransition` 准备目标页。新的点击到达时，React 可以中断旧目标渲染；任何异步准备结果在提交前还必须比较 `sequence`。

不建立 FIFO 导航队列。已经过期的中间页面没有继续渲染的业务价值，最后意图必须优先。

### 即时反馈

- 侧栏 active 状态由 `intentRouteId` 驱动，不等待目标页挂载。
- 旧页面在目标页准备完成前保持可见，避免空白帧。
- 目标页提交时成为唯一交互层，并在旧页上方淡入。
- 页面标题如果未来上移到 Shell，也应由 `intentRouteId` 驱动；本轮不要求调整页面信息架构。

## Shell 页面宿主与渲染隔离

### ShellPageHost

新增 `src/app/ShellPageHost.tsx`，职责仅包括：

- 维护已挂载 shell 页面插槽。
- 计算每个插槽的 `active`、`background`、`inactive` 状态。
- 将活性传给 `PageActivityProvider`。
- 设置 `inert`、`aria-hidden`、pointer 和层级合同。
- 管理 current/previous 两个视觉层的交接。

### ShellPageSlot

每个页面使用稳定的 memoized slot：

```ts
type ShellPageSlotProps = {
  routeId: AppRouteId;
  state: "active" | "background" | "inactive";
};
```

- `ShellPageSlot` 使用 `React.memo`。
- 页面注册表和页面工厂位于组件外部，避免每次 App render 重建映射。
- 页面操作回调必须稳定；需要读取当前导航值时使用 ref，而不是把 `activeRouteId` 放入 callback dependency。
- 一次导航只允许来源页和目标页的 slot 状态发生变化。
- 状态未变化的隐藏页不得重新执行页面组件函数。

### 页面保活

本轮继续保留已访问 shell 页面的 React 实例，因为设置、站点等页面首次挂载成本较高，且页面本地状态与滚动位置有实际价值。

保活不等于后台活跃：

- React 实例和 DOM 可以保留。
- 页面级查询订阅和轮询必须随活性关闭。
- portal、菜单和临时交互在页面失活时必须关闭。
- 隐藏页面不能响应键盘、pointer、焦点或页面级快捷键。

路由策略预留 `retention` 字段，但本轮所有 shell 页面仍采用 `keep`。未来只有在内存采样证明需要时，才允许新增 `lru` 或 `discard`；新策略必须保留 view state 和 query cache。

## 页面活性模型

页面活性拆成两个概念：

```ts
type PageActivity = {
  interactive: boolean;
  refreshEnabled: boolean;
};
```

- `interactive`：页面是否可聚焦、可点击、可响应快捷键。
- `refreshEnabled`：页面级 query 是否允许轮询和激活刷新。

正常 shell active 页两者均为 true。背景父页、退出层和 inactive 页两者均为 false。Shell 全局资源不依赖页面活性，由 Shell 自己唯一订阅。

`usePageActivation()` 可以继续作为兼容层，但新查询代码应直接通过 query hook 的 `enabled`、`subscribed`、`refetchInterval` 和活动订阅控制刷新，不能再创建无条件页面定时器。

- active 页面使用 `enabled: true`、`subscribed: true`，允许按 stale policy 读取并响应 cache 更新。
- inactive/background/exiting 页面使用 `enabled: false`、`subscribed: false`，既不发起读取，也不因其他消费者更新 cache 而重新渲染。
- 隐藏页面重新激活时恢复订阅，并直接取得 cache 中的最新成功快照。
- Query 层固定使用支持 `subscribed` 选项的 TanStack Query v5 版本；业务页面不得绕过统一活动 query wrapper。

## 空闲预热

为降低首次挂载成本，在 Dashboard 首次稳定提交后按低优先级预挂载：

1. Settings
2. Stations
3. Change Center

预热规则：

- 每个空闲窗口最多预热一个页面。
- 任意 pointer、keyboard 或 navigation 输入立即取消当前预热调度。
- 预热页面以 inactive 状态挂载，不允许读取业务数据或创建定时器。
- 支持 `requestIdleCallback` 时检查剩余预算；否则使用可取消的低优先级定时调度。
- `navigator.scheduling.isInputPending` 可用时必须优先让出主线程。
- 预热顺序只能由真实冷挂载数据调整，不能按主观猜测扩展。

## Query 层

### 技术选择

引入 `@tanstack/react-query`。它负责：

- query key 和缓存所有权。
- 相同读取的在途请求去重。
- stale/fresh 生命周期。
- 后台重新验证。
- mutation 后精确失效。
- 保留最近成功数据。
- 页面卸载或失活后的订阅管理。

不手写通用缓存、重试、去重和观察者系统。

### Query key

初始资源至少包括：

```ts
queryKeys.settings
queryKeys.proxyStatus
queryKeys.requestLogs
queryKeys.stations
queryKeys.stationAssets(stationId)
queryKeys.keyPool
queryKeys.balanceSnapshots
queryKeys.changeEvents
queryKeys.localRoutingWorkspace
queryKeys.pricing
queryKeys.channelStatus
```

Dashboard 不再保存一个独立的复合工作区副本，而是从共享 query 数据通过 memoized selector 组合 Dashboard view model。

### 服务器状态与 view state

Query cache 保存服务器事实：

- 站点、密钥、余额、日志、设置、代理状态、变更事件和路由事实。

页面组件保存 view state：

- 筛选、分页、选中项、滚动位置、抽屉状态和未提交表单。

view state 不进入服务器 query；服务器事实不在多个页面 state 中重复复制。

## 刷新策略

| 资源 | 初始策略 | 活性边界 |
|---|---|---|
| settings | Tauri 事件更新；窗口聚焦时校验 | Shell 全局 |
| proxyStatus | 事件立即更新；2 秒轮询兜底 | Shell 唯一订阅 |
| requestLogs | stale-while-revalidate；代理运行时可 2 秒刷新 | Dashboard 或 Logs 活跃 |
| stations | 激活时校验；30 秒兜底 | Stations 活跃 |
| stationAssets | 按站点 query；30 秒兜底 | Stations 或对应详情活跃 |
| balanceSnapshots | 30 秒校验 | Dashboard 或 Stations 活跃 |
| changeEvents | 事件立即失效；10 秒低频校验 | Shell badge 或 Changes 活跃 |
| routing/pricing/channels | 激活且 stale 时重新验证 | 对应页面活跃 |

同一个资源有多个消费者时，只能存在一套 query 和一个在途请求。窗口隐藏或最小化时暂停页面级轮询；恢复焦点后只重新验证已过期且有活跃消费者的资源。

## 数据活性与显示语义

保持页面活性不等于每次进入都清空并重新加载：

- 有最近成功数据时立即显示该数据。
- 同时在后台重新验证已过期资源。
- 只有从未成功读取过数据时才显示骨架。
- 后台刷新不得把内容替换为全页 loading。
- 数据更新后通过 query observer 精确更新消费者。
- 页面隐藏期间收到 Tauri 事件时可以更新 cache；inactive 页面因 `subscribed: false` 不会被唤醒执行昂贵派生工作。

这样页面不是静态的：活跃页面持续获得事件和必要轮询更新，回访页面也会按 stale policy 校验；只是数据读取不再阻塞导航。

## Mutation 与一致性

- Tauri mutation 成功后优先使用返回值执行 `setQueryData`。
- 随后精确失效对应 query，后台验证最终状态。
- mutation 开始前取消相关旧读取，避免旧响应覆盖写后状态。
- query 和 mutation 均携带 generation/version；过期结果不得提交。
- 排序等可逆操作可以乐观更新，但必须保留完整回滚快照。
- 删除、密钥、安全设置和不可逆操作不做猜测性乐观成功。
- mutation 失败时保留原 cache，并恢复可逆的乐观变更。

## 错误处理

### 读取失败

- 短暂网络、超时或后端忙碌：保留最近成功数据，记录 stale/error 元数据并按策略重试。
- 鉴权失效、对象已删除、格式不可解析等确定性错误：立即暴露，不能用旧缓存掩盖。
- 同一 query 的同一失败周期最多提示一次，避免轮询 Toast 风暴。
- 页面级错误不清空其他 query，也不触发全局刷新。

### 导航和渲染失败

- 每个 shell 页面插槽使用 route-level Error Boundary。
- 目标页面渲染失败时显示该页面的局部错误状态，Shell 与侧栏保持可用。
- 渲染错误不能让 `intentRouteId` 和 `committedRouteId` 永久分叉；错误 fallback 视为目标页面的已提交内容。
- 动画完成回调不决定路由是否成功。
- 页面层由 current/previous 派生，不保存可无限增长的退出队列。

### 安全

- 性能和 query 诊断不得记录 API key、Cookie、请求正文、完整凭据或用户本地数据库内容。
- 错误信息继续使用现有脱敏规则。

## 动画与可访问性

### Shell 页面

- 不使用 `mode="wait"`。
- 目标页准备完成后以 120-160ms opacity 淡入覆盖旧页。
- 不使用 translate、scale、blur 或布局动画。
- `will-change: opacity` 仅存在于实际交接层。
- reduced-motion 下动画时长降为接近 0，但状态机和清理合同不变。

### Transient 页面

- 保留现有 `TransientPageHost` 和 200ms opacity 生命周期。
- transient 父 shell 保持同一 React 实例和滚动位置。
- transient 退出期间主内容区域继续由退出层屏蔽 pointer。
- 侧栏仍可接受新的 shell 导航意图。
- 如果实测 transient-to-transient 的 `mode="wait"` 形成明显等待，只能在 Host 内集中评估 `mode="sync"`，业务页不能自行选择模式。

### 唯一交互层

任意时刻必须满足：

- 只有一个层 `interactive=true`。
- 其他层设置 `inert`、`aria-hidden=true` 和 `pointer-events:none`。
- active 页面切换时恢复或迁移焦点，不让焦点落在隐藏节点。
- portal 菜单、select、dialog 在所属页面失活时同步关闭。

## 计划文件边界

计划新增：

- `src/app/navigationController.ts`
- `src/app/ShellPageHost.tsx`
- `src/app/shellPageRegistry.tsx`
- `src/lib/query/queryClient.ts`
- `src/lib/query/queryKeys.ts`
- 按领域拆分的 query hooks
- 聚焦导航、活性和 query 合同的测试脚本

计划修改：

- `src/main.tsx`
- `src/app/App.tsx`
- `src/app/pageTransitionPolicy.ts`
- `src/components/shell/AppShell.tsx`
- `src/components/shell/PageActivity.tsx`
- `src/features/dashboard/DashboardPage.tsx`
- `src/features/stations/StationsPage.tsx`
- `src/features/logs/LogsPage.tsx`
- `src/features/changes/ChangeCenterPage.tsx`
- 其他迁移到共享 query 的 shell 页面
- `src/styles.css`
- `package.json` 和 `pnpm-lock.yaml`

禁止修改：

- `src-tauri/**`
- 数据库 schema 和 migration
- 代理路由、采集、定价和密钥安全逻辑
- 与本任务无关的业务文案和页面布局

## 可观测性

开发环境增加不含敏感数据的 performance marks：

- `navigation:intent`
- `navigation:indicator-committed`
- `navigation:content-committed`
- `navigation:transition-complete`

诊断快照包括：

- 点击到侧栏反馈耗时。
- 点击到内容提交耗时。
- 导航区间的 long task。
- 每个 query key 的在途请求数量。
- 隐藏页面发起的请求数量。
- 被过期 sequence/generation 丢弃的结果数量。
- 页面 slot 的 render count。

这些指标默认仅开发模式启用，不进入用户日志。

## 测试策略

### 导航单元测试

- sequence 单调递增。
- 最后意图胜出。
- 过期准备任务不能提交。
- 点击当前页不产生冗余导航。
- transient caller 和返回 shell 保持正确。

### 渲染隔离测试

- 无关 inactive slot 在其他页面切换时 render count 为 0。
- 稳定回调在路由变化时不改变引用。
- 一次 shell 导航只更新来源和目标 slot。
- 预热不能激活 query 或创建定时器。

### Query 测试

- 相同 query key 的并发读取被去重。
- stale 数据立即显示并后台重新验证。
- 过期 generation 不能覆盖新数据。
- mutation 成功后精确更新和失效。
- mutation 失败执行可靠回滚。
- 隐藏页面不轮询。
- 多个页面订阅同一资源时仍只有一个请求。

### 动画与可访问性测试

- 始终只有一个可交互层。
- inactive/background/exiting 层具有正确 inert、aria 和 pointer 合同。
- reduced-motion 不影响导航正确性。
- 动画结束前后页面高度、滚动和焦点不跳变。
- 快速 shell/transient 交叉导航不残留 overlay。

### 浏览器与 Tauri 验收

- Playwright 以正常快速点击和极端 burst 两种频率执行侧栏导航。
- 正常快速点击逐次产生侧栏反馈；极端 burst 最终目标正确。
- PerformanceObserver 记录长任务和导航阶段耗时。
- Vite 浏览器测试只验证 DOM 合同；最终性能结论必须来自 `pnpm tauri:dev` 的当前源码运行。
- 验收覆盖 Dashboard、Stations、Changes、Settings 四个冷挂载成本最高或更新最频繁的页面。

## 实施顺序

1. 增加可执行性能基线和请求计数测试，证明当前 RED。
2. 引入 QueryClient、query key 和 Shell 全局资源查询。
3. 合并 AppShell/Dashboard 的代理状态请求，删除隐藏页面无条件轮询。
4. 迁移 Dashboard、Logs、Stations、Changes 的共享服务器状态。
5. 稳定导航回调并拆出 `ShellPageHost`/`ShellPageSlot`。
6. 引入 intent/committed 双状态、sequence 和 `startTransition`。
7. 增加受控空闲预热。
8. 完成其余 shell 页面 query 迁移。
9. 运行完整自动化和真实 Tauri 性能验收。
10. 仅对 profiler 已证明的剩余热点进行局部优化。

每一步只解决一个可验证问题，不能同时改写业务 API、布局和动画。

## 四项设计审查

### 可靠性

满足：

- 导航使用单调 sequence，过期任务不能激活页面。
- query/mutation 使用 generation/version，旧响应不能覆盖新状态。
- 页面切换不等待数据或动画。
- 同一资源请求去重，避免竞态和请求风暴。
- 短暂失败保留最近成功数据，确定性失败不被缓存掩盖。
- route-level Error Boundary 保证单页故障不破坏 Shell。
- 唯一交互层、焦点隔离和 transient pointer shield 有明确合同。
- 性能验收必须在真实 Tauri 当前源码环境完成。

剩余风险：

- React concurrent rendering 与 Tauri WebView 的实际调度仍需运行时证明。
- TanStack Query 迁移期间可能出现新旧数据所有权并存；实施计划必须按资源逐个切换并删除旧 state/interval。

### 可维护性

满足：

- Navigation、Host、Query、view state 四个职责边界互不重叠。
- 不手写通用缓存和请求去重系统。
- 页面注册、活性、动画参数和 query key 集中定义。
- 业务页面不自行实现 presence 生命周期或永久轮询。
- 性能指标和 render-count 测试能防止未来无声退化。
- Rust 和业务数据语义保持不变，迁移可分步审查。

剩余风险：

- `App.tsx` 当前仍较集中；实施时只移动导航和页面注册职责，不顺带进行无关重构。

### 可拓展性

满足：

- 新页面通过 `shellPageRegistry` 注册组件、父路由、预热和 retention 策略。
- 新资源通过 query key 和领域 hook 接入，不复制页面工作区状态。
- 多页面共享资源天然复用缓存和在途请求。
- retention 字段为未来基于内存证据的 LRU/discard 留出扩展点。
- 活性模型同时适用于 shell、background parent 和 transient 页面。
- 统一活动 query wrapper 同时控制 `enabled` 与 `subscribed`，新增页面不会误把“停止轮询”当成“停止所有后台重渲染”。
- 性能合同按页面和 query key 计量，页面数量增加后仍可定位退化来源。

剩余风险：

- 如果未来出现超大列表，需要在对应页面单独引入分页或虚拟化；本设计不提前把所有列表虚拟化。

### 页面活性

满足：

- 活跃页面继续通过事件、stale revalidation 和必要轮询获得实时数据。
- 已访问页面保留 React 实例、滚动位置和本地 view state。
- 隐藏页面保留最近成功 cache，但停止页面级轮询和昂贵派生工作。
- Tauri 事件可以更新共享 cache，不要求页面保持交互活跃。
- 页面回访立即显示最近成功数据，并后台验证是否过期。
- transient 父页面保持原实例，不重复挂载，不丢失几何与滚动位置。

页面活性定义为“数据可继续正确演进、页面状态可恢复、活跃时及时验证”，而不是“所有隐藏页面永久运行定时器”。该定义同时满足实时性和资源可控性。

## 自审结论

- 文档没有占位符或未决实现项。
- 导航、动画、数据、页面状态和错误恢复不存在相互等待的循环依赖。
- 性能目标与页面活性不冲突：页面保留状态，数据通过共享缓存保持新鲜，隐藏页面停止重复工作。
- 范围可拆成单一实施计划，不需要同时修改 Rust 或业务模型。
- 可靠性、可维护性、可拓展性和页面活性均有对应架构边界、失败合同和自动化验收。
