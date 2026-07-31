# Relay Pool 路由与运行事实一体化升级实施计划

状态：Proposed，等待按 Task 顺序执行
日期：2026-07-30
目标规范：`docs/superpowers/specs/2026-07-30-routing-operational-unification-upgrade-spec.md`
上位规范：`AGENTS.md`、`docs/README.md`、`docs/PROJECT_PLAN.md`、`docs/PRODUCT_MODEL.md`、`docs/SECURITY_EXPORT_IMPORT.md`
相关冻结合同：数据架构、Pricing、Local Routing Reliability、Request Lifecycle、Architecture Scale、状态监控 V2

## 1. 目标与完成定义

本计划把当前分散的路由候选、价格、倍率、余额、能力、健康、容量、fallback、请求生命周期、请求成本、监控和 UI read model 收敛为一个模块化单体内的生产闭环：

```text
Canonical facts/evidence
  -> snapshot-consistent OperationalFactBundle
  -> pure operational projectors
  -> request-specific RouteCandidateProjection
  -> pure hierarchical RoutePlan
  -> runtime/capacity admission + real leases
  -> revision-fenced execution target
  -> protocol/delivery lifecycle
  -> AttemptOutcome / RequestOutcome
  -> atomic journal/health/capability/cost effects
  -> runtime metrics + success-only affinity
  -> backend-owned read models and integrated desktop UI
```

完成不等于“新类型可以编译”。只有同时满足以下条件才完成：

- default v2 production 只有一套 fact resolution、selector、capacity、feedback、pricing settlement 和 frontend truth；
- `PriorityFirst` 与 `CostFirst` 复用同一个 pure kernel，不回退多权重总分；
- 每个真实 upstream attempt 都持有真实 capacity lease 和预留的 lifecycle writer permit；
- capacity miss、actual attempt failure、wait wake-up 和 execution-fence rebuild 使用不同状态，不互相冒充；
- attempt/protocol 与 request/delivery 生命周期独立终结且顺序可证明；
- monitoring、collector、pricing、routing、logs 和 UI 消费同源 facts/projections；
- 预迁移检查点、正式 cutover、开发期重装恢复策略和 debug-only legacy runtime 均按目标规范执行；
- Stage 7 的 architecture、migration、fault、concurrency、performance、soak、授权真实客户端和开发期本地打包 gates 全部退出 0。

## 2. 执行纪律

1. 每个 Task 开始前运行 `git status --short --branch`，记录所有重叠 dirty paths。当前 monitoring V2 工作区改动属于用户，必须先合并/冻结，不能由本计划覆盖或回退。
2. migration 编号在执行时枚举 `src-tauri/src/persistence/migrations/` 后选择下一可用编号；不得假定本计划中的示意编号仍可用。
3. 严格 RED-GREEN-REFACTOR。行为任务必须先观察指定测试因缺失能力失败，再做最小完整实现，再运行任务回归。
4. 不使用 `git add .` 或 `git add -A`。提交时只 stage 当前 Task 的明确路径；重叠文件使用 `git add -p`，并检查 `git diff --cached --check` 和 `git diff --cached`。
5. Stage 1-4 不接 production composition，不对真实请求双 selector、双 acquire、双 feedback 或双写；验证只使用 pure fixtures、read-only diagnostics 和 loopback harness。
6. Stage 5 与 Stage 6 可以分 commit/review，但不能拆成两个对用户可见的混合版本。cutover candidate 必须同时完成 data-plane cutover、UI 语义切换和 default-v2 旧路径删除。
7. `RELAY_POOL_PROXY_RUNTIME=legacy` 只按 `PROJECT_PLAN.md` 作为 process-start 级、debug-only 的完整旧 owner；不得拼接新组件、自动 fallback、进入 UI 或掩盖 default v2 红项。
8. 所有 queue、registry、candidate set、fan-out、wait、retry、trace、background task 和 response body 都必须有硬上限和 shutdown 行为。
9. fixture、日志、trace、截图和 qualification artifact 不得包含 API key、Cookie、Authorization、完整 URL query/userinfo、完整 prompt/response 或可还原账号身份的数据。
10. 真实 provider 验证必须单独授权、低频、有预算、从环境解析 secret，不进入默认 CI，也不能把原始响应保存进仓库。
11. Task 的任一必跑命令没有真实退出 0，则 Task 保持未完成。不得以“已有类似测试”“只跑了单测”或“看起来可用”代替退出证据。
12. Sub2API、claude-code-hub 和其他外部项目只作为架构模式/行为对照；不得复制 AGPL/LGPL 核心实现。Task 0 必须记录来源、许可证、借鉴点与本项目独立实现边界。

## 3. 开发期序列与不可跨越门禁

**2026-07-31 决策更新：** 当前项目仍处于非稳定成型阶段，不要求维护公开签名预迁移版本或正式 release/rollback 交付链路。用户可接受通过重装、清空本地配置或重新导入来恢复。因此，本计划中的 release gate 降级为开发期本地 qualification gate：保留架构、schema、redaction、build、soak 和真实客户端 smoke；安装/升级脚本若保留，只作为“不写死版本/路径”的可选合同检查，不作为当前升级阻塞项。移除“必须公开发布签名 installer/tag 后才能继续下一 Task”的硬门禁。后续若项目进入稳定产品阶段，需要重新启用发布 ADR，并补回 signed installer、自动更新、升级/回滚矩阵与支持窗口要求。

```text
Stage 0 baseline + ADR freeze
  -> Stage 1 canonical facts/projectors
  -> Stage 2 backend read models + migration readiness UI
  -> PRE-MIGRATION DEVELOPMENT CHECKPOINT (旧 production router 保持原行为)
  -> Stage 3 planner/capacity kernel in non-production harness
  -> Stage 4 outcomes/cost/full loopback harness
  -> Stage 5 atomic default-v2 data-plane cutover
  -> Stage 6 integrated UI + old default-v2 path deletion
  -> ONE CUTOVER CANDIDATE for Stage 5+6
  -> Stage 7 qualification/local package
  -> later debug legacy runtime deletion ticket after required observation or explicit dev reset decision
```

以下情况必须停止推进，不得临场绕过：

- monitoring baseline 未合并或 health/target owner 未冻结；
- Request Lifecycle、Persistence V2 或 Pricing 冻结合同与本 spec 存在未决冲突；
- multiplier/ordering-profile migration 没有可操作 UI；
- lifecycle writer permit、capacity lease 或 response finalization 无法证明所有 drop/cancel 路径归零；
- production composition test 仍能到达 test-only scheduler facade；
- Stage 5/6 形成 new selector + old feedback、old selector + new lease 等混合组合；
- secret/full URL 仍可进入 candidate、trace、request log 或 IPC DTO。

## 4. Task 依赖图

```text
0 Baseline/ADRs
  -> 1 Architecture and ownership gates
  -> 2 Operational domain primitives
  -> 3 Snapshot read session and fact bundle
     -> 4 Group/multiplier/balance projectors
     -> 5 Capability evidence/projector
     -> 6 Health projection/monitor integration
     -> 7 Pricing projection/cost basis
  -> 8 Route request/progress/candidate projection
  -> 9 Backend read models and simulation preview
  -> 10 Migration readiness UI
  -> 11 Pre-migration development qualification
  -> 12 Hierarchical selector kernel
  -> 13 Decision trace persistence/retention
  -> 14 Runtime metrics/outlier/half-open
  -> 15 Composite capacity/retry/wait
  -> 16 Planner/controller/fencing
  -> 17 Execution target and credential boundary
  -> 18 Canonical failure taxonomy/provider semantics
  -> 19 Outcome/effect/cost persistence
  -> 20 Dual terminal finalization integration
  -> 21 Non-production end-to-end harness
  -> 22 Production composition cutover
  -> 23 Integrated routing workspace/deep links
  -> 24 Default-v2 legacy deletion and architecture gates
  -> 25 Security/history migration
  -> 26 Full fault/performance/soak qualification
  -> 27 Local package and reset/reinstall proof
  -> 28 Debug legacy runtime deletion follow-up (separate precondition)
```

Tasks 4-7 可以在 Task 3 后并行开发，但 Task 8 必须等四者的类型和 precedence fixtures 全部冻结。其余 Task 默认按编号顺序执行。

## 5. 目标文件地图

最终文件名可由 Task 0 ADR 按现有模块调整，但职责和依赖方向不得变化。

| 路径 | 完成后职责 |
|---|---|
| `src-tauri/src/models/operational/` | identity、endpoint ref、provenance、capability、economics、health 纯类型 |
| `src-tauri/src/application/operational_facts/` | fact reader、bundle assembler、pure projectors、target resolver ports |
| `src-tauri/src/application/routing_engine/request.rs` | immutable request facts、limits、progress view、classifier |
| `src-tauri/src/application/routing_engine/eligibility.rs` | hard eligibility 与 pool ejection guard |
| `src-tauri/src/application/routing_engine/selector.rs` | 两个 sealed lexicographic profile 的同一 pure kernel |
| `src-tauri/src/application/routing_engine/planner.rs` | immutable RoutePlan、planning evidence、无 I/O orchestration |
| `src-tauri/src/application/routing_engine/capacity.rs` | runtime admission fence、composite capacity、wait plan |
| `src-tauri/src/application/routing_engine/runtime_metrics.rs` | bounded scoped EWMA/window/cooldown/half-open registries |
| `src-tauri/src/application/routing_engine/affinity.rs` | lookup、validation、escape 和 success-only bind |
| `src-tauri/src/application/request_finalization/` | Attempt/Request outcome、effect planner、固定 orchestrator |
| `src-tauri/src/services/proxy/execution.rs` | request-local progress、attempt/fallback 编排，不解释 facts |
| `src-tauri/src/services/proxy/response_body.rs` | upstream attempt 与 downstream delivery 双终态 owner |
| `src-tauri/src/services/proxy/error.rs` | sealed planning/execution failure 到 OpenAI-compatible 映射 |
| `src-tauri/src/persistence/stores/operational_facts/` | 批量 canonical fact readers，不做 eligibility |
| `src-tauri/src/persistence/stores/routing_decisions/` | decision summary/detail、retention、分页 query |
| `src-tauri/src/application/queries/routing_workspace.rs` | durable routing workspace snapshot |
| `src-tauri/src/application/queries/routing_runtime.rs` | lightweight runtime overlay read model |
| `src-tauri/src/application/queries/operational_detail.rs` | Station Key operational detail 与按需 history |
| `src-tauri/src/application/queries/request_decision_trace.rs` | planning rounds、attempts、outcomes timeline |
| `src/features/routing/` | 综合路由工作台、模拟器、迁移 readiness、decision timeline |

## 6. Spec 覆盖矩阵

| Spec 合同 | 实施 Tasks | 主要证据 |
|---|---|---|
| 单一 canonical facts/projectors | 2-8 | projector tables、DTO completeness、snapshot isolation tests |
| capability tri-state 与 complete negative | 5, 18 | evidence precedence、404 proof fixtures |
| durable health + runtime overlay | 6, 14 | revision invalidation、max-ejection、half-open tests |
| `PriorityFirst/CostFirst` 同 kernel | 7, 12 | property/table tests、no-score architecture gate |
| real capacity/retry/wait leases | 14-16 | concurrency、rollback、cancel/drop gauge-to-zero |
| credential/endpoint late resolve | 3, 17, 25 | non-Serialize/Debug、secret scan、revision fence |
| Attempt/Request 双终态 | 19-21 | downstream-first-drop、ack ordering、cost aggregate tests |
| typed failures与 effect target | 18-22 | exhaustive Rust match、HTTP/UI contract fixtures |
| backend read models/UI 贯通 | 9, 10, 23 | no frontend truth、deep links、preview/production parity |
| legacy policy migration | 10, 11, 22 | readiness fixture、user-confirmed config、fail-closed cutover |
| default-v2 单 owner/deletion | 22-24 | production composition and source gates |
| local qualification/security | 25-28 | migration fixtures、fault/soak/live/reset-reinstall evidence |

## 7. Task 0：冻结基线、冲突清单与四份 ADR

**Files:**

- Create: `docs/superpowers/audits/2026-07-30-routing-operational-baseline.md`
- Create: `docs/superpowers/audits/routing-operational-boundary-manifest.json`
- Create: `docs/superpowers/audits/routing-operational-field-ownership.md`
- Create: `docs/superpowers/audits/routing-operational-deletion-ledger.md`
- Create: `docs/superpowers/adrs/` 下四份具名 ADR（实际编号按目录规则）
- Read only: 目标 spec、上位规范、monitoring/request-lifecycle/persistence/pricing 现状

**Steps:**

- [ ] 记录 branch、commit、`git status --short`、migration 最大编号、所有重叠 dirty hunk owner。
- [ ] 确认 monitoring V2 当前改动已经合并或形成不可变 baseline commit；否则停止 Task 1。
- [ ] 绘制当前 production 调用图：ingress -> repository -> router/scheduler -> static candidates -> attempt -> response body -> finalization。
- [ ] 记录所有 test-only production-equivalent API、simulated capacity、static fallback、secret preload、full URL persistence 和 error flattening consumer。
- [ ] 对照六份相关设计列出真实冲突，不允许只写“后续注意”。每个冲突必须有 owner、选定合同、需要同步修订的文档/测试。
- [ ] 建立外部参考记录：Sub2API、claude-code-hub、LiteLLM/Envoy/HAProxy 仅记录可验证的架构模式、算法合同、UI 交互与许可证；明确哪些不采纳，禁止移植受限核心代码。
- [ ] ADR 1：hierarchical kernel，覆盖 `PriorityFirst/CostFirst`、cost basis degradation、affinity、availability/priority/cost strata。
- [ ] ADR 2：SQLite snapshot-consistent read transaction、FactVersionVector、runtime overlay 与 rebuild/fence。
- [ ] ADR 3：RequestLease、RetryPermit、half-open permit、global/station/key CapacityLease、wait queue 和 drop owner。
- [ ] ADR 4：AttemptOutcome/RequestOutcome、writer permits、transaction/ack ordering、crash gap 和 no-outbox tradeoff。
- [ ] deletion ledger 登记旧 `routing_policy.rs` score path、scheduler weights、test-only feedback facade、static ordered fallback、frontend pricing matcher、credential-bearing candidate 和 full URL log field。
- [ ] 运行并记录基线，不修复无关红项；每个红项注明已有/本计划/外部 owner。

**Run:**

```powershell
git status --short --branch
git log -5 --oneline
Get-ChildItem src-tauri/src/persistence/migrations -File | Sort-Object Name | Select-Object -Last 10 -ExpandProperty Name
rg -n "acquired_simulated|report_result|bind_session|ordered.*candidate|upstream_base_url|RuntimeRoutingCandidate|InternalProxyError|cheap_first" src-tauri/src src scripts
cargo test --locked --manifest-path src-tauri/Cargo.toml application::routing_engine -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml services::proxy -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml request_lifecycle -- --nocapture
pnpm.cmd test:contracts
pnpm.cmd exec tsc --noEmit
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** monitoring baseline 已冻结；四份 ADR 均批准；每个跨规范冲突已有唯一结论；调用图、field ledger、deletion ledger 和基线命令可复现。

**Commit:** `docs: freeze routing operational upgrade contracts`

## 8. Task 1：建立架构、字段与删除门禁

**Files:**

- Create: `scripts/routing-operational-architecture.test.mjs`
- Create: `scripts/fixtures/routing-operational-architecture/{pass,red-*}/**`
- Modify: `scripts/run-contract-tests.mjs`
- Modify: `docs/superpowers/audits/routing-operational-boundary-manifest.json`
- Modify: `docs/superpowers/audits/routing-operational-field-ownership.md`

**RED:**

- [ ] red fixture 证明 monitoring import routing candidate DTO 会失败。
- [ ] red fixture 证明 routing kernel import SQLx/Reqwest/SecretManager/Tauri DTO 会失败。
- [ ] red fixture 证明 frontend 出现 authoritative pricing/group/capability matcher 会失败。
- [ ] red fixture 证明 credential-bearing type 实现 Serialize 或可泄露 Debug 会失败。
- [ ] red fixture 证明 production scheduler API 只在 `#[cfg(test)]` 下存在会失败。
- [ ] red fixture 证明 legacy weights 被 `hierarchical_v1` 读取会失败。
- [ ] red fixture 证明新增 boundary symbol 未登记 owner/consumer/deletion status 会失败。

**GREEN:**

- [ ] 使用现有 architecture fixture infrastructure 注册新 gate，不写只匹配当前文件行号的一次性脚本。
- [ ] manifest 定义 facts -> projector -> use case -> read model 单向依赖。
- [ ] field ledger 至少覆盖 group/rate/pricing/capability/health/capacity/affinity/cost/decision/endpoint/credential。
- [ ] gate 允许明确的临时 adapter，但要求 owner、唯一 consumer 和到期 Task。

**Run:**

```powershell
node scripts/routing-operational-architecture.test.mjs
pnpm.cmd architecture:fixtures
pnpm.cmd test:contracts
```

**Exit gate:** 所有 red bypass fixture 被拦截，pass fixture 通过；gate 不依赖用户当前 monitoring hunk 的偶然格式。

**Commit:** `test: add routing operational architecture gates`

## 9. Task 2：引入 operational 纯领域类型

**Files:**

- Create: `src-tauri/src/models/operational/mod.rs`
- Create: `src-tauri/src/models/operational/identity.rs`
- Create: `src-tauri/src/models/operational/provenance.rs`
- Create: `src-tauri/src/models/operational/capability.rs`
- Create: `src-tauri/src/models/operational/economics.rs`
- Create: `src-tauri/src/models/operational/health.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/tests/operational_domain.rs`

**RED:**

- [ ] invalid/empty IDs、negative/NaN/Infinity multiplier/money、unknown currency/unit、invalid timestamps/revisions 构造失败。
- [ ] `CapabilityVerdict` 必须区分 Supported/Unsupported/Unknown，不能从缺行默认 false。
- [ ] `EvidenceCoverage` 必须区分 Complete/Partial/Unknown。
- [ ] Key、Station account、endpoint、model health 使用不同 typed target，不能共用 `healthy: bool`。
- [ ] Endpoint facts 只接受 EndpointRef、sanitized origin、revision 和 outbound-policy ref，不接受 plaintext secret。
- [ ] `StationAccount { station_id }` 与 PRODUCT_MODEL 的 Station 对齐，不创建第二个 Account 聚合根。

**GREEN:**

- [ ] 实现 validated newtypes，不在本次升级替换现有金额存储为 decimal。
- [ ] 所有 facts/assessment 类型无 I/O、无 SQLx/Reqwest/Tauri 依赖。
- [ ] `StationKeyOperationalFacts` 只包含请求无关子事实；价格/model verdict 只出现在请求级 assessment。
- [ ] provenance 可携带 record/revision/hash/source/freshness，但不虚构全局 `snapshot_revision`。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_domain -- --nocapture
node scripts/routing-operational-architecture.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 领域类型只能表达 spec 允许的状态；不存在 `Option` 堆叠成的万能上下文或 credential/full URL 字段。

**Commit:** `feat: add operational domain primitives`

## 10. Task 3：实现 snapshot-consistent fact reader 与 bundle

**Files:**

- Create: `src-tauri/src/application/operational_facts/mod.rs`
- Create: `src-tauri/src/application/operational_facts/reader.rs`
- Create: `src-tauri/src/application/operational_facts/assembler.rs`
- Create: `src-tauri/src/persistence/stores/operational_facts/mod.rs`
- Create: `src-tauri/src/persistence/stores/operational_facts/queries.rs`
- Modify: `src-tauri/src/application/mod.rs`
- Modify: `src-tauri/src/persistence/stores/mod.rs`
- Create: `src-tauri/tests/operational_fact_reader.rs`

**RED:**

- [ ] 并发 writer 在两批 SELECT 之间提交时，reader 仍得到同一 SQLite snapshot；普通连接上的 autocommit SELECT fixture 必须失败。
- [ ] 100 candidates 的 query count 是固定上限，不随 candidate 数量线性增长。
- [ ] 单模型 query 不加载完整模型 inventory/history；`/v1/models` catalog 使用独立 query shape。
- [ ] reader 不返回 secret、encrypted blob、full URL、raw collector JSON 或 UI DTO。
- [ ] candidate 数超过 1024 返回 typed limit failure，不使用 SQL LIMIT 静默截断。

**GREEN:**

- [ ] 使用显式 read transaction 或证明等价 snapshot isolation 的现有 `ReadSession`。
- [ ] 批量读取 enabled keys、credential availability/ref revision、group/rate、pricing inputs、balance、health、capability evidence、routing config/aliases。
- [ ] 按 ID maps 装配 `OperationalFactBundle`，read transaction 在 raw bundle 完整后立即关闭。
- [ ] 生成 request-local snapshot ID 和 FactVersionVector；不持有 transaction 跨 network/wait/body lifetime。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml persistence::stores::routing_store -- --nocapture
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** isolation test 能真实区分 autocommit 与 read transaction；无 N+1；bundle 无 secret/raw JSON；read transaction 无泄漏。

**Commit:** `feat: add snapshot consistent operational fact reader`

## 11. Task 4：统一 group、multiplier 与 balance projectors

**Files:**

- Create: `src-tauri/src/application/operational_facts/group_projector.rs`
- Create: `src-tauri/src/application/operational_facts/multiplier_projector.rs`
- Create: `src-tauri/src/application/operational_facts/balance_projector.rs`
- Modify: `src-tauri/src/application/operational_facts/mod.rs`
- Create: `src-tauri/tests/operational_economics_projectors.rs`

**RED:**

- [ ] group identity 严格按 binding id -> group key hash -> group id hash -> legacy normalized name，两个 hash 不得互换。
- [ ] multiplier 严格覆盖 binding/latest、user/effective/default、manual override、disabled/stale/ambiguous 分支。
- [ ] missing/stale/untrusted multiplier fail closed，不默认 1.0。
- [ ] station-scope balance 与明确 key-scope balance 不按更新时间互相覆盖。
- [ ] Unknown/NotSupported/NotApplicable balance 不得被视为 depleted。
- [ ] 只有 authoritative、scope-matched、fresh depleted 才进入 DepletedEmergency。

**GREEN:**

- [ ] projectors 全部 pure，无 repository/service 调用。
- [ ] 每个输出包含 source chain、confidence、resolved_at、reason 和 revision refs。
- [ ] 兼容 multiplier cache 只按 field ledger 批准 fallback 读取，并登记删除条件。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors group_ -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors multiplier_ -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors balance_ -- --nocapture
node scripts/group-facts-projection.test.mjs
```

**Exit gate:** routing、pricing diagnostics 和 operational detail 对同 fixture 得到同一个 group/multiplier/balance 结论。

**Commit:** `feat: unify routing group multiplier and balance projections`

## 12. Task 5：建立 capability evidence、tri-state projector 与 adapter contract

**Files:**

- Create: `src-tauri/src/application/operational_facts/capability_projector.rs`
- Create: `src-tauri/src/services/proxy/adapters/capability.rs`，只定义 provider-neutral sealed capability signals
- Modify: `src-tauri/src/services/proxy/adapters/mod.rs`、`openai.rs`、`responses.rs`，显式实现/返回上述 signals
- Modify: Task 0 field ledger 指定的 collector model evidence writer；若 monitoring baseline 改变了 owner，先更新 ADR/ledger 再开始本 Task
- Create: `src-tauri/tests/capability_evidence.rs`
- Create: `src-tauri/tests/fixtures/capability/**`

**RED:**

- [ ] adapter 明确不支持的 protocol/feature 不被用户 allow/alias 覆盖。
- [ ] 用户 block 永远优先；scoped allow 不能越过结构协议不兼容。
- [ ] Complete inventory 缺少 model 才能产生 negative；Partial/Unknown 只能 positive/unknown。
- [ ] 429/overload/cooldown 永远不写 model unsupported。
- [ ] generic 403/404 无 adapter semantic signal 时为 Uncertain/neutral。
- [ ] 同 revision 成功请求、adapter semantic negative、collector evidence 冲突时按稳定 policy 解析，不依赖 DB 行顺序。
- [ ] strict capability policy 只把 Unknown 变 hard rejection，不改写 evidence truth。

**GREEN:**

- [ ] evidence 带 source、scope、observed_at、endpoint revision、confidence、coverage、optional expiry。
- [ ] 每个 protocol/model/feature 维度独立归约，输出 winner、overridden evidence 和 conflict reason。
- [ ] 新 evidence source 必须注册 precedence fixture，provider special case 留在 adapter。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test capability_evidence -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml services::proxy::adapters -- --nocapture
node scripts/station-key-capability-defaults.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 不存在 bool capability 默认链；404 所需 negative evidence 可以由 fixture 完整证明来源、coverage 和 freshness。

**Commit:** `feat: add canonical capability evidence projection`

## 13. Task 6：统一 durable health、runtime projection 与 monitoring 接口

**Prerequisite:** monitoring V2 baseline 已完成合并/冻结；如果 `HealthTransitionService`、target types 或 monitoring write path 仍变化，停止执行。

**Files:**

- Modify: `src-tauri/src/application/health_transitions.rs`
- Create: `src-tauri/src/application/operational_facts/health_projector.rs`
- Create: `src-tauri/src/application/operational_facts/runtime_health_port.rs`
- Modify: `src-tauri/src/application/monitoring/write_path.rs`
- Modify: `src-tauri/src/application/monitoring/orchestrator.rs` only through approved observation port
- Modify: `src-tauri/src/persistence/stores/health_observation_store.rs`
- Create: next migration for scoped health target/revision fields；migration 编号按执行时最大编号分配
- Create: `src-tauri/tests/operational_health_projection.rs`
- Modify: existing monitoring health tests without replacing their owner

**RED:**

- [ ] Key、Station account、endpoint、model observations 更新各自 target，不跨 target 污染。
- [ ] old endpoint revision observation 不覆盖新 endpoint/credential config。
- [ ] diagnostic/CLI compatibility probe 不恢复 production passive health。
- [ ] traffic-equivalent monitor/real request success 才推进 ordinary recovery；无效 credential 不因普通 monitor success 恢复。
- [ ] runtime overlay entry revision 落后 durable revision 时被忽略并 bounded cleanup。
- [ ] `PoolEjectionGuard` 只放宽 ordinary runtime suppression，不恢复 auth/user-disabled/model-unsupported/ceiling hard reject。
- [ ] monitoring 与 proxy 走同一个 transition contract，不各写 cooldown。

**GREEN:**

- [ ] 保留 `HealthTransitionService` 作为 durable facade，内部按 FailureTarget 分派窄 reducer/store。
- [ ] `HealthProjector` pure 组合 durable state + runtime overlay，输出 effective admission 和 reasons。
- [ ] `RuntimeHealthProjectionPort` 在 durable commit 后更新/清除同 revision runtime suppression；失败记录 lag，durable truth 不回滚。
- [ ] 不创建动态 event bus 或 nullable giant health row。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_health_projection -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_write_path -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_orchestrator -- --nocapture
node scripts/monitoring-architecture.test.mjs
```

**Exit gate:** 同 fixture 的 monitoring/manual/proxy observation 产生一致 durable transition；runtime stale revision 不会形成永久 cooldown。

**Commit:** `feat: unify operational health projection and transitions`

## 14. Task 7：复用 PricingProjector、定义 CostFirst basis 与 per-attempt snapshot

**Files:**

- Create: `src-tauri/src/application/operational_facts/pricing_projector.rs`
- Modify: `src-tauri/src/application/pricing.rs`
- Modify: `src-tauri/src/persistence/stores/pricing_store.rs` only for batch fact input
- Create: `src-tauri/tests/operational_pricing_projection.rs`
- Modify: existing pricing contract tests

**RED:**

- [ ] public PricingService 与 routing bundle 对相同 raw facts 产生完全相同 `ResolvedPricingContext`。
- [ ] `/v1/models` 返回 NotApplicable，不生成空 pricing context。
- [ ] fixed price 或明确 scalar comparison 可生成 exact `RoutingCostFact`。
- [ ] input/output 双单价不得直接相加；cross-currency/unit/basis 不可比较。
- [ ] `CostFirst` exact facts 缺失时只生成明确 `MultiplierProxy`，不伪装精确总价。
- [ ] exact candidates 存在时 unpriced candidates 进入后置 fallback，不被永久丢弃。
- [ ] pricing gap/unsupported billing mode 是数据状态，不 panic、不变成零成本。

**GREEN:**

- [ ] 数据库读取与 pure resolution 分开；assembler 不调用会打开第二 read session 的 PricingService。
- [ ] `RequestCostComparisonContext`、basis/source/freshness 明确。
- [ ] existing CostCalculator 继续作为唯一结算公式 owner；本 Task 不提前持久化 request cost。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_pricing_projection -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml application::pricing -- --nocapture
node scripts/pricing-facts-projection.test.mjs
node scripts/request-cost-model-pricing.test.mjs
```

**Exit gate:** UI/pricing/routing fixtures 使用同一 projector；不存在新的 input+output estimated-cost 权威公式。

**Commit:** `feat: share pricing projection with routing`

## 15. Task 8：拆分 immutable request facts、progress 与 RouteCandidateProjection

**Files:**

- Create: `src-tauri/src/application/routing_engine/request.rs`
- Create: `src-tauri/src/application/operational_facts/candidate_projector.rs`
- Modify: `src-tauri/src/application/routing_engine/mod.rs`
- Modify: `src-tauri/src/application/routing_engine/routing_types.rs`
- Create: `src-tauri/tests/route_candidate_projection.rs`
- Create: `scripts/routing-dto-completeness.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`

**RED:**

- [ ] `RouteRequestFacts` admission 后不可修改，不包含 exclusion、remaining budget 或 round clock。
- [ ] 只有 `RouteProgress` 拥有 ordinal、actual-attempt exclusions、monotonic deadline、attempt count、snapshot/runtime rebuild counts。
- [ ] planner 只取得 immutable `RouteProgressView/PlanningRoundContext`。
- [ ] OpenAI body/header 无法覆盖 local ordering/group/tag/ceiling/depleted/retry/affinity policy。
- [ ] inference 不能构造 NotApplicable；catalog request 不被 multiplier ceiling 错误阻断。
- [ ] candidate projection fixture 覆盖 group、multiplier、pricing applicability、balance、backup/preferred/tags、tri-state capability、Key/account/endpoint/model health、capacity scopes、provenance。
- [ ] 新增字段没有显式 fixture 时 completeness gate 失败，禁止 `Default::default()` 静默吞字段。

**GREEN:**

- [ ] `RouteRequestClassifier` 从 canonical request + validated local settings 构造 immutable facts。
- [ ] `RouteCandidateProjector` 只消费 facts bundle + request facts + immutable runtime overlay。
- [ ] 删除/隔离 Runtime -> Rich -> Scheduler 的新字段默认链；旧 production consumer 暂留到 cutover。
- [ ] candidate 不携带 secret、encrypted blob、真实 base URL 或 mutable registry。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test route_candidate_projection -- --nocapture
node scripts/routing-dto-completeness.test.mjs
node scripts/routing-operational-architecture.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 一次 candidate projection 包含全部 spec 字段；request facts 与 execution progress 在类型上不可混用。

**Commit:** `feat: add complete route candidate projection`

## 16. Task 9：建立 backend-owned read models 与 preview simulation

**Files:**

- Create: `src-tauri/src/application/queries/routing_workspace.rs`
- Create: `src-tauri/src/application/queries/routing_runtime.rs`
- Create: `src-tauri/src/application/queries/operational_detail.rs`
- Create: `src-tauri/src/application/queries/request_decision_trace.rs`，cutover 前仅返回 legacy summary + typed `trace_unavailable`，不得伪造 planning rounds
- Modify: `src-tauri/src/application/queries/mod.rs`
- Modify: `src-tauri/src/application/routing.rs`
- Modify: routing command facade/IPC DTO/registry/generated bindings
- Create: `src-tauri/tests/routing_read_models.rs`
- Modify: `src/lib/queries/routingQueries.ts`，作为唯一 routing query owner；不得新增第二个 `routingOperationalQueries.ts`
- Create: `src/lib/queries/routingQueries.test.ts`

**RED:**

- [ ] workspace durable snapshot 与 runtime overlay 是不同 query，不高频重读 price/history。
- [ ] candidate/history 使用 cursor pagination，无逐行 IPC fan-out。
- [ ] operational detail 按 Station Key 返回 facts/source/freshness/reason，latency/probe history lazy load。
- [ ] preview simulation 调 pure candidate/projector interface，capacity 明确 `snapshot_only`，永不记录 acquired。
- [ ] preview policy/version 与 current production policy 同时返回，cutover 前不能冒充真实 production decision。
- [ ] read-model failure 显式 unavailable，不让 frontend 重新拼权威 facts 兜底。

**GREEN:**

- [ ] 注册六个明确 read commands：`load_routing_workspace_snapshot`、`load_routing_runtime_overlay`、`list_recent_route_decisions`、`get_station_key_operational_detail`、`get_request_decision_trace`、`simulate_route`；旧 `load_local_routing_workspace` 只作为登记到 Task 24 的 compatibility adapter。
- [ ] command facade 只暴露窄 query methods，不返回 OperationalFactBundle。
- [ ] runtime overlay DTO 低基数、bounded，不包含 secret/full URL/high-cardinality metrics。
- [ ] mutation result 返回 affected entity/revision，供精确 invalidation。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_read_models -- --nocapture
pnpm.cmd generate:bindings
pnpm.cmd test -- src/lib/queries src/features/routing
pnpm.cmd exec tsc --noEmit
pnpm.cmd test:contracts
```

**Exit gate:** backend 是 group/rate/pricing/capability/health 展示真相；preview 明确不是 production acquired result。

**Commit:** `feat: add backend routing operational read models`

## 17. Task 10：删除 frontend 权威拼装并交付预迁移 readiness UI

**Files:**

- Modify: `src/features/routing/**`
- Modify/Delete: `src/lib/projections/pricingFacts.ts` 中 routing consumer
- Modify: pricing、key pool、channels、logs 页面相关窄 view model
- Create: routing migration readiness components/tests
- Modify: local routing settings form/types/generated DTO
- Modify: relevant source-contract scripts

**RED:**

- [ ] frontend 不再匹配 group identity、multiplier precedence、pricing rule 或 capability evidence。
- [ ] readiness UI 对旧五种 policy 显示拟议迁移：Priority/Stable -> PriorityFirst，Cheap/CostStable -> CostFirst；BackupOnly 必须人工选择。
- [ ] 未确认 ordering profile、multiplier ceiling、group scope、backup/depleted 和 affinity 时 readiness=false。
- [ ] readiness UI 只写完整 hierarchical_v1 config，不能部分更新造成可执行半配置。
- [ ] preview simulator 显示 `hierarchical_v1_preview`、cost basis exact/multiplier proxy/unpriced 和 snapshot-only capacity。
- [ ] 窄窗口、loading/error/empty/stale 状态无重叠且不显示零价格/假健康。

**GREEN:**

- [ ] React 只做搜索、排序、展开、表单与展示。
- [ ] 状态/价格/采集/Key 池页面先接 operational detail deep link，不在本 Task 宣称 production selector 已切换。
- [ ] migration confirmation 使用明确事务 mutation；旧配置字段保留但标 legacy ignored only after cutover。

**Run:**

```powershell
pnpm.cmd test -- src/features/routing src/features/pricing src/features/channels src/features/logs
node scripts/local-routing-automatic-settings.test.mjs
node scripts/local-routing-explanation.test.mjs
node scripts/pricing-facts-projection.test.mjs
pnpm.cmd exec tsc --noEmit
pnpm.cmd build
```

**Exit gate:** 用户可以在旧 production router 仍工作的版本中完成迁移配置；UI 没有第二套权威公式。

**Commit:** `feat: add hierarchical routing migration readiness`

## 18. Task 11：预迁移 checkpoint 与本地资格

**Files:**

- Create: `docs/superpowers/audits/routing-hierarchical-premigration-qualification.md`
- Modify: local candidate/checkpoint metadata for the authorized pre-migration checkpoint
- Optional only for future stable-release ADR: install/upgrade matrix scripts may be kept as non-blocking contract checks, but they are not part of this development gate
- Modify: `scripts/run-contract-tests.mjs`
- Modify: `src-tauri/src/application/request_finalization.rs` 与 request-log write/query DTO，停止对新 request 持久化完整 `upstream_base_url`
- Modify: `scripts/local-routing-redaction.test.mjs`
- No production selector changes

**Steps:**

- [ ] fresh schema、released schema、已有五种 legacy policy fixtures 均能启动。
- [ ] 未迁移用户继续使用旧 router 原行为；readiness 只读检查不得改变 selection。
- [ ] 已迁移 config 被完整保存，但本版本不让新 selector接真实流量。
- [ ] import/export 保留 legacy fields 并标记 ignored-after-cutover，不丢用户数据。
- [ ] 新 binary 能容忍历史 `request_logs.upstream_base_url` 为 NULL/redacted；这是 Task 25 清洗后开发期 reset/reinstall/reimport 恢复的前置合同，不再作为旧 binary rollback 合同。
- [ ] 预迁移 binary 对所有新 request 将 legacy `upstream_base_url` 写为 NULL，只保留已有 station/key identity 和安全 path/endpoint classification；UI/query 不用当前 Station URL 回填历史显示。
- [ ] 记录本地 configuration readiness 统计时只保存低基数聚合，不记录实体 ID/模型/URL。
- [ ] 冻结 checkpoint revision；开发期不要求 release tag、签名 installer 或公开更新渠道。
- [ ] 开发期不要求 install/upgrade matrix；若脚本保留，只作为稳定产品阶段的可选合同测试，且不得写死版本或 artifact 路径。
- [ ] 开发期以 fresh/known schema、readiness、redaction、import/export 和 local build 证据作为本 Task gate。
- [ ] 不发布预迁移版本；Task 12 可在本地 qualification 通过后继续。用户恢复策略为重装、清空本地数据或重新导入配置，不承诺旧 binary rollback。

**Pre-migration development freeze:**

- [ ] 先提交实现、脚本和 qualification 文档，记录 clean `premigration_revision`，后续 Task 以该 commit 作为开发检查点。
- [ ] build/install evidence 若存在写 ignored output/CI artifact；若 tracked 文件变化，形成新 commit 后重跑对应 qualification。

**Run:**

```powershell
pnpm.cmd verify:fast
pnpm.cmd verify:full
node scripts/local-routing-redaction.test.mjs
cargo build --release --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```

**Exit gate:** 本地 qualification 证明旧 production behavior 未变、用户能完成所有 cutover-required config、新 request log 不再写完整 upstream URL、fresh/known schema 与导入导出合同仍通过。开发期不要求签名发布或 installer 升级矩阵；Task 12 可在上述本地 gate 通过后开始。若重新启用稳定发布策略，必须恢复 tag、签名 bundle、install/upgrade matrix 和发布渠道证据。

**Commit:** `chore: qualify hierarchical routing premigration checkpoint`

## 19. Task 12：实现 hierarchical eligibility 与两个 sealed ordering profiles

**Files:**

- Create: `src-tauri/src/application/routing_engine/eligibility.rs`
- Create: `src-tauri/src/application/routing_engine/selector.rs`
- Create: `src-tauri/src/application/routing_engine/planner.rs` initial pure kernel
- Modify: `src-tauri/src/application/routing_engine/mod.rs`
- Create: `src-tauri/tests/hierarchical_route_planner.rs`
- Create: `src-tauri/tests/fixtures/routing_planner/**`

**RED:**

- [ ] 任意 input ordering 产生相同 strata/rank；相同 request/snapshot seed 可复现。
- [ ] hard gates 顺序与 rejection codes 固定：asset/credential、protocol、model/features、group、tag、health、runtime guard、economics、actual-attempt exclusion。
- [ ] pool ejection guard 在 candidate-local provisional rejection 后运行，只保护 ordinary runtime suppression。
- [ ] Primary -> ConfiguredBackup -> DepletedEmergency 严格分层；depleted emergency 仍满足全部 hard gates。
- [ ] `PriorityFirst`：priority -> preferred -> cost/multiplier band。
- [ ] `CostFirst`：exact tier/band -> priority -> preferred -> unpriced fallback；无 exact 时使用带 reason 的 multiplier proxy。
- [ ] CostFirst 不跨 currency/unit/basis、不使用 input+output 求和。
- [ ] PriorityFirst affinity 可在同 availability/priority 内跨软 cost band但不跨 ceiling；CostFirst affinity 不跨 5% band或提前 unpriced fallback。
- [ ] affinity hit 仍必须重新通过 hard eligibility、health/outlier、capacity 和 configured escape thresholds；不能恢复被拒候选或绕过 ready backup。
- [ ] tags 无 explicit filter 时不加权；preferred model 不提升 availability tier。
- [ ] 所有 eligible candidates 保留在 immutable RoutePlan strata；soft stage 不 destructive filter。

**GREEN:**

- [ ] 一个 selector kernel 接收 sealed profile 定义的 dimension sequence，不复制两个主循环。
- [ ] full scan 时间/空间 O(n)，durable candidate hard limit 1024。
- [ ] planner 不读取 DB/HTTP/SecretManager/registry，不修改 RouteProgress。
- [ ] output 包含 bounded decision evidence、policy/projector version、snapshot/version refs。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test hierarchical_route_planner -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml application::routing_engine -- --nocapture
node scripts/routing-operational-architecture.test.mjs
```

**Exit gate:** 两个 profiles 的 table/property tests 全绿；没有 score normalization、weighted random 或 legacy weight 读取。

**Commit:** `feat: add hierarchical routing planner kernel`

## 20. Task 13：实现 bounded decision trace schema、writer 与 retention

**Files:**

- Create: next migration for `route_decisions`/`route_candidate_decisions` only；逐 attempt cost schema 唯一归 Task 19
- Create: `src-tauri/src/persistence/stores/routing_decisions/mod.rs`
- Create: `src-tauri/src/persistence/stores/routing_decisions/write.rs`
- Create: `src-tauri/src/persistence/stores/routing_decisions/queries.rs`
- Create: `src-tauri/src/persistence/stores/routing_decisions/retention.rs`
- Modify: persistence store registry/runtime wiring in non-production test composition
- Create: `src-tauri/tests/routing_decision_store.rs`
- Create: known-schema fixtures/SQLx metadata required by repository rules

**RED:**

- [ ] summary 始终保存，candidate detail 每 round 最多 32；selected/attempted 和 primary rejection representatives 优先保留。
- [ ] truncated flag、aggregate rejection counts、ordering profile、cost basis、snapshot/version refs 完整。
- [ ] simulator 不持久化 decision。
- [ ] retention 同时执行 10,000 request decisions 与 30-day 上限，bounded batch 且不阻塞 live writer。
- [ ] cursor pagination 稳定，无 offset drift；1,000,000 candidate rows fixture 可查询。
- [ ] schema/store 拒绝 secret、full URL、payload、upstream error body 和 raw high-cardinality labels。
- [ ] crash reconciliation 只标 trace_incomplete，不生成未知 candidate detail。

**GREEN:**

- [ ] decision evidence 先保存在 bounded request memory，后续随 AttemptOutcome/RequestOutcome transaction upsert；本 Task 不添加 pre-upstream decision DB barrier。
- [ ] migration forward-only/idempotent；new binary 对缺失或旧数据给出明确 unavailable/ignored 状态，开发期不承诺旧 binary 回滚。
- [ ] maintenance 与 request-log retention 同 owner 编排。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_decision_store -- --nocapture
pnpm.cmd verify:persistence-artifacts
pnpm.cmd test:contracts
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** fresh/known schema、retention、pagination、redaction 和 million-row query fixture 全绿；尚未接 production writer。

**Commit:** `feat: add bounded routing decision persistence`

## 21. Task 14：实现 scoped runtime metrics、outlier、half-open 与 affinity registry

**Files:**

- Create: `src-tauri/src/application/routing_engine/runtime_metrics.rs`
- Create: `src-tauri/src/application/routing_engine/affinity.rs` 作为新 canonical implementation
- Keep unchanged until Task 22/24: `src-tauri/src/application/routing_engine/scheduler/affinity.rs` 仅服务旧完整 production owner，并登记 deletion ledger
- Modify: `src-tauri/src/application/routing_engine/mod.rs`
- Create: `src-tauri/tests/routing_runtime_state.rs`

**RED:**

- [ ] RuntimeMetricKey 至少包含 key + endpoint kind + bounded normalized model class。
- [ ] unknown/high-cardinality model 进入 bounded other bucket；registry LRU/TTL/max size 生效。
- [ ] endpoint/credential revision 变化后旧 cooldown/EWMA/half-open state 被忽略或清理。
- [ ] failure window max 20 samples/5m、minimum 5、threshold 60%、cooldown 30s exponential max15m。
- [ ] 429 仅使用成功解析且 clamp 到 `1s..1h` 的 `Retry-After`；缺失/非法时使用 versioned durable cooldown policy，不接受负值或无界等待。
- [ ] max passive ejection 50% 向下取整，单候选普通 outlier 只 degraded；hard reject 不受保护。
- [ ] cooldown 到期同 RuntimeMetricKey 最多一个 half-open probe permit；success/recovery count 2、failure re-eject、cancel release。
- [ ] half-open 恢复后进入 versioned 60s slow-start，选择 penalty 随 monotonic time 逐步消退；revision 变化或再次失败立即终止旧 slow-start。
- [ ] runtime feedback 对同 attempt ID apply once；restart 丢 runtime 不重复 durable effect。
- [ ] affinity lookup/bind bounded TTL；只提供 validate/bind primitive，不自行判断 request success。
- [ ] expired/group-scope/revision/model mismatch affinity 返回 typed miss 并 bounded cleanup；lookup 不延长 TTL，失败/等待不提前 rebind。

**GREEN:**

- [ ] `RuntimeRouteState` 是 registry owner；planner 只取得 immutable bounded overlay snapshot。
- [ ] outlier/window/cooldown/slow-start/max-ejection 参数集中在 validated `RuntimeOutlierPolicyV1`，trace/read model 保存 policy version；禁止散落 magic constants 或在线自调参。
- [ ] 使用 monotonic time 处理 runtime durations，wall time 只做持久时间。
- [ ] registry 由 composition root 唯一构造，TaskSupervisor/shutdown 可清理。
- [ ] 本 Task 只在非生产 harness composition 构造 `RuntimeRouteState`；default production 仍使用完整旧 owner，直到 Task 22 原子切换。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_runtime_state -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml application::routing_engine::runtime_metrics -- --nocapture
```

**Exit gate:** registry bounds、revision invalidation、max-ejection、half-open、apply-once 和 shutdown tests 全绿。

**Commit:** `feat: add bounded routing runtime state`

## 22. Task 15：实现 RequestLease、RetryPermit、composite CapacityLease 与统一 wait plan

**Files:**

- Create: `src-tauri/src/application/routing_engine/capacity.rs` 作为新 canonical composite admission implementation
- Keep unchanged until Task 22/24: `src-tauri/src/application/routing_engine/scheduler/capacity.rs` 仅服务旧完整 production owner；禁止让它 re-export 或部分调用新 lease
- Modify: `src-tauri/src/services/proxy/limits.rs`，把现有 downstream admission/body budget 明确为 `RequestLease`；复用原 semaphore/行为，不创建新的 request limit
- Create: `src-tauri/tests/routing_capacity.rs`
- Create: `src-tauri/tests/routing_capacity_faults.rs`

**RED:**

- [ ] RequestLease 统计 active downstream requests/body budget；CapacityLease.global 统计 active upstream attempts，两个 semaphore 不复用。
- [ ] fixed acquire order：optional half-open -> global -> Station/account -> Key；任一失败反向释放。
- [ ] Station account limit 为所有下属 keys 共享，不能复制为每 Key 配额。
- [ ] provider account limit 只有 scope/source/freshness 可信时启用；否则只报告 evidence gap。
- [ ] max_concurrency=0 为 unlimited；load_factor 只作 utilization denominator，不扩大 hard limit。
- [ ] CapacityLease/RetryPermit/half-open permit 不可 clone，success/error/timeout/cancel/panic unwind/drop/shutdown 后归零，无 underflow。
- [ ] RetryBudgetRegistry 使用全局 20% active/pending initial + minimum 1，不给每 request 各自 minimum。
- [ ] ordinal>0 fallback round acquire RetryPermit；未开始 attempt 的 timeout/cancel 也释放。
- [ ] 所有 eligible intents non-blocking scan 后才可 wait；只在一个 constraint 上等待；waiter queue/count/duration 有界且公平。
- [ ] sticky/fallback wait 可有不同上限，但共用 request wait budget/registry/plan。
- [ ] runtime limit 下调不取消在途 lease，只阻止新 acquire。

**GREEN:**

- [ ] 保留成熟 Tokio/Rust RAII primitive；不引入 Redis/分布式锁。
- [ ] `PlanningRoundCapacityState` 只记录 unavailable_this_pass 和 wait observations，不写 actual-attempt exclusion。
- [ ] resource gauges 和 impossible transition diagnostics 可用于 soak gate。
- [ ] 新 capacity registry 只由非生产 harness composition 构造；旧/new capacity 不共享 semaphore、counter、waiter 或 feedback。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml services::proxy::limits -- --nocapture
```

**Exit gate:** 100-way concurrency/fallback、middle-acquire failure、wait cancel 和 shutdown 后所有 permit/gauge 归零。

**Commit:** `feat: add composite routing admission leases`

## 23. Task 16：实现 planner/controller、plan progression、wait wake-up 与双 fence

**Files:**

- Modify: `src-tauri/src/application/routing_engine/planner.rs`
- Create: `src-tauri/src/application/routing_engine/controller.rs`
- Modify: `src-tauri/src/application/routing_engine/capacity.rs`
- Create: `src-tauri/tests/routing_planner_controller.rs`

**RED:**

- [ ] capacity miss 只进入 unavailable_this_pass、继续当前 plan；不增加 ordinal/retry token/journal/exclusion。
- [ ] actual upstream terminal 后 actual-attempt exclusion 单调增加，同 request 不重复 Key。
- [ ] wait wake-up 清空 pass state、刷新 overlay、创建新 planning round，允许重试尚未 attempt 的 Key。
- [ ] runtime admission generation 变化使旧 intent 失效；最多 8 次 runtime-only replan，然后 typed temporary failure。
- [ ] config/endpoint/credential fence 变化使 candidate 失效；最多一次批量 durable snapshot rebuild，无逐候选 DB recheck。
- [ ] all strata exhausted 后才 wait/capacity error；ready backup 可在 primary 无 lease 时执行。
- [ ] max attempts 包含 initial，默认 min(3, eligible)；commit certainty/idempotency/deadline 共同限制 fallback。
- [ ] PossiblyAccepted 非幂等请求没有 stable idempotency key 时不 retry。

**GREEN:**

- [ ] controller 编排 immutable plan + runtime/capacity owner，不把 I/O 塞回 pure planner。
- [ ] every planning/acquire/wait transition 写 bounded reason/evidence。
- [ ] typed failures区分 no eligible、temporary health、capacity exhausted、deadline、config unstable、candidate limit。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_planner_controller -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test hierarchical_route_planner -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --nocapture
```

**Exit gate:** static-list fallback 已在 harness 中被 plan progression/replan 替代；无 lease 绝不能产生 SelectedRoute。

**Commit:** `feat: add routing plan and admission controller`

## 24. Task 17：实现 late-bound ExecutionTargetResolver 与 secret/URL 边界

**Files:**

- Create: `src-tauri/src/application/operational_facts/target_resolver.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`，增加仅返回 refs/revisions 的新窄 port；旧 secret-bearing method 在 Task 22 前只供完整旧 owner 使用
- Modify: `src-tauri/src/persistence/stores/routing_store.rs`，增加 batch ref/availability query；不得改变旧 production method 的返回合同
- Modify: `src-tauri/src/application/credentials.rs` 与 `src-tauri/src/services/secrets/vault.rs`，只增加按 opaque ref + expected revision 单次解析 primitive
- Reuse: `src-tauri/src/models/station_endpoints.rs` 的 endpoint normalization primitive；不创建第二套 URL normalizer
- Create: `src-tauri/tests/execution_target_resolver.rs`
- Modify: `scripts/local-routing-redaction.test.mjs`

**RED:**

- [ ] fact load/selection/simulation 不读取或解密全部 credentials。
- [ ] 只有 SelectedRoute + lease 后可 resolve station key/endpoint ref/expected revision。
- [ ] revision/key-disabled/credential-changed mismatch 返回 typed stale target 并释放所有 leases。
- [ ] target handle 不实现 Serialize，Debug 只输出 IDs/revisions，不能 clone plaintext secret。
- [ ] credential handle 只活到 request build/send，不进入 response body、outcome、retry state 或 read model。
- [ ] retry/fallback 对新 route 重新 resolve，不缓存 request-scoped plaintext credential。
- [ ] URL sanitizer 处理 userinfo/query/fragment/percent-encoding/non-http scheme，解析失败 redacted。

**GREEN:**

- [ ] monitoring 使用并列 `MonitoringTargetResolver`，两者只复用 endpoint/credential primitives，不依赖彼此 DTO。
- [ ] 新 `OperationalExecutionTargetRepository` 返回 refs/availability，不预加载 secret-bearing candidates；Task 17-21 只由 non-production harness 消费。
- [ ] 旧 production repository adapter 保持完整隔离且登记 Task 24 删除；不得出现 old selector + new resolver 或 new selector + old secret preload 的混合 composition。
- [ ] source/security gate 禁止 full URL/credential 进入 DTO/log/trace。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test execution_target_resolver -- --nocapture
node scripts/local-routing-redaction.test.mjs
node scripts/routing-operational-architecture.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 新 facts/planner/harness path 的 secret 只在选中并持有 lease 后短暂解析；全仓 scan 只允许 deletion ledger 中现有完整 legacy production adapter 继续预加载，且没有新增 consumer。该例外在 Task 22 切换、Task 24 删除。

**Commit:** `refactor: resolve routing credentials after selection`

## 25. Task 18：建立 canonical FailureTarget/Class/Effect 与 sealed public error mapping

**Files:**

- Replace atomically: existing `src-tauri/src/application/request_finalization.rs` -> `src-tauri/src/application/request_finalization/mod.rs`，原 `RequestFinalizationService` API/`RequestLifecycleStore` 实现先原样迁入
- Create: `src-tauri/src/application/request_finalization/failure.rs`
- Create: `src-tauri/src/application/request_finalization/effect_planner.rs` initial pure layer
- Modify: `src-tauri/src/services/proxy/error.rs`
- Modify: `src-tauri/src/services/proxy/adapters/openai.rs`、`responses.rs` 与 `endpoint_adapter.rs` 的 sealed error semantic mappings
- Modify: `src-tauri/src/application/routing_engine/routing_failure.rs`
- Create: `src-tauri/tests/routing_failure_contract.rs`
- Create: `scripts/routing-error-contract.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`

**RED:**

- [ ] FailureTarget 精确覆盖 Request、ModelOnKey、StationKeyCredential、StationAccount、StationEndpoint(revision)、ProviderProtocol、LocalAdapter、Downstream、Uncertain。
- [ ] FailureClass、RetryDisposition、HealthEffect、CapabilityEffect 分离，不能从 string 重推导。
- [ ] generic 403/404 -> Uncertain/neutral；adapter-confirmed auth/model-not-found 才进入对应 target/effect。
- [ ] CapabilityApplicabilitySet 不被 health/capacity/economics 缩小；unknown/positive/load gap 阻止 model 404。
- [ ] user group/tag/block 空池返回 policy rejected，不伪装 provider 404。
- [ ] sealed RoutePlanningFailure 每个 variant 有 exhaustive HTTP/error code/UI fixture；新增 variant 无 catch-all 时编译/contract 失败。
- [ ] config required、economics、health、capacity、candidate/catalog limit、facts、config churn、lifecycle、deadline、invariant code 稳定。

**GREEN:**

- [ ] 单文件转目录必须在同一 commit 完成；`application::request_finalization::RequestFinalizationService` 路径和现有 production behavior 不变，避免 Rust 同名 file/module 冲突。
- [ ] provider-specific parser 只返回 sealed typed semantic signal，不传任意 body/string 给 health writer。
- [ ] OpenAI-compatible body只返回 stable code/message/correlation ID；内部 detail 只入脱敏 trace。
- [ ] 删除 planner -> string -> InternalProxyError 路径的非生产 harness consumer；production consumer 留待 Task 22 原子切换。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract -- --nocapture
node scripts/routing-error-contract.test.mjs
pnpm.cmd test:contracts
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 新 path 的 failure target/effect/public error 三层均 exhaustive，没有通用字符串分类或“unknown=500”兜底；旧 production mapper 是 deletion ledger 中唯一临时例外，Task 22 切换、Task 24 删除。

**Commit:** `feat: add typed routing failure and effect contracts`

## 26. Task 19：实现 AttemptOutcome、RequestOutcome、EffectPlan 与逐 attempt 成本事务

**Depends on:** Tasks 13-18；Task 7 的 frozen pricing context 已稳定。

**Files:**

- Create: `src-tauri/src/application/request_finalization/outcome.rs`
- Complete: `src-tauri/src/application/request_finalization/effect_planner.rs`
- Create: `src-tauri/src/application/request_finalization/outcome_orchestrator.rs`
- Create: `src-tauri/src/application/request_finalization/reconciliation.rs`
- Modify: `src-tauri/src/application/request_finalization/mod.rs`，扩展现有 `RequestFinalizationService`，不创建第二 service/writer
- Modify: `src-tauri/src/application/request_lifecycle/ports.rs` 与 `src-tauri/src/services/proxy/lifecycle/writer.rs`，扩展现有 permit/ack command contract
- Create: next migration for attempt journal、cost snapshot、request aggregate and uniqueness/CAS
- Modify: request-log query/compatibility projection stores
- Create: `src-tauri/tests/routing_outcome_domain.rs`
- Create: `src-tauri/tests/routing_outcome_persistence.rs`
- Create: `src-tauri/tests/routing_lifecycle_reconciliation.rs`
- Create: known-schema fixtures and SQLx metadata required by repository rules

**RED:**

- [ ] `AttemptOutcome` 与 `RequestOutcome` immutable、non-secret、无 lease/URL/plaintext credential，且 upstream protocol 与 downstream delivery 字段不能互相代替。
- [ ] 一个 `attempt_id` 的 duplicate/replayed finalization 只产生一个 journal、一个 cost snapshot 和一组 scoped effects；ack 明确区分 `inserted`/`already_exists`。
- [ ] `AttemptEffectPlanner` 只消费 typed classified outcome，输出显式 `EffectPlan`；store 不解析 HTTP/body/string，不重新决定 target/class/effect。
- [ ] attempt transaction 原子提交 journal、decision evidence、scoped health/capability observations 与 per-attempt cost；任一 SQL fault 不留下部分数据。
- [ ] target revision 已变化时 journal/cost 仍提交，旧 health/capability effect 记录为 `stale_target_ignored`，不能污染新 revision。
- [ ] 每个 SelectedRoute 冻结自己的 pricing assessment；A 失败后 B 成功时两份 cost context、usage 和 status 互不覆盖。
- [ ] missing usage、stream usage missing、unpriced、pricing incomplete、mixed currency 是显式数据状态；不得把 unknown 计为零。
- [ ] request aggregate 只从已持久化 attempt costs 计算，按币种聚合 fallback attempts，不扫描当前价格、不 double count。
- [ ] request aggregate transaction 在所有 started attempt durable ack 之前必须失败；单币种 compatibility 字段仅在恰好一种币种时投影。
- [ ] decision evidence 超过 32 rows 时按 Task 13 规则截断；无候选 request 可随 RequestOutcome 保存 summary。
- [ ] writer transient fault 有界重试；permanent fault 标记 writer unhealthy 并停止新 admission，不能吞掉已接受 job。
- [ ] startup reconciliation 在没有 active proxy producer 时，将上次进程遗留的 durable admitted/in-progress request 标记 `interrupted/trace_incomplete`；若已有 durable attempt-start marker 则只标其 incomplete，没有 marker 时不猜测 attempt ID/detail。
- [ ] 不为改善 crash 可见性新增同步 `StartAttempt`/decision pre-upstream barrier；未知 crash gap 不伪造 AttemptOutcome、usage、cost、health 或 candidate detail。
- [ ] reconciliation 使用 CAS/唯一键、bounded batch 和 durable progress，重复启动幂等；SQLite fault 阻止 proxy ready/admission。

**GREEN:**

- [ ] 复用 Request Lifecycle 的 Start/Finish writer、permit reservation、bounded queue、ack 与 shutdown owner；不创建第二个 routing writer。
- [ ] 旧 production lifecycle command 在 Task 22 前继续走兼容 transaction；新 `AttemptOutcome`/`RequestOutcome` command 只由 harness composition 提交，二者共享同一 writer capacity/health gate 但绝不双写同一 terminal。
- [ ] 显式 orchestrator 固定调用 journal、scoped observation、cost 和 decision store ports；不实现动态 handler/event bus。
- [ ] transaction schema 使用稳定 unique key 和 CAS；migration forward-only、幂等；new binary 对旧数据/缺失列有明确 projection fallback，开发期不承诺旧 binary 回滚。
- [ ] runtime feedback 仍由 AttemptLifecycle 唯一 owner apply once；durable transaction 不反向操作 runtime registry。
- [ ] `/v1/models` outcome 的 pricing/cost sealed 为 `NotApplicable`，不进入 inference cost aggregation。
- [ ] reconciliation 由 `RequestFinalizationService` 的窄 startup method 拥有，不另建后台 writer；完成后返回明确 ack 供 composition gate 使用。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_domain -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_persistence -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_lifecycle_reconciliation -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_persistence -- --nocapture
pnpm.cmd verify:persistence-artifacts
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** transaction fault matrix、duplicate replay、stale revision、fallback cost 与 mixed-currency fixtures 全绿；production execution 尚未调用新 orchestrator。

**Commit:** `feat: persist typed routing outcomes and attempt costs`

## 27. Task 20：拆分 upstream attempt 与 downstream request 双终态

**Depends on:** Task 19；现有 Request Lifecycle tests 仍全绿。

**Files:**

- Modify: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Create: `src-tauri/src/services/proxy/attempt.rs`
- Modify: `src-tauri/src/application/request_lifecycle/` narrow contracts only
- Modify: `src-tauri/src/application/request_finalization/outcome_orchestrator.rs`
- Create: `src-tauri/tests/routing_dual_terminal_lifecycle.rs`
- Create: `src-tauri/tests/routing_stream_finalization_faults.rs`
- Extend: `src-tauri/tests/proxy_protocol_contracts.rs`

**RED:**

- [ ] 2xx、headers、first chunk、upstream EOF、downstream EOF/drop 是不同事件；只有协议机 terminal 才能形成 AttemptOutcome。
- [ ] `UpstreamAttemptFinalizationLease` 与 downstream `RequestLease`/request finalization lease 类型分离，均不可 clone，并有各自唯一 owner。
- [ ] buffered success、stream success、malformed EOF、idle timeout、upstream reset、downstream slow/drop、explicit cancel、panic unwind 的事件顺序可断言。
- [ ] downstream 先 drop 时：先观察 delivery terminal，取消/耗尽 upstream，提交所有 started AttemptOutcome 并等待 ack，冻结 aggregate 后才提交 RequestOutcome。
- [ ] pre-commit upstream failure 可进入 fallback；post-commit failure 和 `PossiblyAccepted` 非幂等请求无 stable idempotency key 时不可 retry。
- [ ] attempt terminal 释放 CapacityLease、RetryPermit、half-open permit；request terminal 才释放 RequestLease/body budget，二者不提前也不重复。
- [ ] retry round 必须等待上一 attempt durable ack，再刷新 overlay、推进 actual-attempt exclusion 和重新规划。
- [ ] runtime feedback 对 attempt ID apply once；success affinity 只在 selected attempt 与 RequestOutcome durable success 后 bind。
- [ ] writer permit/ack 不可得时不发 upstream；send 前已获得的 target/capacity/retry/half-open leases 全部释放。
- [ ] local auth 成功后先取得 RequestLease，并在任何 upstream 前预留 StartRequest/FinishRequest permits、等待 StartRequest durable ack；任一失败都不产生 upstream call。
- [ ] shutdown 时 active stream 和 pending outcome job 有界 drain；强制超时只留下 interrupted/trace_incomplete，不伪造成功或 usage。

**GREEN:**

- [ ] 在现有 response-body wrapper 内拆 lease，不另写一套 response streaming adapter。
- [ ] Task 20 只增加由显式 composition dependency 选择的 dual-terminal path，loopback harness 选择新 path；default production constructor 继续选择完整旧 finalizer，直到 Task 22 原子切换。
- [ ] 临时 old/new finalizer adapter 必须进入 deletion ledger，不能用 `#[cfg(test)]` 隐藏，也不能对同一 request 同时激活。
- [ ] `execution.rs` 只推进 request-local progress、attempt/fallback 与 typed observation，不计算价格、不写健康、不解析 provider 错误字符串。
- [ ] outcome coordinator 使用显式 ack barrier，不能只依赖 channel send 顺序假设事务已提交。
- [ ] lifecycle diagnostics 记录 attempt/request state、permit/lease gauges 和 sanitized correlation IDs。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_dual_terminal_lifecycle -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_stream_finalization_faults -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test proxy_protocol_contracts -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_faults -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_concurrency -- --nocapture
```

**Exit gate:** 所有 terminal、cancel、drop、panic 和 writer fault 路径的 lease/permit/gauge 归零；request ack 绝不越过 started attempt ack；default production 仍完整走旧 finalizer，未形成混合 data-plane。

**Commit:** `refactor: separate upstream and downstream finalization`

## 28. Task 21：建立完整非生产 loopback 端到端 harness

**Depends on:** Tasks 12-20；仍不得接 default production composition。

**Files:**

- Create: `src-tauri/tests/support/routing_loopback/`
- Create: `src-tauri/tests/routing_loopback_e2e.rs`
- Create: `src-tauri/tests/routing_catalog_loopback.rs`
- Create: `src-tauri/tests/routing_policy_field_e2e.rs`
- Create: `scripts/routing-operational-loopback-contract.test.mjs`
- Create: `scripts/run-routing-operational-soak.ps1` initial deterministic loopback runner
- Modify: `scripts/run-contract-tests.mjs`

**RED / scenarios:**

- [ ] 从 SQLite fixtures 真实装配 facts，经 projection、planner、lease、late target resolve、loopback HTTP、protocol、outcome transaction 到 read model；不得 mock 掉中间 owner。
- [ ] A auth fail -> B success、A 429 -> B success、A endpoint 5xx -> B success、A capacity miss -> 同 plan B、wait wake-up -> 未 attempt A 可重试。
- [ ] `PriorityFirst`/`CostFirst` 的 simulation 与执行 trace 对相同 snapshot/profile/basis 一致；production-only slot 结果只由真实 lease 写入。
- [ ] `ordering_profile`、`only_use_as_backup`、`preferred_models`、`routing_tags`、`allow_depleted_fallback`、account shared concurrency、model alias 均有正/负/边界 E2E。
- [ ] inference missing multiplier ceiling fail closed；`/v1/models` 使用独立 `NotApplicable` admission，不错误触发该门禁。
- [ ] `/v1/models` 最多 64 eligible candidates、8-way fan-out；网络调用前整批预留 FinishAttempt permits，预留失败时零 upstream calls。
- [ ] `/v1/models` 至少一个 upstream 成功时返回稳定去重的 partial catalog，并在 trace 保留其他 attempt failures；全部失败返回 typed aggregate failure，不把 partial/all failure 映射成模型不存在。
- [ ] generic 403/404 保持 Uncertain/neutral；仅 adapter-confirmed auth/model-not-found 产生 scoped effect，只有 complete applicability negative 返回本地 404。
- [ ] response streaming、downstream drop、deadline、config/runtime fence churn、writer transient/permanent failure 均留下正确 journal/trace/error mapping。
- [ ] failed/fallback attempt 携带 usage 时计费；missing usage 不伪造；request aggregate 与 attempt rows 核对一致。
- [ ] loopback recorder 断言请求未包含错误 Key、trace/diagnostic 不含 Authorization/URL query/payload。

**GREEN:**

- [ ] harness 使用 deterministic clock/seed、临时 SQLite、受控 loopback server 和真实 composition factory 的非生产入口。
- [ ] 禁止为 harness 增加 production `#[cfg(test)]` facade；测试只替换 clock、transport、secret handle 和 persistence path 等窄 ports。
- [ ] 每个 scenario 结束核对 active requests/attempts/leases/waiters/writer jobs/tasks 全部为零。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_catalog_loopback -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_policy_field_e2e -- --nocapture
node scripts/routing-operational-loopback-contract.test.mjs
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -DurationMinutes 5
pnpm.cmd test:contracts
```

**Exit gate:** 从 facts 到 durable outcomes/read model 的非生产完整闭环全绿，且 production composition 尚未切换。

**Commit:** `test: prove routing operational loopback lifecycle`

## 29. Task 22：原子切换 default-v2 production composition

**Depends on:** Task 21 全绿；预迁移版本已发布且本机 readiness 没有未确认 blocker。

**Files:**

- Modify: `src-tauri/src/app_composition.rs` 与 `src-tauri/src/application/app_services.rs`，构造唯一 default-v2 operational/routing/finalization owner
- Modify: `src-tauri/src/runtime_composition.rs`，注册 ready-service、startup reconciliation、TaskSupervisor 和 shutdown drain 顺序
- Modify: `src-tauri/src/services/proxy/startup.rs`、`runtime.rs`、`limits.rs`，原子切换 admission/execution/finalization composition
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: routing/query/finalization composition ports
- Create: `src-tauri/tests/routing_production_composition.rs`
- Create: `src-tauri/tests/routing_production_startup_shutdown.rs`
- Modify: `scripts/local-proxy-v2-boundary.test.mjs`
- Modify: `scripts/manual-proxy-default.test.mjs`
- Modify: boundary manifest/deletion ledger

**Pre-cutover checklist:**

- [ ] 对所有本地 profiles 执行 readiness scan；缺少 ordering profile、可信 multiplier ceiling 或 required migration confirmation 时停止，不能静默猜测。
- [ ] 保存 cutover 前 `verify:full`、known-schema、loopback、redaction 和 reset/reimport compatibility 证据。
- [ ] 审核 composition diff，确认 selector、capacity、writer、feedback、pricing、target resolver 与 shutdown 是一个完整 owner 组合。

**RED:**

- [ ] default-v2 production test 从真实 ingress 到 loopback upstream，证明只经过新 fact reader/projectors/planner/controller/outcome orchestrator。
- [ ] 每个 SelectedRoute 都有真实 composite lease、reserved FinishAttempt permit 和 revision-fenced target；任何缺失都 fail closed。
- [ ] production build 不能引用 test-only scheduler facade、simulated acquire、static ordered candidate fallback 或 preload-secret candidate。
- [ ] default-v2 每个 terminal 只提交新 typed outcome command；旧 `finish_attempt`/`finish_request` compatibility mapping 只允许 isolated debug legacy owner 调用，不能与新 effect/cost transaction 双写。
- [ ] lifecycle writer unhealthy、fact reader unavailable、config required、candidate limit 和 startup invariant 都阻止新 upstream admission。
- [ ] persistence ready 后先完成 request lifecycle reconciliation，再发布 proxy ready/start admission；reconciliation 失败时 UI 可显示 typed startup failure，但不能启动本地转发。
- [ ] process-start debug legacy runtime 若仍保留，构造完整旧 composition；它与 default-v2 无共享 selector/lease/feedback/write path，也不是请求级自动 fallback。
- [ ] shutdown 严格执行：stop admission -> stop monitor/collector schedule -> wake/cancel waiters -> drain attempts/runs -> release leases -> close producers -> drain writers -> close persistence。
- [ ] Stage 5 transaction/writer blocker 不自动回到 legacy 或双写。

**GREEN:**

- [ ] composition root 唯一构造 fact/runtime/capacity/finalization registries 并交给 `TaskSupervisor`；planner 仍只看 immutable input。
- [ ] 切换与 legacy compatibility config 保留分开；开发期恢复依赖 reset/reinstall/reimport，而不是同 binary 混合 owner。若未来进入稳定产品阶段，再用独立发布 ADR 恢复完整 rollback 合同。
- [ ] 本 Task 可以合并到 cutover-candidate branch，但在 Tasks 23-26 完成前不得交付为默认可用路径。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_composition -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_startup_shutdown -- --nocapture
node scripts/local-proxy-v2-boundary.test.mjs
node scripts/manual-proxy-default.test.mjs
pnpm.cmd verify:full
```

**Exit gate:** default-v2 只有一个完整 production owner；未满足 readiness 的安装明确拒绝新 admission；尚未发布正式版本。

**Commit:** `feat: cut over default v2 routing composition`

## 30. Task 23：交付综合路由工作台、deep links 与 decision timeline

**Depends on:** Tasks 9、10、22；只消费后端 read models。

**Files:**

- Modify: `src/features/routing/RoutingPage.tsx`
- Create/Modify: `src/features/routing/` workspace、candidate table、detail、simulator、timeline components/view models
- Modify: `src/lib/api/routing.ts`
- Modify: `src/lib/queries/routingQueries.ts`
- Modify: Rust IPC DTO modules/registry fixtures and regenerate `src/lib/bridge/generated.ts`；`src/lib/types/routing.ts` 只保留 UI-local/domain aliases，不重复声明 transport DTO
- Modify: routing links in channels、pricing、collectors、stations/key pool、logs and change center
- Create: focused Vitest tests for workspace/read-model/deep-link behavior
- Create: `scripts/routing-workspace-integration.test.mjs`
- Modify: `scripts/local-routing-page-layout.test.mjs`

**RED:**

- [ ] durable workspace snapshot 与 1-second lightweight runtime overlay 使用独立 query/cache key；overlay 不刷新价格或历史表。
- [ ] candidate table 显示 group、multiplier/price basis、capability evidence、Key health、endpoint health、in-flight/max、last dispatch，不把不同作用域合成一个状态。
- [ ] detail 显示 source/revision/freshness/rejection impact；history 延迟加载且 cursor pagination，无逐行 IPC fan-out。
- [ ] simulator 调用同一后端 planner，明确 `snapshot_only` capacity 与 policy/projector version；UI 不计算 eligibility/rank/cost。
- [ ] request decision timeline 区分 planning round、slot/wait、attempt protocol、fallback、downstream delivery 与 cost aggregate。
- [ ] monitoring“查看路由影响”、pricing“模拟此模型”、collector evidence、Key pool eligibility、request log、station endpoint health 均能 deep link 到稳定 entity/request scope。
- [ ] mutation 返回 revision 后只 invalidation 受影响 scope；页面卸载仅取消订阅，不取消 monitor/collector 权威任务。
- [ ] loading/error/empty/stale 状态不显示假健康、零价格或旧拼装兜底；typed route error 有稳定文案和 tooltip。
- [ ] 1024 candidates、窄窗口和长本地名称下表格/详情/按钮无重叠，保持浅色紧凑桌面工具风。

**GREEN:**

- [ ] React 只做搜索、排序、展开、格式化和 navigation，不导入 pricing/group/capability projector。
- [ ] 复用现有 query client、typed operation/status channel、badge/tooltip/table 组件；不创建跨页面 mutable singleton。
- [ ] 变更中心只接 material projection transition 的聚合摘要；runtime sample 不产生事件洪流。

**Run:**

```powershell
pnpm.cmd generate:bindings
pnpm.cmd test -- src/features/routing src/lib/api/routing.test.ts
node scripts/routing-workspace-integration.test.mjs
node scripts/local-routing-page-layout.test.mjs
pnpm.cmd architecture:typescript
pnpm.cmd lint
pnpm.cmd build
```

**Manual verification:**

- [ ] 在 Tauri dev 构建中走通 monitoring -> Key detail -> route simulation -> decision trace -> request log 的往返导航。
- [ ] 检查 1280x800、1024x768 和最小支持窗口；确认表格、drawer、tooltip、错误和长文本无覆盖。
- [ ] 使用脱敏 fixture 截图，不录入真实站点 URL、Key 或请求正文。

**Exit gate:** 状态、价格、采集、Key 池、路由和日志在一个工作流中贯通；所有权威语义来自后端 projection/read model。

**Commit:** `feat: integrate routing operational workspace`

## 31. Task 24：删除 default-v2 旧 selector、static fallback、frontend matcher 与临时 facade

**Depends on:** Tasks 22、23；同一个 cutover candidate 已完成稳定观察窗口，deletion ledger 获得批准。

**Files:**

- Delete/modify: legacy/default-v2 scheduler selector、weights、simulated capacity and static fallback paths identified by Task 0
- Move only if observation gate still requires it: complete old scheduler/runtime implementation into `src-tauri/src/services/proxy/legacy_runtime/` with a single process-start composition entry; otherwise delete directly
- Delete/modify: test-only production-equivalent facade in `src-tauri/src/application/routing_engine/scheduler/`
- Delete/modify: secret-bearing candidate conversion and duplicate pricing/group/capability transforms
- Delete/modify: `src/lib/projections/pricingFacts.ts` and frontend authoritative matchers after consumers migrate
- Modify: architecture boundary manifest and deletion ledger
- Create/extend: `scripts/routing-single-owner.test.mjs`
- Modify: `scripts/routing-operational-architecture.test.mjs`
- Modify: Rust production-composition/architecture tests

**Pre-deletion cutover-candidate gate:**

- [ ] 从 Tasks 22-23 的同一 commit 构建未公开的内部 cutover candidate；它不是独立正式版本，也不允许用户在 old/new owner 间切换。
- [ ] 使用 Task 21 runner 完成一次至少 1 小时的代表性 loopback soak，并运行 production composition、stream/drop、writer fault 和 redaction tests。
- [ ] 观察窗口内 lease/waiter/writer/task gauges 归零，无未决 P0/P1；结果写入 deletion ledger approval。
- [ ] 删除完成后仍必须按 Task 26 对最终代码重新执行完整 1 小时 soak；删除前结果不能替代最终资格。

**Run before deletion:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_composition -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_stream_finalization_faults -- --nocapture
node scripts/local-routing-redaction.test.mjs
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -DurationMinutes 60
```

**Deletion checklist:**

- [ ] 删除 default-v2 第二套 selector/score/weighted random/legacy weights reader；保留的 compatibility config 只能供 isolated debug owner 或未来稳定发布 ADR 明确要求的兼容检查使用。
- [ ] 删除 default-v2 旧 `scheduler/{affinity,metrics,capacity,scoring,selection}` runtime 及其 feedback/bind facade；若观察期仍需 debug legacy，则整套原样移动到 `services/proxy/legacy_runtime/`，只允许 process-start legacy composition import，并登记 Task 28 删除。
- [ ] 删除 acquire 后立即 release 的 simulated capacity 与 `slot unavailable -> ordered candidate` 语义。
- [ ] 删除 proxy 对静态 candidate IDs 的遍历 fallback；所有 fallback 由 controller replan 驱动。
- [ ] 删除 candidate 构造阶段全量 credential 解密、full endpoint URL 携带和 routing DTO 被 monitoring 复用的路径。
- [ ] 删除 duplicate group/multiplier/pricing/capability resolver 和 frontend authoritative matcher。
- [ ] 删除 planner failure -> string -> internal 500 与 arbitrary error-body health classification。
- [ ] 删除已迁移 IPC/read-model adapter、test-only facade、unused fields/imports/feature flags；每项在 ledger 标记 commit 与 gate。
- [ ] compatibility cache 若因 debug 观察或未来稳定发布 ADR 暂留，必须有只读 owner、expiry condition 和独立删除票据，不能参与 default-v2 truth。

**Architecture gates:**

- [ ] source scan 证明 production composition 只构造一套 selector、capacity、feedback、pricing settlement 和 target resolver。
- [ ] forbidden dependency gate：monitoring 不 import routing candidate；planner 不 import SQL/HTTP/registry/SecretManager；frontend 不 import authority projectors。
- [ ] forbidden symbol/path gate 对被删除 facade、weights、static fallback 和 simulated acquire 建立失败 fixture，防止回流。
- [ ] `#[cfg(test)]` 不能定义 production-equivalent routing API；test factory 只能替换窄 ports。
- [ ] dead-code allow、temporary manifest exception 和 deletion ledger 项均归零或带具体后续 owner/date。

**Run:**

```powershell
node scripts/routing-single-owner.test.mjs
node scripts/routing-operational-architecture.test.mjs
node scripts/local-proxy-v2-boundary.test.mjs
pnpm.cmd architecture:typescript
pnpm.cmd test:contracts
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_composition -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -DurationMinutes 60
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** default-v2 old paths 物理删除且有反回流门禁；剩余 debug legacy runtime 是完整隔离 owner，并由 Task 28 单独治理。

**Commit:** `refactor: remove default v2 routing legacy paths`

## 32. Task 25：迁移历史 full upstream URL 并完成安全验证

**Depends on:** Task 11 已停止 request-log 新写 full URL，Task 17 已统一在线 sanitizer/target boundary，Task 24 已删除旧写入路径。

**Files:**

- Create: next additive migration，增加 sanitized endpoint metadata、sanitization status 与 resumable progress state；SQL migration 不解析 URL
- Create: `src-tauri/src/persistence/maintenance/mod.rs`
- Create: `src-tauri/src/persistence/maintenance/request_log_url_sanitizer.rs`，由 persistence upgrade coordinator 以固定 batch 执行结构化解析和原字段清空
- Modify: `src-tauri/src/persistence/mod.rs`
- Modify: persistence upgrade/ready-service composition，使 sanitizer 完成前 request-log query/export 与 proxy admission 均不发布 ready；UI 只显示既有 upgrade progress
- Modify: `src-tauri/src/persistence/stores/request_log_store.rs`
- Modify: request lifecycle/log DTOs and query projections
- Create: `src-tauri/tests/routing_url_sanitizer_migration.rs`
- Create: `src-tauri/tests/routing_security_boundaries.rs`
- Extend: `scripts/local-routing-redaction.test.mjs`
- Modify: known-schema fixtures and persistence artifact manifest

**RED:**

- [ ] sanitizer 使用结构化 URL parser，覆盖 userinfo、query、fragment、percent encoding、IPv6、non-http scheme、invalid UTF-8/parse failure。
- [ ] 新 authoritative 字段只保留允许的 scheme/host classification、endpoint ref/revision 和安全 path template；不复制 userinfo/query/fragment。
- [ ] 解析失败宁可置空并记录 `redacted_unparseable`，不能把原字符串转存到 fallback/detail/error。
- [ ] migration/maintenance job bounded batch、幂等、可中断续跑、有 cursor/progress/error count，SQLite busy 时有界退避。
- [ ] 每个 batch 在一个 transaction 内写 sanitized projection/status 并把对应原字段置 NULL；crash 前整批回滚、crash 后按 durable cursor 幂等续跑，不保存含原 URL 的额外 backup/journal。
- [ ] “可恢复”严格指 crash/resume，不指把敏感原 URL 反向恢复；不得在日志、升级 journal 或 error 中打印 before/after raw value。
- [ ] 全部 logical rows 完成后执行受控 WAL checkpoint/truncate 与 SQLite rebuild/`VACUUM`，清理 free pages；关闭相关 producers 后再处理 `-wal`/`-shm` sidecars。
- [ ] 枚举并清理应用管理范围内由本次升级产生的 pre-sanitization backup/temp artifact；路径必须由 persistence artifact policy 验证，不能递归删除用户目录或外部备份。
- [ ] canary fixture 对主 DB、WAL、SHM、upgrade journal 和应用管理 backup 做原始字节扫描；发布证据明确这只是应用管理存储范围内的 best-effort purge，不宣称 SSD/文件系统级不可恢复。
- [ ] fresh schema、known schema 和中断恢复 fixture 均可升级；开发期不承诺旧 binary rollback，清洗后的恢复路径是 new binary reset/reinstall/reimport。
- [ ] source/runtime scan 证明 candidate、outcome、decision、IPC、UI、trace、error、snapshot 和 qualification artifact 不含 secret/full URL/user payload。
- [ ] redaction 对 Authorization、API key、Cookie、token、完整 headers 和 prompt/response 同样生效。

**GREEN:**

- [ ] sanitizer primitive 被 execution target、request log 和 diagnostics 共同复用，但 migration owner 与在线写路径分离。
- [ ] upgrade coordinator 是唯一 sanitizer lifecycle owner；不使用 UI-mounted task、后台无限循环或普通 SQL migration callback 承担清洗。
- [ ] rebuild/checkpoint 阶段复用 Persistence V2 的 artifact publish、path validation 和 crash-recovery primitive，不手写第二套数据库替换协议。
- [ ] migration 只清洗历史数据，不回填当前价格、能力或健康，不把 legacy estimate 冒充权威事实。
- [ ] 安全测试使用 canary secrets 并只断言其不存在；测试失败输出也必须脱敏。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_url_sanitizer_migration -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_security_boundaries -- --nocapture
node scripts/local-routing-redaction.test.mjs
pnpm.cmd verify:persistence-artifacts
pnpm.cmd architecture:security
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 所有新写入和历史 rows 均符合 URL/secret boundary；migration 可恢复、可审计且无原文泄漏。

**Commit:** `security: sanitize persisted upstream routing metadata`

## 33. Task 26：执行 fault、并发、性能与 soak 本地资格

**Depends on:** Tasks 22-25；release build 配置与最终 schema 已冻结。

**Files:**

- Create/extend: routing fault/concurrency/performance integration tests
- Extend: `scripts/run-routing-operational-soak.ps1` with final fault mix、metrics and report output
- Create: `scripts/routing-operational-qualification.mjs`
- Modify: `scripts/verify.ps1` release profile，只增加 deterministic routing qualification/artifact validator；1 小时 soak 保持独立显式 step
- Modify: `.github/workflows/release.yml` 与 `scripts/release-verification-entrypoint.test.mjs`，在 signed bundle 前运行同 revision soak + qualification artifact validation
- Modify: architecture scale baseline datasets/reports
- Modify: version/changelog/release metadata，和实现/门禁脚本一起形成最终 candidate commit
- Create: `docs/superpowers/audits/` 下不含运行结果的 qualification manifest/template
- Do not commit: 最终运行结果；写入现有 ignored `output/architecture-scale/qualification/release/` 并由 CI 上传 artifact

**Candidate freeze before final qualification:**

- [ ] 先完成所有实现、tests、workflow、version/changelog 和 qualification manifest，运行 focused tests 后提交 `test: qualify routing operational cutover`。
- [ ] 记录 `candidate_revision = git rev-parse HEAD`，要求 `git status --porcelain` 为空；以下 final Run 全部针对该 revision。
- [ ] soak/performance/security 输出携带 candidate revision、工具版本和参数，只写 ignored output/CI artifact；不得在 Run 后修改 tracked 文件。
- [ ] 任一 tracked change、依赖变化或 generated drift 都使证据失效：提交修复后从 `verify:full` 与 1 小时 soak 重新开始。

**Fault and concurrency matrix:**

- [ ] optional half-open -> global -> station/account -> key 每个 acquire 点注入失败，断言反向释放、无死锁、无 underflow。
- [ ] target resolve、connect、timeout、protocol、downstream drop、panic unwind、writer transient/permanent、SQLite busy/full/corrupt simulation 后所有 lease/permit/gauge 符合合同。
- [ ] 同 Key 与 Station/account shared concurrency 在 100-way 并发下不超限；runtime limit 下调只阻止新 acquire。
- [ ] 100-way fallback 不形成 retry storm；global retry budget、waiter limits、公平唤醒、deadline 和 max 8 runtime replans 生效。
- [ ] half-open 每 `RuntimeMetricKey` 同时最多一个 probe；success count 2、failure re-eject、cancel release、60s slow-start、revision invalidation 与 max-ejection 正确。
- [ ] duplicate/out-of-order terminal job 不重复 durable/runtime effect；request ack 不越过 attempt ack。
- [ ] endpoint/config generation churn 最多批量 durable rebuild 一次，无逐候选 DB recheck；超过预算返回 typed temporary failure。
- [ ] active stream、waiter、monitor/collector run 和 pending writer 下 shutdown 在限定时间内结束并留下真实 incomplete diagnostics。

**Performance gates:**

- [ ] release build、记录 CPU/OS：100 candidates pure planner p95 `<= 2ms`。
- [ ] warmed SQLite、100 candidates、单 read session fact assembly p95 `<= 50ms`；SQL query count 是固定上限。
- [ ] runtime overlay query p95 `<= 5ms`，且 query plan 不访问价格/历史表。
- [ ] 10,000 requests / 30,000 attempts / 1,000,000 candidate rows：decision detail 首屏 p95 `<= 100ms`，cursor pagination 无 offset drift。
- [ ] 1024-candidate hard-limit rejection bounded；64-candidate catalog、8-way fan-out 内存和 task count 不越界。
- [ ] 基线回归超过 Stage 0 约定阈值时必须分析并记录，不得只因绝对门槛通过而忽略显著退化。

**Soak gates:**

- [ ] 至少 1 小时混合 buffered/stream/catalog/fallback/cancel/slow-client workload；使用 deterministic loopback，不消耗真实 provider 配额。
- [ ] 期间周期采样 active request/attempt/lease/retry/half-open/waiter/body budget/writer/task/runtime registry/SQLite size。
- [ ] 结束并 shutdown 后所有瞬态计数归零；registry/trace/DB size 符合 TTL/count/retention 上限，无单调泄漏。
- [ ] tracing 开/关各跑一轮 canary secret scan；报告仅保存聚合指标和脱敏失败代码。
- [ ] Windows sleep/resume、wall-clock rollback 和 monotonic deadline 有单独实机验证记录。

**Run:**

```powershell
pnpm.cmd verify:full
pnpm.cmd architecture:scale-baseline
cargo build --release --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -DurationMinutes 60
node scripts/routing-operational-qualification.mjs
```

**Exit gate:** fault/concurrency/performance/soak 全绿，ignored/CI qualification artifact 记录同一 candidate commit、命令、版本、环境、阈值与脱敏结果，且 worktree 仍 clean；任一红项阻止 Task 27。开发期不要求 release tag/签名密钥；Task 27 负责本地 package、重装恢复和授权客户端 smoke。

**Commit:** `test: qualify routing operational cutover`（在 final Run 之前创建并冻结，Run 后不得再修改 tracked 文件）

## 34. Task 27：开发期 package、重装恢复与真实客户端验证

**Depends on:** Task 26；开发期不要求公开发布、签名 installer 或自动更新渠道。若项目进入稳定产品阶段，本 Task 必须重新升级为正式 release/upgrade/rollback gate。

**Files:**

- Produce: local package/reinstall/reset evidence under ignored output/CI artifact path
- After successful local qualification only: Modify `docs/PROJECT_PLAN.md` completion status and remaining debug-legacy gate in a separate documentation commit
- Do not commit: real credentials、local DB、raw logs、screenshots containing private data

**Local package freeze:**

- [ ] Task 26 通过后禁止任何 unqualified tracked change，包括实现、schema、generated binding 或依赖；若发生变化，提交后回到 Task 26 重跑完整资格。
- [ ] package evidence 必须记录 qualification artifact 中的 `candidate_revision`，且 package 前 worktree clean。
- [ ] 本地/CI 优先使用仓库既有 verify/build phases；不得维护另一份手抄 test 列表。

**Reinstall/reset matrix:**

- [ ] fresh local data -> new binary；legacy fixture DB -> new binary；configured/unconfigured profiles -> new binary。
- [ ] known-schema fixtures 升级后 route readiness、monitoring facts、pricing、request logs 和 decision/cost stores 正常。
- [ ] unconfigured legacy policy 明确阻止 routing admission，但 UI 仍可打开并完成配置或提示 reset/reimport；不 panic、不静默自动映射。
- [ ] sanitizer migration 中断、resume、完成后的 startup 行为可证明。

**Reset/reinstall recovery proof:**

- [ ] 在隔离副本上证明 reset local data/reimport config 后新 binary 可启动，旧数据不作为受支持 rollback 合同。
- [ ] reset/reinstall 不 drop 用户未授权路径、不按请求混用 owner；回到新 binary 后 migration/decision/outcome uniqueness 仍正确。
- [ ] writer unhealthy 或 cutover blocker 的操作手册是 stop admission + reset/reinstall/reimport，不是切局部 feature flag。
- [ ] 明确开发期不承诺旧 binary rollback；新版本期间产生的 trace/cost 在 reset 后可丢弃或显示为 unavailable/ignored，而不是伪造兼容结果。

**Real client/provider verification:**

- [ ] 使用用户授权的至少一种真实 OpenAI-compatible 客户端验证 buffered、streaming、cancel、model listing、fallback 和稳定错误 body。
- [ ] 真实 provider 验证低频、有预算，覆盖 adapter-confirmed auth/model semantic fixture；无法确认的 403/404 仍为 Uncertain/neutral。
- [ ] CCSwitch 配合场景验证固定本地入口、启动/停止、端口占用、sleep/resume 和 upgrade 后配置不漂移。
- [ ] 核对 SQLite journal/decision/health/cost 与 UI timeline；所有数量、attempt order、currency aggregate 和 selected route 一致。
- [ ] 检查 release logs、trace、UI、errors 和 support snapshot 无 secret、完整 headers/URL 或 payload。

**Run:**

```powershell
pnpm.cmd verify:full
cargo build --release --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
node scripts/routing-operational-qualification.mjs
```

**Exit gate:** 本地 package、reset/reinstall recovery、授权真实客户端与安全核对全部通过；保存版本/commit/证据索引，不保存秘密或原始 provider payload。公开签名 release、升级矩阵和旧 binary rollback 仅在稳定产品阶段恢复。

**Commit:** `docs: record unified routing operational local qualification`

## 35. Task 28：登记并执行 debug legacy runtime 后续删除票据

**Depends on:** 不是本次 cutover 的组成部分；必须满足 `PROJECT_PLAN.md` 规定的 default-v2 正式发布后真实客户端观察门禁。

**Files:**

- Create/update: deletion ticket/ledger entry with owner、deadline、evidence and exact paths
- Later modify/delete: process-start legacy runtime composition and `RELAY_POOL_PROXY_RUNTIME=legacy`
- Modify: `docs/PROJECT_PLAN.md`
- Extend: default-v2 single-owner architecture gates

**Ticket acceptance before code deletion:**

- [ ] default-v2 完成本地观察期，真实客户端、provider、sleep/resume、reset/reinstall/reimport 无未决 P0/P1。
- [ ] support/debug 记录证明 legacy process-start runtime 不再是必要诊断手段。
- [ ] debug legacy runtime 已不再承担开发期恢复职责；删除不会破坏受支持的 reset/reinstall/reimport 路径。
- [ ] ledger 列出 legacy composition、env switch、tests、docs、compat config consumers 与最终 migration owner，不只删除入口字符串。

**Deletion execution:**

- [ ] 删除完整 debug legacy composition、环境开关、专属 tests/docs/dependencies 和仅服务于它的 compatibility adapter。
- [ ] 保留仍有历史数据语义的 compatibility fields 时，迁移到只读 projection 或明确清理；不得让 default-v2 重读 legacy policy。
- [ ] architecture gate 禁止 legacy runtime symbol/env/config owner 回流。
- [ ] 重跑 production composition、upgrade fixture、release 和真实客户端 smoke tests。

**Run when ticket preconditions are met:**

```powershell
node scripts/routing-single-owner.test.mjs
node scripts/local-proxy-v2-boundary.test.mjs
pnpm.cmd verify:full
cargo build --release --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```

**Exit gate:** debug legacy runtime 及其专属债务物理删除，support 文档指向受支持的 reset/reinstall/reimport 流程；稳定产品阶段若需要 binary rollback，另开 ADR 重新定义。

**Commit:** `refactor: remove debug legacy proxy runtime`

## 36. 每个 Task 的执行、提交与证据模板

每个 Task 必须按以下顺序执行，不能把“实现完成”和“本地资格”混为一谈：

1. **Preflight**：运行 `git status --short --branch`，确认依赖 Task 的 exit gate 和证据；枚举 migration 最大编号；记录重叠 dirty files 与 owner。
2. **Scope freeze**：从本计划复制该 Task 的 files、RED、Run、Exit gate 到工作记录；新增文件或行为必须说明为何仍在 spec 范围内。
3. **RED evidence**：先增加行为测试/contract fixture，保存预期失败的测试名与原因；不是编译错误凑 RED。
4. **GREEN**：实现最小完整 vertical slice；不得临时接 production 双轨绕过尚未完成的 owner。
5. **Focused verification**：运行 Task 列出的全部命令；失败必须修复或保持 Task 未完成。
6. **Regression**：至少运行受影响模块的 Rust/TS/contract tests；跨 stage、composition、schema 或 security 的 Task 运行 `verify:full`。
7. **Diff review**：运行 `git diff --check`、`git diff --stat`、`git diff -- <明确路径>`，检查 secret、generated churn、临时 allow 和未登记 adapter。
8. **Stage**：只用 `git add <明确路径>`；重叠 dirty file 使用 `git add -p`。禁止 `git add .`、`git add -A`。
9. **Cached review**：运行 `git diff --cached --check` 和 `git diff --cached`；确认没有 stage 用户的无关 monitoring/UI 改动、local DB、日志或 credentials。
10. **Commit**：使用 Task 建议 message 或同等清晰的 conventional commit；schema、consumer、cutover、deletion 保持可独立 review。
11. **Evidence**：在 stage audit 中记录 commit、命令、exit code、环境、阈值、已知限制和下一个 gate。只有最终 qualification 保存聚合报告，不提交原始敏感输出。

Tasks 11 和 26 是 qualification 顺序例外：先完成 focused verification、提交并冻结 candidate revision，再在 clean worktree 上执行 local/final qualification；运行证据写 ignored output/CI artifact，Run 后不再形成 tracked diff。开发期不要求 Task 11 预迁移 tag 或 Task 27 正式 tag；若 final Run 失败，修复形成新 commit 后必须重新执行相应完整资格。

一个 Task 的状态只能是：`not_started`、`red_confirmed`、`implemented_unverified`、`blocked`、`complete`。只有所有 Run 命令退出 0 且 Exit gate 满足才可标 `complete`。

## 37. Spec 26 条验收标准的最终追踪矩阵

| # | 验收结果 | 实施 Tasks | 最终证据 |
|---:|---|---|---|
| 1 | SelectedRoute 必有真实 lease | 15、16、21、22 | capacity faults + production composition |
| 2 | capacity miss/attempt failure/wait 后 replan 分离 | 8、16、21 | planner-controller table/E2E trace |
| 3 | facts 与 request projection 分离且字段完整 | 2-8 | DTO completeness + projector fixtures |
| 4 | scheduler/simulation/pricing/detail 同事实 | 4-9、21、23 | parity fixtures + read-model tests |
| 5 | monitoring 与 routing 共享 facts、不共享 DTO | 3、6、24 | dependency gate + monitoring integration |
| 6 | frontend 不再拥有权威语义 | 9、10、23、24 | source gate + UI contract tests |
| 7 | durable attempt effect-once、crash gap 诚实 | 13、19、20、26 | transaction fault + reconciliation tests |
| 8 | composite lease 全路径释放且 RequestLease 分离 | 15、20、26 | fault/concurrency/stream soak |
| 9 | frozen per-attempt cost + multi-currency aggregate | 7、19-21 | persistence/E2E cost reconciliation |
| 10 | FailureTarget 精确、effect 不跨 scope | 18、19、21 | typed failure/effect fixtures |
| 11 | durable selected request success 后才 affinity bind | 14、19、20 | duplicate/drop/ack-order tests |
| 12 | trace 解释 filter/slot/wait/fallback/outcome | 13、16、19、23 | decision timeline fixtures |
| 13 | 状态/价格/采集/Key/路由/日志贯通 | 9、10、23 | deep-link workflow + manual UI check |
| 14 | 全部资源/fan-out 有上限 | 3、9、13-16、19-21、26 | bounds table + soak gauges |
| 15 | default production 单 owner | 22、24、28 | production composition + source gate |
| 16 | weights/cache/adapter 有删除账本 | 0、10、24、28 | approved zero/dated deletion ledger |
| 17 | 全部 local/package qualification 通过 | 11、21、25-27 | qualification audit index |
| 18 | secret/header/payload 不泄露 | 11、17、18、21、25-27 | canary redaction scans |
| 19 | 所有写入配置字段有生产语义 | 8、10-12、15、21 | `routing_policy_field_e2e` |
| 20 | planner failures 穷尽稳定映射 | 18、21-23 | Rust exhaustive + HTTP/UI fixtures |
| 21 | 真 read transaction；planner 无 I/O/registry | 3、12、14、24 | snapshot isolation + dependency gate |
| 22 | capability 按维度/coverage，404 proof 正确 | 5、18、21 | reducer/negative-proof fixtures |
| 23 | outlier revision/max-ejection/half-open 正确 | 6、14、15、26 | runtime-state fault/soak tests |
| 24 | downstream drop 等待 upstream ack/cost | 19、20、26 | dual-terminal ordering tests |
| 25 | inference 不能绕倍率；query 独立 admission | 7、8、12、21 | inference/catalog E2E |
| 26 | 两 profile 同 kernel；CostFirst basis 真实一致 | 7、12、21 | property + simulation/trace parity |

Stage 7 qualification audit 必须逐行填写实际 commit/test/report 链接。任何一行只有单元测试、没有 production composition 或 E2E/授权客户端 smoke 证据时，都不能签字完成。

## 38. 最终删除与保留清单

**本次 default-v2 cutover qualification 前必须删除：**

- default-v2 simulated capacity、acquire-then-release 和 slot-unavailable-as-selected 语义；
- default-v2 static candidate fallback、第二 selector、weighted score/legacy weight reader；
- routing candidate 中的 plaintext/encrypted credential 与完整 endpoint URL；
- monitoring 对 routing candidate DTO 的依赖；
- duplicate backend group/multiplier/pricing/capability resolver；
- frontend authoritative pricing/group/capability matcher；
- arbitrary string/body failure classification 与 planner string-to-500 路径；
- 已完成迁移的 temporary adapter、test-only production facade 和 boundary exceptions。

**在 debug 观察窗口或未来稳定发布 ADR 明确要求时允许暂留，但必须隔离：**

- import/export、debug 观察或未来稳定发布 ADR 可能需要的 legacy config values；
- debug-only、process-start 级完整 legacy runtime；
- 只读 legacy request cost/trace compatibility projection；
- 尚在 debug 观察期或稳定发布 ADR 允许期内的 compatibility cache。

允许保留项不得进入 default-v2 selection、pricing、feedback、capacity、read-model truth 或 UI 设置。每项必须在 deletion ledger 有 owner、理由、最后使用者、删除前置条件和截止版本；Task 28 结束后相应项归零。

**明确长期保留并复用：**

- 现有 SQLite/Persistence V2、Request Lifecycle writer/permits、response-body wrapper 和 TaskSupervisor；
- PricingProjector/CostCalculator、HealthTransitionService 和 monitoring/collector 的 typed evidence ports；
- Tauri/React Query/IPC registry、现有安全和本地 package gates；
- 单进程模块化单体、SQLite、Tokio RAII primitives，不引入 Redis/outbox/microservice/event bus。

## 39. 风险、暂停条件与决策升级

| 触发条件 | 必须动作 | 禁止的临时绕法 |
|---|---|---|
| monitoring V2 baseline/target scope 仍变化 | 暂停 Task 3/6，先冻结共享 fact/observation port | routing 复制 monitoring DTO/reducer |
| provider account concurrency scope 无可信 provenance | 只显示 evidence gap，不启用 account lease | 猜测为 per-key 或 station limit |
| automatic profile 缺 multiplier ceiling/ordering confirmation | 预迁移 UI 引导并停止 cutover | 默认 1.0、无限上限或猜 enum |
| CostFirst 缺 exact comparable basis | 使用已批准 multiplier proxy 或 unpriced fallback | input+output 单价直接相加 |
| provider 403/404 无 sealed semantic signal | `Uncertain` + neutral effect | 解析任意正文并 hard block |
| lifecycle permit/ack 或 writer unhealthy | 停止新 upstream admission，drain/诊断 | 先发送再补 journal、自动回 legacy |
| target/config revision churn 超预算 | typed temporary failure | 每候选循环查 DB 或忽略 fence |
| crash 发生在 observation 与 durable commit 间 | reconciliation 标 `interrupted/trace_incomplete` | 伪造 usage/cost/success terminal |
| performance/soak 未达标 | 保持 qualification blocked，profile/query/index 后重跑 | 调大无界 queue/registry |
| Stage 5 后发现结构性 blocker | 停止 admission，要求用户 reset/reinstall/reimport 到一致状态 | 请求级双 selector/双写/局部回退 |
| 新需求需要 LLM、bandit 或在线学习 | 独立 RFC、离线收益和隐私评估 | 塞进 hierarchical_v1 kernel |
| 实测证明 crash gap 不可接受 | 单独评估轻量 local WAL ADR | 直接引入分布式 outbox/Redis |

会改变 canonical ownership、事务边界、public error、恢复策略 owner 或资源上限的新事实，必须更新 spec/ADR 后再实现；普通文件移动、测试命名和局部 trait 命名可在 Task diff 中记录，不需要重开架构评审。

## 40. 推荐执行批次与评审切点

| 批次 | Tasks | 可并行项 | 评审/资格结果 |
|---|---|---|---|
| A 基线冻结 | 0-2 | 无 | ADR、ownership、纯类型；不改运行行为 |
| B 事实投影 | 3-7 | 4/5/6/7 可在 Task 3 后并行 | canonical facts/projectors；不接 data-plane |
| C 控制面预迁移 | 8-11 | 9/10 在 8 后部分并行 | backend preview/readiness + 一次预迁移 checkpoint |
| D 决策与运行内核 | 12-18 | 13/14 可在 12 后并行；15 依赖 runtime contract | non-production planner/capacity/failure kernel |
| E 生命周期闭环 | 19-21 | outcome domain 与 harness fixtures 可分工 | 完整 loopback，无 production cutover |
| F 原子切换 | 22-25 | UI 与 deletion review 可预备，不可提前交付 | 一个 Stage 5+6 cutover candidate，单 owner + security migration |
| G 本地资格 | 26-27 | performance、security、reset/reinstall fixtures 可并行执行 | fault/soak/build/授权真实 E2E 后完成本地 package qualification |
| H 后续清债 | 28 | 无 | 满足观察门禁后删除 debug legacy runtime |

推荐评审至少设置六个强制切点：ADR/ownership、projector contracts、预迁移 checkpoint、planner/capacity kernel、outcome/loopback、production cutover/deletion。F 批次内部可以多 commit，但只能形成一个 production owner；不得为了减少单次 diff 而交付混合 composition。

## 41. 整体 Definition of Done

最终只有以下全部为真才能关闭升级：

- Tasks 0-27 均为 `complete`；Task 28 已有满足格式的独立票据，若其观察前置条件已满足则也完成删除；
- 第 37 节 26 行均有实际自动化和 production/E2E 证据；
- pre-migration checkpoint 与 Stage 5+6 cutover candidate 的顺序可从 commit 证明；
- Tauri release-mode build、local package verification、reset/reinstall recovery、1 小时 soak 和授权真实客户端验证退出 0；
- fresh/known schema、sanitizer resume、reset/reimport recovery、new binary re-open 均通过；
- default-v2 source/composition 没有第二 truth/selector/capacity/feedback；
- 所有瞬态资源在测试与 shutdown 后归零，持久化 totals/journal/trace 可核对；
- secret/full URL/header/payload canary 扫描无泄漏；
- deletion ledger 无无 owner、无期限的临时项；
- `docs/PROJECT_PLAN.md`、相关 ADR/spec、命令/IPC fixtures 与实现一致。

本计划不以算法复杂度作为先进性目标。完成后的技术路线是成熟工程软件常用的 deterministic layered eligibility、lexicographic ordering、bounded retry/wait、RAII admission、outlier/half-open、immutable snapshot、typed outcomes 和 request-time settlement；其优势来自单一事实所有权、可证明生命周期、可解释决策和完整发布闭环，而不是 LLM 或不可审计的在线学习。
