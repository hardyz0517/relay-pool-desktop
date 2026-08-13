# 上游错误分类与重试：当前完成计划

状态：Ready for implementation

日期：2026-08-13

适用范围：本计划根据两个并行任务截至当前的实现与验证记录，列出仍需完成的实质工作。它承接 `2026-08-13-upstream-error-classification-retry-closure.md`，不替代其中已冻结的安全不变量，也不把真实 provider smoke 纳入未经授权的本地工程验收。

## 当前判断

以下能力已有实现基础，但不能全部视为专项已完成：

- 同一 target 的 capacity 有界重试、同域 sibling 抑制和规划 revision fence 已进入生产链。
- group/subscription 与 `model_not_found` 的 scoped verdict 写入及 Planner 消费正在补 production-composition E2E；其中 key/model 场景曾单独通过，两个场景合并运行、group 场景、回归测试与快速门禁尚未形成验收证据。
- runtime `DecisionTrace` ring、2xx buffered error-envelope 分类和低基数 metrics 的生产入口已存在；跨进程可读的 durable outcome 尚未完成。
- `ProviderCapacityDomain` 目前只能在 target 解析后可信构造，尚不能支持 PlanningSnapshot 中权威且可重验的一次跨域 fallback。

以下事项不属于本地工程完成条件：真实 OpenAI-compatible/Sub2API/Codex smoke。它需要用户明确授权、隔离测试范围和非敏感测试凭据。

## 不变量

- 非幂等请求在 acceptance 不确定时 fail closed，绝不透明重试。
- 不从 body poll、HTTP 状态、下游 commit、URL、station 或 key 名称推断发送阶段或 capacity domain。
- 已提交响应不 retry、不产生第二终态；terminal/effect/replay 保持幂等并对 payload collision fail closed。
- durable record、IPC、日志、fixture 和 metrics 只使用稳定 code、闭合枚举、版本和经审核的内部标识，禁止原始 message、secret、认证信息、完整 URL/query 和高基数 request 标识。
- 并行工作区改动保持原样；每批工作前后记录 `git status --short`，只修改该批所需文件。

## 工作批次

### Batch 0：关闭正在进行的 scoped-verdict E2E

目标：将终态 effect 到下一次 `PlanningSnapshot` 的 revision-fenced 生命周期变成可重复的生产组合证据。

1. 合并运行 group/subscription 和 `model_not_found` 两条 E2E，排查 fixture、migration baseline、group binding/revision trigger 或并行工作区导致的不稳定性。
2. 对 group 场景证明：终态只写 group verdict；下一 snapshot 只排除对应 group；无关 group/subject 仍可选；group revision 变化后恢复。
3. 对 key/model 场景证明：终态只写 key/model/deployment capability verdict；下一 snapshot 只排除该 commitment；无关 key/model 仍可选；key、model alias 或 profile 的权威 revision 变化后恢复。
4. 补一个维度正交性回归：同一 subject 的 credential、account、group、balance、quota、rate-limit、capability verdict 可共存，恢复一个维度不会清除其他维度。
5. 移除仅为排查添加的生产 debug 输出，保留必要的测试断言；确认 planner 继续采用单批读取，未引入 candidate N+1。

验收：两条 E2E 与 `routing_health_verdict_persistence`、`routing_dual_terminal_lifecycle` 通过，`cargo fmt --check`、`git diff --check` 通过；随后重跑 `pnpm.cmd verify:fast`，记录所有非本批失败的归属。

### Batch 1：持久化的 routing outcome 与可审计查询

目标：进程重启或 runtime ring 淘汰后，仍能读取与 canonical terminal 一致、经脱敏的版本化决策摘要。

1. 冻结 outcome/trace profile：classification、confidence、evidence source、acceptance、send phase、replay/billing、retry disposition、typed effect、failure-domain commitment 和 profile version 均为稳定闭集。
2. 设计 additive migration 及 typed read/write API，使 outcome summary 与 request/attempt terminal 同一事务写入；重复终态只接受完全相同的重放。
3. 将 canonical outcome 经 lifecycle/finalization 传入 store，禁止从 raw body、annotation 或 public error 反推。
4. 让 `get_request_decision_trace` durable summary 优先，runtime ring 仅补充当前进程的有界事件；同步 DTO、ACL、生成绑定、portable migration、schema fingerprint 与红线测试。
5. 将 metrics 收敛为有界、闭合标签的 production recorder，移除无实际 consumer 的死代码豁免。

验收：重启后 IPC 仍返回终态摘要；schema/read-write/IPC/redaction/collision 回归通过；摘要、fixture、metric label 均不含敏感或高基数数据。

### Batch 2：可信 transport send phase

目标：replay/acceptance 只依赖 transport owner 报告的单调事实。

1. 先完成 Windows transport spike，验证现有或候选 adapter 对 direct/system/HTTP/SOCKS proxy、TLS、HTTP/2、timeout 和 streaming 的兼容性及许可证。
2. 以同一 production/test reporter 报告 `NotConnected`、`ConnectedNoHeaders`、`HeadersSent`、`BodyPartiallySent`、`BodyFullySent`、`ResponseStarted`；不能证明时返回 `Unknown`。
3. 使用本地 TCP/HTTP harness 覆盖 connect、TLS、headers、partial/full body、response-started、mid-stream failure，证明 body poll 不是 socket-send 事实。
4. 将 HTTP/stream/timeout/cancellation 的最后可靠 phase 传给 canonical outcome；unknown 或可能已接受的非幂等请求保持 fail closed。

验收：`upstream.rs` 不再只产生三种 phase；生产和测试无两套 reporter；每个不支持路径有明确 `Unknown` 与 no-retry 回归。

### Batch 3：权威 capacity-domain 与一次跨域终局

前置条件：Batch 2 完成，provider/deployment/region 身份已有经确认的权威来源与 revision。

1. 将权威 domain identity 及 revision 加入 durable operational facts、target resolver、PlanningSnapshot、candidate、attempt 与 finalization commitment。
2. 由唯一 Coordinator 持有请求级 attempt、deadline、sleep、admission、domain exclusion 与 cross-domain-consumed 状态。
3. 同 target 耗尽后先由普通 planner 实施同域抑制；只在 portability 和 replay gate 均通过时选择一个不同且可重验 domain。
4. 跨域 attempt 无论结果均终止该逻辑请求的 retry 链，并在 trace/outcome 中记录闭集决策原因与 send ordinal。
5. 以 fake clock/loopback 覆盖 revision 漂移、FIFO、queue-full、cancel/shutdown、HalfOpen race、累计 sleep/deadline 和 100 并发。

验收：production composition 证明无同域 sibling outbound、最多一次可信跨域 outbound，且 Execution 没有独立预算或 domain 拼装逻辑。

### Batch 4：故障、并发、协议与恢复矩阵

目标：用确定性本地测试锁定资源释放、单终态、边界协议与恢复语义。

1. 覆盖 capacity 与 diagnostic-memory 的 100 并发：active/waiter 上限、FIFO、deadline、cancel、panic、shutdown 和 permit 回收。
2. 覆盖 gzip/deflate、压缩膨胀、JSON 限额、malformed UTF-8、任意 chunk split、SSE EOF/terminal、256 KiB event 与慢消费者背压。
3. 覆盖 pre/post-commit persistence failure、writer unavailable、outbox/checkpoint replay、duplicate/late terminal、payload collision、crash/restart 与 revision 漂移。
4. 覆盖 classifier/projector/profile mismatch、热更新只影响新请求、runtime 重启不泄漏 permit/cooldown，以及最大 body 的 retry backing storage 复用。

验收：测试不依赖真实 provider；所有计数回到基线；committed 路径永不 retry 或产生第二终态。

### Batch 5：删除旧 owner、全量资格与文档收口

前置条件：Batches 0-4 有 production 调用和回归证据。

1. 删除重复 classifier、fallback、compatibility writeback、parser owner 和仅为旧行为服务的测试 API；不得删除仍承载保守安全语义的分支。
2. 收紧 architecture/dead-code/contract gate，证明 consumer 不会按 public status/message 重新分类，且没有无界 buffer/queue、同域 sibling fallback、committed retry 或 test-only 核心合同。
3. 按顺序完成 `git diff --check`、专项 contract/architecture tests、`cargo fmt --check`、`cargo check --locked`、相关及全量 Cargo 测试、`pnpm.cmd test`、`pnpm.cmd build`、`pnpm.cmd verify:fast`、`pnpm.cmd verify:full`。
4. 将专项台账、acceptance/qualification、deletion ledger、boundary manifest、`docs/README.md` 与 release note 更新为同一 revision 的 `done with evidence`、`partial`、`pending` 或 `externally blocked` 状态。

验收：所有工程 exit gate 退出码为 0；文档与代码同 revision；真实 smoke 未授权时明确标为 `pending external authorization/evidence`。

## 依赖关系

```text
Batch 0 ──> Batch 1 ──> Batch 4 ──> Batch 5
Batch 2 ──> Batch 3 ────────────────┘
真实 provider smoke：仅在 Batch 5 后、获得明确授权时执行
```

Batch 0 和 Batch 2 可以并行准备。Batch 1 的 profile/schema 设计可与 Batch 2 并行，但 durable 写入必须基于已经稳定的 canonical outcome 合同。每批结束后更新本计划，记录实际命令、退出码、失败归属与未验证范围。
