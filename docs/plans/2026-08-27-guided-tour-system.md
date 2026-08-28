# 教程引导系统实施计划

状态：Completed

日期：2026-08-27

关联入口：[`../README.md`](../README.md)、[`../PROJECT_PLAN.md`](../PROJECT_PLAN.md)、[`../PRODUCT_MODEL.md`](../PRODUCT_MODEL.md)、[`../SECURITY_EXPORT_IMPORT.md`](../SECURITY_EXPORT_IMPORT.md)

适用范围：React 前端的场景化教程、Driver.js 集成、跨页面导航续接、安装级教程进度、设置中的教程中心、页面 `data-tour` 锚点、教程相关测试与第三方许可记录。

不适用范围：新增业务功能、修改 Station / Station Key / Router / Collector 领域模型、Rust IPC、SQLite schema、跨设备数据包、账号/支付/云同步、公共帮助网站、教程统计上报和真实网络请求。

> 本计划是一次性实施记录，不替代当前代码、自动化契约或 `docs/README.md` 中列出的长期规范。所有任务均在现有工作区改动之上执行，不回退或覆盖无关改动；没有通过相应验证不得修改任务状态。

## 0. 实施结论

本项目采用 Driver.js 作为渲染引擎，并由前端自己的 `TourManager` 负责教程生命周期。第一版不做一个独立的“引导页面”，而是在真实业务页面上显示遮罩、高亮和气泡；设置中提供一个“教程中心”对话框，让用户按场景重新启动教程。

最终调用链固定为：

```text
Driver.js
    -> TourManager
    -> typed tour catalog
    -> active page data-tour anchors
    -> installation-scoped localStorage progress
```

以下决定已经确认，不在实现期间重新解释：

1. 用户所说的“订阅导入”是口误，教程目录不加入该场景。
2. 教程中心提供一个“完整体验”入口，按 `basic -> proxy -> station-setup` 的配置顺序播放全部已发布步骤；场景入口仍可单独重看。完整体验的完成状态聚合其组成场景：全部当前 revision 已完成时显示“已完成”，完整体验走完时同步提交组成场景的完成状态；中途退出不能伪造完成。
3. 只有 `basic` 在首次启动时自动播放；新增场景只在教程中心显示“新增”，不打断老用户。
4. 教程进度按当前安装保存，不进入跨设备数据导入，也不随 SQLite 数据目录切换。
5. 第一版不修改 `AppSettings`、Rust `settings` store、IPC DTO 或数据库 migration。
6. 教程状态不得依赖“是否有中转站”或“是否有密钥”等业务数据判断首次使用。
7. 教程只讲现有能力，不为了让步骤成立而自动创建站点、密钥、监控或修改路由策略。

第一版交付后，用户可以：

- 首次进入可用业务界面后看到一次 `basic` 教程；
- 随时按 `Esc`、关闭、跳过退出教程；
- 在设置中重新启动任意已发布的教程；
- 在页面切换后继续下一步，而不高亮到后台页面或已卸载的 DOM；
- 在加载、空数据、开发者模式关闭或窄窗口时安全跳过不可用步骤；
- 在应用升级后只看到新增或 revision 变化的教程提示，不重复播放已完成的旧教程。

### 0.1 架构审查结论与修正

这套方案可以可靠落地，但“可靠、可扩展、易维护”依赖下面几条边界必须落实。教程与业务的耦合不能完全消失，因为教程必须知道页面 route、锚点，以及少数既有视图的可逆切换能力；目标是把耦合压缩在三个稳定契约上：`TourNavigationPort`、`data-tour` anchor contract 和显式 `TourPreparationRegistry`。教程不得依赖业务查询、业务 mutation、React Query cache 或 feature controller 的内部状态。

本次审查对原方案做以下修正：

1. **拆开 Manager 与 Driver.js 的所有权。** `TourManager` 只拥有 session、状态机、进度和导航编排；`TourDriverAdapter` 只拥有 Driver.js instance、popover callbacks 和销毁。Manager 通过接口调用 adapter，不直接 import Driver.js。这样 Manager 可以在无 DOM、无 Driver.js 的测试中运行。
2. **准备动作不再是散落的 App callback。** catalog 只能声明有限的 `prepareKey`；真正的动作由注入的 `TourPreparationRegistry` 注册。没有注册的动作不能进入 catalog，禁止把 `openAddProvider`、保存设置或采集任务等业务 callback 直接塞进教程配置。
3. **按交付阶段添加锚点。** 第一版只修改 basic/proxy/station-setup 所需的壳层、总览、设置、站点、密钥池和路由页面；monitoring/advanced 的锚点在对应教程进入实现阶段时再加入，避免一次性扩大页面改动面。
4. **完成状态必须显式提交。** 最后一个必要步骤被实际展示后，Manager 才执行一次 `complete()` 并持久化；Driver 的普通 `destroy()` 不能隐式表示完成。由 Manager 主动销毁产生的 adapter callback 必须被 session token 忽略，防止完成和关闭竞态。
5. **导航 ready 只是页面层就绪。** page-ready 不代表数据查询成功。每一步仍必须经过 target resolver；动态数据目标默认 optional，不能让数据层错误阻塞教程。
6. **完成操作只能出现一次。** 最后一个可见步骤的 Driver 按钮直接显示“完成”，点击后由 Manager 提交 `completed` 并退出，不再追加结束确认卡片。optional 末步不可见时直接跳过并结束，不能制造一个没有讲解内容的第二次确认。
7. **启动命令先校验、后替换。** 教程不存在、没有步骤、被业务 Dialog 阻塞或自动播放已被处理时，不得先关闭当前 session；只有有效且未被阻塞的手动启动才允许替换旧 session。
8. **异步销毁按实例身份隔离。** Manager 使用 `sessionId + driverGeneration`，adapter 还必须校验 Driver instance identity；旧实例迟到的 `onDestroyed` 不得恢复焦点、结束新教程或触发回调。焦点只在逻辑教程开始时捕获一次，step-change 不重新捕获。
9. **Dialog 关闭使用真实退出信号。** 教程中心通过共享 `Dialog.onExited` 在 portal 实际卸载、body overflow 恢复后启动教程，不使用固定 240ms 延迟猜测清理完成。

因此，第一版可以与现有业务保持低耦合，但不能承诺“新增教程完全不改业务文件”：新增一个可讲解区域仍需要该页面提供一个稳定锚点。维护目标是“页面只加一个标记，教程逻辑和页面逻辑不互相调用”。

## 1. 当前代码约束

### 1.1 导航不是 URL 路由

当前前端使用 `AppPageId` 和 `useNavigationController`，不是 React Router。`App` 负责把 `navigateTo` 传给 `AppShell`、`ShellPageHost` 和各 feature 页面。

相关事实：

- `E:\Dev\Projects\relay-pool-desktop\src\app\App.tsx`
- `E:\Dev\Projects\relay-pool-desktop\src\app\navigationController.ts`
- `E:\Dev\Projects\relay-pool-desktop\src\app\ShellPageHost.tsx`
- `E:\Dev\Projects\relay-pool-desktop\src\app\pageTransitionPolicy.ts`

因此教程不能把“调用 navigate”视为“目标 DOM 已经可用”。必须等待导航提交、页面进入和目标元素可见。

### 1.2 页面会预热、保留和隐藏

`ShellPageHost` 可能同时保留多个 shell 页面，后台页面使用 `inert`、`aria-hidden` 和 `data-page-transition-state` 进入非交互状态。相同锚点可能存在于后台页面和当前页面中。

教程目标解析必须：

- 只从当前活动的 shell/transient layer 取元素；
- 对位于 `AppShell`、不属于页面 layer 的全局导航锚点，只有显式标记 `data-tour-scope="global"` 才允许作为例外解析；
- 排除 `inert`、`aria-hidden="true"`、不可连接和无布局尺寸的元素；
- 不依赖 `querySelector` 返回的第一个匹配项；
- 不使用 Tailwind class、中文文本或 DOM 层级作为稳定标识。

### 1.3 启动有数据恢复闸门

`BackendBootstrap` 和 `DataStoreBootstrap` 在业务应用之前完成运行时握手、本地数据库判断和迁移恢复。首次教程只能在 `renderReady` 之后启动，不能放在 `main.tsx` 的全局初始化阶段。

相关事实：

- `E:\Dev\Projects\relay-pool-desktop\src\main.tsx`
- `E:\Dev\Projects\relay-pool-desktop\src\app\bootstrap\BackendBootstrap.tsx`
- `E:\Dev\Projects\relay-pool-desktop\src\features\data-recovery\DataStoreBootstrap.tsx`

### 1.4 业务设置与教程状态必须分离

当前 `AppSettings` 由 Rust/SQLite 的 `settings` key-value 表投影，并被 portable migration 的严格 allowlist 管理。把教程状态放入 `AppSettings` 会引入 DTO、生成绑定、migration、导入导出策略和 Rust 回归测试的额外范围，但教程状态不是业务事实，也不需要跨设备复制。

第一版使用与主题、渠道状态窗口相同的安全 localStorage 访问模式：

- 读取失败回退为空进度；
- 写入失败不阻塞教程；
- 不记录日志、不暴露 DOM 文本、不访问任何凭据。

## 2. 产品范围与教程目录

### 2.1 教程设计原则

- 以用户目标和产品领域边界组织教程，不以组件或数据库表组织教程。
- 每个场景默认 3--7 步；超过 7 步必须拆成新的场景。
- 每一步只解释一个概念或一个操作区域。
- 只导航和高亮，不自动点击会修改数据的按钮。
- 不要求用户拥有真实 Station、Station Key 或 Monitor 才能完成基础教程。
- 需要真实数据才能解释的动态表格，只作为可选步骤；缺失时跳过。
- 开发者模式关闭时，不渲染或不启动依赖高级页面的步骤。

### 2.2 第一版目录

| ID | 标题 | 主要页面 | 主要内容 | 首次自动播放 | 第一版状态 |
|---|---|---|---|---|---|
| `full` | 完整体验 Relay Pool | 已发布场景的全部页面 | 按 `basic`、`proxy`、`station-setup` 顺序完整浏览 | 否 | 必须交付 |
| `basic` | 认识 Relay Pool | `dashboard`、侧边栏 | 总览、风险、密钥健康、路由队列、使用记录、设置入口 | 是 | 必须交付 |
| `proxy` | 配置本地代理 | `settings`、`routing` | 本地代理启动、端口、访问密钥、路由状态 | 否 | 必须交付 |
| `station-setup` | 配置中转站与密钥 | `stations`、`keyPool`、`collectors` | Station 资产、Station Key、采集职责和路由参与 | 否 | 必须交付 |
| `monitoring` | 查看渠道状态 | `channels`、`logs` | 渠道监控、最近状态、请求记录的职责边界 | 否 | 可在基础能力稳定后交付 |
| `advanced` | 路由与成本解释 | `routing`、`pricing`、`changes` | 路由策略、模型映射、价格/倍率、变更中心 | 否 | 仅开发者模式；可后置 |

`station-setup` 的文案必须使用现有术语“中转站”“中转站 Key”“信息采集”，不能使用不存在的“订阅导入”能力。

### 2.3 版本规则

每个教程拥有独立的整数 `revision`。修改目标、步骤顺序、核心文案或可见条件时递增该教程 revision；仅修复拼写或 CSS 不改变 revision。

建议初始版本：

```ts
const TOUR_REVISIONS = {
  basic: 1,
  proxy: 1,
  "station-setup": 1,
  monitoring: 1,
  advanced: 1,
} as const;
```

新增教程不触发自动播放，只在教程中心显示“新增”。已完成旧 revision 的教程在新 revision 下显示“有更新”，但仍由用户主动启动。

## 3. 目标架构

### 3.1 模块职责

| 模块 | 建议路径 | 唯一职责 | 禁止职责 |
|---|---|---|---|
| 类型 | `src/app/tours/tourTypes.ts` | Tour ID、step、session、progress 类型 | Driver.js 调用、DOM 查询 |
| 目录 | `src/app/tours/tourCatalog.ts` | 声明教程、步骤、revision、场景条件 | React state、导航副作用、业务写入 |
| 进度 | `src/app/tours/tourProgressStorage.ts` | 解析/校验/写入 localStorage | 数据库、IPC、日志 |
| 目标解析 | `src/app/tours/tourTargetResolver.ts` | 当前页面中查找可见锚点并等待出现 | 修改页面、猜测目标 |
| 导航适配 | `src/app/tours/tourNavigation.ts` | 调用现有导航并等待 page-ready | 直接操作 navigation state 内部字段 |
| Driver 适配 | `src/app/tours/TourDriverAdapter.ts` | 封装 Driver.js instance、popover 配置、回调转发和销毁 | 教程进度、业务导航、业务数据变更 |
| 准备动作注册表 | `src/app/tours/tourPreparationRegistry.ts` | 注册并执行有限、可测试、可逆的页面准备动作 | 任意业务 callback、创建/保存/删除数据 |
| 首次启动协调器 | `src/app/tours/tourAutoStart.ts` | 在有限时间内等待首个锚点，只有 Manager 接受 session 后锁定本进程自动启动 | 判断教程进度、复制 Manager 状态、无限轮询 |
| Manager | `src/app/tours/TourManager.ts` | 唯一的教程命令、session 状态机、导航/目标/Driver 编排和完成提交 | 直接 import Driver.js、页面业务逻辑、业务数据变更 |
| React 适配 | `src/app/tours/TourProvider.tsx` | 注入端口、创建/销毁 Manager、订阅 snapshot、监测业务 overlay 与教程的运行时互斥 | 复制一份教程状态机、读取业务缓存 |
| Overlay host | `src/app/tours/TourOverlay.tsx` | 连接 React 生命周期与 Manager，承载等待/错误的非业务状态提示；实际遮罩、高亮、气泡全部由 Driver adapter 渲染 | 自己实现遮罩、popover、focus trap，或维护 progress、导航和页面状态 |
| 设置中心 | `src/features/settings/TourCenterDialog.tsx` | 展示教程列表、状态和启动操作 | 实现 Driver.js、解析 DOM |
| 页面锚点 | 各 feature 页面 | 添加 `data-tour` 属性 | 判断教程状态、调用 Manager |

`TourManager` 是用户提出的 `TourManager` 与“Tour Controller”概念的合并实现。不要再增加一个平行的 Controller 状态源；`TourProvider` 只是 React 生命周期适配器。Manager 依赖 `TourDriverPort`、`TourNavigationPort`、`TourTargetResolver`、`TourPreparationRegistry` 和 `TourProgressStore` 等窄接口，生产环境注入真实实现，单元测试注入 fake 实现。

### 3.2 Manager 公共接口

建议接口如下，具体字段可随实现调整，但命令语义必须保持。为避免与 class 名称冲突，公共接口使用 `TourManagerApi`，实现类使用 `TourManager`：

```ts
export type TourSource = "auto" | "settings" | "test";

export type TourManagerApi = {
  /** false means the request was rejected before a session was created. */
  start(tourId: TourId, source?: TourSource): boolean;
  next(): void;
  previous(): void;
  retry(): void;
  skip(): void;
  close(): void;
  resetProgress(tourId?: TourId): void;
  getSnapshot(): TourManagerSnapshot;
  dispose(): void;
};

export type TourManagerDeps = {
  driver: TourDriverPort;
  navigation: TourNavigationPort;
  targetResolver: TourTargetResolver;
  preparation: TourPreparationRegistry;
  progress: TourProgressStore;
  isDeveloperMode: () => boolean;
  hasBlockingModal: () => boolean;
};
```

这些依赖都必须是最小端口，而不是把整个 App context 或 query client 注入 Manager。约定的最小能力为：`TourProgressStore` 提供读取、`commitCompletion`、`commitSkipped` 和 reset；`TourTargetResolver` 提供带 `AbortSignal` 的 `waitForTarget(anchor, route, signal)`；`TourPreparationRegistry` 提供 `has(prepareKey)` 和带取消信号的 `run(prepareKey, context, signal)`。端口返回可识别的取消/不可用错误，Manager 将其转换为 snapshot，不把异常抛到业务页面。

Manager 状态至少包含：

```ts
type TourManagerSnapshot = {
  phase: "idle" | "preparing" | "running" | "waiting-target" | "completed" | "skipped" | "error";
  tourId: TourId | null;
  stepIndex: number;
  stepCount: number;
  source: TourSource | null;
  message: string | null;
};
```

Manager 必须是单实例。重复调用 `start` 时，先校验教程 ID、步骤、自动播放条件和业务 modal；只有有效的手动请求才关闭旧 session，再启动新 session。旧 session 的异步导航、目标等待和 Driver callback 不得污染新 session。每次 session 都生成单调递增的 `sessionId`，每次 Driver 展示都生成 `driverGeneration`，所有异步 continuation、Driver callback 和 ready 事件必须校验 sessionId/generation 后才能写入状态。

`getSnapshot()` 必须在状态未变化时返回同一引用；状态更新时以新对象原子替换。这样才能满足 React `useSyncExternalStore` 的缓存契约，避免教程 overlay 因每次读取都得到新对象而重复渲染。

完成语义必须显式区分：最后一个必要步骤被 adapter 报告为“已展示”且用户点击该步骤的“完成”后，Manager 立即调用内部 `complete()`，再调用 progress store 的 `commitCompletion(tourId, revision)`，最后销毁 adapter 并进入 `completed`，不得追加第二个确认界面。最后一个 optional 步骤不可见时直接跳过并结束。普通 `destroy()`、组件卸载、窗口失焦或异常都不得隐式写入 `completed`；`skip()`/`close()` 才能写入 `skipped`。活动 session 存在时 `resetProgress()` 必须拒绝且不得改写进度，用户先退出教程后才可从设置重置，避免可见 session 与持久化记录发生分叉。

### 3.3 声明式步骤模型

教程目录只保存可序列化数据，不保存 React component、HTMLElement、Promise、闭包或任意 callback。跨页行为和页面准备通过受限的字符串 key 声明，key 必须在注入的 registry 中存在：

```ts
export type TourStep = {
  id: string;
  route: AppPageId;
  target: { anchor: string };
  title: string;
  description: string;
  side?: "top" | "right" | "bottom" | "left";
  align?: "start" | "center" | "end";
  optional?: boolean;
  prepareKey?: TourPreparationKey;
  requires?: "always" | "developer-mode";
};
```

首版 catalog 只使用内置 no-op key `none`，跨页由导航端口负责；`TourPreparationKey` 是受限字符串 key，而不是任意 callback。新增 key 必须先在 `TourPreparationRegistry` 注册并证明当前页面确实提供了幂等、可逆的视图准备，再写入 catalog。没有实现和测试过的 key 不得出现。任何创建、删除、保存、启动采集、启动代理或修改路由策略的动作都禁止注册。需要打开 transient form、dialog 或二次确认框的步骤放入后续任务，等 transient readiness 和 modal z-index 契约明确后再加入目录。

## 4. 教程进度存储

### 4.1 存储格式

存储键固定为：

```text
relay-pool.tours.progress.v1
```

数据格式：

```ts
export type TourProgressV1 = {
  schemaVersion: 1;
  tours: Partial<Record<TourId, {
    revision: number;
    state: "completed" | "skipped";
    updatedAt: number;
  }>>;
};
```

只接受 `schemaVersion === 1`、已知 Tour ID、正整数 revision、有限 timestamp 和允许的 state。未知字段可以忽略；类型错误、JSON 损坏和超大 payload 均回退为空进度，不抛出到业务页面。

### 4.2 状态语义

| 状态 | 含义 | 是否阻止 `basic` 自动播放 | 教程中心显示 |
|---|---|---:|---|
| 无记录 | 从未启动当前教程 revision | 否 | 新增 / 未开始 |
| `skipped` 当前 revision | 用户主动跳过或关闭 | 是 | 未完成 |
| `completed` 当前 revision | 所有必要步骤完成 | 是 | 已完成 |
| 旧 revision | 曾经看过旧版本 | 是；升级不再自动打断用户 | 有更新 |

`basic` 的自动启动条件必须同时满足：

1. 运行时已经进入 `renderReady`；
2. 当前页面为 `dashboard`，不是 transient page；
3. 当前安装没有任何已处理的 `basic` 记录；旧 revision 只在教程中心显示“有更新”；
4. 当前没有另一个教程 session；
5. 当前没有测试模式禁用标记；
6. 目标页面经过一次 layout/render 后至少有一个必需锚点可见。

自动播放只针对 `basic`。`proxy`、`station-setup`、`monitoring` 和 `advanced` 永远由教程中心启动。自动启动调用 `manager.start` 后只有在返回 `true`（session 已建立）时才写入进程内的 `autoStarted` 闸门；被业务 modal、重复 session、无目标或其他前置条件拒绝时保留重试机会。

### 4.3 安全存储行为

参考现有 `src/theme/themeStorage.ts` 和 `src/features/channels/channelStatusWindowStorage.ts`：

- `browserStorage()` 捕获访问异常；
- `readProgress()` 对 JSON、字段和大小做校验；
- `writeProgress()` 返回 boolean，不让 quota/security error 破坏教程；
- localStorage 不可用时只在当前内存中运行，不伪造“已持久化”；
- 不使用 `sessionStorage` 保存完成状态；sessionStorage 只适合临时异步操作；
- 不把教程状态加入 portable migration 的 `settings` 或任何业务表。

### 4.4 重置与数据目录语义

设置中的“重置教程进度”只删除上述教程 key，并立即刷新教程中心状态。它不重置站点、密钥、路由、日志或数据库。活动教程期间不允许重置；UI 应保持教程中心与 Driver overlay 互斥，Manager 仍要在端口层拒绝这一命令，不能只依赖 UI 遮挡。

切换数据目录、导入跨设备数据包、创建新数据库和业务数据恢复不改变教程进度。这是安装级状态的明确语义；新机器仍会根据自己的 localStorage 独立判断是否首次使用。

### 4.5 Schema 演进规则

`schemaVersion` 不是“解析失败就尽量猜”的兼容开关。v1 之后如需改变字段语义，必须新增明确的 `TourProgressV2` 解析/迁移函数，并为旧数据写回和迁移失败补测试；不能在 v1 parser 中静默接受新字段。若无法证明迁移安全，宁可丢弃教程进度并以“新增”重新提示，也不能把旧记录误判为已完成。教程 revision 只表示某个教程内容变化，不替代 progress schema migration。

## 5. 跨页面与 DOM 就绪协议

### 5.1 导航端口

`TourManager` 不直接读取或修改 `useNavigationController` 内部 state，使用窄端口：

```ts
export type TourNavigationRequest = {
  routeId: AppPageId;
  sessionId: number;
  requestToken: number;
  afterSequence: number;
  signal: AbortSignal;
};

export type TourNavigationPort = {
  navigate(routeId: AppPageId, requestToken: number): void;
  getCurrent(): {
    routeId: AppPageId;
    shellRouteId: AppRouteId;
    sequence: number;
    pending: boolean;
  };
  waitForReady(request: TourNavigationRequest): Promise<NavigationReadySnapshot>;
};
```

`App` 负责把现有 `navigateTo` 和导航完成事件接给该端口。`TourManager` 只关心“请求哪个页面”和“页面何时可用”，不接触 React navigation state。每次等待都绑定 `AbortSignal` 和 `sessionId/requestToken`：匹配 route 且 sequence 大于 `afterSequence` 的最新 ready 才能 resolve；取消、关闭 session、组件卸载或过时 sequence 必须让 promise 以可识别的取消错误 reject，并清理监听器。导航到当前 route 时也要经过同一协议，不能靠固定延时猜测 ready。

### 5.2 page-ready 信号

现有 `ShellPageHost` 已经在 `completeEntering` 处掌握页面进入动画完成时机。第一版应增加一个向上层传递的窄 callback，例如：

```ts
onPageReady?: (routeId: AppRouteId, sequence: number) => void;
```

约束：

- 只报告当前最新导航 sequence；
- 过时 sequence 不得 resolve 当前等待者；
- 初始 `dashboard` 没有 entering 动画时，由 `App` 在业务应用挂载后的下一帧报告 ready；
- transient page 暂不作为第一版必要目标；后续支持时必须为 `TransientPageHost` 增加对应 ready 语义；
- page-ready 只表示页面层进入完成，不表示数据查询成功，目标解析仍需检查可见性和 optional 条件。

### 5.3 每一步的执行顺序

```text
next()
  -> 取消当前 target wait
  -> 销毁当前 Driver instance
  -> 读取下一个 step
  -> 校验 requires / route / prepareKey
  -> 必要时 navigate(route, requestToken)
  -> 等待 page-ready(route, sequence, sessionId)
  -> 在活动 layer 中等待 target
  -> target 可见则创建 Driver 并 highlight
  -> target 超时则 optional skip 或显示可重试错误
```

同页步骤也必须经过 target resolver；不能因为没有导航就直接传入 selector。这样可以统一处理加载、折叠区、标签页和后台页面重复 DOM。

### 5.4 目标解析

目标解析器接收 `TourStep.target.anchor`，生成精确 selector：

```ts
const selector = `[data-tour="${escapeCssIdent(anchor)}"]`;
```

选择候选时按以下顺序过滤：

1. 位于当前 `data-page-transition-page-id` 对应的活动 layer；若节点位于 AppShell 全局区域，则必须有 `data-tour-scope="global"`，且不在任何隐藏/inert 容器内；
2. 不在 `[inert]` 子树中；
3. `aria-hidden !== "true"`；
4. `isConnected === true`；
5. `getBoundingClientRect()` 有正宽高；
6. `getComputedStyle` 不是 `display:none`、`visibility:hidden` 或完全透明；
7. 当前页面与 step 的 route 一致。

等待实现使用 `MutationObserver` 监听 DOM 变化，并通过 `requestAnimationFrame` 重试布局；默认超时 3 秒，超时后清理 observer 和 timer。不得用无上限的 `setInterval`。

## 6. Driver.js 生命周期

### 6.1 实例边界

Driver.js instance 只服务当前可见页面和当前 session。跨页面时必须由 `TourManager` 调用 `TourDriverAdapter.destroy()` 后重新创建，不能让旧实例持有已卸载或隐藏的 DOM。Manager 不直接 import Driver.js；adapter 是唯一允许依赖第三方库的模块。

adapter 对 Manager 暴露类似如下的窄接口（端口类型放在 `tourTypes.ts`，字段以安装版本类型定义为准）：

```ts
export type TourDriverPort = {
  showStep(input: {
    element: HTMLElement;
    title: string;
    description: string;
    side?: TourStep["side"];
    align?: TourStep["align"];
    stepIndex: number;
    stepCount: number;
    callbacks: {
      next: () => void;
      previous: () => void;
      close: () => void;
      destroyed: () => void;
    };
  }): void;
  destroy(reason: "step-change" | "skip" | "close" | "complete" | "dispose"): void;
};
```

`TourDriverAdapter` 内部配置 `showProgress`、关闭/遮罩行为、键盘支持和官方 CSS。由于跨页会重建 Driver instance，不能只依赖 Driver.js instance 内部的 step 总数；adapter 必须接收 `stepIndex/stepCount`，通过 Driver 支持的 progress text 或可访问的 fallback 保持全教程进度一致。实际 API 字段以安装的 Driver.js 版本类型定义为准，不手写 `any` 绕过类型检查。Driver.js 只接受已经解析的当前元素，不能把跨页等待逻辑塞进 Driver callback。

### 6.2 交互规则

- `下一步` 由 Manager 接管，默认 callback 不得再推进一次；
- `上一步` 可以跨页返回，并重新执行目标准备；
- `跳过` 结束当前教程并记录 `skipped`；
- `完成` 只在所有必要步骤完成后出现；Manager 在最后一步的用户继续动作中先提交完成，再销毁 adapter；
- optional 末步不可见时，Manager 显示可访问的“完成教程”确认，不自动提交完成；
- overlay 点击关闭时等同于跳过当前 session，但必须有明确的退出结果；
- `Esc`、关闭按钮和系统窗口失焦不能留下活动 overlay；
- 高亮区域必须设置 `disableActiveInteraction: true`，教程仅解释控件，不能因点击高亮区域而触发保存、删除、启动代理或打开业务流程；
- Manager 主动销毁 adapter 时要带 reason，并忽略由该销毁同步/异步触发的 `destroyed` callback；外部销毁才作为异常关闭处理，不能被误判为完成；
- Manager dispose 时恢复启动前的 focus，且不聚焦到 `inert` 元素；adapter 按 Driver instance identity 忽略迟到的旧 `onDestroyed`，并在一个逻辑 session 内只捕获一次焦点；
- 不记录用户点击了哪个业务控件，不发送遥测。

### 6.3 不可用步骤

步骤缺少目标时：

- `optional: true`：记录 session 内部的 skipped step，继续下一个；
- `optional: true` 且已是最后一步：跳过该不可见步骤并直接结束，不显示额外确认；
- `optional: false`：气泡区域显示“当前页面暂不可用”，提供“重试”和“跳过教程”；
- 必要步骤被跳过时，教程不能写入 `completed`；
- 失败只影响当前教程，不影响 React 页面、React Query 或业务导航。

第一版不尝试让 Driver overlay 与业务 modal 叠加。启动前若存在教程中心以外的 Dialog、ConfirmDialog 或 transient editor，Manager 直接拒绝启动；运行中若业务 Dialog、ConfirmDialog 或 active transient 页面出现，`TourProvider` 必须结束当前教程，避免两个 focus/z-index owner 竞争。共享 Dialog 使用 `data-tour-blocking="true"` 标记；检测时必须排除 Driver.js 自身的 `.driver-popover`，不能让教程 overlay 自己触发退出。不以“把 z-index 调到更高”作为兼容方案。

## 7. 页面锚点计划

页面组件只增加锚点，不导入 `TourManager`，不判断教程是否运行。

### 7.1 全局壳层

修改：`E:\Dev\Projects\relay-pool-desktop\src\components\shell\AppShell.tsx`

计划锚点：

| 锚点 | 目标 |
|---|---|
| `shell-sidebar` | 侧边栏整体导航区域 |
| `nav-dashboard` | 总览导航按钮 |
| `nav-stations` | 中转站资产导航按钮 |
| `nav-key-pool` | 密钥池导航按钮 |
| `nav-routing` | 路由规则导航按钮 |
| `nav-settings` | 设置导航按钮 |

锚点应放在稳定的按钮或容器上；不能把 badge 数字本身作为目标。

### 7.2 总览

修改：`E:\Dev\Projects\relay-pool-desktop\src\features\dashboard\DashboardPage.tsx`

第一版只选择在空数据和有数据时都存在的区域：

- `dashboard-metrics`
- `dashboard-risk`
- `dashboard-key-health`
- `dashboard-routing-queue`
- `dashboard-recent-usage`

动态请求行、告警行和具体密钥名称不作为基础教程的必要目标。

### 7.3 设置

修改：`E:\Dev\Projects\relay-pool-desktop\src\features\settings\SettingsPage.tsx`

首版计划锚点：

- `settings-local-proxy`
- `settings-network`
- `settings-tutorial-entry`

`settings-tutorial-entry` 放在新增的“帮助与教程”区段的行容器上；按钮本身另外保留正常的 accessible label，不让教程 target 与点击动作耦合。

该入口锚点允许先于已发布步骤存在，作为教程中心和后续设置教程的稳定 UI 契约；锚点契约测试允许这类“已声明但暂未发布”的扩展锚点，不要求它出现在当前 catalog 中。

### 7.4 第一版业务页面锚点

第一版计划锚点：

| 页面 | 锚点 |
|---|---|
| `StationsPage` | `stations-list` |
| `KeyPoolPage` | `key-pool-list` |
| `RoutingPage` | `routing-status`（稳定外层容器；展示前通过 `routing-status-tab` 切换到概览，结束后恢复原 tab） |
| `CollectorsPage` | `collectors-summary` |

这些锚点只覆盖第一版 `basic`、`proxy` 和 `station-setup`。`monitoring` 所需的 `ChannelStatusPage`/`LogsPage` 锚点，以及 `advanced` 所需的 `PricingPage`/`ChangeCenterPage` 锚点，等对应教程真正进入交付阶段再添加，避免先扩大稳定页面的改动面。

动态行、具体站点、具体密钥和需要打开 dialog 的操作先作为 optional 或后续任务处理。第一版不为了教程自动打开“添加供应商”“新增密钥”表单。

`*-toolbar`、`*-add`、`*-edit`、tab、筛选器和高级区域不是首版预留锚点；只有某个已发布步骤确实需要解释该区域时，才在对应场景任务中增加并测试，避免为未来设想提前修改页面。

### 7.5 锚点契约测试

锚点是跨模块的稳定契约，不能只靠人工记忆维护。增加 catalog/anchor contract 测试，至少校验：

- 每个场景教程的 `target.anchor` 非空且在场景目录内唯一；`full` 只复用已经通过场景契约验证的 anchor，不引入新的页面目标；
- catalog 中的 `route` 属于现有 `AppPageId`，`prepareKey` 属于 registry 已注册 key；
- 第一版发布的锚点清单必须被页面测试 fixture 覆盖；页面可以保留未发布的扩展锚点（例如 `settings-tutorial-entry`），但不得出现与已发布 anchor 同名的第二个可见节点；
- 目标容器在 loading、empty、error 和正常数据状态下仍存在，若做不到必须显式标记 `optional`；
- 页面不因重复预热而产生两个可见同名 anchor，后台 layer 的同名节点不能通过 resolver。

测试只验证稳定标记和可见性，不把业务文案、Tailwind class、DOM 层级或动态数据值纳入契约。

该测试分为两层：catalog 单元测试负责 ID、route、anchor 格式和 revision；页面集成/冒烟测试负责“已发布 anchor 实际存在且位于正确活动 layer”。不能把仅验证字符串非空的 catalog 测试宣称为完整的页面契约测试。页面集成测试必须真实挂载已发布页面组件并放入 active layer；仅读取源码字符串的测试不能作为页面契约证据。每次删除或重命名页面 anchor 都必须同时更新 catalog、对应集成测试和 revision 评估。

## 8. 设置中的教程中心

### 8.1 UI 位置

在 `SettingsPage` 的“数据与备份”之后、“高级”之前加入“帮助与教程”区段。复用现有 `SectionCard`、`SettingRow`、`Button` 和 `Dialog`，不新造第二套设置布局。

首版设置页只放一个稳定入口；详细状态集中在教程中心，避免 SettingsPage 复制一份进度投影：

| 内容 | 行为 |
|---|---|
| 使用教程 | 打开教程中心对话框 |
| 重置教程进度 | 在教程中心操作，需要确认，只删除教程 localStorage |

### 8.2 教程中心对话框

对话框列出当前 catalog 中满足 `requires` 的教程。每一项显示：

- 标题；
- 一句话用途；
- `开始` 或 `重新查看`（当前只持久化完成/跳过状态，不伪造“从上次步骤继续”的能力）；
- `已完成`、`未完成`、`新增` 或 `有更新`状态。

点击启动时的顺序：

1. 关闭教程中心 `Dialog`；
2. 等待 Dialog portal 卸载和 body overflow 恢复；
3. 恢复启动教程前的 opener focus，再调用 `manager.start(tourId, "settings")`；`start` 返回 false 时不替换现有 session；
4. 必要时导航到首步 route；
5. 目标解析成功后显示 Driver overlay。

教程中心本身不显示业务数据，不显示任何 secret、endpoint、cookie 或原始错误。

## 9. 样式、窗口和可访问性

### 9.1 依赖与样式

在 `package.json` 中加入 Driver.js，并更新 `pnpm-lock.yaml`。版本必须以仓库实际安装的类型定义为准，不能用 `any` 规避 API 差异。

引入官方 CSS 后，在 `E:\Dev\Projects\relay-pool-desktop\src\styles.css` 增加项目 token 覆盖：

- overlay 使用 `--scrim`；
- popover 使用 `--surface`、`--foreground`、`--border` 和 `--popover-shadow`；
- primary button 使用现有 primary token；
- 圆角不超过现有 `--surface-radius`；
- z-index 高于普通页面和 `PageScaffold` sticky header，但不覆盖确认弹窗的 `z-60` 语义；
- 同时覆盖 `.dark`，但默认视觉仍保持浅色桌面工具风格。

Driver.js 是外部 MIT 依赖。安装时必须检查许可证兼容性，并在 `E:\Dev\Projects\relay-pool-desktop\THIRD_PARTY_NOTICES.md` 增加项目、版本、许可证和官方链接，不复制或手改第三方实现源码。

### 9.2 可访问性

- Driver popover 必须具备可读标题、描述和明确的下一步/上一步/关闭名称；
- 键盘可以完成全部教程操作；
- focus 不得落入 `[inert]` 或 `aria-hidden` 页面；
- 退出后恢复设置按钮或启动前的可连接元素；
- 窄窗口下 popover 文案换行，不遮挡按钮；
- `prefers-reduced-motion` 下不增加额外动画；
- 页面 loading、error、empty、disabled 状态都不能造成无限等待；
- 启动教程不能阻塞业务页面的正常键盘操作，除 Driver overlay 本身外不修改全局 tabindex。

### 9.3 Dialog 与 overlay 互斥

第一版只支持在没有业务 Dialog、ConfirmDialog、临时编辑页覆盖时启动教程。Manager 启动前检查教程中心以外的活动 modal；如果发现冲突则拒绝启动并给出非阻塞提示。

需要高亮 modal 内部控件的能力另列后续任务，必须先为 `Dialog`/`TransientPageHost` 增加可观察的 ready 和 z-index 契约。业务 modal 互斥既要在启动前同步检查，也要在教程运行中通过 `MutationObserver` 监测；该 observer 只在 active snapshot 下调用 `close()`，且销毁时必须 disconnect。

### 9.4 可靠性分级与发布门槛

教程功能按以下级别交付：

| 级别 | 允许范围 | 必须证据 |
|---|---|---|
| L1 核心 | 活动 shell 页面、静态 anchor、同页/跨 shell 导航 | Manager/adapter/navigation/target/storage focused tests、`pnpm build` |
| L2 视图准备 | 已存在 tab/filter 的可逆切换 | preparation registry 单测、取消/失败测试、窄窗口手测 |
| L3 transient/modal | 表单、Dialog、临时页内部控件 | transient ready、overlay 层级、focus cleanup 的自动化和真实 Tauri WebView 手测 |

没有达到对应证据级别时，catalog 不得声明该步骤。L1 的稳定性不能推导出 L3 的可靠性。

## 10. 实施任务

### Task 0：冻结范围、依赖和 RED 证据

目标：建立实现前基线，不改变产品行为。

文件：

- Create: `docs/plans/2026-08-27-guided-tour-system.md`
- Inspect: `src/app/App.tsx`、`src/app/ShellPageHost.tsx`、`src/components/shell/AppShell.tsx`
- Inspect: `src/theme/themeStorage.ts`、`src/features/channels/channelStatusWindowStorage.ts`

步骤：

1. 确认只有 `basic` 允许自动启动。
2. 确认教程状态不进 SQLite、IPC、portable migration。
3. 确认目标只从活动 layer 解析。
4. 为 manager、Driver adapter、preparation registry、storage、target resolver、跨页续接分别列出 focused test 文件。
5. 记录 Driver.js 安装版本、许可证和 bundle 影响。

完成条件：范围和不变项进入文档；没有实现代码变化；工作区原有改动保持不变。

### Task 1：安装 Driver.js 并建立第三方记录

目标：引入成熟渲染引擎，确保依赖门禁和许可信息完整。

文件：

- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Modify: `THIRD_PARTY_NOTICES.md`
- Modify: `src/styles.css`

步骤：

1. 使用 `pnpm add driver.js` 或仓库等价命令安装依赖。
2. 检查安装版本的类型定义、API 和 MIT license。
3. 引入官方 CSS，使用项目 token 覆盖视觉和 z-index。
4. 不复制第三方 CSS 到多个 feature 文件。

完成条件：依赖可被 Vite/TypeScript 正常解析，许可记录完整，`pnpm build` 的 theme audit 不出现新的违规。

### Task 2：定义类型、目录和进度存储

目标：建立纯配置和可靠的安装级状态，不接入 Driver 或页面。

文件：

- Create: `src/app/tours/tourTypes.ts`
- Create: `src/app/tours/tourCatalog.ts`
- Create: `src/app/tours/tourProgressStorage.ts`
- Create: `src/app/tours/tourProgressStorage.test.ts`

步骤：

1. 定义 `TourId`、`TourStep`、`TourDefinition`、`TourProgressV1` 和状态类型。
2. 先编写 `basic`、`proxy`、`station-setup` 三个首版发布目录；`monitoring`、`advanced` 只能作为未发布 metadata 或留到 Task 8，不得因为占位定义而出现在教程中心或触发等待。
3. 实现 localStorage 安全读取、严格校验、写入和 reset。
4. 测试空值、旧 schema、未知 ID、旧 revision、损坏 JSON、超大 JSON、storage 抛错和 quota error。
5. 测试“跳过当前 revision 不再自动播放，但仍可手动启动”。

完成条件：storage 单元测试完整通过；该层没有 React、Driver.js、DOM 或 IPC 依赖。

### Task 3：实现目标解析与等待

目标：在页面预热、隐藏、加载和切页条件下只返回真正可见的活动目标。

文件：

- Create: `src/app/tours/tourTargetResolver.ts`
- Create: `src/app/tours/tourTargetResolver.test.ts`

步骤：

1. 实现 anchor 到 selector 的安全转换。
2. 实现活动 shell/transient layer 过滤。
3. 实现 `isConnected`、`inert`、`aria-hidden`、布局尺寸和 computed style 检查。
4. 实现 `MutationObserver + requestAnimationFrame` 的有界等待。
5. 测试相同 anchor 同时出现在后台和前台页面时只返回前台目标。
6. 测试目标一直缺失、目标先缺失后出现、目标出现但隐藏、目标被卸载和 timeout cleanup。

完成条件：target resolver 不依赖 React component，不留下 observer、timer 或未处理 Promise。

### Task 4：接入导航 ready 协议

目标：让 Manager 能安全等待当前导航完成。

文件：

- Modify: `src/app/ShellPageHost.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/components/ui/Dialog.tsx`（增加可选 `onExited` 生命周期回调）
- Create or modify: `src/app/tours/tourNavigation.ts`
- Create: `src/app/tours/tourNavigation.test.ts`

步骤：

1. 将 `ShellPageHost` 的最新页面进入完成信号通过窄 callback 暴露给 `App`。
2. 处理初始 dashboard 无 entering animation 的 ready 事件。
3. 创建 `TourNavigationPort`，只暴露 `navigate`、`getCurrent` 和 `waitForReady`。
4. 使用 `sessionId`、request token 和 navigation sequence 忽略过期导航完成事件；关闭 session 时通过 AbortSignal 取消等待。
5. 测试快速连续点击、导航到同一路由、旧 sequence 迟到和页面切换期间关闭教程；后续组合测试还需覆盖“已有 Driver overlay 不应被误当作业务 modal”。

完成条件：导航 port 不暴露 `useNavigationController` 内部 state；没有为教程另造路由系统。

### Task 5：实现 TourManager 和 React 适配

目标：建立唯一教程状态机和 Driver 生命周期。

文件：

- Create: `src/app/tours/TourManager.ts`
- Create: `src/app/tours/TourDriverAdapter.ts`
- Create: `src/app/tours/tourPreparationRegistry.ts`
- Create: `src/app/tours/TourProvider.tsx`
- Create: `src/app/tours/TourOverlay.tsx`
- Create: `src/app/tours/TourManager.test.ts`
- Create: `src/app/tours/TourDriverAdapter.test.ts`
- Create: `src/app/tours/tourPreparationRegistry.test.ts`
- Create: `src/app/tours/TourOverlay.test.tsx`

步骤：

1. 实现 `idle -> preparing -> waiting-target -> running -> completed`，最后一个可见步骤直接完成。
2. 实现 `start`、`next`、`previous`、`retry`、`skip`、`close`、`resetProgress`。
3. 同页切换复用 session；跨页由 Manager 通过 `TourDriverPort` 销毁旧 Driver 并重建。
4. 将 Driver callbacks 全部接到 Manager，不允许 Driver adapter 自己推进 React state、导航或进度。
5. 将 optional/required target timeout 转为确定的 session 结果；optional 末步不可见时跳过并结束，不追加确认界面。
6. 在 Manager destroy 时清理 observer、timer、Driver、navigation waiter 和 focus restore。
7. 使用 session ID、driver generation 和 adapter instance identity 防止旧异步任务修改新 session；焦点只捕获一次。
8. 测试最后一步单击完成、跳过、关闭、上一步、跨页、缺失 optional target、缺失 required target、optional 末步直接结束、重复/无效 start、活动 session 的 reset 拒绝、持久化失败、过期 session/Driver callback 和 unmount；Provider 还要测试业务 modal 出现时关闭，而 Driver.js popover出现时不关闭。

完成条件：Manager 可以通过 fake Driver/fake navigation/fake preparation 独立测试；Driver.js 只在 adapter 测试或浏览器集成测试中出现；React 只订阅 snapshot，不复制状态机。

### Task 6：添加页面锚点

目标：为基础教程提供稳定的、与业务逻辑解耦的 DOM 目标。

文件：

- Modify: `src/components/shell/AppShell.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/features/collectors/CollectorsPage.tsx`

步骤：

1. 只添加稳定容器、标题区域、工具栏或固定按钮的 `data-tour`。
2. 不给 secret、endpoint 原文、动态行 ID、随机值或业务数据内容添加 target。
3. 保持既有 aria-label、键盘行为和按钮 disabled 逻辑不变。
4. 对空、加载、错误状态确认必要 anchor 仍存在；否则把步骤标记 optional。
5. 对预热页面确认后台 anchor 不会被 resolver 选中。

完成条件：首版 anchor 清单与已发布 catalog 一一对应；页面组件不导入教程模块；monitoring/advanced 页面不因本任务提前增加锚点。

### Task 7：加入设置教程中心和首次自动启动

目标：提供用户可重复进入的入口，并在正确的启动时机只自动播放 basic。

文件：

- Create: `src/features/settings/TourCenterDialog.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/app/shellPageRegistry.tsx`
- Modify: `src/main.tsx` 或 `BackendBootstrap` 的 renderReady 调用点（仅在确有需要时）
- Create/update: settings UI tests

步骤：

1. 在设置中加入“帮助与教程”区段和教程入口。
2. 只通过一个 `onOpenTourCenter` 窄回调把设置入口接到 App/TourProvider；`SettingsPage` 不接收 Manager、Driver、导航端口或多组教程 callback。
3. 启动教程中心时显示每个场景的状态、revision 更新和新增提示。
4. 启动 Driver 前关闭教程中心 dialog，通过 `Dialog.onExited` 等待其 portal/body overflow 实际清理后再调用 Manager，避免两个 overlay 叠加。
5. 业务应用 ready 后以一次 requestAnimationFrame/微任务延迟检查 basic 自动条件。
6. 仅 desktop ready 模式启用首次自动启动；demo/测试可以通过显式参数关闭自动启动，避免污染截图和 UI 测试。
7. 提供 reset progress 确认操作，失败时给出非阻塞 toast；活动 session 时 Manager 拒绝 reset，设置层不得绕过此命令语义。

完成条件：首次启动只自动显示 basic 一次；设置可以再次启动所有可用场景；教程中心关闭不会影响页面导航和 body overflow。

### Task 8：补齐场景内容和高级条件

目标：在核心引擎稳定后加入实际产品场景。

文件：

- Modify: `src/app/tours/tourCatalog.ts`
- 仅在 `monitoring` 进入实现时修改：`src/features/channels/ChannelStatusPage.tsx`、`src/features/logs/LogsPage.tsx`
- 仅在 `advanced` 进入实现时修改：`src/features/pricing/PricingPage.tsx`、`src/features/changes/ChangeCenterPage.tsx`
- Update: `src/app/tours/TourManager.test.ts`

步骤：

1. 先完成 `proxy`：设置本地代理 -> 路由状态。
2. 再完成 `station-setup`：中转站资产 -> 密钥池 -> 信息采集，强调 Station 与 Station Key 的职责差异。
3. 加入 `monitoring`：渠道状态与使用记录，只解释读取职责，不触发监控执行。
4. 只有 developer mode 开启时才展示 `advanced`；关闭后不允许 Manager 等待被隐藏的目标。
5. 对页面 tab、筛选器和动态表格只能使用 registry 中已经实现并测试过的 `prepareKey` 打开已有可逆视图状态；没有对应页面能力的 key 不得写入 catalog；禁止自动提交业务修改。
6. 每个场景保持 3--7 步，必要时拆分新教程并递增对应 revision。

完成条件：每个已发布场景都有可重复启动、可退出、可跳过和可完成的路径；不存在与实际功能不符的业务文案或能力。

### Task 9：验证、文档和交付证据

目标：完成前端行为、依赖、可访问性和跨层影响的验证。

文件：

- Update: 本计划的实施状态和证据段落
- Update: `docs/README.md`（仅在仓库维护者要求把计划加入导航时）

步骤：

1. 运行教程 focused Vitest。
2. 运行完整前端 Vitest。
3. 运行 `pnpm build`。
4. 如果修改 `ShellPageHost` 导航基础设施，运行 `pnpm verify:fast`。
5. 使用窄窗口和系统 reduced-motion 手测 overlay、popover 换行、focus 和 Esc。
6. 手测首次启动、跳过、设置重开、跨页、目标缺失、开发者模式切换和 storage 被禁用。
7. 记录未能验证的真实 Tauri 窗口行为，不用浏览器开发模式结果冒充桌面验证。
8. 记录 anchor 集成/冒烟测试是否运行；若只运行 catalog 单测，交付记录必须明确写成“未验证页面 anchor 完整性”。

完成条件：所有必要 gate 绿色，或在交付说明中明确列出未完成范围、原因和风险；不得把截图手测替代自动化测试。

## 11. 测试矩阵

### 11.1 单元测试

| 测试对象 | 必须覆盖 |
|---|---|
| `tourProgressStorage` | 正常读写、空值、损坏 JSON、未知字段、旧 revision、storage 异常、quota error、reset |
| `tourCatalog` | ID 唯一、step ID 唯一、route 合法、anchor 非空、revision 正整数、requires 条件完整 |
| `tourTargetResolver` | 活动/后台重复 DOM、inert、aria-hidden、display none、零尺寸、observer cleanup、timeout |
| `tourNavigation` | page-ready、旧 sequence、快速导航、初始 dashboard、关闭 session 后迟到事件 |
| `TourPreparationRegistry` | 未注册 key 拒绝、已注册动作幂等、取消/异常传播、不执行业务 mutation |
| `TourDriverAdapter` | Driver 配置映射、callback 转发、主动 destroy reason、外部 destroy、重复 destroy、旧实例迟到回调和 focus 恢复 |
| `TourManager` | 状态机、跨页重建、上一步、跳过、关闭、必要/可选缺失、optional 末步显式完成、重复/无效 start、活动会话 reset 拒绝、异步竞态、Driver generation、完成/跳过持久化失败 |

### 11.2 React/UI 测试

- `TourCenterDialog` 显示新、未完成、已完成和有更新状态；
- 点击启动先关闭 Dialog，再启动 Manager；
- SettingsPage 不直接导入 Driver.js；
- basic 在 ready 后自动启动，在已有 skipped/completed 记录时不启动；
- demo/test 模式不会意外自动弹出；
- 页面锚点存在且不改变既有按钮行为；
- 设置、主题、页面切换和教程 overlay 不发生 body overflow 竞争；
- 教程运行中出现业务 Dialog 或 active transient 时主动关闭；Driver.js 自身 popover 不得被识别为业务 Dialog；
- narrow viewport 下长文案不溢出 popover。

### 11.3 需要手测的桌面行为

- Tauri WebView 首次加载完成后 overlay 的 z-index；
- 最小支持窗口尺寸下 Driver popover 定位；
- 窗口最小化、恢复、失焦、关闭请求期间的 cleanup；
- 系统主题变化时 Driver CSS token；
- Windows WebView localStorage 被禁用或清理时的降级行为。

## 12. 故障与失败语义

| 场景 | 用户可见行为 | 持久化行为 | 禁止行为 |
|---|---|---|---|
| localStorage 读取失败 | 教程仍可临时运行 | 不声称已保存 | 不阻塞业务应用 |
| localStorage 写入失败 | toast 提示“教程进度未保存” | 内存状态继续 | 不抛出未处理异常 |
| 目标加载超时且 optional | 跳到下一步 | 不记录完成 | 不无限等待 |
| 目标加载超时且 optional 且为末步 | 显示“完成教程”确认 | 用户确认后才记录 completed | 不自动完成 |
| 目标加载超时且 required | 显示重试/退出 | 不写 completed | 不高亮后台页面 |
| 导航 sequence 过时 | 忽略事件 | 保持当前 session | 不覆盖新 session |
| 页面被卸载 | destroy 当前 Driver | 记录 skipped 或保持未完成 | 不保留 stale element |
| developer mode 关闭 | 不显示 advanced 或跳过该步 | 不误记完成 | 不等待隐藏 target |
| 用户点击 overlay/关闭 | 结束教程 | 写入 skipped 当前 revision | 不自动重新弹出 |
| 业务 Dialog 已打开 | 拒绝启动或稍后重试 | 不改业务状态 | 不叠加多个 modal |
| 教程运行中出现业务 Dialog / transient | 立即结束教程 | 写入 skipped 当前 revision | 与业务 overlay 竞争 z-index、focus 或 body overflow |
| 活动 session 请求重置进度 | 拒绝重置并提示先退出教程 | 不改动 progress | 让当前 session 与持久化记录分叉 |

## 13. 验收标准

### 13.1 功能验收

1. 新安装的 desktop 运行时在业务应用 ready 后只自动播放一次 `basic`。
2. `basic` 被跳过、关闭或完成后，下一次启动不会再次自动出现。
3. 设置中的教程中心可以重新启动每个当前可用场景。
4. `proxy` 至少能完成设置页面到路由页面的跨页续接。
5. 新增教程或 revision 变化只显示新增/有更新提示，不自动打断工作流。
6. 没有站点、密钥、日志或监控数据时，教程不会卡在动态目标上。
7. hidden/inert 页面永远不会被 Driver 高亮。
8. 教程状态不出现在 SQLite、IPC、portable migration 或日志中。
9. 业务 modal 与教程不并存；教程 popover 本身不会触发该互斥退出。

### 13.2 架构验收

- 页面只声明 `data-tour` 锚点，不依赖 Manager；
- `TourManager` 是唯一教程状态 owner；
- Driver.js 不负责业务导航、版本和持久化；
- `TourDriverAdapter` 是唯一直接依赖 Driver.js 的模块，Manager 只依赖 `TourDriverPort`；
- `TourPreparationRegistry` 只暴露有限、可测试、可逆的视图准备动作；
- `TourNavigationPort` 不暴露导航内部 state；
- 没有 arbitrary selector callback、DOM 引用或 Promise 出现在 catalog；
- 没有为了教程新增业务 API、Rust command 或 migration；
- 没有自动点击会修改业务数据的控件；
- 外部依赖许可与锁文件完整。

### 13.3 验证门槛

前端实现至少运行：

```powershell
pnpm test -- src/app/tours src/features/settings
pnpm build
```

如果 Task 4 修改导航基础设施或跨层公共行为，再运行：

```powershell
pnpm verify:fast
```

Rust 检查只有在实现范围意外扩大到 `src-tauri` 时才是必要门槛；若发生该扩展，必须同时运行仓库规则要求的 `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml` 和相关 Cargo 测试，并重新审查本计划的“不适用范围”。

自动化绿色只证明代码契约；Driver.js 的实际 WebView 定位、z-index、窗口缩放、最小化/恢复和浏览器存储禁用仍属于桌面手测门槛。未完成这些手测时，计划状态只能写明“自动化完成、桌面验证未完成”，不能笼统宣称教程系统已完全验证。

## 14. 暂缓事项与扩展门槛

以下内容不应在第一版中顺手加入：

- 将教程进度迁移到 SQLite 或 portable migration；
- 云同步、账号同步、跨设备教程历史；
- 自动创建 Station、Station Key、Monitor 或路由规则；
- 自动点击“启动代理”“保存”“删除”“运行采集”等业务动作；
- 依赖 transient form、dialog 内部控件的复杂教程；
- 任何尚未存在且未通过产品评审的新业务能力；
- 教程埋点、远程统计、外部反馈服务；
- 在 Driver.js 之外自研 overlay、popover 或 focus trap；
- 通过增加固定延时掩盖导航 ready 或目标解析缺陷。

只有在以下条件满足后，才允许扩展 transient/modal 教程：

1. `TransientPageHost` 和 `Dialog` 提供明确、可测试的 ready/close 信号；
2. target resolver 能区分活动 transient 与背景 shell；
3. overlay 与 `z-50`/`z-60` modal 层级有自动化或手测证据；
4. 业务动作仍由用户明确执行，教程不会代替用户提交；
5. 新步骤有独立 optional/required 失败语义和回归测试。

## 15. 交付记录模板

实现完成后在本文末尾补充：

```text
实施状态：Completed / Partially completed
完成日期：YYYY-MM-DD
实现 revision：basic=?, proxy=?, station-setup=?, monitoring=?, advanced=?
已运行验证：...
未验证范围：...
已知风险：...
```

如果实现过程中发现必须修改 SQLite、IPC、portable migration 或产品业务能力，先暂停当前任务，在本计划中记录范围变化和理由，再决定是否拆出新的 spec/plan；不得用未记录的旁路状态或兼容字段绕过现有架构门禁。

## 16. 维护性与耦合复核

### 16.1 可接受的耦合

教程必须与少量稳定契约耦合，才能在真实页面上工作：

- catalog 使用现有 `AppPageId`/`AppRouteId`，只表达“要到哪个页面”；
- 页面提供稳定的 `data-tour` anchor，anchor 是 UI 结构契约，不是业务数据契约；
- App 提供一个导航适配器和一个设置入口回调，负责把现有导航生命周期接给教程；
- `TourPreparationRegistry` 只连接已经存在、可逆且有测试的视图准备能力。

这些耦合点都应集中在 `src/app/tours/` 和 App 的适配边界内。需要视图准备的 feature 只暴露通用、可逆的 view port（例如“显示概览并返回恢复函数”），不得 import 教程模块，也不应知道 session、revision、localStorage 或 Driver.js。

### 16.2 禁止的耦合

以下做法视为架构违规，需要在 code review 中拒绝：

- feature hook、React Query controller 或业务 service 直接 import `TourManager`/Driver.js；
- catalog 保存函数、闭包、DOM 引用、业务对象或任意 selector callback；
- 为了教程新增 Rust command、IPC DTO、SQLite 字段或 portable migration 旁路；
- `prepareKey` 触发保存、创建、删除、启动代理/采集或修改路由策略；
- 通过业务文案、Tailwind class、DOM 层级或动态 ID 代替 anchor；
- 把 page-ready 当成数据请求成功，或用固定延时掩盖未定义的 ready 语义；
- 让 Driver 的 `destroy` callback 直接写入完成状态。

### 16.3 新增教程的维护流程

新增一个教程或步骤时按固定顺序执行：

1. 在 `tourTypes.ts` 增加必要的受限类型，在 `tourCatalog.ts` 增加纯数据定义和独立 revision；
2. 只在确有讲解价值的页面添加稳定 anchor，并更新 anchor contract 测试；
3. 若需要切换已有视图，先在 `TourPreparationRegistry` 增加幂等、可逆的 key、单元测试和失败语义；没有 registry 支持就不在 catalog 声明；
4. 为 Manager 的跨页、取消、缺失目标和完成提交补 focused test；
5. 通过教程中心手动启动验证；只有 `basic` 的首次自动播放规则可以改变自动行为；
6. 修改核心步骤或条件时递增对应 revision，并在交付记录中说明迁移/提示语义。

如果一个新教程需要修改多个业务页面的控制器、依赖业务数据才能通过，或需要 modal/transient 内部状态才能成立，应先拆成独立设计评审，不把复杂度继续塞进 `TourManager`。

### 16.4 本次可靠性审计结论

| 审计项 | 结论 | 维护规则 |
|---|---|---|
| 与业务耦合 | 可接受、受控 | 耦合仅落在 `AppPageId`、`data-tour` anchor、App 导航适配器和显式 preparation registry；feature 不导入 Manager/Driver.js，不读取教程状态 |
| 跨页竞态 | 已收口 | 每次请求带 `sessionId/requestToken/afterSequence`；新 token 取消旧 waiter；迟到 ready、关闭后的 ready 和旧 sequence 都忽略 |
| Driver 生命周期 | 已收口 | Manager generation + adapter instance identity 双重隔离；主动销毁不转发 destroyed；旧实例不恢复焦点；逻辑 session 只捕获一次焦点；教程中心在 portal 退出完成后恢复 opener focus，再启动 Driver |
| 完成/跳过语义 | 已收口 | 用户点击最后一个可见步骤的“完成”后立即提交 completed 并退出，不追加确认卡片；skip/close 提交 skipped；store 返回 false 或抛错时显示未持久化警告 |
| 进度重置 | 已收口 | reset 只允许在不存在活动 session 时操作；活动 session（包括错误恢复态）必须拒绝，UI 和 Manager 均不允许产生持久化分叉 |
| Overlay 互斥 | 已收口 | 启动前检查 blocking modal；运行中观察显式 Dialog/ConfirmDialog/active transient 并 close；Driver.js popover 被 guard 排除 |
| 错误暴露 | 已收口 | target/preparation/navigation 错误映射为白名单用户文案，未知异常使用通用文案，不透传内部 selector、route 或异常文本 |
| 可扩展性 | 有边界地扩展 | 新场景先加纯 catalog + anchor contract；需要切换 tab/filter 时必须先增加可逆、幂等、可取消且有测试的 preparation key；禁止把业务 mutation 塞进教程 |

该结论意味着教程不会与原有业务“零耦合”，但耦合面窄、可测试、可审查。若新增步骤要求修改多个 feature controller、依赖业务查询结果或操作 modal/transient 内部控件，应先拆独立 spec，而不是继续扩张 `TourManager`。

## 17. 实施交付记录

实施状态：Completed
完成日期：2026-08-27
实现 revision：basic=1, proxy=1, station-setup=1, monitoring=未发布, advanced=未发布
已运行验证：教程 focused tests（14 files / 98 tests，包含完整体验目录组合断言）；完整 `pnpm exec vitest run`（此前基线 135 files / 602 tests）；`pnpm exec tsc --noEmit --pretty false`；教程及集成改动的 scoped ESLint；`pnpm build`（theme audit、TypeScript 和 Vite build 通过）；`pnpm verify:fast`（全部 gate 通过）；`git diff --check`（无 whitespace error）。真实窗口结果见 [`../audits/2026-08-27-guided-tour-webview-acceptance.md`](../audits/2026-08-27-guided-tour-webview-acceptance.md)。完整体验组合逻辑由 catalog 单测覆盖；其 Driver/WebView 渲染路径复用同一 Manager，场景级 WebView 证据仍适用于所有组合步骤。
未验证范围：第一版未发布 modal/transient 内部目标、并行教程或网络驱动的动态目标，因此这些 L3 能力不在本次完成声明内。
已知风险：生产构建仍有仓库既有的单 chunk 超过 500 kB warning；完整测试中的既有 `RuntimeDiagnosticsPage`/`ShellPageErrorBoundary` fixture 会输出 React `act`/预期错误日志，但 135 个测试文件全部通过。Driver.js 的定位受未来页面布局变化影响，必须继续由 anchor smoke 和真实窗口发布前抽查守护。
