# 变更中心告警闭环升级实施计划

状态：Draft；仅用于规格评审后的实施拆分，不是当前实现基线，不得在批准前据此开始代码改造

日期：2026-08-08

批准前置：[`../../proposals/CHANGE_CENTER_ALERTING_UPGRADE_SPEC.md`](../proposals/CHANGE_CENTER_ALERTING_UPGRADE_SPEC.md)

适用范围：变更中心、总览风险摘要、侧栏徽标、Collector/健康/余额/价格/绑定/路由的告警投影、提醒与告警设置、应用内提醒、桌面通知、旧 `change_events` 栈的迁移与删除。

本计划实施后，将以 `occurrence -> incident -> attention/policy -> delivery -> read model` 取代旧 `change_events.status` 复合模型。它不增加账号、云同步、邮件、Webhook、脚本执行或公共状态页。

> 每个任务使用 RED-GREEN-REFACTOR：先证明缺失能力或旧反模式，再实现最小完整路径，最后真实运行 task gate。任何命令未以退出码 0 完成，则对应任务未完成。

> 当前工作区可能已经存在未提交的告警领域、迁移、IPC 或设置页面试作代码。它们一律视为“待归类、待验证的实施候选”，不构成任何任务的通过证据；执行时必须先在 Task 0 登记来源、所属任务、测试证据和保留/重写/删除结论，不得通过保留现状来跳过 RED-GREEN-REFACTOR 或 cutover 门禁。

---

## 0A. 当前工作区执行基线与闸门映射

本节只记录本计划进入执行后的事实，不改变 Spec 和本计划的 Draft/待批准状态，也不把“代码已存在”当作任务验收证据。执行者在继续工作前必须以此表校准任务状态；若实际命令结果与表中事实不同，应先更新审计记录，再继续推进。

| 范围 | 当前事实 | 可继续做什么 | 明确禁止 |
|---|---|---|---|
| Task 0-11 | 新告警领域、迁移、升级、投影、策略、投递、IPC、UI 和设置实现已在工作区形成候选实现；已有局部测试和契约证据，但仍需按各 Task 的 RED/GREEN/REFACTOR 与 Exit gate 复核 | 修复测试、死代码、边界和文档；补齐缺失证据 | 以局部通过替代 Task gate；宣称已发布 |
| Task 12 | `change-center-alerting-cutover-manifest.json` 已记录生产 cutover；旧生产 writer、reader、IPC、binding 和前端 view model 已清除；允许命中仅限历史迁移/导入/升级/fixture 路径 | 完成格式、dead-code、全量验证和 cutover 证据收敛 | 回接旧 writer、旧 IPC、旧 query/cache 或双写；提前删除历史表 |
| Task 13 | 观察期、零 legacy-reader 调用证据、备份/恢复和独立 destructive migration 尚未完成 | 设计观察指标、备份恢复演练和删除前置检查 | 删除 `change_events` 表/索引；删除升级 adapter 或兼容 catalog |
| Task 14 | qualification、acceptance matrix、性能基准、Windows smoke 和发布文档尚未完成 | 建立逐项验收矩阵并执行专项检查 | 将 Draft 改为已实施/发布；用计划或手工检查替代命令证据 |

当前已知开放风险必须始终显示在交付说明中：`verify:fast` 仍可能被 Rust dead-code policy 阻断；Windows 原生通知点击回调只能提供 best-effort deep link，不能承诺可靠点击跳转。只有对应证据补齐后，才能关闭风险并推进 Task 14。


## 0. 执行总览与里程碑

本节把 Spec 的产品目标转换为可执行的工程批次。任务编号、职责边界和验收标准以本计划后续 Task 0-14 为准；本节只规定实施顺序、并行边界、交付物和停止条件，不能替代任何 Task 的测试门禁。

### 0.1 实施批次

| 批次 | 任务 | 目标 | 关键交付物 | 通过后才允许 |
|---|---|---|---|---|
| A. 冻结与盘点 | 0-1 | 冻结产品决策，建立领域类型、事件注册表和旧栈删除账本 | baseline、deletion ledger、boundary manifest、domain contract | 进入 schema 和升级实现 |
| B. 数据底座 | 2-3 | 建立新表、约束、持久化 store、历史回填和当前事实重建 | append-only migration、postcondition、schema15 fixture、upgrade journal | 打开新 writer gate |
| C. 运行时闭环 | 4-7 | 完成投影、恢复、策略、调度、投递、保留和 worker 生命周期 | ingress、projector、policy resolver、delivery ledger、retention evidence | 接入产品消费层 |
| D. 产品消费 | 8-10 | 完成 cursor read model、IPC、绑定、三视图、总览侧栏和设置入口 | DTO/ACL/binding、Change Center、Settings workspace、UI contract | 开始桌面通知和 cutover 准备 |
| E. 平台能力 | 11 | 完成原生通知、权限、降级、crash recovery 和 deep link | Tauri adapter、permission matrix、Windows smoke | 进入原子生产切换 |
| F. 原子切换 | 12 | 同一 revision 切换所有 producer/consumer 并删除旧运行时入口 | cutover manifest、legacy zero-reader evidence、删除后的架构检查 | 开始观察期；禁止回接旧栈 |
| G. 清理与交付 | 13-14 | 观察期、备份恢复、删除旧表、资格验证、发布文档和审计闭环 | destructive migration、qualification、acceptance matrix、release notes | 才能宣称升级完成 |

### 0.2 推荐执行顺序

1. 先完成 Task 0 的产品决策冻结和 legacy ownership map；任何未决默认值都必须记录为 `blocked`，不能由代码隐式决定。
2. 完成 Task 1 后冻结领域类型和 event registry；Task 2 的 SQL、DTO 和测试只能消费这些类型，不能重新定义状态字符串。
3. 完成 Task 2 的 migration/postcondition 后执行 Task 3。Task 3 在历史回填、高水位覆盖、current-facts rebuild 和 typed recovery 全部通过前，不得打开生产 writer。
4. Task 4、Task 6 的纯 resolver、Task 7 的纯 planner 可以并行准备；验收必须按 Task 4 -> Task 6 -> Task 7 串行确认，避免策略和投递绕过 incident owner。
5. Task 8 完成 DTO/ACL/binding 后，Task 9 与 Task 10 可并行开发；两者必须使用同一版生成绑定和同一套 query key，禁止各自维护兼容类型。
6. Task 11 只允许在 delivery ledger 和 read model 稳定后接入；OS 通知不可成为事实存储，也不能替代应用内提醒。
7. Task 12 是唯一生产切换点：后端 producer、IPC registry/ACL、generated bindings、所有 UI consumer、query/cache key 和旧运行时删除必须处于同一可交付 revision。
8. Task 13 必须等待观察期、零 legacy reader、备份恢复证据和 schema15 -> latest recovery 通过；旧表删除采用独立 append-only migration，不能与 Task 12 混入。
9. Task 14 最后执行全量资格验证；任何未解释失败、未生成文件、未清零 allowlist 或未关闭风险项都会阻止文档状态更新。

### 0.3 可并行工作流与合并边界

允许并行的工作流只有三条：

- 领域与架构：Task 1，负责 models、registry、架构检查和领域测试。
- 数据与运行时：Task 2-7，负责 migration、upgrade、store、projector、policy、delivery 和 worker。
- 产品消费：Task 8-11，负责 IPC、binding、UI、设置和桌面通知。

以下文件只能由一个工作流在同一时段修改：migration/schema registry、`app_composition.rs`/`lib.rs`、IPC registry/ACL、generated binding、共享 query key、`AppShell`、`SettingsPage` 和 deletion ledger。跨边界修改前，先更新任务记录中的契约和 owner，再合并变更。

### 0.4 每个任务的固定执行模板

每个 Task 必须按以下顺序留下证据：

1. 记录 `git status --short --branch`、最近提交、最新 migration 编号和需保护的用户改动。
2. RED：提交至少一个失败测试、结构反例或可复现的旧行为，证明待解决缺陷真实存在。
3. GREEN：实现最小完整路径，执行 Task 列出的全部命令，并确认每条命令退出码为 0。
4. REFACTOR：移除重复 owner、兼容分支、临时日志和无效状态字段，重跑架构、安全和相关回归检查。
5. 形成脱敏证据包：变更文件、测试摘要、生成物状态、migration/postcondition、残余风险、回滚点和下一 Task 输入。
6. 只有 Exit gate 全部满足时才能将任务从 `in_progress` 标记为 `verified`；“代码已存在”或“局部测试通过”不算完成。

### 0.5 里程碑与停止条件

| 里程碑 | 必须满足 | 失败时动作 |
|---|---|---|
| M1 契约冻结 | Task 0-1 verified，Spec 第 28 节决策关闭，registry 完整 | 停止后续实现，补充产品决策和 fixture |
| M2 数据可恢复 | Task 2-3 verified，当前库和 schema15 fixture 可恢复，writer gate 可证明 | 保持旧产品路径，修复 migration/upgrade；不得暴露半成品 read model |
| M3 闭环可运行 | Task 4-7 verified，异常/恢复/复发、CAS、delivery 和 worker 测试通过 | 禁止接入 UI 和 OS 通知，保留测试隔离运行 |
| M4 产品可消费 | Task 8-10 verified，三视图、设置、DTO、ACL、binding 和窄窗口状态通过 | 不进入 cutover，修复 contract 或 UI，不保留兼容 alias |
| M5 可发布切换 | Task 11-12 verified，通知权限/降级/deep link 和 legacy zero-reader 通过 | 回到新栈测试环境，禁止删除旧表或回写旧栈 |
| M6 完成交付 | Task 13-14 verified，观察期、恢复证据、destructive migration、全量验证和发布文档齐全 | 延后删除/发布，记录未完成项和影响，不修改状态为完成 |

### 0.6 执行角色、交接与时间盒

本节定义执行责任，避免跨层改动无人负责或多个 owner 同时维护同一边界。具体人员可由项目负责人替换，但每个角色在一个 revision 内必须唯一。

| 角色 | 负责范围 | 必须签收的交付物 |
|---|---|---|
| 领域 owner | Task 0-1，事件注册表、状态机术语、恢复矩阵 | 评审决策、registry、domain tests、架构边界结果 |
| 数据/升级 owner | Task 2-4，migration、store、backfill、current-facts rebuild、恢复 | postcondition、schema15 evidence、upgrade journal、并发测试 |
| 运行时 owner | Task 5-7，producer ingress、policy reconcile、delivery worker、retention | producer matrix、delivery ledger、worker lifecycle、retention evidence |
| 产品消费 owner | Task 8-10，IPC、generated binding、Change Center、Settings、query/cache | DTO/ACL manifest、三视图、设置工作区、前端回归结果 |
| 平台通知 owner | Task 11，Tauri capability、权限、降级、Windows smoke、deep link | permission matrix、脱敏 payload 审计、平台 smoke 证据 |
| 发布/审计 owner | Task 12-14，cutover、观察期、destructive migration、资格与发布文档 | cutover manifest、zero-reader evidence、acceptance matrix、qualification |

执行规则：

1. 每个 Task 开始前，owner 必须在任务记录中写明当前工作区状态、最新 migration、受保护的用户改动、前置 Task 证据和预计变更文件；未满足即保持 `planned`。
2. 每个 Task 结束时，owner 交付 RED/GREEN/REFACTOR 结果、命令退出码、生成物状态、残余风险和下一 Task 输入；接收方只按证据签收，不按代码数量签收。
3. 同一时间只允许一个 owner 修改 migration 编号、runtime composition、IPC registry/ACL、generated binding、共享 query key、AppShell、Settings 路由和 deletion ledger；其余工作流通过接口记录交接。
4. 每个 Task 使用固定时间盒：准备与 RED 不超过 0.5 个工作日，GREEN 与专项测试不超过 2 个工作日，REFACTOR 与证据整理不超过 1 个工作日。若超出时间盒，不得跳过门禁，应记录阻塞原因、影响和新的拆分方案。
5. 并行任务只能并行“准备实现”，不能并行推进共享边界的验收；Task 12、Task 13、Task 14 始终由发布/审计 owner 串行推进。

### 0.7 执行前置条件与环境检查

在 Task 0 之外，不得以本机状态推断环境可用。执行者必须先确认：

- Windows PowerShell、锁定版 Node/pnpm、Rust toolchain 和 SQLite 测试依赖可用；命令统一使用 `pnpm.cmd` 与 PowerShell 语法。
- 工作区没有未登记的凭据、数据库、日志、导入导出包或测试产物；`git status --short --branch` 输出已保存到 Task 0 证据包。
- `docs/README.md`、AGENTS.md、当前 Spec、schema authoring 规范和安全规范已阅读；若规范冲突，停止并登记决策，不自行改写规范。
- 生成绑定、ACL、capability、schema manifest、fixture 和架构报告均由仓库脚本生成；禁止手工编辑生成文件作为完成证据。
- Windows 通知实机 smoke、备份恢复和 schema15 fixture 需要单独的测试数据与临时目录；测试数据必须是假站点、假 Key 和假 incident，并在任务结束后清理。

若任一前置条件不满足，Task 只能标记为 `blocked` 或继续进行不依赖该条件的准备工作；不得把缺少工具、缺少实机或命令未执行解释为通过。

## 1. 完成定义与不可违背规则

完成必须同时满足：

- 所有首期状态型问题都有权威异常、恢复、复发和新鲜度合同；恢复后当前风险计数自动下降。
- occurrence 不可变；incident 生命周期、attention、policy 和 delivery 分别有唯一 owner，互不复用状态字段。
- 任何已注册问题总能解析出 `system_default` 或用户有效策略；关闭提醒只抑制 delivery，不中止事实投影。
- 时间型触发/恢复、重复提醒、snooze、quiet hours、claim lease 和 crash-boundary retry 均持久化且可重启恢复。
- 总览、侧栏和三个变更中心视图只消费后端 cursor/read model；前端不持有完整历史并自行汇总风险。
- 新旧 producer 没有双写；原子 cutover 后，旧 `ChangeService` / `ChangeStore` / 旧 IPC 不存在运行时 fallback。
- 观察期结束且删除前置条件满足后，旧表、旧命令、旧 DTO、旧 query key、旧 view model、绑定和 fixture 全部删除。

实施期间遵守：

1. 先读取 `docs/README.md`、`docs/SCHEMA_UPGRADE_AUTHORING.md`、批准后的 Spec 与相关当前代码；带日期计划只作为记录，不能覆盖现行契约。
2. 每个任务开始前记录 `git status --short --branch`、`git log -5 --oneline`、最新 migration 编号；不覆盖用户已有改动。
3. migration 使用执行时的下一可用编号 `00NN`，不修改任何已嵌入 migration。
4. 不使用 `git add .`、`git add -A`、`git commit -a`。本计划不授权提交或推送。
5. 新后台任务必须纳入既有 lifecycle/task supervision，具有单实例、取消、抖动、上限和 shutdown 行为；禁止 detached loop。
6. secret、完整 URL、认证正文、原始错误和请求正文不得进入 occurrence、delivery、日志、DTO、fixture、通知或诊断标签。
7. Task 12 是原子生产切换：后端 producer、IPC/binding、所有 consumer UI 与旧运行时删除必须处于同一可交付 revision；此前的新模块只能由具名测试或 upgrade step 到达。

## 2. 依赖图

```text
0 评审冻结、基线与删除账本
  -> 1 领域契约、事件注册表与架构门禁
  -> 2 Schema、postcondition 与持久化类型
  -> 3 Durable upgrade：历史回填与当前事实重建
  -> 4 Alerting store、projector、状态机与幂等
  -> 5 Producer ingress 与各事实恢复闭环
  -> 6 Policy/settings resolver 与 lifecycle reconcile
  -> 7 Delivery ledger、应用内调度与 retention worker
  -> 8 Cursor read model、IPC DTO 与生成绑定
  -> 9 变更中心、总览和侧栏 UI
  -> 10 提醒与告警设置工作区
  -> 11 桌面通知 adapter 与权限路径
  -> 12 原子 production cutover 与旧栈删除
  -> 13 观察期、legacy table destructive migration 与清理
  -> 14 全量资格验证、文档与审计闭环
```

依赖关系按“可准备”和“可验收”区分：Task 1 完成后才能冻结领域类型；Task 2 必须消费 Task 1 的持久化类型，Task 3 必须在 Task 2 的 migration/postcondition 通过后执行；Task 4 可在 Task 2 通过后实现，但只有 Task 3 的 writer gating 和 current-facts rebuild 通过后才允许接入产品运行时；Task 5 依赖 Task 4；Task 6 可提前准备纯 resolver，但其 reconcile 验收依赖 Task 4 和 Task 7；Task 7 依赖 Task 4，并为 Task 8 提供 delivery/read model 合同；Task 8 完成后，Task 9、Task 10 可并行开发，但必须共享同一版 IPC/DTO；Task 11 依赖 Task 7-8；Task 12-14 严格顺序执行。

允许的并行工作流如下，任何一项都不能绕过前置硬门禁：

| 工作流 | 可并行任务 | 共享边界锁 | 合并前必须满足 |
|---|---|---|---|
| 领域契约 | Task 1 | `models/alerting`、事件注册表、架构脚本 | 领域测试、注册表完整性和依赖反向检查通过 |
| Schema/升级 | Task 2、Task 3（仅在 Task 2 migration 草案稳定后） | migration 编号、schema registry、startup upgrade composition | postcondition、schema15 fixture、可中断恢复测试通过 |
| 投影/策略/投递 | Task 4、Task 6（纯 resolver）、Task 7（纯 planner） | alerting store trait、时间/ID provider、runtime composition | 不得让半成品 writer 进入生产路径；最终按 4→6→7 顺序验收 |
| 产品消费 | Task 8、Task 9、Task 10、Task 11 | IPC registry、generated binding、共享 query/type、Settings/AppShell | DTO 版本锁定；前后端 contract test 和生成命令通过 |

以下文件只能由一个工作流在同一时间修改：migration、`app_composition.rs`/`lib.rs`、IPC registry/ACL、generated binding、共享 query key、`AppShell`、`SettingsPage` 和删除账本。并行工作流不得直接改写另一工作流的文件；需要跨边界时先更新任务记录和接口契约，再由边界 owner 合并。

### 2.1 任务状态与执行协议

每个 Task 只允许按 `planned -> in_progress -> verified` 推进；遇到外部阻塞使用 `blocked`，不得以“代码已存在”或“局部测试通过”标记完成。进入 `verified` 前必须完成：

1. RED：至少一个失败测试、结构性反例或可复现旧行为，明确证明待解决缺陷；
2. GREEN：最小完整路径实现，以及该 Task 列出的全部命令，命令退出码必须为 0；
3. REFACTOR：删除重复 owner、兼容分支、无效状态字段和临时日志，重新运行相关架构/安全检查；
4. 证据包：变更文件、测试结果、生成物状态、数据库/迁移 postcondition、残余风险和下一 Task 输入，全部脱敏；
5. 边界确认：确认没有新增旧栈调用、双写、detached worker、未登记 migration 或手工维护 generated 文件。

任务验收不等同于提交代码。未经用户明确授权，本计划不执行 stage、commit、push、建分支或创建 PR；实施者只需在任务记录中保留可复现的工作区和命令证据。

每个任务完成后必须提交一份最小证据包：变更文件清单、RED 测试或反例、GREEN 测试结果、结构性检查结果、已知残余风险和下一任务的输入。证据包只记录脱敏结果，不得包含本地数据库、日志、凭据、完整 URL 或真实对象标识。

## 3. 目标文件地图

执行时允许依据 Task 0 的证据调整文件名，但不得改变职责边界。

| 路径 | 最终职责 |
|---|---|
| `src-tauri/src/models/alerting/*` | closed enum/newtype、事件注册表输入、incident 状态机、policy/delivery 不变量；不依赖 SQLx/Tauri |
| `src-tauri/src/application/alerting/{ingress,incident_projector,policy_resolver,delivery_planner,attention_service}.rs` | 事实输入、纯评估与应用编排；不实现 UI 查询 |
| `src-tauri/src/application/queries/change_center_workspace.rs` | 当前问题摘要、cursor、详情与历史查询；不写状态 |
| `src-tauri/src/persistence/stores/alerting/*` | occurrence/incident/attention/policy/delivery/upgrade-progress SQL；不解释前端状态 |
| `src-tauri/src/application/alerting_worker.rs` | deadline、delivery、reconcile、retention 的 supervised lifecycle owner |
| `src-tauri/src/commands/alerting.rs` 与 `src-tauri/src/ipc/dto/alerting_*.rs` | 显式输入校验和脱敏 DTO；不暴露旧泛化 ChangeEvent |
| `src/lib/{api,queries,types}/alerting*` | 生成 binding 上的客户端 API、query/mutation owner 和稳定 UI types |
| `src/features/changes/*` | 当前问题、变化历史、提醒记录及详情；不做全量内存分页 |
| `src/features/settings/AlertingSettings*.tsx` | 提醒与告警设置工作区与规则编辑器 |
| `src/components/shell/AppShell.tsx` | 仅消费 incident aggregate 徽标；不批量 mark read |
| `src-tauri/src/application/changes.rs`、`persistence/stores/change_store.rs` | 旧产品栈，Task 12 删除；Task 3 的 private legacy reader 必须独立实现，不能依赖这些模块 |

## 4. Task 0：评审冻结、基线与删除账本

**Files**

- Create: `docs/audits/change-center-alerting-baseline.md`
- Create: `docs/audits/change-center-alerting-deletion-ledger.md`
- Create: `docs/audits/change-center-alerting-boundary-manifest.json`
- Modify: 已批准的 Spec 第 28 节，仅记录评审决策和 revision，不隐式修改默认值

**Steps**

- [ ] 关闭 Spec 第 28 节的产品决策：三视图、warning 默认阈值、手动解决边界、保留期、Key 规则首期范围、critical quiet-hours 行为和桌面 deep link 范围。
- [ ] 枚举当前所有 `change_events` producer、读者、IPC 命令、DTO、query key、AppShell 副作用、页面 view model、测试、生成绑定、portable import catalog 与 migration 引用。
- [ ] 为每个遗留符号登记最终 owner、替换任务、临时 allowlist、删除任务和退出证据；未登记入口不得进入后续任务。
- [ ] 记录当前变更中心行为基线：200 条窗口、unread 徽标、打开即已读、状态混用、collector failure recovery、清除记录和现有测试结果。
- [ ] 建立架构边界测试骨架：新 `models/alerting` 不得依赖 persistence/services/Tauri；legacy 模块只能由具名 adapter/backfill 引用。
- [ ] 盘点工作区已有告警试作代码：逐文件标记为“纳入对应 Task 并补测试”“重写后纳入”或“删除”；禁止以“代码已存在”替代任务退出条件。
- [ ] 固定第一版交付切片：MVP 必须包含状态型 incident、恢复/复发、应用内提醒、设置入口和三视图；桌面通知、观察期删除和 destructive migration 必须作为后续可独立验收的切片，不得在 MVP 中隐式降级。

**Run**

```powershell
git status --short --branch
git log -5 --oneline
Get-ChildItem src-tauri/src/persistence/migrations -File | Sort-Object Name | Select-Object -Last 5
rg -n "upsert_change_event|resolve_change_event|mark_change_event_read|mark_change_events_read|dismiss_change_event|clear_change_events|list_change_events|ChangeService|ChangeStore|changeEvents" src-tauri/src src scripts
pnpm.cmd test:contracts
pnpm.cmd test -- src/features/changes src/lib/api/changeEvents.test.ts
cargo test --manifest-path src-tauri/Cargo.toml --lib change -- --nocapture
```

**Exit gate:** 决策已批准；基线结果和删除账本可复现；任何既有红项已归因，且所有 legacy consumer 都有 owner 和删除任务。

## 5. Task 1：领域契约、事件注册表与架构门禁

**Files**

- Create: `src-tauri/src/models/alerting/{mod,event,incident,policy,delivery,attention}.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/tests/alerting_domain.rs`
- Create: `scripts/change-center-alerting-architecture.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`、boundary manifest

**RED / GREEN**

- [ ] RED：为每种 lifecycle transition、复发 episode、base severity、fresh/stale deadline、policy scope 排序、无匹配 fallback、delivery key 和 claim token 写表驱动测试。
- [ ] GREEN：以 closed enum/newtype 表达 event type、severity、lifecycle、scope、delivery kind/status、suppression reason、cursor；禁止让 Rust/TS 用任意字符串猜状态。
- [ ] GREEN：实现纯 event registry，固定首期事件的 condition-key 构成、异常/恢复 owner、fact freshness、敏感字段和手动解决许可。
- [ ] GREEN：实现可注入时钟的 incident reducer；无 SQL、网络、通知或 UI import。
- [ ] RED/GREEN：架构测试拒绝 domain 层依赖 SQLx、Tauri、Reqwest 或旧 `models::change_events`。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_domain -- --nocapture
node scripts/change-center-alerting-architecture.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 每个状态型事件都有异常、恢复和复发测试；领域层能在固定时钟下确定性运行；不存在“未注册也进入 active incident”的路径。

**交付物：** 事件注册表、condition-key/恢复矩阵、生命周期转移表、策略匹配优先级表、敏感字段清单和架构边界脚本。任何后续任务新增事件都必须先更新注册表和该任务的测试矩阵。

## 6. Task 2：Schema、postcondition 与持久化类型

**Files**

- Create: `src-tauri/src/persistence/migrations/00NN_change_center_alerting_foundation.sql`
- Modify: schema compatibility metadata、postcondition、`schema_registry.rs`、migration fixture manifest
- Create: `src-tauri/src/persistence/stores/alerting/{mod,occurrence,incident,attention,policy,delivery,upgrade_progress}.rs`
- Create: `src-tauri/tests/alerting_persistence.rs`

**Steps**

- [ ] 只在 append-only migration 中创建 Spec 第 14 节的表、索引、CHECK、`source_observation_key` 唯一约束、delivery sequence 复合唯一约束和 `alerting_upgrade_progress`；不得在 SQL migration 内复制历史。
- [ ] 编写 postcondition，证明所有表/索引/约束存在、compatibility metadata 正确且 schema15 -> latest 可规划。
- [ ] 为持久化类型限制长度、整数范围、JSON 脱敏校验和 cursor 排序；用参数化 SQL，不拼接筛选条件。
- [ ] 写查询计划测试，覆盖 100k occurrences/10k incidents 的 current、history、delivery 和 scheduled-delivery 查询。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_persistence -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 新 schema 可由当前和 schema15 正确到达；迁移只做结构改变；唯一约束、索引与 postcondition 都由测试证明。

## 7. Task 3：Durable upgrade、历史回填与当前事实重建

**Files**

- Create: `src-tauri/src/services/data_store/alerting_upgrade.rs`
- Modify: `startup_upgrade_plan.rs`、`startup_upgrade_executor.rs`、typed recovery DTO/UI routing、backup/journal manifests
- Create: `src-tauri/tests/alerting_upgrade_fixture.rs`
- Modify: legacy import/portable migration catalog 仅加入必需的过渡读取范围

**Steps**

- [ ] 将 `AlertingHistoryBackfill` 建模为 planner 选择的 durable transition，而非普通启动 service repair；遵循 probe -> planner -> executor -> postcondition -> typed recovery。
- [ ] 以 source high-water 和 last copied cursor 有界复制旧记录为 legacy occurrence，使用旧 ID 构造幂等 source key；不把旧 `read/dismissed/resolved` 推断成 attention/lifecycle。
- [ ] 在 backfill 完成、alerting writer 对外开放前，从余额、绑定、价格、共享健康、collector task 和 route impact 当前事实重建 incident。
- [ ] 支持中断恢复、重复运行、失败分类、已完成门禁和有界进度查询；`complete` 前阻止新 Change Center 和 writer 对外可用。
- [ ] 实现只读 private legacy reader，仅用于迁移诊断核对；它直接隶属 alerting upgrade、不得依赖 `ChangeService` / `ChangeStore`，默认关闭、不可写、不可从产品 UI/IPC 调用，并写入调用计数。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_upgrade_fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade -- --nocapture
pnpm.cmd verify:fast
```

**Exit gate:** current/latest 与 schema15/latest 均能恢复完成；高水位、去重、重建、typed failure 和 UI/writer gating 通过；没有普通启动隐式 repair。

**执行注意：** 回填期间禁止新旧 writer 双写。升级 executor 必须先完成旧记录复制和 current-facts rebuild，再一次性打开 alerting writer；失败时保持旧产品读写路径和可恢复升级状态，不把半成品 alerting 数据暴露给正常 UI。

## 8. Task 4：Store、projector、状态机与幂等写入

**Files**

- Create: `src-tauri/src/application/alerting/{condition_key,incident_projector,attention_service}.rs`
- Modify: alerting stores、persistence composition、application service composition
- Create: `src-tauri/tests/{alerting_projector,alerting_concurrency}.rs`

**Steps**

- [ ] 在单写事务内写 occurrence、读取/创建 incident、状态转换、policy snapshot、attention episode 边界和 scheduled/suppressed delivery。
- [ ] 以 `source_observation_key` 捕获重复输入：冲突返回既有结果，绝不重新计数、重新开 episode 或重新安排 delivery。
- [ ] 实现 pending/open/recovering/resolved、duration deadline、新鲜度验证、endpoint revision 隔离、复发和 severity change。
- [ ] 对同 condition key 使用 write runtime 串行化或 CAS；为并发 observation、deadline 与 policy reconcile 写竞争测试。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_projector -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_concurrency -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_persistence -- --nocapture
```

**Exit gate:** 任意 observation 至多一次投影；旧/陈旧 observation 不能打开或关闭错误 incident；事务失败不会留下事实与 incident 漂移。

## 9. Task 5：Producer ingress 与恢复闭环

**Files**

- Create: `src-tauri/src/application/alerting/ingress.rs`
- Modify: `application/collectors.rs`、balance/pricing/group-binding/health/routing 的当前事实 owner 与 composition
- Create: `src-tauri/tests/alerting_producer_contracts.rs`
- Modify: deletion ledger 和 producer architecture test

**Steps**

- [ ] 为 Collector failure、group missing、key group unresolved、balance low/depleted、price expired、key invalid、station down、route impacted 接入统一 observation ingress。
- [ ] 每种 producer 在同一事务写权威事实与 observation；恢复只能由 Spec 指定的当前事实 owner 产生。
- [ ] 一次性 rate/price/model 变化只追加 audit occurrence；如配置提醒，使用 delivery planner，但绝不创建 active incident。
- [ ] 从生产路径删除 `upsert_change_event` 与 `resolve_by_dedupe_key`；此时临时 allowlist 仅保留 Task 3 adapter/backfill。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_producer_contracts -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib collectors -- --nocapture
node scripts/change-center-alerting-architecture.test.mjs
pnpm.cmd verify:fast
```

**Exit gate:** 恢复矩阵全部通过；页面读取、标记已读和刷新不产生恢复；生产 module graph 无旧写路径。

## 10. Task 6：Policy、settings 与 lifecycle reconcile

**Files**

- Create: `src-tauri/src/application/alerting/{policy_resolver,policy_service,reconcile}.rs`
- Modify: alert policy store、settings application model/owner
- Create: `src-tauri/tests/{alerting_policy,alerting_reconcile}.rs`

**Steps**

- [ ] 实现 scope grammar、Station/Key 归属校验、orphaned/disabled/tombstone、稳定排序和 `system_default` profile snapshot。
- [ ] 以 CAS/revision 保存全局设置和 policy；禁止 React 直接写自由 settings key。
- [ ] 实现 global/channel enabled、pause、quiet hours/DST、severity offset、trigger/recovery/repeat/cooldown/recovery notification。
- [ ] 变更策略后有界分页 reconcile：重算 future deadline；未 claim delivery 取消/抑制/替换；claimed/delivered 保留 snapshot。
- [ ] lifecycle 字段变化建立新的 evaluation epoch；不得用旧 observation 立即开告警、解决或复发。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_policy -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_reconcile -- --nocapture
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 所有输入组合可解释地匹配唯一策略；无有效用户规则仍返回 system default；更新不会重放已投递 opened notification。

## 11. Task 7：Delivery ledger、应用内调度与 retention worker

**Files**

- Create: `src-tauri/src/application/alerting/{delivery_planner,delivery_worker,retention_worker}.rs`
- Modify: application/runtime composition、task lifecycle/supervision owner
- Create: `src-tauri/tests/{alerting_delivery,alerting_worker,alerting_retention}.rs`

**Steps**

- [ ] 实现 delivery key/sequence、scheduled/claimed/delivered/suppressed/failed/outcome_unknown、lease、claim token、固定有限 retry 与退避。
- [ ] 实现 nearest-due 或有界轮询 worker，覆盖 state deadline、repeat、snooze/global pause 到期、quiet-hours 结束、启动恢复和 shutdown cancel。
- [ ] 应用内提醒只从 delivery/current read model 派生；任何通知结果不得回写 incident lifecycle。
- [ ] 实现 retention：current incident 永不按时间删除；resolved/occurrence/delivery 批量清理，保留脱敏摘要和 policy snapshot。
- [ ] 为时间跳变、lease 到期、OS 调用后崩溃、重启、quiet hours、global disabled 和 notification storm 写固定时钟测试。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_delivery -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_worker -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_retention -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

**Exit gate:** delivery 有完整审计和 crash-boundary 语义；worker 无重复实例、无无限 timer、无 secret；retention 不破坏 current incident 解释。

## 12. Task 8：Cursor read model、IPC DTO 与生成绑定

**Files**

- Create: `src-tauri/src/application/queries/change_center_workspace.rs`
- Create: `src-tauri/src/commands/alerting.rs`
- Create: `src-tauri/src/ipc/dto/{alerting_reads,alerting_mutations}.rs` 及 `.typescript.txt`
- Modify: `ipc/registry.rs`、`ipc/dto/mod.rs`、ACL/capability、command facade/composition、serialization fixtures
- Create: `src-tauri/tests/alerting_ipc.rs`

**Steps**

- [ ] 提供 incident workspace、occurrence history、delivery history cursor APIs 和 seen/snooze/policy/settings/permission/test-notification mutations。
- [ ] cursor 绑定规范化 query fingerprint、稳定排序与 limit 上限；摘要与列表从同一 ReadSession 获得。
- [ ] DTO 只包含脱敏字段；搜索不扫描原始 JSON/错误正文；desktop payload 使用独立最小 DTO。
- [ ] 更新 command registry、ACL、TypeScript binding 和 pilot serialization；不添加旧命令 alias。
- [ ] 扩展 `BackendClient` 的可选 alerting domain client，并同步接入 `DesktopBackend` 的生成 binding、`DemoBackend` 的明确 unsupported 实现和前端 DTO 映射；客户端缺少该能力时只能显示受控 disabled 状态，不能静默伪造保存成功。
- [ ] 核对生成的 `AlertPolicy`、`AlertingSettings`、删除输入和分页 DTO 是否与 Rust 命名、可选字段、revision/CAS 语义完全一致；将生成物和 fixture 纳入同一命令验证，不手工修改 generated 文件。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_ipc -- --nocapture
pnpm.cmd generate:bindings
pnpm.cmd architecture:commands
pnpm.cmd test:contracts
pnpm.cmd verify:fast
```

**Exit gate:** cursor/summary、安全 DTO、mutation CAS 和 generated contracts 均通过；旧 API 尚未由产品 consumer 使用，只保留到 Task 12 的删除边界。

**交付物：** command registry/ACL revision、生成 TypeScript、Desktop/Demo backend 接入、序列化 fixture、IPC contract test 和脱敏字段审计结果。

## 13. Task 9：变更中心、总览与侧栏 UI

**Files**

- Modify/Create: `src/features/changes/{ChangeCenterPage,CurrentIncidentsView,OccurrenceHistoryView,DeliveryHistoryView,IncidentDetail}.tsx`
- Create: `src/lib/{api,queries,types}/alerting*`
- Modify: `src/lib/query/{queryKeys,resourceQueries}.ts`、dashboard consumer、`src/components/shell/AppShell.tsx`
- Create: `src/features/changes/*.test.tsx`

**Steps**

- [ ] 改为“当前问题 / 变化历史 / 提醒记录”三视图；每个视图使用独立 cursor query、loading/empty/error/disabled/窄窗口状态。
- [ ] 当前问题支持后端筛选、摘要、详情、seen/snooze/deep link；详情显示状态进度、有效 policy 与 delivery 解释。
- [ ] 总览和侧栏只请求 incident aggregate；徽标遵守 `open + unseen + warning/critical` 合同。
- [ ] 删除进入路由即批量 mark-read、旧 `changeEvents` 全量 query、内存 filter/paginate/active aggregate、全局“清除记录”操作。
- [ ] 通过键盘、焦点、长文本和分页测试验证新工作台；不以自定义 DOM event 同步缓存。
- [ ] 将旧 `ChangeCenterPage` 的路由、筛选、详情和用户已有改动逐项合并到新查询模型；先保留可回滚的组件边界，完成新三视图和 mutation 后再移除旧 query/cache 分支，禁止直接覆盖无关页面改动。
- [ ] 用固定 fixture 验证 200 条以上、恢复后计数下降、复发新 episode、seen 与 snooze 分离、snooze 到期和 query fingerprint 变化时 cursor 失效。

**Run**

```powershell
pnpm.cmd test -- src/features/changes src/components/shell src/lib
pnpm.cmd build
pnpm.cmd test:contracts
```

**Exit gate:** 超过 200 条时所有筛选和摘要正确；进入页面不会写全量状态；恢复后的计数在所有消费面同步下降。

## 14. Task 10：提醒与告警设置工作区

**Files**

- Create: `src/features/settings/{AlertingSettingsPage,AlertPolicyEditor,AlertPolicyList,AlertingDeliveryPreview}.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`、route/deep-link registry、settings tests
- Create: `src/features/settings/AlertingSettings*.test.tsx`

**Steps**

- [ ] 在设置增加“提醒与告警”入口，变更中心工具栏提供同一工作区的齿轮 deep link。
- [ ] 实现通知方式、默认规则、规则列表、时间与保留策略四个非嵌套区域；严格复用现有表单/弹窗/焦点模式。
- [ ] 编辑器使用选择器、stepper、时间输入和 segmented controls；显示字段校验、scope 归属错误、规则优先级与自然语言预览。
- [ ] 保存后显示 reconcile 状态和 effective policy 来源；desktop 权限不可用时明确降级，不伪造已启用。

**Run**

```powershell
pnpm.cmd test -- src/features/settings src/features/changes
pnpm.cmd build
```

**Exit gate:** 用户能完成 Spec 3.3 的全部配置；规则保存失败不改变本地表单外的 canonical state；设置页面不会越权修改 incident 生命周期。

## 15. Task 11：桌面系统通知

**Files**

- Modify: `src-tauri/Cargo.toml`、`package.json`、Tauri capability/ACL、dependency license inventory
- Create: `src-tauri/src/services/alerting/desktop_notification.rs`
- Modify: delivery worker、desktop DTO/command、notification tests

**Steps**

- [ ] 审计并引入官方 Tauri notification 能力；只在用户 opt-in 后请求权限。
- [ ] adapter 接收 claim 后的最小 payload，调用 OS，再以 claim token 回写 delivery；不持有 DB transaction，不读取 secret。
- [ ] 覆盖允许、拒绝、不可用、测试通知、点击打开 incident deep link 与 crash-boundary `outcome_unknown`。
- [ ] Windows 实机 smoke 使用假 incident，不记录真实 Station/Key 数据。
- [ ] 将 desktop adapter 纳入统一 supervised runtime：单实例、取消、shutdown、lease reclaim、有限重试和平台不可用降级都由生命周期 owner 管理；禁止在 command 或 React effect 中启动 detached timer/worker。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test alerting_delivery -- --nocapture
pnpm.cmd architecture:security
pnpm.cmd verify:fast
```

**Exit gate:** 权限三路径、降级、deep link 和敏感信息扫描通过；OS 通知从未成为唯一告警存储。

## 16. Task 12：原子 production cutover 与旧栈删除

**Files**

- Modify: app/runtime composition、producer 调用点、IPC registry/ACL/DTO/binding、routes、shell、dashboard、changes/settings UI
- Delete: `src-tauri/src/{application/changes.rs,application/command_facades/change_events.rs,commands/change_events.rs,models/change_events.rs,persistence/stores/change_store.rs}` 及对应 module exports
- Delete: `src/lib/{api,types}/changeEvents*`、`src/lib/changeEvents/changeEventViewModels.ts`、旧 changes view model/query keys/mocks/tests
- Modify: deletion ledger、architecture gate、dead-code inventory、serialization fixture

**Steps**

- [ ] 在同一 revision 切换所有 producer 到 alerting ingress，所有页面到新 query/DTO，所有 mutation 到 alerting commands。
- [ ] 删除旧 command registry 条目、ACL、binding、bridge 方法、query key、cache merge 和旧页面操作；不保留 alias 或 “新失败回旧” fallback。
- [ ] 删除旧 backend modules 和 production tests；Task 3 的 private legacy reader 可以保留到观察期结束，但必须不依赖已删除模块、无产品调用，并由 Task 13 删除。
- [ ] 重新生成绑定、fixture 和 architecture manifest；检查 portable/import catalog 没有过时表以外的 runtime 写入口。
- [ ] 执行从新观测到 UI、恢复计数、策略变化、delivery 的端到端组合测试。
- [ ] 在 cutover 前补齐并锁定架构检查脚本（例如 `scripts/change-center-alerting-architecture.test.mjs`）；若计划引用的脚本不存在，必须先创建、纳入 `package.json`/contract runner，再执行 Task 12，不得把“命令缺失”当作通过。
- [ ] 生成一份 cutover manifest，记录 producer、composition、IPC、binding、路由、query key、UI consumer 和删除文件的同一 revision；任何列表不完整都阻止切换。

**Run**

```powershell
rg -n "upsert_change_event|resolve_change_event|mark_change_event_read|mark_change_events_read|dismiss_change_event|clear_change_events|list_change_events|ChangeService|ChangeStore|changeEvents" src-tauri/src src
pnpm.cmd generate:bindings
pnpm.cmd verify:fast
pnpm.cmd build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml alerting -- --nocapture
```

**Exit gate:** 允许的命中仅能来自历史 migration、Task 3 upgrade test fixture 和删除账本；生产 module graph、IPC/binding、UI consumer 均为零。Task 12 未全部通过不得部分交付。

## 17. Task 13：观察期、destructive migration 与清理

**Files**

- Create: `docs/plans/YYYY-MM-DD-change-center-legacy-table-removal.md`（观察期结束后才创建）
- Create: `src-tauri/src/persistence/migrations/00NN_remove_legacy_change_events.sql`
- Delete: Task 3 legacy adapter、legacy catalog/import entry、旧表性能 fixture
- Modify: postcondition、schema15 fixture、portable migration catalog、release data-upgrade docs

**Steps**

- [ ] 至少完成一个发布观察周期，证明新 writer/read model 稳定、upgrade progress complete、legacy adapter 调用计数为零且存在可恢复备份。
- [ ] 在独立计划/revision 中删除 `change_events` 表及索引；不把 destructive SQL 混进 Task 12。
- [ ] 同步移除 legacy adapter、import/export catalog 条目、旧性能测试、允许列表和 migration-only compatibility 说明。
- [ ] 验证当前 schema 和 schema15 都能到达删除后的 latest；没有运行时 repair 或永久 fallback。

**Run**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade -- --nocapture
pnpm.cmd verify:fast
pnpm.cmd verify:persistence-artifacts
```

**Exit gate:** legacy table、adapter、allowlist 与 catalog 均已删除；备份/恢复和 schema15 upgrade 仍通过；删除无法满足前置条件时必须延后。

## 18. Task 14：资格验证、文档和审计闭环

**Files**

- Create: `docs/audits/change-center-alerting-qualification.md`
- Create: `docs/audits/change-center-alerting-acceptance-matrix.md`
- Update: `docs/PROJECT_PLAN.md`、`docs/PRODUCT_MODEL.md`、`docs/README.md`、release/checklist、deletion ledger

**Steps**

- [ ] 将 Spec 第 22、23、26 节逐项映射到测试、命令、结果、版本和残余风险；不以“计划存在”代替通过证据。
- [ ] 运行 schema/current/schema15 upgrade、failure/restart/concurrency/property、IPC/generated binding、Rust、Vitest、build、architecture/security/artifact、性能查询计划和 Windows notification smoke。
- [ ] 生成脱敏诊断证据，核对无 secret、原始错误、URL query 或真实账号数据。
- [ ] 仅在所有验证通过后，将 Spec/计划状态从 Draft/待实施更新为事实状态；否则明确未完成项目与影响。

**Run**

```powershell
pnpm.cmd verify:fast
pnpm.cmd verify:full
pnpm.cmd build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --tests -- --nocapture
pnpm.cmd verify:persistence-artifacts
```

**Exit gate:** 验收矩阵无未解释失败；删除账本清零；文档、生成物、schema、代码和 UI 使用同一套术语与行为合同。

**交付物：** qualification、acceptance matrix、性能基准、敏感信息扫描、Windows 通知 smoke、migration/recovery 证据、最终 deletion ledger 和发布说明。只有这些证据齐全，才允许把 Spec/计划从 Draft 改为已实施或发布状态。

## 19. 回滚与故障边界

- Task 1-11 未进行 Task 12 cutover 前：新模块只能由测试/upgrade step 使用，移除接线即可回到当前产品路径；不得 shadow write。
- Task 12 后：不回退到旧事件状态机。回滚依赖已验证数据库备份和应用版本，或使用 typed upgrade recovery；不能执行 ad hoc repair SQL。
- Task 13 的旧表删除是不可逆数据结构操作，必须等待观察期、备份和 schema15 recovery 通过；不满足即延后。
- 桌面通知权限/平台失败只降低 delivery 渠道，不阻塞 incident projection；worker 失败保留可读问题并在 UI 暴露调度降级。

## 20. 最终交付清单

- 事件注册表与异常/恢复矩阵；
- append-only schema、postcondition、durable upgrade、typed recovery 和 schema15 evidence；
- occurrence/incident/attention/policy/delivery stores、状态机、worker 与 retention；
- 各权威事实 producer 的统一 ingress 和恢复闭环；
- cursor read model、IPC/ACL/generated binding 与脱敏 DTO；
- 变更中心三视图、总览/侧栏 aggregate、设置工作区与桌面通知；
- 原子 cutover、删除账本、观察期 legacy table removal；
- qualification、acceptance matrix、性能与安全证据、更新后的产品与发布文档。

## 21. 阶段交付与排期编排

| 阶段 | 包含任务 | 必须产出 | 可进入下一阶段的硬门禁 |
|---|---|---|---|
| A. 契约冻结 | 0-1 | 评审决策、基线、事件注册表、删除账本、架构脚本 | 待评审决策已关闭；每个状态型事件都有异常/恢复 fixture；旧入口都有 owner |
| B. 数据与投影 | 2-4 | migration/postcondition、durable upgrade、stores、reducer、并发测试 | current/schema15 upgrade 可恢复；writer gating 正确；事务幂等和 CAS 通过 |
| C. 生产闭环 | 5-7 | 全部 producer ingress、policy/settings、delivery ledger、supervised workers、retention | 生产路径无旧 writer；恢复矩阵、策略重算、重启/崩溃边界测试通过 |
| D. 产品消费 | 8-10 | IPC/generated binding、Desktop/Demo backend、三视图、总览/侧栏、设置工作区 | 超过 200 条查询准确；UI 不全量汇总、不进入即批量已读；设置 CAS/disabled 状态正确 |
| E. 平台通知 | 11 | desktop adapter、权限/降级、deep link、Windows smoke | OS 三路径通过；应用内提醒不依赖桌面通知；worker 受统一生命周期管理 |
| F. 切换与清理 | 12-13 | cutover manifest、旧栈删除、观察期证据、独立 destructive migration | 单一生产路径；legacy allowlist/调用计数为零；备份和 schema15 recovery 通过 |
| G. 资格交付 | 14 | qualification、acceptance matrix、发布文档 | 全量验证无未解释失败；才可更新文档状态并交付 |

建议按阶段创建独立可审查 revision；同一阶段内部可拆多个提交，但不在本计划中授权提交、建分支或推送。任何阶段未通过硬门禁，后续阶段只能继续补证据，不能提前删除旧实现或开放新 UI。

### 21.1 建议排期与关键路径估算

以下为单个主责 owner、其余角色按需协作时的工程估算，单位为有效工作日；它用于排期，不替代 Exit gate，也不包含外部等待时间。任何任务若因测试失败、迁移恢复问题或边界决策未关闭而超出时间盒，应保持未完成并重新拆分，不得压缩验证步骤。

| 批次 | 任务 | 建议工期 | 关键输入 | 关键输出 | 备注 |
|---|---|---:|---|---|---|
| A | Task 0-1 | 2-3 | Spec、现状基线、旧栈调用清单 | 评审决策、事件注册表、删除账本、边界门禁 | 未关闭的产品决策会阻塞后续实现 |
| B | Task 2-3 | 4-7 | Task 1 领域类型 | foundation migration、postcondition、schema15 回填与恢复 | 关键路径；需预留失败重跑时间 |
| B | Task 4 | 3-5 | Task 2 stores/schema | projector、状态机、幂等与并发证据 | 不能提前开放生产 writer |
| C | Task 5-7 | 6-10 | Task 4 投影合同 | producer 恢复矩阵、policy reconcile、delivery/worker | 可并行准备，按 5→6→7 验收 |
| D | Task 8-10 | 5-8 | Task 7 read/delivery 合同 | IPC/binding、三视图、设置工作区 | Task 9/10 可并行，但共用同一 DTO revision |
| E | Task 11 | 2-4 | Task 7-8 delivery 与 DTO | 桌面通知 adapter、权限/降级、Windows smoke | 实机 smoke 可能产生外部等待 |
| F | Task 12 | 2-3 | Task 5-11 全部 verified | 原子 cutover、zero-reader 证据、旧运行时删除 | 唯一生产切换点，必须串行 |
| G | Task 13 | 2-4 + 观察期 | Task 12 cutover、备份与恢复演练 | 独立 destructive migration、旧 adapter/catalog 清理 | 观察期按发布周期计算，不折算为开发日 |
| G | Task 14 | 3-5 | Task 13 清理结果 | qualification、acceptance matrix、发布与审计文档 | 任一未解释失败都会阻止交付 |

按上述估算，纯开发与验证的关键路径约为 29-49 个有效工作日；其中 Task 2-4、Task 5-7、Task 8-10 可在不触碰共享边界的前提下并行准备。正式交付至少还需等待一个完整观察周期，并完成 Windows 通知实机 smoke、备份恢复和 schema15 到最新版本的升级演练。排期时应把 Task 12、Task 13、Task 14 作为不可压缩的发布闸门，而不是与前端开发并行的“收尾工作”。

## 22. 任务执行记录模板

每个 Task 的实施记录至少包含：

1. 起始工作区状态、最新 migration 编号和相关用户改动保护清单；
2. RED 反例或失败测试，以及它证明的旧缺陷/缺失能力；
3. 变更文件和职责边界，注明新增、修改、保留和删除；
4. GREEN/REFACTOR 命令、退出码、测试数量和脱敏摘要；
5. schema、binding、fixture、ACL、架构脚本等生成物是否通过对应生成命令更新；
6. 未解决风险、回滚方式、下一 Task 的前置输入；
7. 退出 gate 的逐项证据链接或文件路径。

若某项无法验证，必须将任务标记为未完成并写明原因、影响和补验计划，不得用“代码已存在”“本地手工验证”或“计划已写明”替代证据。

## 23. Spec 覆盖矩阵

| Spec 章节 | 执行任务 | 主要证据 |
|---|---|---|
| 1-4 执行摘要、问题、目标、非目标 | Task 0、Task 14 | 评审决策、范围基线、最终资格报告 |
| 5-6 术语、事件分类与生命周期 owner | Task 1、Task 5 | event registry、恢复矩阵、producer contract |
| 7-8 incident 状态机与 attention | Task 1、Task 4、Task 9 | 固定时钟 reducer、并发/幂等测试、三视图交互测试 |
| 9-10 policy/settings 与健康阈值关系 | Task 5、Task 6、Task 10 | policy resolver、CAS/reconcile、设置工作区测试 |
| 11 通知渠道与投递语义 | Task 7、Task 11 | delivery ledger、worker、权限/崩溃边界、Windows smoke |
| 12 设置入口与规则编辑器 | Task 10 | 表单校验、优先级预览、disabled/error/focus 测试 |
| 13 变更中心 UI | Task 8、Task 9 | cursor workspace、三视图、200+ 数据 fixture |
| 14 数据模型与 SQLite | Task 2、Task 3、Task 13 | migration/postcondition、schema15 upgrade、destructive migration |
| 15 写入与评估架构 | Task 1、Task 4、Task 5、Task 7 | module boundary、事务/CAS、supervised runtime |
| 16 Read Model 与 IPC | Task 8、Task 9 | DTO 脱敏、ACL、generated binding、query contract |
| 17 迁移与兼容策略 | Task 0、Task 3、Task 12、Task 13 | upgrade progress、high-water、cutover manifest、recovery |
| 18 retention 与清理 | Task 7、Task 13 | retention property test、观察期和删除前置条件 |
| 19 安全与隐私 | Task 1、Task 7、Task 8、Task 11、Task 14 | 敏感字段审计、通知 payload 扫描、artifact 检查 |
| 20 可观测性 | Task 3、Task 7、Task 14 | upgrade/delivery 指标、脱敏诊断和资格报告 |
| 21 错误处理与降级 | Task 3、Task 6、Task 7、Task 11 | typed failure、policy invalid、worker/OS 降级测试 |
| 22 测试策略 | Task 1-14 | 各 Task gate、端到端组合、property/concurrency 测试 |
| 23 性能与容量 | Task 2、Task 8、Task 9、Task 14 | EXPLAIN QUERY PLAN、固定数据集基准、首屏/聚合时延 |
| 24 实施阶段 | Task 0-14 | 阶段硬门禁和阶段证据包 |
| 25 解耦、兼容与旧实现删除 | Task 0、Task 5、Task 8、Task 12、Task 13 | deletion ledger、allowlist、module graph、旧表删除 |
| 26 验收标准 | Task 14 | acceptance matrix 逐项结果 |
| 27 风险与控制 | Task 0、Task 3、Task 7、Task 12-14 | 风险登记、回滚记录、观察期和发布门禁 |
| 28 待评审决策 | Task 0 | 决策记录和批准 revision |
| 29 交付物 | Task 14 | 交付清单、发布文档、审计证据包 |

该矩阵用于防止“实现了主流程但遗漏规范章节”。任何新增规范章节或修改默认行为，都必须同时更新对应任务、测试和退出门禁。
