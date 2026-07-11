# Relay Pool 页面切换优化设计

## 目标

让 Relay Pool Desktop 的页面切换从瞬时替换变成轻量、稳定、可降级的桌面应用过渡效果。目标手感参考 CCSwitch 的克制切换体验，同时保留类似 Cockpit 的页面保活和导航状态稳定性。

本设计只覆盖应用内页面切换层，不改业务页面的数据逻辑、表单逻辑、后端命令或路由能力。

## 成熟度判断

CCSwitch 的成熟点是视觉手感：切换轻、快、不抢注意力，适合本地桌面工具。

Cockpit 的成熟点是状态模型：shell 稳定、页面实例可复用、当前页面和子页面状态可恢复。Cockpit 使用 iframe frame 管理和 `display` 切换，这是为 Web 管理后台、多 host、多 package 隔离服务的架构，不适合照搬到 Relay Pool 的 Tauri React 单体应用。

Relay Pool 的方案应组合两者优点：使用 Cockpit 式页面保活思路，呈现 CCSwitch 式轻量过渡手感。

## 范围

纳入过渡的页面：

- 主导航页面：总览、中转站资产、密钥池、路由策略、价格 / 倍率、渠道状态、变更中心、请求日志、设置、开发者采集页。
- 内部子页面：新增中转站、编辑中转站、中转站详情、添加密钥、编辑密钥、模型基础价格。

不纳入本次设计：

- 业务数据刷新节奏。
- 页面内容重排。
- 表格、卡片、弹窗、Toast 的独立动效。
- View Transition API 或 Motion 动画库引入。
- Cockpit iframe 架构。

## 交互设计

主导航页面切换：

- AppShell、sidebar、main 滚动容器保持稳定。
- 只让内容页在 `main` 内做淡入和轻微上浮。
- 推荐参数：`opacity 0 -> 1`，`translateY(4px) -> 0`，时长 `140-180ms`，缓动 `ease-out`。
- 非活动主页面保持挂载但不可见，避免切回后丢失本地状态。

内部子页面切换：

- 从主页面进入新增、编辑、详情或模型基础价格时，内容从右侧轻微推入。
- 返回父页面时，使用反向轻推回的方向感。
- 推荐参数：`opacity 0 -> 1`，`translateX(12-20px) -> 0`，时长 `160-200ms`。
- 子页面打开时，sidebar 高亮保持在父级页面。例如中转站详情继续高亮“中转站资产”，模型基础价格继续高亮“价格 / 倍率”。

减少动态效果：

- 遵守 `prefers-reduced-motion: reduce`。
- 系统减少动画时，过渡降级为无位移、极短淡入或直接切换。

## 技术设计

实现位置优先放在 `src/app/App.tsx` 附近的页面容器层。

当前 `App.tsx` 已有两个关键基础：

- `mountedRouteIds` 会让 shell 页面首次访问后保活。
- `getShellRouteId()` 已经把内部子页面映射回父级 sidebar 路由。

建议新增一个轻量页面过渡容器：

- 主导航页面容器负责 active/inactive 的可见性、指针事件和 aria 状态。
- 子页面容器负责 transient page 的 push/return 方向。
- CSS 类集中定义在现有全局样式或靠近 App 的样式层，避免每个业务页面单独处理动画。

显示策略应避免继续使用 active 为 `contents`、inactive 为 `hidden` 的瞬时切换。可改为稳定叠放容器：

- 外层页面栈占满 `main` 可用空间。
- 每个页面层使用相同尺寸，避免切换时布局跳动。
- inactive 页面设置 `opacity: 0`、`pointer-events: none`、`visibility: hidden` 或等效策略。
- active 页面设置 `opacity: 1`、`pointer-events: auto`、`visibility: visible`。

需要注意滚动行为：

- 主页面保活意味着旧页面滚动位置可能保留，这是期望行为。
- 子页面返回父页面时，不应强制重置父页面滚动。
- 如果叠放容器改变滚动上下文，应保持 `AppShell` 的 `main` 仍是唯一主滚动容器，避免产生双滚动条。

## 数据流

页面激活仍由 `activeRouteId` 驱动。

主导航页：

- `activeRouteId` 是 shell page 时，确保 route id 在 `mountedRouteIds` 中。
- 所有已挂载 shell page 都渲染在页面栈中。
- 只有当前 shell page 标记为 active。

内部子页面：

- `renderTransientPage()` 继续根据 `activeRouteId` 渲染子页面。
- `getShellRouteId(activeRouteId)` 继续决定 sidebar 高亮。
- 页面容器根据当前 page id 是否为 transient 决定 push 动效。

## 错误与降级

- 如果 CSS 动效不可用，页面仍应正常切换。
- 如果用户系统设置减少动画，位移动效必须关闭。
- 页面加载失败、Toast、后端错误保持原页面行为，不额外包装错误状态。
- 不依赖 WebView2 的特定新 API，因此不同 Windows 环境下行为更稳定。

## 测试与验证

最低验证：

- 运行 TypeScript / Vite 可用检查。
- 启动应用或前端 dev server，手动验证主导航页面切换。
- 手动验证进入和返回内部子页面：中转站详情、编辑中转站、模型基础价格至少各一次。
- 验证 sidebar 高亮仍跟随父级页面。
- 验证主页面切回后状态不被重置。

可选自动化：

- 增加轻量测试覆盖 `getShellRouteId()` 和页面分类逻辑。
- 如果已有前端回归脚本可复用，可增加 class/state 级断言，确认 active/inactive 页面标记正确。

## 自查结果

本设计没有引入新动画库，没有依赖 View Transition API，没有复制 Cockpit iframe 架构。范围集中在页面容器和样式层，符合当前本地桌面工具的轻量 UI 方向。
