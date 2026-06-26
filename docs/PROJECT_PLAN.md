\# Relay Pool Desktop 项目规划



\## 1. 项目定位



Relay Pool Desktop 是一个本地桌面端 AI 中转池管理工具。



它不是网站，不是 SaaS，不是中转站后台，也不是 CCSwitch 的升级版或替代品。它和 CCSwitch 的关系是配合关系：



\* CCSwitch 负责管理本机 AI 工具配置；

\* Relay Pool Desktop 负责管理多个真实中转站，并对外暴露一个固定的本地 API 入口；

\* Codex、Claude Code、Gemini CLI、CCSwitch 等工具只需要连接 Relay Pool Desktop 的本地入口；

\* 背后的中转站选择、余额监控、倍率采集、低价路由、失败切换由 Relay Pool Desktop 完成。



一句话定义：



> Relay Pool Desktop 是一个本地 AI 中转池调度器：对外提供固定 OpenAI-compatible 入口，对内管理多个 Sub2API / NewAPI / OpenAI-compatible 中转站，自动采集余额和倍率，换算真实人民币价格，并根据手动优先级、最低价和健康状态进行本地路由。



\---



\## 2. 核心痛点



\### 2.1 多个中转站切换麻烦



用户可能收藏多个中转站，但直接在 CCSwitch 或 Codex 里切换 provider 往往需要重新加载配置，甚至重启工具。



本项目要解决：



\* 外部工具只配置一次本地入口；

\* 中转站切换在本地工具内部完成；

\* 拖动排序、禁用站点、最低价切换后，下一次请求立即生效。



\### 2.2 中转站余额分散



用户可能有多个中转站账号，不同站点余额单位不同、兑换比例不同，难以统一查看。



本项目要解决：



\* 统一展示各站余额；

\* 支持充值兑换比例配置，例如 `1 元 = 1 credit` 或 `1 元 = 10 credits`；

\* 支持低余额提醒；

\* 支持余额采集失败提醒。



\### 2.3 倍率变化频繁



Sub2API / NewAPI / 魔改站的模型倍率、分组倍率可能经常变化，手动维护不可靠。



本项目要解决：



\* 自动采集倍率；

\* 保存倍率快照；

\* 显示采集时间和采集来源；

\* 倍率变化后自动重新计算实际价格。



\### 2.4 真实价格难以判断



不同站点可能有模型倍率、分组倍率、充值兑换比例、补全倍率等多种计价因素。用户真正关心的不是倍率本身，而是实际人民币成本。



本项目要解决：



\* 将不同站点统一换算为 `¥ / 1M input tokens` 和 `¥ / 1M output tokens`；

\* 在价格表中直观显示每个模型在哪个站点最便宜；

\* 支持按最低实际价格路由。



\### 2.5 中转站健康状态不可见



中转站可能出现 key 失效、余额不足、模型不存在、限流、上游超时、流式断开等问题。



本项目要解决：



\* 支持低 token 健康检测；

\* 支持错误分类；

\* 支持失败熔断；

\* 支持请求失败后自动 fallback；

\* 请求日志中记录实际走了哪个站点、失败原因和 fallback 过程。



\---



\## 3. 第一版目标



第一版目标是做出一个真实可日常使用的本地工具，而不是完整平台。



第一版必须做到：



1\. 本地桌面 App；

2\. 本地 OpenAI-compatible 代理入口；

3\. 添加多个中转站；

4\. 优先支持 Sub2API / Sub2API 魔改站采集；

5\. 支持中转站拖拽排序；

6\. 支持手动排序优先路由；

7\. 支持最低价优先路由；

8\. 支持失败自动切换；

9\. 支持余额监控；

10\. 支持低 token 健康检测；

11\. 支持请求日志；

12\. 支持一键复制 CCSwitch provider 配置。



第一版可以做但不强求：



1\. NewAPI `/api/pricing` 和 `/api/ratio\_config` 标准适配；

2\. 简单图表；

3\. 系统托盘菜单；

4\. 开机自启；

5\. 价格历史。



第一版明确不做：



1\. 网站；

2\. SaaS；

3\. 多用户系统；

4\. 支付系统；

5\. 云同步；

6\. 团队权限；

7\. 插件市场；

8\. 完整替代 CCSwitch；

9\. 所有魔改站的全自动适配；

10\. 复杂规则引擎。



\---



\## 4. 技术栈



\### 4.1 桌面端



\* Tauri 2

\* React

\* TypeScript

\* Vite

\* Tailwind CSS



\### 4.2 UI 组件



\* shadcn/ui

\* Radix UI

\* dnd-kit

\* TanStack Table

\* TanStack Query

\* TanStack Virtual

\* React Hook Form

\* Zod

\* Lucide React

\* LobeHub Icons

\* Sonner

\* Recharts



\### 4.3 本地数据



\* SQLite

\* 本地配置文件

\* 本地 key 加密存储



\### 4.4 后端服务



\* Rust / Tauri commands

\* 本地 HTTP proxy

\* SQLite DAO

\* 采集器服务

\* 路由服务

\* 健康检测服务

\* 日志服务



\---



\## 5. 核心模块



\### 5.1 App Shell



负责桌面应用外壳：



\* 左侧导航；

\* 顶部状态栏；

\* 主内容区域；

\* 系统托盘；

\* 全局主题；

\* 全局通知。



页面结构：



\* 总览；

\* 中转池；

\* Sub2API 采集；

\* 价格表；

\* 路由规则；

\* 请求日志；

\* 设置。



\### 5.2 Local Proxy



本地代理模块，对外暴露固定入口：



```txt

http://127.0.0.1:<port>/v1

sk-local-pool-xxxx

```



第一版至少支持：



```txt

GET  /v1/models

POST /v1/chat/completions

POST /v1/responses

```



职责：



\* 校验本地 key；

\* 解析请求模型；

\* 调用路由器选择上游；

\* 转发请求；

\* 支持流式转发；

\* 失败时 fallback；

\* 记录请求日志。



\### 5.3 Station Manager



中转站管理模块。



每个站点包含：



```ts

type Station = {

&#x20; id: string

&#x20; name: string

&#x20; type: 'sub2api' | 'newapi' | 'openai-compatible' | 'custom'

&#x20; baseUrl: string

&#x20; apiKey: string

&#x20; enabled: boolean

&#x20; priority: number

&#x20; creditPerCny: number

&#x20; balanceRaw?: number

&#x20; balanceCny?: number

&#x20; lowBalanceThresholdCny?: number

&#x20; lastCheckedAt?: number

&#x20; lastPricingFetchedAt?: number

&#x20; note?: string

}

```



职责：



\* 新增站点；

\* 编辑站点；

\* 删除站点；

\* 启用 / 禁用站点；

\* 拖拽排序；

\* 测试连接；

\* 刷新余额；

\* 刷新倍率。



\### 5.4 Sub2API Collector



第一版优先实现的采集器。



采集策略：



1\. 识别站点是否像 Sub2API；

2\. 尝试常见前端接口；

3\. 支持内置 WebView 登录；

4\. 捕获登录后的 XHR / fetch JSON；

5\. 从返回数据中识别余额、key、group、rate\_multiplier；

6\. 解析页面作为兜底；

7\. 允许用户手动修正字段。



重点字段：



```txt

balance

quota

credit

amount

group

group\_id

group\_name

rate\_multiplier

ratio

multiplier

token

key

api\_key

usage

```



输出统一快照：



```ts

type PricingSnapshot = {

&#x20; stationId: string

&#x20; source: 'api' | 'frontend-api' | 'webview-capture' | 'html' | 'manual'

&#x20; fetchedAt: number

&#x20; groups: PricingGroup\[]

&#x20; models: PricingModel\[]

&#x20; balance?: BalanceSnapshot

&#x20; raw?: unknown

}

```



\### 5.5 NewAPI Collector



NewAPI 适配器第一版可以放在 Sub2API 后面。



优先尝试：



```txt

GET /api/pricing

GET /api/ratio\_config

GET /api/models

```



采集内容：



\* 模型列表；

\* 模型倍率；

\* 补全倍率；

\* 模型价格；

\* 分组倍率；

\* 可用分组；

\* 支持 endpoint。



\### 5.6 Price Normalizer



价格归一化模块。



目标是把各种原始倍率统一换算成：



```txt

¥ / 1M input tokens

¥ / 1M output tokens

```



基础公式：



```txt

effective\_input\_cny\_per\_1m =

&#x20; base\_input\_usd\_per\_1m

&#x20; × model\_input\_ratio

&#x20; × group\_ratio

&#x20; ÷ credit\_per\_cny



effective\_output\_cny\_per\_1m =

&#x20; base\_output\_usd\_per\_1m

&#x20; × model\_output\_ratio

&#x20; × group\_ratio

&#x20; ÷ credit\_per\_cny

```



第一版可以允许用户手动配置模型基础价格，后续再内置官方价格表。



\### 5.7 Router



本地路由模块。



第一版支持三种策略：



1\. 手动排序优先；

2\. 最低价优先；

3\. 失败自动切换。



路由流程：



```txt

收到请求

&#x20; ↓

解析模型名

&#x20; ↓

映射 canonical model

&#x20; ↓

找到支持该模型的站点

&#x20; ↓

过滤禁用、余额不足、熔断中的站点

&#x20; ↓

按策略排序

&#x20; ↓

选择第一个候选站点

&#x20; ↓

转发请求

&#x20; ↓

失败则分类错误并 fallback

&#x20; ↓

记录日志

```



\### 5.8 Health Checker



低 token 健康检测模块。



检测类型：



\* 免费检测：`/v1/models`、余额接口、key 状态接口；

\* 低 token 检测：`max\_tokens: 1` 或 `max\_output\_tokens: 1`；

\* 请求失败后被动检测。



错误分类：



```txt

401 invalid\_api\_key        key 失效，硬失败

403 forbidden              权限不足或模型不可用

402 insufficient\_quota     余额不足

404 model\_not\_found        模型不存在

429 rate\_limit             限流，短暂冷却，不判死

5xx                        上游异常，进入熔断

timeout                    超时，进入熔断

stream disconnected        流式稳定性问题，降低稳定性评分

```



\### 5.9 Request Log



请求日志模块。



记录：



```ts

type RequestLog = {

&#x20; id: string

&#x20; createdAt: number

&#x20; model: string

&#x20; canonicalModel?: string

&#x20; upstreamModel?: string

&#x20; stationId?: string

&#x20; strategy: 'manual' | 'cheapest' | 'stable'

&#x20; status: 'success' | 'failed' | 'fallback'

&#x20; latencyMs?: number

&#x20; inputTokens?: number

&#x20; outputTokens?: number

&#x20; estimatedCostCny?: number

&#x20; errorCode?: string

&#x20; errorMessage?: string

&#x20; fallbackTrace?: FallbackStep\[]

}

```



\---



\## 6. UI 设计方向



UI 风格参考 CCSwitch：



\* 本地工具感；

\* 默认浅色主题；

\* 参考 CCSwitch 的白色简约桌面工具风格；

\* 左侧导航；

\* 右侧详情；

\* 紧凑表格；

\* 高信息密度；

\* 拖拽排序；

\* 状态徽标；

\* 浅灰窗口背景，例如 `#f6f7f9`；

\* 白色或近白色卡片和面板；

\* 浅灰细边框；

\* 主色只保留一种，建议蓝色或蓝紫色；

\* 状态色克制：绿色代表正常，黄色代表警告，红色代表错误，灰色代表禁用；

\* 控件紧凑，不做大圆角、大阴影、大色块；

\* 不做纯白刺眼大面积背景；

\* 不做网站后台风；

\* 不做企业后台模板风；

\* 不做营销页风格。



深色主题后续预留为可选项，不作为第一版重点。



\### 6.1 页面



\#### 总览



展示：



\* 本地代理状态；

\* Base URL；

\* Local Key；

\* 当前路由策略；

\* 可用站点数量；

\* 余额告警；

\* 今日请求数；

\* 今日估算花费；

\* 最近请求；

\* 价格变化。



\#### 中转池



左侧站点拖拽列表，右侧站点详情。



站点卡片显示：



\* 名称；

\* 类型；

\* 余额；

\* 状态；

\* 延迟；

\* 上次采集时间；

\* 是否启用。



详情分区：



\* 基础信息；

\* 连接配置；

\* 余额信息；

\* 采集状态；

\* 模型与倍率；

\* 健康检测；

\* 最近错误。



\#### Sub2API 采集



显示：



\* 登录状态；

\* 捕获到的接口；

\* 识别出的余额字段；

\* 识别出的分组字段；

\* 识别出的倍率字段；

\* 最近采集快照；

\* 手动修正入口。



\#### 价格表



表格字段：



\* 模型；

\* 推荐站点；

\* 输入价格；

\* 输出价格；

\* 可用站点数；

\* 更新时间；

\* 价格变化。



点开模型后展示各站价格对比。



\#### 路由规则



第一版只做简单规则：



\* 默认策略；

\* 失败自动切换；

\* 余额阈值；

\* 熔断时间；

\* 模型固定路由。



\#### 请求日志



表格字段：



\* 时间；

\* 模型；

\* 站点；

\* 状态；

\* 耗时；

\* token；

\* 估算花费；

\* 错误。



详情展示：



\* 候选站点排序；

\* 最终选择站点；

\* fallback 过程；

\* 上游错误；

\* 脱敏请求信息。



\#### 设置



包含：



\* 本地代理端口；

\* Local Key；

\* 数据目录；

\* 采集频率；

\* 低余额阈值；

\* 外观主题；

\* 导入 / 导出配置；

\* 开机自启；

\* 托盘行为。



\---



\## 7. 推荐项目结构



```txt

relay-pool-desktop/

&#x20; docs/

&#x20;   PROJECT\_PLAN.md

&#x20;   ARCHITECTURE.md

&#x20;   COLLECTORS.md

&#x20;   ROUTING.md

&#x20;   UI\_GUIDE.md



&#x20; src/

&#x20;   app/

&#x20;     App.tsx

&#x20;     routes.tsx



&#x20;   components/

&#x20;     shell/

&#x20;     station/

&#x20;     pricing/

&#x20;     routing/

&#x20;     logs/

&#x20;     settings/

&#x20;     ui/



&#x20;   features/

&#x20;     dashboard/

&#x20;     stations/

&#x20;     collectors/

&#x20;     pricing/

&#x20;     routing/

&#x20;     logs/

&#x20;     settings/



&#x20;   lib/

&#x20;     api/

&#x20;     hooks/

&#x20;     utils/

&#x20;     types/



&#x20; src-tauri/

&#x20;   src/

&#x20;     main.rs

&#x20;     commands/

&#x20;     db/

&#x20;     services/

&#x20;       proxy/

&#x20;       stations/

&#x20;       collectors/

&#x20;       pricing/

&#x20;       routing/

&#x20;       health/

&#x20;       logs/

&#x20;     models/

&#x20;     utils/



&#x20; package.json

&#x20; pnpm-lock.yaml

&#x20; tsconfig.json

&#x20; vite.config.ts

&#x20; tailwind.config.ts

&#x20; README.md

&#x20; AGENTS.md

```



\---



\## 8. 开发阶段



\### Phase 0：项目骨架



目标：



\* 创建 Tauri + React + TypeScript 项目；

\* 接入 Tailwind；

\* 接入 shadcn/ui；

\* 建立 AppShell；

\* 建立文档；

\* 建立 AGENTS.md；

\* 建立基础 lint / format。



验收：



\* App 能启动；

\* 有左侧导航和空页面；

\* 代码结构清晰；

\* 文档存在。



\### Phase 1：假数据 UI



目标：



\* 总览页；

\* 中转池页；

\* Sub2API 采集页；

\* 价格表页；

\* 路由规则页；

\* 请求日志页；

\* 设置页；

\* 拖拽排序假交互。



验收：



\* UI 结构完整；

\* 假数据能展示；

\* 拖拽排序可用；

\* 不接真实后端。



\### Phase 2：本地数据库



目标：



\* SQLite 初始化；

\* stations 表；

\* pricing\_snapshots 表；

\* request\_logs 表；

\* health\_checks 表；

\* settings 表；

\* Tauri commands CRUD。



验收：



\* 站点可新增、编辑、删除；

\* 拖拽排序可持久化；

\* 设置可保存。



\### Phase 3：Sub2API 采集原型



目标：



\* 添加 Sub2API 站点；

\* 内置登录窗口；

\* 捕获前端接口；

\* 识别余额 / group / rate\_multiplier；

\* 保存采集快照；

\* 页面展示采集结果。



验收：



\* 至少能对一个真实 Sub2API 站点完成登录态采集；

\* 采集失败时有明确错误；

\* 支持手动修正字段。



\### Phase 4：本地代理



目标：



\* 启动本地 OpenAI-compatible proxy；

\* 支持 `/v1/models`；

\* 支持 `/v1/chat/completions`；

\* 支持 `/v1/responses`；

\* 支持流式转发；

\* 请求日志落库。



验收：



\* CCSwitch / Codex 能连接本地入口；

\* 单个上游站点能正常请求。



\### Phase 5：多站路由



目标：



\* 手动排序优先；

\* 失败自动 fallback；

\* 错误分类；

\* 熔断；

\* 请求日志显示 fallback trace。



验收：



\* A 站失败后能自动切 B 站；

\* 日志能说明失败原因。



\### Phase 6：低价路由



目标：



\* 价格归一化；

\* 价格表；

\* 最低价优先策略；

\* 余额不足过滤。



验收：



\* 软件能显示每个模型各站真实人民币价格；

\* 最低价路由能按最新价格选择站点。



\### Phase 7：NewAPI 适配



目标：



\* `/api/pricing`；

\* `/api/ratio\_config`；

\* `/api/models`；

\* 统一价格快照。



验收：



\* NewAPI 站点能自动采集模型、分组和倍率。



\---



\## 9. Codex 开发工作流



本项目开发以 Codex 为主。所有任务都应尽量拆成明确、可验证的小任务。



\### 9.1 每次让 Codex 做什么



每次指令只要求完成一个阶段或一个模块，不要一次塞太多。



推荐格式：



```txt

你现在只做 X。

开始前先阅读 AGENTS.md、docs/PROJECT\_PLAN.md 和相关文件。

不要改无关文件。

不要引入和任务无关的大改。

完成后运行可用的检查命令。

最后说明修改了哪些文件、如何验证、还有哪些未完成。

```



\### 9.2 禁止事项



\* 不要随意重构无关代码；

\* 不要引入大而全模板；

\* 不要把它做成网站；

\* 不要加入账号系统；

\* 不要加入支付系统；

\* 不要用 `git add .`；

\* 不要提交用户本地配置、key、日志；

\* 不要在日志里打印完整 API key；

\* 不要直接复制 AGPL / LGPL 项目的核心代码。



\### 9.3 UI 修改要求



UI 修改应先保持整体风格一致：



\* 默认浅色简约工具风格；

\* 参考 CCSwitch 的白色桌面工具感；

\* 浅灰窗口背景；

\* 白色或近白色卡片和面板；

\* 细边框；

\* 控件紧凑；

\* 表格信息密度高；

\* 状态颜色克制；

\* 不默认做深色主题；

\* 不做 VS Code 暗色风；

\* 不做营销页风格；

\* 不做传统企业后台风格。



\---



\## 10. 许可证与参考项目边界



参考项目：



\* CCSwitch：可参考本地桌面架构、UI 结构、拖拽交互、本地 proxy 思路；

\* Sub2API：可参考账号池、健康检测、倍率字段、调度思路；

\* NewAPI：可参考公开 pricing API、ratio config、分组倍率体系；

\* CLIProxyAPI：可参考本地 proxy、管理 API、日志与配置结构。



注意：



\* CCSwitch 是 MIT，通用 UI / 架构可在遵守许可证前提下参考；

\* Sub2API / NewAPI 许可证更严格，不直接复制核心实现；

\* 优先自己实现 Collector / Router / Health Checker；

\* 必要时在文档中保留参考项目 attribution。



\---



\## 11. 第一版验收标准



第一版完成后，用户应能完成以下流程：



1\. 打开 Relay Pool Desktop；

2\. 添加一个或多个 Sub2API 中转站；

3\. 登录站点并采集余额、分组和倍率；

4\. 软件显示各站余额；

5\. 软件显示模型真实人民币价格；

6\. 拖动站点改变手动优先级；

7\. 启动本地代理；

8\. 复制 CCSwitch provider 配置；

9\. CCSwitch / Codex 连接本地入口；

10\. 请求进入后自动选择站点；

11\. 某站失败后自动 fallback；

12\. 请求日志显示实际站点、耗时、估算花费和失败原因。



做到这一步，项目即可进入日常试用阶段。



