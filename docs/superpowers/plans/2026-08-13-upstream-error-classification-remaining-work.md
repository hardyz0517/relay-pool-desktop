# 上游错误分类与重试：剩余实质工作计划

状态：Ready for implementation

日期：2026-08-13

适用范围：本计划承接 `2026-08-13-upstream-error-classification-retry-closure.md`，只列出截至当前工作区仍未关闭的实质功能、可靠性和资格工作。它不替代原专项计划，也不将真实 provider smoke 计入本地工程完成。

## 当前事实

以下成果已有实现和针对性验证，不重复开发：

- `pnpm.cmd verify:fast` 已通过；专项新增 production dead-code group 已清零。
- capacity 已支持同一 target 的有界重试；耗尽后会记录 `capacity_same_domain_fallback_suppressed`，不会请求同域 sibling key。
- scoped health 的终态写入使用规划时捕获的 credential/account/group/model revision，不会把旧 attempt 的结果写到新 revision。
- runtime Decision Trace 已由真实 execution 写入有界 ring，并能由 `get_request_decision_trace` IPC 查询；2xx buffered error envelope 已进入 canonical 链。
- 已存在 `ProviderCapacityDomain`，但其身份目前只在 target 解析后才可可信构造，尚不能作为 PlanningSnapshot 的跨域选择依据。

当前工作区有大量并行未提交改动。每一批开始、结束都记录 `git status --short`；不得回退、清理或重写无关变更。

## 不变量

- 不得根据 `reqwest` 的普通发送错误、body poll 次数、HTTP status 或下游 commit 推测请求体已经或尚未写入 socket。
- 非幂等且 acceptance 不确定的请求保持 fail closed，禁止透明重试。
- 没有 provider/deployment/region 的权威隔离事实时，禁止跨 capacity domain fallback；不得用站点、key、URL 或站点类型替代。
- durable trace、DTO、metrics、fixture 和日志只保存稳定 code、闭合枚举、版本和审核过的内部标识；不得保存 message、authorization、完整 URL/query 或 secret。
- terminal、health effect 与 replay/outbox 的 exactly-once 语义不能通过测试专用分支或 suppress 规避。

## 实施批次

### Batch 1：补齐 health 与 capability 的生产 E2E

目标：关闭 revision-fenced scoped verdict 从真实终态到下一次规划的生命周期证据。

1. 为 group/subscription failure 增加 production-composition E2E：终态只写 group verdict，下一 PlanningSnapshot 只排除该 group，无关 subject 可选；group revision 变化后恢复。
2. 为 `model_not_found` 增加 E2E：只写 key/model/deployment capability verdict，下一 snapshot 精确排除；model alias、profile 或 subject revision 变化后恢复。
3. 断言同一 subject 上的 credential/account/group/balance/quota/rate-limit/capability 维度可并存，恢复一个维度不删除其余 verdict。
4. 为 projector shadow rebuild、migration/reimport、crash/restart 与 duplicate/late terminal 补齐最小故障回归；保持单批读取和 revision fence，不引入 candidate N+1 查询。

完成条件：两条 required E2E 和相关 persistence/fault tests 通过，且 planner 只消费 typed scoped verdict。

### Batch 2：建立可证明的 transport send phase

目标：让 replay/acceptance 决策基于 transport owner 的单调事实，而非当前 `NotConnected | Unknown | ResponseStarted` 的保守近似。

1. 先完成 transport spike/design record：评估现有 `hyper`、`hyper-util`、`tokio-rustls`、`http-body` 能否在 Windows 下保留 direct/system/HTTP/SOCKS proxy、Rustls、HTTP/2、timeout 与流式收发。
2. 定义并验证单调 reporter：`NotConnected`、`ConnectedNoHeaders`、`HeadersSent`、`BodyPartiallySent`、`BodyFullySent`、`ResponseStarted`；无法证明的路径必须为 `Unknown`。
3. 用本地 TCP/HTTP harness 注入 connect、TLS、headers、partial/full body、response-started 和 mid-stream failure，证明“body 被 poll”不等于“socket 已接收”。
4. 仅在同一 reporter 已供生产和测试使用后，将中间 phase 从 `cfg(test)` 移入生产；把 HTTP、stream、timeout 和 cancellation 的最后可靠 phase 传到 canonical outcome。
5. 针对幂等性、body replayability、idempotency identity 和 provider capability 补 replay matrix，确保 uncertain/possibly accepted 的非幂等请求无透明 retry。

完成条件：`upstream.rs` 不再只产出三种 phase；生产与测试共享 reporter；所有不可支持路径被显式证明 fail closed。

### Batch 3：完成权威 capacity domain 与至多一次跨域终局

前置条件：Batch 2 关闭 replay-safety 事实；provider identity 的来源、版本和可信边界经设计确认。

1. 在 durable operational facts、target resolver 和 PlanningSnapshot 中增加权威的 provider/deployment/region identity 及其 revision；不从 URL、station 或 key 推导。
2. 在 snapshot 内构造 `CapacityDomainCommitment`，由唯一 Coordinator 维护 request-local attempt、deadline、sleep、admission、排除 domain 和 cross-domain-consumed 状态。
3. 同 target 耗尽后先交由普通 planner 执行同域抑制；仅当 portability 与 replay gate 都允许时，选择一个不同且可信的 domain。
4. cross-domain attempt 结束后无论成功、失败、取消或 timeout 都终止该逻辑请求的 retry 链；trace 必须记录选择、排除、send ordinal 和终局原因。
5. 用 fake clock/loopback 覆盖 revision 漂移、FIFO waiter、queue full、cancel/shutdown、HalfOpen race、累计 sleep/deadline 和 100 并发限制。

完成条件：production-composition trace 可证明没有同域 sibling outbound、最多一次可信跨域 outbound，且 Execution 未自行拼装 domain 或第二套预算。

### Batch 4：将可观测性闭合到持久化与低基数 metrics

目标：现有 runtime ring 继续用于短期诊断，同时为已结束请求提供可审计的 durable outcome 摘要。

1. 冻结版本化 canonical outcome/trace profile，覆盖 classification、confidence、evidence source、target kind、acceptance、send phase、replay/billing、retry disposition、effect、profile version 和 failure-domain commitment。
2. 设计 additive migration（预期在 `0036` 之后）和 request/attempt write/read model；在既有 terminal transaction 写入稳定摘要，不存动态文本或网络凭据。
3. 让 `get_request_decision_trace` 先返回 durable versioned summary，并将 runtime ring 作为近期 bounded events 的补充；同步 DTO、ACL、bindings、fixture 和 redaction contract。
4. 实现由 `RoutingRuntimeState` 持有的有界 metrics recorder，标签只用闭合枚举（classification、confidence、retry、pre/post-commit、saturation/fail-closed、profile version）；删除 `metrics.rs` 的模块级 `allow(dead_code)`。
5. 验证 ring 的 512 traces/16 MiB、每请求 64 events/32 KiB、字段 512 bytes 与一次性 truncation，以及 durable DTO/metrics 不含高基数或敏感字段。

完成条件：真实 proxy execution 的 canonical failure 可在进程重启后通过 IPC 读取同一版本化摘要；runtime events 与低基数 metrics 都有生产 producer 和 consumer。

### Batch 5：补齐故障、并发、协议与恢复矩阵

目标：用确定性本地测试证明资源释放、单终态和恢复语义。

1. 扩展现有 loopback/fake-clock harness，覆盖 100 并发 capacity 和 100 并发诊断内存请求的 active/waiter/32 MiB 上限、FIFO、deadline、cancel、panic、shutdown 与 permit 回收。
2. 覆盖 gzip/deflate 失败与压缩膨胀、JSON 深度/node/token/string 上限、malformed UTF-8、任意 chunk split、SSE control-only EOF、semantic 后缺 terminal、合法空 completed、256 KiB event 与慢消费者背压。
3. 覆盖 pre/post-commit persistence failure、writer unavailable、outbox/checkpoint replay、duplicate/late terminal 与 payload collision；明确定义无法跨网络原子化的 degraded/recovery 结果。
4. 覆盖 classifier/projector/profile mismatch fail closed、热更新只影响新请求、runtime 重启不泄漏 cooldown/permit、最大 body 复用不发生 attempt 深拷贝。

完成条件：测试不依赖真实 provider；所有资源计数回到基线；committed 路径永不 retry 或产生第二终态。

### Batch 6：删除旧 owner、资格验证与文档收口

1. 在上述行为有 E2E 证据后，删除旧 classifier/fallback/compatibility writeback/parser owner；同步 deletion ledger 和 architecture manifests，新增禁止规则而非扩大 allowlist。
2. 按顺序运行并修复根因：`git diff --check`、专项 contract/architecture tests、`cargo fmt -- --check`、`cargo check --locked`、相关 Cargo tests、全量 `cargo test`、`pnpm.cmd test`、`pnpm.cmd build`、`pnpm.cmd verify:fast`、`pnpm.cmd verify:full`。
3. 更新原专项第 11 节、`docs/README.md`、acceptance matrix、qualification、deletion ledger 和 release note，分别标注 `done with evidence`、`partial`、`pending` 或 `externally blocked`。

完成条件：Tasks 0-8 的工程 exit gate 全部通过，文档与代码同一 revision；未完成的真实 smoke 不得写为 done。

### Batch 7：真实 provider 与 Codex smoke（外部授权）

仅在 Batch 6 完成且用户明确授权、提供隔离测试账号/Key 后执行。验证 HTTP/SSE capacity、Sub2API 成功与 401/429/5xx、Codex 对 `server_error` 的行为，以及 trace/log 不泄露认证数据。无授权时状态为 `pending external authorization/evidence`，不阻塞工程计划的诚实交付。

## 交付顺序与依赖

```text
Batch 1 (health/capability E2E) ─┐
                                 ├─> Batch 4 (durable observability) ─> Batch 5 ─> Batch 6
Batch 2 (transport phase) ─> Batch 3 (capacity cross-domain) ───────┘
Batch 7 only after Batch 6 and explicit authorization
```

Batch 1 与 Batch 2 可并行准备；Batch 3 必须等待 Batch 2；Batch 4 的 schema/DTO 设计可提前，但生产持久化须等待 stable outcome contracts。每一批完成后更新本文件和原专项台账，记录实际命令、退出码与未验证项。
# 上游错误分类与重试升级：剩余实质工作计划

> 状态：待执行。本文只列出截至 2026-08-13 尚未完成、且影响专项闭环的实质内容；已验证的同目标 capacity 重试、同域 sibling 抑制、revision fence、group/key-model effect 写入与基础运行时 trace 不重复列为待办。

## 当前边界

已经具备的行为：

- capacity 只会对同一已解析目标做有界重试；没有可信跨域身份时，不会把请求轮转至同一 capacity domain 的 sibling。
- group/subscription 和 `model_not_found` 已能以 revision fence 写入持久化 effect，并已证明下一次 planning snapshot 的精确排除与 revision 后恢复。
- 运行时 `DecisionTrace` 有有界 ring；2xx 缓冲响应中的错误 envelope 已纳入分类；低基数分类指标已有生产写入入口。

尚未具备的证据或实现：跨进程的决策结果、精确 transport 发送阶段、可信跨 capacity-domain fallback、故障矩阵、旧 owner 删除与完整验证。真实提供商 smoke 需单独授权。

## 实施顺序

### 1. 先恢复可重复的快速基线

目的：确保后续失败来自本专项而非陈旧生成物或测试夹具。

1. 重新运行 `pnpm.cmd verify:fast`，确认此前因 `V2Fixture` 缺少 helper 导致的生成绑定编译失败已消失。
2. 对 metrics 的 `#[expect(dead_code)]` 逐项复核：只保留确实尚无 producer 的通用维度；优先收窄到 variant/field，不能恢复模块级 `allow(dead_code)`。
3. 固化本轮基线的失败清单。只有验证失败可稳定复现时，才能把它列为后续工作项。

出口条件：`verify:fast` 通过，或有一份按命令、失败点和所有者归类的剩余失败记录。

### 2. 交付 durable routing outcome（优先级最高）

目的：让决策诊断在 runtime ring 丢失、应用重启和事后审计时仍可查询，而不写入原始请求、URL、认证信息或高基数标签。

1. 新增版本化迁移（建议 `0037_routing_decision_outcomes.sql`），建立一请求一行的 `request_routing_outcome_summaries`。字段只接受闭集/受限值：分类、置信度、证据来源、请求接受状态、发送阶段、replay/billing 状态、retry disposition、health/capability effect、profile/provider/retry 版本和脱敏的 domain commitment digest/version。
2. 扩展 typed persistence API，使 outcome summary 与 request terminal 在同一写入事务内落库；重复终结只允许完全一致的重放，否则 fail closed。
3. 将执行层 canonical outcome 通过 typed lifecycle/finalization 路径传到 writer，禁止从 annotations 或原始 upstream body 反向推断。
4. 将 `get_request_decision_trace` 改为 durable summary 优先，runtime ring 仅作为当前进程的补充时间线；缺少 runtime 事件不能掩盖 durable terminal。
5. 更新 IPC DTO、生成绑定、portable migration catalog、schema reader、fingerprint 和 schema version 测试。

出口条件：

- 进程重启/没有 runtime trace 时仍能读到完整的受限 terminal summary。
- summary 不含请求正文、headers、URL、provider 原始名称、secret 或 request id 标签。
- collision、迁移、读写、redaction 与 IPC 合约回归均通过。

### 3. 设计并实现可信 capacity-domain 身份

目的：在保守 sibling 抑制的基础上，只有域身份可验证时才开放一次跨域 capacity fallback。

1. 定义由 provider/deployment/region 的权威配置或持久化事实产生的 `CapacityDomainCommitment`；不得从 URL、station id、key 名称、provider 类型或模型名称推断。
2. 将 commitment 及其 revision 传入 `PlanningSnapshot`，并在 candidate/attempt/finalization 中保持 revision fence。
3. 在 coordinator 增加“同域重试耗尽后最多一次可信跨域 fallback”；身份缺失、过期、冲突或无法重验时记录抑制原因并 fail closed。
4. 在 durable outcome 和 runtime trace 中记录闭集的 domain 决策原因，不记录可识别提供商信息。

出口条件：相同权威域永不 cross-fallback；不同且可重验的权威域只执行一次 fallback；身份变更后的 in-flight attempt 不能污染新 snapshot。

### 4. 替换/封装 transport 以取得真实发送阶段

目的：区分“确定未发送”与“不确定是否已发送”，同时维持当前 direct/system/HTTP/SOCKS、TLS、HTTP/2、timeout 和 streaming 的能力。

1. 先完成技术 spike，选择能报告 connect、request headers、body partial、body complete、response started 的底层 adapter；不能从 body polling、异常文本或 reqwest 私有行为猜测阶段。
2. 为已有 upstream client 引入最小 transport boundary；不支持的路径显式返回 `Unknown`。
3. 仅在证明确实未发送时允许非幂等请求的透明 replay；headers/body 已发送或未知均保持当前保守停止策略。
4. 将真实阶段传给 canonical outcome 和 durable summary，删除只在测试中模拟且无生产 owner 的中间阶段状态机。

出口条件：各 proxy/TLS/HTTP 组合的契约测试证明阶段真实性；非幂等 POST 在 unknown/post-send 状态从不被透明重试。

### 5. 完成 fault、并发与协议矩阵

目的：覆盖网络提交窗口、持久化和流式边界，而不是仅验证分类函数。

1. capacity：100 并发下 admission、同目标重试次数、等待队列、deadline、lease release 与 shutdown/cancellation。
2. 分类输入：gzip/deflate、异常 UTF-8、超限 JSON/body、2xx envelope、错误 envelope 冲突及诊断内存上限。
3. SSE：EOF、畸形事件、提交前后错误、慢客户端背压和取消传播；提交后不得错误地 replay/billing。
4. 持久化：attempt/request terminal 写入失败、已提交后的失败、重放、payload collision、进程恢复与 effect 幂等性。
5. 配置：热更新仅影响新请求；旧请求以原 snapshot/revision 完整收尾。

出口条件：每类故障都有可重复的回归测试和明确 terminal/retry/effect 断言，无 orphan lease、双 terminal 或跨 revision 写入。

### 6. 删除旧 owner 并完成架构/文档闭环

1. 仅在新 durable outcome、transport boundary 和 capacity commitment 已有生产调用与回归后，删除被替代的旧路由决策/健康 owner、测试专用兼容路径和重复 revalidation API。
2. 更新 architecture manifest、依赖边白名单、deletion ledger、当前专项计划台账、`docs/README.md` 所指向的当前验收文档。
3. 对每个删除项记录替代 owner、调用证据和回归用例，避免“死代码清理”删除仍承载安全语义的保守分支。

出口条件：架构门禁通过，所有删除都有替代证明，文档状态与实际代码一致。

### 7. 最终验证与外部授权项

本地完成后依次运行：

1. `git diff --check`
2. upstream contract 与 intelligent-routing architecture 脚本
3. `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
4. `cargo check --locked --manifest-path src-tauri/Cargo.toml`
5. 相关 Cargo 测试及全量 `cargo test --locked --manifest-path src-tauri/Cargo.toml`
6. `pnpm.cmd test`、`pnpm.cmd build`、`pnpm.cmd verify:fast`、`pnpm.cmd verify:full`

真实 OpenAI/Sub2API/Codex 请求 smoke 是独立的最后一项：只在用户提供明确授权、测试站点范围和可使用的非敏感凭据后执行。它不能被本地模拟或默认开发凭据替代。

## 完成定义

仅当第 1-7 节的出口条件均有命令输出或测试名可追溯地证明，且真实 smoke 已授权并完成（或被明确从验收范围排除）时，才能将本专项标记为完成。
