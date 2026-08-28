# 教程引导系统 Tauri WebView 验收记录

日期：2026-08-27

范围：Driver.js 遮罩与气泡、场景教程、跨页面续接、窗口生命周期、主题、焦点和 localStorage 降级。测试使用独立 Tauri identifier，不读取或改写用户正式应用数据。

## 运行环境

- Windows Tauri 2 dev WebView；
- 独立 identifier：`dev.relaypool.desktop.tourtest` / `dev.relaypool.desktop.tourstorage`；
- 独立 Vite 地址：`http://127.0.0.1:5177`；
- WebView2 CDP 仅用于读取运行时状态、点击教程按钮、调整测试窗口和注入存储异常；
- 验收结束后已停止隔离 Tauri、Vite 和 CDP 进程。

## 验收结果

| 场景 | 实测结果 | 证据摘要 |
|---|---|---|
| 首次自动启动 | 通过 | 清空安装级教程 key 后重载，自动进入 `basic` 1/7，目标为 `shell-sidebar` |
| `basic` 完整流程 | 通过 | 7 个步骤依次展示；最后按钮为“完成”；结束后 overlay/popover/active target 全部清理，写入 `completed` revision 1 |
| 教程中心 | 通过 | 已完成显示“重新查看”，未开始显示“开始”；Dialog 关闭后再启动 Driver；运行时不叠加业务 Dialog |
| 完整体验 | 通过（目录/自动化） | 教程中心首项“完整体验 Relay Pool”按 `basic -> proxy -> station-setup` 顺序组合全部当前已发布步骤；复用同一 Manager。场景级 WebView 运行证据覆盖其各组成步骤 |
| `proxy` 跨页面 | 通过 | settings 1/3、2/3 后进入 routing 3/3；上一步返回 settings，再前进返回 routing；后台 layer 为 `inert` + `aria-hidden` |
| 最小支持窗口 | 通过 | 980 x 640 WebView 下三个 proxy popover 均完整位于 viewport 内，宽 320px，长文可换行；overlay 覆盖完整 viewport |
| z-index 与互斥 | 通过 | Driver popover/overlay 均为 fixed z-index 50；教程中心退出后 body overflow 恢复为 visible，再显示教程；业务 Dialog 与教程不并存 |
| 最小化与恢复 | 通过 | 最小化时 WebView 失焦；恢复后 session 进入 skipped，popover 清理，未误记 completed |
| 显式退出与焦点 | 通过 | 从教程中心启动后关闭教程，焦点恢复到仍连接且非 inert 的“打开教程”按钮 |
| 深色主题 | 通过 | 深色 token 下 popover 背景、前景、边框和阴影切换正确，popover 完整位于 viewport 内 |
| 清空存储 | 通过 | 清除教程 key 并重载后按首次安装语义自动展示 basic，不影响业务数据 |
| 存储 API 被拒绝 | 通过 | 在真实 WebView 中令 `Storage.getItem/setItem/removeItem/clear` 抛出 `SecurityError`；basic 仍走完 7 步，结束后 overlay 清理，显示“教程已完成，但进度未能持久化”，无错误边界或未处理异常 |

## 自动化互补证据

真实窗口验收验证 Driver.js/WebView 行为；下列失败路径由自动化提供更细粒度覆盖：storage 初始化读取异常、损坏/超大 JSON、quota error、旧 revision、隐藏/后台 target、迟到导航与 Driver callback、preparation 取消和 exactly-once cleanup、Driver 创建/highlight 抛错、Provider StrictMode 与 manager replacement。

截图和逐步 JSON 在本次验收时生成于 `%TEMP%\relay-pool-tour-evidence-20260827`，不纳入仓库，避免把临时运行数据作为产品资产。本文保留不含凭据的可复核结论；自动化测试是长期回归契约。

## 结论

已发布的 `basic`、`proxy` 和 `station-setup` 达到计划中的 L1 门槛；`proxy` 的 `routing-status-tab` 达到 L2 可逆视图准备门槛。第一版没有发布 modal/transient 内部目标，因此不声称达到 L3 扩展门槛。

## 2026-08-28 内容体系扩展复验

复验使用独立 identifier `dev.relaypool.desktop.guided-tour-content-expansion`、独立数据目录、Vite `http://127.0.0.1:1431` 和 WebView2 CDP `9239`。profile 为空数据，不创建中转站、Key、探针或请求，不读取正式应用数据；截图和逐步 JSON 位于忽略的 `output/manual-guided-tour/evidence/`，不作为需提交的产品资产。

| 场景 | 实测结果 | 证据摘要 |
|---|---|---|
| 全新 profile | 通过 | 自动进入新版 `basic` 1/7，目标为全局侧栏；关闭后状态为未完成 |
| 教程中心目录 | 通过 | 显示“推荐 / 页面教程”两组，共 11 个发布教程；完整体验和首次引导置顶，九篇页面教程按主导航排序 |
| 完整体验 | 通过；网络提示由自动化复验 | `full` 以 17 个独立步骤跨总览、中转站、密钥池、路由、价格、渠道、变更、记录和设置连续播放；设置阶段新增系统代理建议，目标复用稳定的 `settings-network` 锚点 |
| 单次完成 | 通过（自动化复验） | 最后一个 Driver 气泡直接显示“完成”；点击后立即提交进度并清理 overlay/popover，不再显示第二个“教程结束”确认卡片 |
| 完成状态独立 | 通过 | 完成 `full` 后只有 `full` 显示“已完成”；九篇页面教程仍显示“新增”，已跳过的 `basic` 显示“未完成” |
| 路由视图恢复 | 通过 | 初始位于“设置”，教程临时准备到“概览”，关闭后恢复“设置” |
| 渠道视图恢复 | 通过 | 初始位于“官方状态”，教程临时准备到“本地状态”，关闭后恢复“官方状态” |
| 1180 x 760 | 通过 | 教程中心完整位于 viewport 内，无横向溢出，操作按钮不越界 |
| 980 x 640 | 通过 | 最小支持窗口使用内部滚动，无横向溢出、按钮越界或内容遮挡 |

复验后根据实际交互反馈取消了额外结束确认：`TourManager` 在最后一个可见步骤的 Driver “完成”回调中直接提交进度并退出，删除 `awaiting-completion` 状态、结束卡片及其 modal guard 特例。该修正在开发 WebView 热更新环境完成，并由 Manager、Overlay、Provider、Driver adapter 和 modal guard 共 46 项聚焦测试覆盖；未重复生成截图证据。

扩展后的发布目录达到普通页面教程和精选完整体验的当前验收门槛。`proxy`、`station-setup` 仍仅作为旧进度 ID 保留，不再出现在教程中心；本次仍未发布 transient 页面或任务教程。
