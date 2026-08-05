# Relay Pool Desktop 智能路由引擎升级实施计划

状态：Planned，尚未开始生产代码实施

日期：2026-08-05

批准设计：[`../../proposals/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](../../proposals/INTELLIGENT_ROUTING_ENGINE_SPEC.md)

适用范围：本地 OpenAI-compatible Proxy、Station / Station Key operational facts、主动监控与真实流量反馈、Routing Policy、PlanningSnapshot、Planner / Dispatcher、fallback / capacity / exploration、决策解释，以及 Station / Key Pool / Pricing / Channel / Dashboard / Routing 页面共享 read model。

历史关系：本计划取代 `2026-07-30-routing-operational-unification-upgrade.md` 中关于 `PriorityFirst / CostFirst` selector、旧 Local Routing Workspace、前端 pricing/group matcher、粗粒度 health、旧 scheduler settings 和旧 gate 正向要求的后续实施含义。旧计划已经交付的 Persistence V2、Request Lifecycle、Monitoring V2、capacity lease、late target resolution、route decision store、loopback、自检和安全边界继续作为现有资产，不重复从零实现。旧计划只保留为历史证据，不能覆盖本计划和批准设计。

> 本计划按 Task 执行。每个行为 Task 必须先观察 RED，再实现 GREEN，最后运行 task gate。任何未真实退出 0 的必跑命令都意味着 Task 未完成。

---

## 1. 目标与完成定义

目标生产链路固定为：

```text
Canonical facts / CanonicalOutcome / Observation
  -> versioned projectors and durable summaries
  -> one-transaction PlanningSnapshotBuilder
  -> immutable PlanningSnapshot + RuntimeOverlay
  -> Eligibility -> Tier -> Factor -> Utility -> Band
  -> exploit dispatch or bounded exploration
  -> shared capacity / retry / exploration admission
  -> late TargetResolver + revision fence
  -> protocol attempt + dual-terminal lifecycle
  -> atomic finalization / observation append
  -> health / quality / dashboard / channel / change projections
```

只有同时满足以下条件才完成：

- production planner 只接收 `PlanningSnapshot`，单候选只使用 `CandidateSnapshot`；
- 四因子、可靠性风险门槛、fixed-point utility、failure domain、exploration 和 dispatch profile 全部进入同一 Planner pipeline；
- production、simulation、fallback 和 replay 使用同一个 compiler / planner，不存在隐藏 fast path；
- Routing Policy 是独立版本化 aggregate，通用 Settings 不再携带路由配置；
- Request Outcome 与 Monitor Result 进入同一个 typed observation vocabulary，Health / Quality / Channel / Dashboard / Change 是同源投影而不是互调 Service；
- Station、Key Pool、Pricing 和 Routing 页面只消费后端 read model，不在 React 中重做 group、pricing、balance、capability、health 或 score；
- route decision 持久化完整 profile / revision / seed commitment / factor contribution，按 ID 和 cursor 直接查询；
- candidate load 不读取或解密所有 API key，只有选中并取得 lease 后解析单一 target；
- 新后端、新 IPC、新路由页和旧后端/前端删除属于同一个 cutover candidate；
- 旧 Runtime Candidate、selector、scheduler settings、LocalRoutingWorkspace、frontend matcher、compat gate 和 dead code 沿完整依赖链清零；
- Rust、TypeScript、Vite、contracts、architecture、migration、replay、property、concurrency、fault、performance、soak 和 reset/reimport 自检全部通过。

## 2. 执行纪律

1. 每个 Task 开始和结束记录 `git status --short --branch`、`git log -5 --oneline` 和当前 migration 最大编号。工作区已有用户改动不得覆盖或回退。
2. 不使用 `git add .`、`git add -A` 或 `git commit -a`。如果执行阶段被要求提交，只 stage Task 明确列出的路径，并检查 staged diff。
3. 使用 RED-GREEN-REFACTOR。RED 必须因目标能力缺失或旧反模式仍存在而失败，不能写永远成功的 source regex。
4. 不建立 production 双 selector、双 observation writer、双 health reducer、双 policy store 或双 frontend workspace。兼容只允许出现在一次性 migration / red fixture / historical audit 中。
5. Tasks 3-15 的新 Planner、Observation/Quality writer 和 Runtime Coordinator 在 cutover 前只通过具名 `intelligent_routing_qualification` 测试组合可达；该临时边界由 Task 0 ledger 登记，Task 16 必须整体 production 化，Task 17 必须删除 qualification wiring。不得在旧 writer/selector 旁做 shadow write，也不得用 qualification 测试宣称 production 已切换。
6. qualification 边界必须使用 `#[cfg(test)]` 或已有 test-support 机制，不得增加 `allow(dead_code)`、伪 public API、空 trait default 或 production feature flag。
7. Tasks 16-18 是一个原子 cutover 单元：三者可以分步骤执行，但不能独立合并、发布、交付或作为完成状态。Task 16 接线后必须立即执行 Task 17 删除旧链和 Task 18 schema / generated / docs cleanup；三者进入同一 cutover revision。
8. migration 编号执行时从目录枚举，本文用 `00NN` 表示“下一可用编号”。不得修改已发布 migration 内容。
9. 所有整数权重、固定点尺度、窗口、预算、上限、退避和 profile version 在 Task 11 前冻结；不得在实现中散落 magic number。
10. 所有 queue、registry、history page、candidate batch、observation rebuild、trace detail 和 notification fan-out 必须有硬上限、取消和 shutdown 行为。
11. API key、Cookie、Authorization、完整 prompt/response、完整 replay seed、完整 URL query/userinfo 不得进入 log、trace、IPC、fixture 或 qualification artifact。
12. 当前仍是非稳定本地开发阶段。结构性不兼容可以 fail closed 并要求 reset/reimport/重新配置；不得因此恢复 legacy runtime、旧 binary fallback 或长期 dual-read。
13. 外部项目只用于模式对照，不复制 AGPL/LGPL 核心实现。现有 attribution 和审阅 commit 保留在 audit 中。
14. Task gate 失败时先定位本 Task 的真实问题；不得删 gate、放宽 manifest、添加 blanket allow 或把失败登记为永久 exception。

### 2.1 Claude Code review hard gates

This plan was audited read-only by the local Claude Code CLI on 2026-08-05 against the current repository. The following are pre-start blockers; they cannot be deferred behind a compatibility layer:

- Add a revision migration bridge: initialize a one-time baseline for existing timestamp-as-revision data, then consume only transaction-sequence revisions. Remove every `fallback = 1` path.
- Remove production `#[cfg(test)]` gates from the `operational_facts` and `routing_decisions` stores. Their production APIs must be bounded, typed, and error-safe rather than discarding rows.
- Task 14 must choose a production evidence source for ProviderAccount capacity. If V1 cannot provide provider-account identity, freeze that limitation as an explicit `NotApplicable` capability boundary and remove the fake test-only contract.
- Task 15 must perform a cutover rehearsal against a real SQLite PlanningSnapshot, verify generated IPC registry coexistence, and compile the production composition before Tasks 16-18.
- Task 6 must stop `station_keys.status` health writeback. Until Task 18 removes the legacy column, it may be migration/read-only compatibility only; there must be one health writer.
- Task 10 must include a six-value legacy `RoutingPolicy` to V1 preset mapping matrix with reasons; semantically different policies may not be silently merged.
- Task 13 must freeze the `ExplorationBudgetRegistry` concurrency model, restart recovery behavior, and starvation-bound proof.
- Task 16 must explicitly migrate the UI from `LocalRoutingWorkspace` to `RoutingWorkspaceSnapshot` and add staged compilation checkpoints. Tasks 16-18 remain one non-deliverable cutover revision.

## 3. 原子 Cutover 规则

以下组合绝不允许进入可运行提交：

- old selector + new Routing Policy；
- new Planner + old scheduler settings；
- new Planner + old health writeback；
- new observation writer + old outcome classifier；
- new backend workspace + old LocalRoutingWorkspace 同时作为页面真相；
- new pricing read model + frontend canonical group/hash matcher；
- new target resolver + candidate bulk secret preload；
- new route decision writer + request-log scan detail query；
- new production symbols + old architecture gate 正向要求。

Tasks 16-18 的 staged snapshot 必须同时包含：production composition、Proxy data plane、Routing Policy admission、generated IPC、Routing UI、old backend deletion、old frontend deletion、gate/manifest 重写、destructive schema 和 migration cutover。任何一项未完成都不能把该 snapshot 称为 cutover candidate。

## 4. Task 依赖图

```text
0 Baseline / supersession / ledgers
  -> 1 Target architecture gates
  -> 2 Schema and revision primitives
  -> 3 Domain contracts / fixed-point profile
       -> 4 OperationalFactSource / PlanningSnapshot
       -> 5 Shared operational projectors
       -> 6 Canonical outcome / observation / health ingestion
            -> 7 Reliability / responsiveness summaries
  5 + 7 -> 8 Station / Key / Pricing backend read models
  2 + 6 -> 9 Read purity / revisions / UI notices
  2 + 3 -> 10 Routing Policy aggregate / compiler / migration
  3 + 7 + 10 -> 11 Factors and profile calibration
  4 + 5 + 11 -> 12 Eligibility / tier / safety / failure domains
  12 -> 13 Dispatch / exploration / deterministic replay
  13 -> 14 Proxy-instance runtime registries
  4 + 7 + 10 + 14 -> 15 Coordinator / fallback / trace / loopback
  8 + 9 + 15 -> 16 Atomic production and UI cutover
  16 -> 17 Full old-path deletion
  17 -> 18 Destructive schema / generated / docs cleanup
  18 -> 19 Full qualification
  19 -> 20 Close ledgers and implementation status
```

Tasks 4-6 可以在 Task 3 后并行准备，但 Task 7 需要 6，Task 11 需要 7 和 10，Task 15 需要所有算法和 runtime 合同冻结。本文默认单工作区顺序执行；并行时仍不得让两个任务同时修改同一 owner 文件。

## 5. 目标文件地图

文件名允许在 Task 0 根据现有模块做一次有证据的微调，但最终职责和依赖方向不得改变。

| 路径 | 最终职责 |
|---|---|
| `src-tauri/src/models/routing_policy.rs` | `RoutingPolicyConfigV1`、版本、状态和 mutation input；不含 runtime state |
| `src-tauri/src/models/routing_observation.rs` | immutable typed observation、source、traffic equivalence、scope 和 ordering identity |
| `src-tauri/src/models/routing_read_models.rs` | consumer-specific stable application outputs；不 re-export engine internals |
| `src-tauri/src/application/routing_policy.rs` | policy load / compile / CAS save / migration-required admission |
| `src-tauri/src/application/observation_ingestion.rs` | CanonicalOutcome / ProbeResult 到 Observation 的唯一生产入口 |
| `src-tauri/src/application/quality_projection.rs` | reliability / responsiveness incremental projection、checkpoint、rebuild |
| `src-tauri/src/application/operational_facts/reader.rs` | production raw fact port；单一 read transaction、固定批量 query bound |
| `src-tauri/src/application/operational_facts/planning_snapshot.rs` | `PlanningSnapshotBuilder`、version vector、runtime join fence |
| `src-tauri/src/application/operational_facts/*_projector.rs` | shared pure domain projectors；页面与 Router 复用同一实现 |
| `src-tauri/src/application/routing_engine/fixed_point.rs` | basis points / score / contribution 安全算术 |
| `src-tauri/src/application/routing_engine/algorithm_profile.rs` | 完整 `DispatchAlgorithmProfile` 和 canonical serialization |
| `src-tauri/src/application/routing_engine/factors.rs` | reliability / responsiveness / cost / preference factor |
| `src-tauri/src/application/routing_engine/eligibility.rs` | hard gates、max-ejection guard、typed rejection |
| `src-tauri/src/application/routing_engine/tiers.rs` | Primary / Backup / Emergency tier，不计算 score |
| `src-tauri/src/application/routing_engine/failure_domains.rs` | endpoint/account/key/model/provider failure-domain projection |
| `src-tauri/src/application/routing_engine/dispatch.rs` | exploit band、weighted rendezvous、integer rank、tie-break |
| `src-tauri/src/application/routing_engine/exploration.rs` | Unknown lane、预算、starvation bound、seed domain separation |
| `src-tauri/src/application/routing_engine/planner.rs` | 唯一 pure Planner pipeline；无 SQL/HTTP/secret/runtime mutation |
| `src-tauri/src/application/routing_engine/coordinator.rs` | request-local planning round、replan、fallback、deadline |
| `src-tauri/src/services/proxy/routing_runtime.rs` | proxy-instance capacity/retry/exploration/circuit/affinity registries |
| `src-tauri/src/persistence/stores/routing_policy_store.rs` | versioned policy aggregate 和 history SQL |
| `src-tauri/src/persistence/stores/routing_observation_store.rs` | append / idempotency / ordering / checkpoint source SQL |
| `src-tauri/src/persistence/stores/routing_quality_store.rs` | durable quality / health axes / projector checkpoint SQL |
| `src-tauri/src/persistence/stores/operational_facts/` | bounded raw fact batch queries；不解释 policy |
| `src-tauri/src/application/queries/{routing_workspace,request_decision_trace,operational_detail}.rs` | 直接回答 read contract；不调用页面 query |
| `src-tauri/src/application/queries/{station_assets,station_detail,key_pool,pricing_comparison}.rs` | 共享 projector 的 consumer read models |
| `src-tauri/src/ipc/dto/routing_*.rs` | versioned IPC input/output；由 generator 生成 TS |
| `src/lib/query/domainRevisionInvalidation.ts` | 唯一 frontend scope-to-query-family mapping |
| `src/features/routing/` | Routing Policy 编辑、状态、模拟、decision trace presentation |

## 6. 共享类型台账

| 类型 | 引入 Task | 最终 owner |
|---|---:|---|
| `DomainRevision`, `FactVersionVector`, `SnapshotIdentity` | 2 | persistence/application revision boundary |
| `RoutingPolicyConfigV1`, `CompiledRoutingPolicy` | 3, 10 | models + policy compiler |
| `BasisPoints`, `FactorScore`, `UtilityScore`, `FactorContribution` | 3 | routing engine fixed point |
| `RoutingObservation`, `ObservationScope`, `ObservationOrder` | 3 | routing observation model |
| `CandidateSnapshot`, `PlanningSnapshot`, `RuntimeOverlaySnapshot` | 3, 4 | operational facts / routing domain |
| `HealthAxesSummary`, `ReliabilitySummary`, `ResponsivenessSummary` | 6, 7 | health / quality projectors |
| `FailureDomainSet` | 12 | failure-domain projector |
| `DispatchAlgorithmProfile`, `DispatchSeedCommitment` | 11, 13 | routing algorithm profile |
| `RoutingRuntimeState`, `RetryPermit`, `ExplorationPermit` | 14 | Proxy runtime instance |
| `MutationReceipt`, `DomainRevisionNotice` | 9 | application mutation / UI freshness boundary |
| `RoutingWorkspaceReadModel`, `DecisionTraceReadModel` | 15, 16 | routing application queries |

## 7. Program Exit Gates

| 合同 | 主要 Tasks | 必须证据 |
|---|---:|---|
| 单一事实与 projector owner | 4-9 | projector parity、no frontend truth、one-read-session tests |
| observation ordering / rebuild | 6-7 | duplicate、late、out-of-order、checkpoint fault、replay equivalence |
| policy truth / config liveness | 10-11 | CAS、migration required、trace contribution、preset golden |
| algorithm correctness | 11-13 | property、golden、fixed-point overflow、distribution、starvation |
| runtime safety | 14-16 | shared budgets、lease cleanup、ABA restart、shutdown、deadline |
| target / secret safety | 4, 15-17 | no bulk secret、late revision fence、redaction/canary |
| one production route owner | 16-18 | production composition、loopback、old symbol absence |
| shared read models | 8-9, 16 | single revision join、no N+1、pure query、typed invalidation |
| aggressive deletion | 17-18 | deletion ledger zero、dead-code policy、generated/fixture/gate cleanup |
| operational qualification | 19 | full Rust/TS/contracts/fault/perf/soak/reset-reimport evidence |

---

## Workstream A：基线、门禁与持久化基础

### Task 0：冻结当前调用图、替代关系和删除台账

**Files：**

- Create: `docs/superpowers/audits/2026-08-05-intelligent-routing-baseline.md`
- Create: `docs/superpowers/audits/intelligent-routing-acceptance-matrix.md`
- Create: `docs/superpowers/audits/intelligent-routing-deletion-ledger.md`
- Create: `docs/superpowers/audits/intelligent-routing-boundary-manifest.json`
- Modify: `docs/superpowers/audits/routing-operational-boundary-manifest.json`
- Read only: approved spec、旧 routing plan、current routing/monitoring/request lifecycle/persistence modules

**Steps：**

- [ ] 记录 branch、HEAD、dirty paths、migration 最大编号、Rust/Node/pnpm 版本和基线命令耗时。
- [ ] 从真实 production composition 画出 ingress -> candidate load -> selector -> capacity -> target -> attempt -> finalization -> health / trace 调用图。
- [ ] 列出所有页面 join、query-on-query、Service-on-Service、raw JSON truth、status string、timestamp revision 和 request-log scan 证据，记录精确文件与 symbol。
- [ ] 把 spec 78 条验收标准逐条映射到 Task、测试、owner 和最终证据；任何 unmapped 条目阻止 Task 1。
- [ ] deletion ledger 至少登记 `RuntimeRoutingCandidate`、`runtime_candidate_adapter`、old selector/controller/types、SchedulerAdvancedSettings、LocalRoutingWorkspace、old policy literals、frontend projectors/hash、station/key status、wide RoutingService/Store methods、wrapper chain、old gates/fixtures/generated DTO。
- [ ] 为 `intelligent_routing_qualification` 建立唯一 temporary entry，owner=Tasks 3-15，delete_by=Task 17；不得登记其他泛化 temporary exception。
- [ ] 将旧 routing boundary manifest 的正向 compatibility 条目分类为 `rewrite`、`delete` 或 `negative-regression-only`，不能原样继承。
- [ ] 记录 AgentGate / Sub2API 等参考只影响设计原则，不进入实现依赖或复制来源。

**Run：**

```powershell
git status --short --branch
git log -5 --oneline
Get-ChildItem src-tauri/src/persistence/migrations -File | Sort-Object Name | Select-Object -Last 10 -ExpandProperty Name
rg -n "RuntimeRoutingCandidate|runtime_candidate_adapter|SchedulerAdvancedSettings|LocalRoutingWorkspace|buildCurrentStationGroupFacts|hashCanonicalPricingGroupRefs|repair_rollups_if_needed|stations SET status|list_recent\(PageLimit::new\(500" src-tauri/src src scripts
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
pnpm.cmd exec tsc --noEmit
pnpm.cmd test:contracts
node scripts/dead-code-inventory.mjs --mode ci --scope production
```

**Exit gate：** 78/78 spec criteria 已映射；每个旧 owner 有明确最终动作和删除 Task；temporary ledger 只有 qualification boundary；基线红项有真实 owner。

**Commit：** `docs: freeze intelligent routing upgrade baseline`

### Task 1：原子重写目标架构门禁

**Files：**

- Create: `scripts/intelligent-routing-architecture.test.mjs`
- Create: `scripts/fixtures/intelligent-routing-architecture/{pass,red-*}/**`
- Modify: `scripts/routing-operational-architecture.test.mjs`
- Modify: `scripts/routing-single-owner.test.mjs`
- Modify: `scripts/routing-read-model-architecture.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`
- Modify: `docs/superpowers/audits/intelligent-routing-boundary-manifest.json`
- Modify: `docs/superpowers/audits/routing-operational-boundary-manifest.json`

**RED：**

- [ ] red fixture 证明 Planner 导入 SQLx、Reqwest、Tauri、IPC、secret、request log 或 monitoring DTO 会失败。
- [ ] red fixture 证明 production planner 接收裸 candidate slice / `RouteCandidateProjection` 会失败。
- [ ] red fixture 证明前端出现 group/pricing/balance/capability/health/score 权威 reducer 会失败。
- [ ] red fixture 证明 application Query 开 write transaction 或调用另一个页面 Query 会失败。
- [ ] red fixture 证明 status 派生字符串写回 station/key、timestamp revision、fallback `1`、bulk secret preload 会失败。
- [ ] red fixture 证明 `requireRegistration(old_symbol)`、空匹配 gate、永久 temporary ledger 会失败。
- [ ] red fixture 证明连续无不变量 facade/API/query wrapper 和 `shared_capabilities` 跨领域 DTO 会失败。

**GREEN：**

- [ ] gate 从 manifest 读取目标 owner、allowed edge、forbidden symbol、temporary deadline 和 red-fixture allowlist。
- [ ] 保留旧 gate 中仍有效的负向安全不变量；删除要求 old adapter、`default-v2`、LocalRoutingWorkspace 或 compatibility marker 存活的正向断言。
- [ ] gate 同时证明新 owner 存在，不能仅因旧字符串消失而通过。
- [ ] contracts runner 注册新 gate；旧 pre-deletion/task 命名 gate 标记在 Task 18 删除或重命名。

**Run：**

```powershell
node scripts/intelligent-routing-architecture.test.mjs
node scripts/routing-operational-architecture.test.mjs
node scripts/routing-single-owner.test.mjs
node scripts/routing-read-model-architecture.test.mjs
pnpm.cmd architecture:fixtures
pnpm.cmd test:contracts
```

**Exit gate：** pass fixture 通过；每个 red fixture 因目标不变量失败；旧正向 compatibility 要求已不再阻止最终删除。

**Commit：** `test: add intelligent routing target architecture gates`

### Task 2：建立 schema、revision 和 projector checkpoint 基础

**Review addendum (must be completed in Task 2):**

- Add a revision migration bridge in the foundation migration. For every existing station, key, settings, and policy aggregate, create a `domain_revisions` baseline from the maximum known record revision; when no trustworthy source exists, write an explicit `baseline_snapshot` provenance and a monotonic value, never a silent `1` fallback.
- Replace `CAST(updated_at AS INTEGER)` and all constant revision fallbacks in existing assemblers/queries. A missing or conflicting baseline returns `revision_unavailable` and fails closed until repaired.
- Remove production `#[cfg(test)]` gates from `persistence/stores/operational_facts` and `persistence/stores/routing_decisions`. Add bounded-query, error-propagation, restart, and production-compilation tests before downstream tasks import these stores.
- Include same-millisecond writes, restart, interrupted migration, portable import, and concurrent mutation in the revision bridge fixture; the fixture must fail if the legacy timestamp path is still reachable.

**Files：**

- Create: `src-tauri/src/persistence/migrations/00NN_intelligent_routing_foundation.sql`
- Create: `src-tauri/src/persistence/stores/domain_revision_store.rs`
- Create: `src-tauri/src/persistence/stores/routing_policy_store.rs`
- Create: `src-tauri/src/persistence/stores/routing_observation_store.rs`
- Create: `src-tauri/src/persistence/stores/routing_quality_store.rs`
- Modify: `src-tauri/src/persistence/stores/mod.rs`
- Modify: `src-tauri/src/persistence/schema_registry.rs`
- Modify: portable migration / schema fixture manifests as required by current authoring contract
- Create: `src-tauri/tests/intelligent_routing_persistence.rs`

**Schema contract：**

- `domain_revisions(scope PRIMARY KEY, revision, updated_at_ms)`，revision 由 write transaction 单调推进；
- `routing_policy` singleton aggregate，包含完整 config、config revision、policy/system version、status 和 timestamp；
- `routing_policy_history` 保存 replay 所需不可变版本，不保存 secret；
- `routing_observations` 保存全局 ID、producer identity/sequence、event/ingest time、scope、source、traffic equivalence、typed outcome/evidence、bounded measurements；
- `routing_projector_checkpoints` 保存 projector/version/scope/checkpoint/status/error；
- `routing_quality_summaries` 和 `routing_health_axes` 保存版本化 durable projection；
- 所有 JSON 列有 `json_valid`、bounded decode 和 typed serde owner；核心 identity/order/status 不藏在 JSON 中；
- additive foundation migration 不修改 production selector，不 dual-write observation；旧列在 Task 18 destructive migration 删除。

**RED / GREEN：**

- [ ] RED：duplicate observation ID、producer sequence reuse with different payload、revision rollback、unknown policy version、invalid JSON、negative mass / latency、checkpoint skip 都被拒绝。
- [ ] GREEN：append 幂等；同 payload duplicate 返回 existing，不同 payload collision 返回 invariant error；revision 与 domain write 同 transaction；checkpoint 只在 projection commit 后推进。
- [ ] migration 从 Task 0 枚举的 current known schema 和 fresh DB 均可执行；中断恢复、portable import、known-schema fingerprint 和 downgrade compatibility 按项目当前开发期合同更新。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_persistence -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_startup_cutover -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture
node scripts/sqlx-offline-metadata.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
```

**Exit gate：** schema 约束能阻止伪 revision、重复/冲突 observation 和越序 checkpoint；production routing 行为尚未改变。

**Commit：** `feat: add intelligent routing persistence foundation`

---

## Workstream B：领域事实、Observation 与共享投影

### Task 3：冻结纯领域合同、固定点和版本 profile

**Review addendum:** rename the existing production `planner.rs` to `planner_legacy.rs` at the qualification start, mark its imports as legacy, and add a compile gate proving the new and legacy planners cannot export or import the same `plan_route` contract.

**Files：**

- Create: `src-tauri/src/models/routing_policy.rs`
- Create: `src-tauri/src/models/routing_observation.rs`
- Create: `src-tauri/src/application/routing_engine/fixed_point.rs`
- Create: `src-tauri/src/application/routing_engine/algorithm_profile.rs`
- Create or rewrite: Candidate / Planning / Runtime Overlay domain types under `application/routing_engine/`
- Modify: `src-tauri/src/application/routing_engine/mod.rs`
- Create: pure unit/property tests in matching modules

**Steps：**

- [ ] 定义 `CandidateSnapshot` 与 `PlanningSnapshot` 为不同非 Serialize engine types；Candidate 不含 secret、in-flight、waiter 或 mutable handle。
- [ ] 定义 `BasisPoints(0..=10_000)`、bounded factor/utility score、checked multiply/add/divide、round-half rule 和 canonical integer contribution。
- [ ] 定义 `DispatchAlgorithmProfile` 的完整字段：factor normalization、posterior、decay、utility mapping、band、hash/rank/tie-break、exploration、seed derivation 和版本。
- [ ] 定义 Observation closed enums、scope、source、traffic equivalence、event/ingest ordering；不得用自由字符串驱动健康或评分。
- [ ] 定义 Routing Policy config / compiled type 分离；compiled type 不序列化、不进入 IPC。
- [ ] qualification-only wiring 记录到 manifest；不增加 production flag 或 dead-code suppression。

**RED / Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml application::routing_engine::fixed_point -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml application::routing_engine::algorithm_profile -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml models::routing_observation -- --nocapture
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
node scripts/dead-code-inventory.mjs --mode ci --scope production
```

**Exit gate：** overflow、rounding、canonical serialization、unknown version 和 invalid enum 均有 deterministic test；production 无 test-only equivalent 声称已接线。

**Commit：** `feat: define intelligent routing domain contracts`

### Task 4：完成目标 OperationalFactSource 与 PlanningSnapshotBuilder

**Files：**

- Rewrite: `src-tauri/src/application/operational_facts/reader.rs`
- Rewrite: `src-tauri/src/application/operational_facts/assembler.rs` 或 rename 为 `planning_snapshot.rs`
- Rewrite: `src-tauri/src/persistence/stores/operational_facts/{mod,queries}.rs`
- Create: `src-tauri/tests/intelligent_routing_planning_snapshot.rs`
- Modify: test qualification composition only before Task 16

**RED：**

- [ ] 当前 test-only reader、discarded rows、timestamp revision、fallback `1` 和 secret-bearing candidate 被 target gate 捕获。
- [ ] 并发 fixture 在 policy / key / price / health 更新中间构建 snapshot，证明多 transaction 拼接会产生不可能组合。
- [ ] 100/1,000 candidate fixture 证明 N+1 和无上限 wide join 会超过 query/latency/memory gate。

**GREEN：**

- [ ] 顶层 builder 开启唯一 `ReadSession`，所有 raw readers 接收同一 context；固定批量读取 identity/policy/group/pricing/balance/capability/health/quality/revisions。
- [ ] 每条 SQL row 进入 typed raw fact 或删除 query；query count 来自 instrumentation / spy。
- [ ] version vector 来自 domain revisions / aggregate revisions，不解析 timestamp、不回退常量。
- [ ] candidate 只携带 credential availability/reference revision；不查询 ciphertext、明文 key 或完整 execution target。
- [ ] durable snapshot 后捕获 immutable runtime overlay，并记录 runtime instance/revision/candidate-set join fence；qualification 中使用 fake overlay。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_planning_snapshot -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_domain -- --nocapture
node scripts/intelligent-routing-architecture.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
```

**Exit gate：** snapshot isolation、query bound、no-secret、revision vector、unknown/stale provenance 全部由真实 SQLite fixture 证明。

**Commit：** `feat: build snapshot-consistent routing facts`

### Task 5：收敛 Group、Pricing、Balance、Capability 和 Asset Projectors

**Files：**

- Rewrite: `src-tauri/src/application/operational_facts/{group,multiplier,pricing,balance,capability}_projector.rs`
- Create: `src-tauri/src/application/operational_facts/asset_status_projector.rs`
- Create: versioned golden fixtures under `src-tauri/tests/fixtures/intelligent_routing/projectors/`
- Modify: affected projector tests and read-model parity tests

**Steps：**

- [ ] 为 group identity、rate precedence、pricing match、balance scope、capability evidence 建立唯一 pure reducer。
- [ ] 每个输出包含 typed verdict、reason code、source refs、observed_at、confidence 和 projector version。
- [ ] Unknown、stale、ambiguous、invalid、unsupported 和 missing 分开表达，不能压成 null/default success。
- [ ] Asset Status rollup 只为 UI 组合 badge，不写 canonical row、不被 Router 反向读取。
- [ ] 同一 fixture 分别经过 CandidateSnapshot、Station/Key/Pricing read model，verdict 必须一致。
- [ ] raw collector JSON 只作为离线 fixture/rebuild evidence；production page/query 不解析它。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml operational_facts -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test capability_evidence -- --nocapture
node scripts/shared-capabilities-contract.test.mjs
node scripts/intelligent-routing-architecture.test.mjs
```

**Exit gate：** Router 与所有 read-model fixture 对同一 source refs 得到同一 group/economics/capability/balance verdict；无 frontend-equivalent reducer 被新增。

**Commit：** `refactor: unify operational fact projectors`

### Task 6：统一 CanonicalOutcome、Observation 与多轴 Health 写路径

**Review addendum:** stop `update_station_key_status` writeback in the same change that introduces the multi-axis health writer. Keep `station_keys.status` migration/read-only compatibility only until Task 18 drops it; add a source search and runtime test that proves there is exactly one production health writer.

**Files：**

- Create: `src-tauri/src/application/observation_ingestion.rs`
- Rewrite: `src-tauri/src/application/health_transitions.rs`
- Rewrite: `src-tauri/src/application/request_finalization/{failure,effect_planner,outcome}.rs`
- Rewrite: `src-tauri/src/application/monitoring/write_path.rs`
- Modify: `src-tauri/src/persistence/stores/health_observation_store.rs`
- Modify: `src-tauri/src/persistence/stores/routing_observation_store.rs`
- Create: `src-tauri/tests/intelligent_routing_observations.rs`

**RED：**

- [ ] 同一 HTTP / provider signal 经过 request 与 monitoring 路径得到不同 failure/health taxonomy 时测试失败。
- [ ] duplicate、late success、late failure、producer restart、same-sequence conflict、endpoint revision change、anonymous probe elevation 被测试覆盖。
- [ ] source terminal row 成功而 observation/critical health commit 失败时，事务原子性测试必须失败而不是产生 gap。

**GREEN：**

- [ ] Transport/provider adapter 只输出 typed signal；唯一 classifier 生成 CanonicalOutcome 和 fixed effect plan。
- [ ] request lifecycle 与 monitoring 共用 observation vocabulary，但保留 source 和 traffic equivalence 权重。
- [ ] qualification path 证明 source terminal row、observation append、domain revision 和 critical health axes 在同一 transaction 提交；Task 16 一次性替换 production writer，不在当前 writer 旁 shadow write。
- [ ] watermark 决定是否更新各 axis；迟到事件保留 evidence，但不能覆盖较新 credential/circuit/quality/throttle 状态。
- [ ] 删除第二套 `AttemptFailureKind` / monitoring `FailureKind` 解释 switch，或将其限制为协议 adapter 输入并只转换一次。
- [ ] collector terminal status 不再写 station/key health；旧 status 写回在 Task 18 schema 删除前停止使用。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_observations -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test station_key_health_transitions -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_write_path -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_domain -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_persistence -- --nocapture
```

**Exit gate：** 一个 typed classifier、一个 observation append contract、一个多轴 health transition；乱序与重复结果可重放且一致。

**Commit：** `refactor: unify routing observations and health effects`

### Task 7：实现 Reliability / Responsiveness Projector、Checkpoint 与 Rebuild

**Files：**

- Create: `src-tauri/src/application/quality_projection.rs`
- Create: `src-tauri/src/application/operational_facts/{reliability,responsiveness}_projector.rs`
- Modify: `src-tauri/src/persistence/stores/routing_quality_store.rs`
- Create: `src-tauri/src/background_tasks/routing_projection_runner.rs`
- Modify: qualification composition only；production observation writer / runner wiring 留到 Task 16 原子切换
- Create: `src-tauri/tests/intelligent_routing_quality_projection.rs`

**Steps：**

- [ ] generalized Beta posterior 使用 fractional sample weight、minimum effective mass、versioned decay 和一次 prior shrinkage。
- [ ] latency 按 request type/model family/context bucket 归一化；记录 TTFT/total latency coverage，缺失不伪装为 0。
- [ ] projector 以 `(projector_version, scope, checkpoint)` 幂等推进；失败不推进 checkpoint；dirty range 有界重建。
- [ ] online incremental 与 full rebuild 对 duplicate/out-of-order/late fixtures 产生 byte-equivalent summary。
- [ ] projection lag 进入 read model / PlanningSnapshot freshness；lag 时 Router 按 Unknown/stale 降级，不能扫 request logs 补算。
- [ ] runner 由 supervisor 拥有，有 bounded batch、cancel、shutdown、retry/backoff 和 metrics。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_quality_projection -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_buckets_retention -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml application::quality_projection -- --nocapture
node scripts/intelligent-routing-architecture.test.mjs
```

**Exit gate：** incremental=rebuild；minimum mass / risk / decay 数学 golden 通过；lag、runner fault 和 shutdown 可诊断。

**Commit：** `feat: add routing quality projection pipeline`

---

## Workstream C：共享 Read Model 与 UI 信息收敛

### Task 8：切换 Station、Key Pool 与 Pricing 后端 Read Model

**Review addendum:** server-issued `group_identity_hash` is the only join key. Delete the frontend SHA-256 implementation and matcher reducers as part of this task's UI migration; the frontend may format and filter returned rows but may not recompute group, pricing, capability, health, or score truth.

**Files：**

- Create: `src-tauri/src/application/queries/{station_assets,station_detail,key_pool,key_detail}.rs`
- Rewrite: `src-tauri/src/application/queries/pricing_comparison.rs`
- Rewrite: `src-tauri/src/application/queries/pricing_group_monitor_status.rs` 为 revision-compatible overlay
- Create/modify: matching persistence read repositories
- Modify: commands / IPC DTO / registry / binding generator inputs
- Modify: `src/features/stations/**`, `src/features/key-pool/**`, `src/features/pricing/**`
- Delete by end of Task: authoritative functions in `src/lib/projections/{balanceFacts,groupFacts,pricingFacts,pricingGroupRefs}.ts`
- Create: backend/frontend read-model tests

**RED：**

- [ ] Station controller 的 multi-query join、latest snapshot per-station loop、frontend current balance/group reducer 被 gate 捕获。
- [ ] Pricing page 对 raw arrays 重新 join、计算 group ref/hash、回传完整 fact list 的测试先失败。
- [ ] read-model revision mismatch、stale monitoring overlay、cursor mixing、missing credential-safe summary 有 fixture。

**GREEN：**

- [ ] Station Asset / Detail、Key Pool / Detail 在单一 ReadSession 内批量组合共享 projector outputs；history 使用独立 cursor。
- [ ] Pricing Comparison 返回 projected rows；Monitoring Overlay 只接受 server-issued workspace identity/revision/cursor，按 stable group identity join。
- [ ] 前端只做文案、格式、筛选、排序和布局；删除 group identity、rate precedence、pricing match、balance status、capability verdict 计算。
- [ ] generated binding 替代手写 mirror types；未知 contract version / enum 返回 typed error，不默认成 healthy/balanced。
- [ ] sensitive credential edit flow 与 display read model 分离。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_read_models -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test pricing_group_monitor_status -- --nocapture
pnpm.cmd test -- src/features/stations src/features/key-pool src/features/pricing
pnpm.cmd generate:bindings
pnpm.cmd exec tsc --noEmit
node scripts/intelligent-routing-architecture.test.mjs
node scripts/routing-read-model-architecture.test.mjs
```

**Exit gate：** 页面无权威 projector/hash；后端 current facts 无 N+1；Pricing durable/overlay revision join 有 stale 行为；bindings clean。

**Commit：** `refactor: move asset and pricing truth to backend read models`

### Task 9：清除 Query 写副作用并建立 Revision Notice 失效机制

**Review addendum:** move all rollup repair to the startup/supervised projector owner and add a two-concurrent-dashboard-load test. Query code must never open a write transaction, including on stale or missing rollups.

**Files：**

- Create: `src-tauri/src/application/mutation_receipt.rs`
- Create: `src-tauri/src/application/domain_revision_notice.rs`
- Create: `src/lib/query/domainRevisionInvalidation.ts`
- Modify: collector/monitor/proxy finalization commit notification wiring
- Rewrite: `src-tauri/src/application/queries/dashboard_metrics.rs`
- Modify: Dashboard rollup writer/reconciliation owner
- Modify: affected frontend controllers and query key definitions
- Delete: scattered pricing/routing/station invalidation helpers after callers migrate
- Create: `src-tauri/tests/intelligent_routing_revision_notice.rs`
- Create: `src/lib/query/domainRevisionInvalidation.test.ts`
- Modify: Dashboard unit/contract/performance tests

**Steps：**

- [ ] 所有共享 mutation 返回 `MutationReceipt { mutation_id, affected_scopes, revision_vector }`。
- [ ] background commit 后发送 payload-free `DomainRevisionNotice`；notice 只影响 UI freshness，不参与 backend correctness。
- [ ] frontend 唯一 scope-to-query-family mapper 精确 invalidates；组件不再手写跨页面 key arrays。
- [ ] Dashboard `repair_rollups_if_needed` 从 query path 删除；repair 归 source writer、startup reconciliation 或 supervised projector。
- [ ] 所有 application Query gate 禁止 `begin_write`；stale projection 返回 typed status/checkpoint/lag。
- [ ] notice drop/reorder/duplicate 测试证明下次 query revision fence 仍正确。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml application::queries::dashboard_metrics -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_revision_notice -- --nocapture
pnpm.cmd test -- src/lib/query/domainRevisionInvalidation.test.ts
node scripts/dashboard-performance-metrics.test.mjs
node scripts/query-client-contract.test.mjs
node scripts/intelligent-routing-architecture.test.mjs
pnpm.cmd exec tsc --noEmit
```

**Exit gate：** application queries 全部纯读；revision notice mapper 唯一；Dashboard rebuild 不由页面读取触发。

**Commit：** `refactor: make read models pure and revision aware`

---

## Workstream D：Routing Policy 与智能算法

### Task 10：实现 Routing Policy Aggregate、Compiler 和一次性迁移

**Review addendum:** before implementation, create a small policy-mapping ADR and fixture with all six legacy enum values (`AutomaticBalanced`, `PriorityFallback`, `StableFirst`, `BackupOnly`, `CheapFirst`, `CostStableFirst`). Each mapping must state the V1 preset, preserved fields, intentionally lost semantics, and whether the result is `routing_configuration_required`; no silent merge is allowed.

**Files：**

- Create: `src-tauri/src/application/routing_policy.rs`
- Modify: `src-tauri/src/persistence/stores/routing_policy_store.rs`
- Create: `src-tauri/src/commands/routing_policy.rs`
- Create: `src-tauri/src/ipc/dto/routing_policy.rs` and generator input
- Create: `src-tauri/tests/intelligent_routing_policy.rs`
- Modify: foundation migration or add next migration for one-time legacy classification
- Modify later in atomic cutover: Settings model/store/UI fields

**Migration rules：**

- 无歧义迁移 max multiplier、group/tag scope、allow-depleted、Key enabled/role/preference/max concurrency、明确 affinity enable/TTL；
- Top K、旧七权重、sticky bonus、旧六策略、wait/escape 裸参数一律不猜测；
- 发现歧义旧配置写 `routing_configuration_required`，Router admission fail closed，直到用户完整保存 V1 config；
- Task 10 冻结并测试最终 migration transaction 的分类结果；真正删除 active legacy setting rows 的 destructive migration 只在 Task 18 与 production cutover 同 revision 加入，runtime Store/Compiler/UI 此后不解析旧 key；
- reset/reimport 丢弃旧 routing config，保留 canonical assets，并生成 required 状态。

**RED / GREEN：**

- [ ] unknown version、invalid weights sum、negative/overflow constraints、missing system policy、stale expected revision、partial patch 均拒绝且不写。
- [ ] CAS save 使用 complete config；semantic no-op 幂等；成功 bump revision/history/domain revision。
- [ ] compiler 产出 immutable compiled policy 和 stable reason codes；不读 Settings fallback。
- [ ] draft simulation 调同一 compiler，但不保存、不获取 runtime permits。
- [ ] Task 10 之前的 additive foundation 不删除旧 settings，也不让旧 production selector 读取新 aggregate；Task 18 fixture 必须证明 copy/classify/delete 在一次 migration transaction 中完成。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_policy -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_policy_field_e2e -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_upgrade routing_policy -- --nocapture
pnpm.cmd generate:bindings
node scripts/routing-dto-completeness.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
```

**Exit gate：** policy save 原子/CAS；歧义数据 fail closed；production current selector 尚未读取新 policy，等待 atomic cutover。

**Commit：** `feat: add versioned intelligent routing policy`

### Task 11：实现四因子、固定点 Utility 与 Profile 校准

**Files：**

- Create: `src-tauri/src/application/routing_engine/factors.rs`
- Rewrite: `src-tauri/src/application/routing_engine/algorithm_profile.rs`
- Create: `src-tauri/tests/fixtures/intelligent_routing/profiles/v1/**`
- Create: `docs/superpowers/audits/intelligent-routing-profile-v1.md`
- Create: pure golden/property tests

**Steps：**

- [ ] Reliability 使用 posterior/risk/coverage，不重复惩罚 circuit hard gate。
- [ ] Responsiveness 按 request class/model family/context normalization，TTFT 和 total latency coverage 分开。
- [ ] Cost 使用请求前 estimate、币种/单位可比性、unpriced/unknown，不把 unknown 当 0。
- [ ] Preference 只消费用户明确 preference/role/tags，不复制 load/capacity。
- [ ] weights 为整数 basis points，合计规则和 preset 明确；system load correction 不作为可关闭用户权重。
- [ ] 冻结 V1 所有数值：priors、decay、minimum mass、risk threshold、normalization curves、band、exploration、load curve、rank precision、tie-break、limits。
- [ ] 每个编辑字段至少有一组 fixture 证明改变它会改变 factor/contribution/decision trace；无活性字段删除。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml application::routing_engine::factors -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_profile_v1 -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_config_liveness -- --nocapture
node scripts/intelligent-routing-architecture.test.mjs
```

**Exit gate：** V1 profile 无 TBD/magic number；每个 factor 与 UI 字段有 golden/contribution evidence；浮点平台差异不影响排序。

**Commit：** `feat: implement intelligent routing objective factors`

### Task 12：实现 Eligibility、Tier、可靠性保护和 Failure Domains

**Files：**

- Rewrite: `src-tauri/src/application/routing_engine/eligibility.rs`
- Create: `src-tauri/src/application/routing_engine/tiers.rs`
- Create: `src-tauri/src/application/routing_engine/failure_domains.rs`
- Rewrite: relevant health/capability gating modules
- Create: property/table tests

**Steps：**

- [ ] hard eligibility 只处理 administrative、credential、endpoint、protocol/model capability、explicit constraints 和 hard balance boundary。
- [ ] tier projector 处理 Primary/Backup/Emergency；score 不得跨 tier 翻转。
- [ ] reliability safety 只有达到 minimum effective mass 后按 posterior risk / credible bound 拒绝；sample decay 不自动解除 circuit。
- [ ] max-ejection guard 在 failure domain 范围保留有界 emergency candidate，不恢复 hard credential/capability violation。
- [ ] `FailureDomainSet` 区分 endpoint/account/key/model/provider；相关失败不会按多 Key 重复采样/重试。
- [ ] 每个 rejection/tier reason 为 stable code，进入 trace，不使用 debug string。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml application::routing_engine::eligibility -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_tiers -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_failure_domains -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_max_ejection -- --nocapture
```

**Exit gate：** hard/tier/score 类型隔离；failure correlation、minimum mass、max ejection 和 emergency 边界均通过 property tests。

**Commit：** `feat: add routing eligibility tiers and failure domains`

### Task 13：实现候选带、Weighted Rendezvous、Exploration 与重放

**Review addendum:** freeze `ExplorationBudgetRegistry` semantics before coding: choose atomic counter, mutex, or SQLite reservation; specify per-runtime scope, cancellation, restart recovery, and concurrent reservation behavior. Add either a mathematical starvation bound or exhaustive bounded simulation to the exit gate.

**Files：**

- Create: `src-tauri/src/application/routing_engine/dispatch.rs`
- Create: `src-tauri/src/application/routing_engine/exploration.rs`
- Rewrite: `src-tauri/src/application/routing_engine/planner.rs`
- Create: distribution/replay/starvation tests and fixtures

**Steps：**

- [ ] Planner 固定执行 Eligibility -> Tier -> Factor -> Utility -> Band -> Dispatch；无 preset/single-candidate fast path。
- [ ] exploit band 只含同 tier 近优 known candidates；integer weighted rendezvous 冻结 canonical bytes/hash/rank/round/tie-break。
- [ ] Unknown exploration lane 独立于 exploit band；known-bad 不能伪装 Unknown；预算和并发受 profile 限制。
- [ ] starvation fixture 证明符合资格的 Unknown 在稳定流量/预算下有上界获样；distribution test 使用统计容差而非单次顺序。
- [ ] production root seed 由内部安全随机源产生；client request ID 不直接作为 seed；fallback/exploration 使用 domain-separated derivation。
- [ ] trace 普通输出只存 seed commitment；受保护 replay store 保存 full seed；simulation 可使用显式 test seed 但标记 source。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_dispatch -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_exploration -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_replay -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_distribution -- --nocapture
```

**Exit gate：** same input/profile/seed byte-for-byte replay；exploit distribution 和 exploration starvation bound 通过；普通 trace 不泄露 seed。

**Commit：** `feat: implement deterministic routing dispatch`

---

## Workstream E：Runtime、Coordinator、Proxy 与 Routing UI Cutover

### Task 14：收敛 Proxy-instance Runtime State 和共享预算

**Review addendum:** make `ProviderAccount` a production capacity dimension only when a stable provider-account identity is available from canonical station/provider configuration. Otherwise compile only an explicit `NotApplicable` capability, document the V1 limitation, and delete `Trusted`/`EvidenceGap` test-only variants rather than pretending the constraint is enforced.

**Files：**

- Create: `src-tauri/src/services/proxy/routing_runtime.rs`
- Rewrite/split: `src-tauri/src/application/routing_engine/{capacity,affinity,runtime_metrics}.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Create: concurrency/fault tests

**Steps：**

- [ ] 每次 Proxy start 生成唯一 runtime instance ID；revision 单调；restart 防 ABA。
- [ ] capacity、retry、exploration、circuit、half-open、affinity 和 wait queue 由同一 proxy instance state 拥有。
- [ ] planner 只读 immutable overlay；acquire 在 registry 原子校验 candidate-set/runtime/profile fence。
- [ ] 不跨 SQL/HTTP/secret/stream await 持锁；所有 permit/lease 在 EOF/error/cancel/drop/shutdown 归零。
- [ ] retry/exploration 是共享全局/域预算，不在每个 request 新建伪全局 counter。
- [ ] simulation capture overlay 但不 acquire/consume/mutate；shutdown 后旧 permit 不能写新 instance。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_runtime_state -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml intelligent_routing_runtime_concurrency -- --nocapture
```

**Exit gate：** ABA、permit leak、over-admission、shutdown race、simulation mutation 和 per-request budget tests 全部通过。

**Commit：** `refactor: unify proxy routing runtime state`

### Task 15：实现 Route Coordinator、Fallback、Target Fence 与 Decision Trace

**Review addendum:** record the Coordinator/ExecutionEngine boundary: Coordinator owns planning rounds, fallback, admission, decision trace, and replan; ExecutionEngine owns HTTP transport, retries at the protocol boundary, and streaming body lifecycle. Before Task 16, run a cutover rehearsal using a real SQLite PlanningSnapshot and production-shaped composition, and verify new and old IPC commands can coexist in the registry without old commands reaching Proxy.

**Files：**

- Create: `src-tauri/src/application/routing_engine/coordinator.rs`
- Rewrite: `src-tauri/src/application/operational_facts/target_resolver.rs`
- Rewrite: `src-tauri/src/application/queries/{routing_workspace,request_decision_trace,operational_detail}.rs`
- Rewrite: `src-tauri/src/persistence/stores/routing_decisions/**`
- Modify: qualification loopback harness only
- Create: coordinator/trace/loopback tests

**Steps：**

- [ ] Coordinator 拥有 planning rounds、attempt progress、deadline、retry disposition、failure-domain exclusion、bounded replan 和 fallback。
- [ ] 每轮从最新允许的 runtime overlay 重规划；durable facts 变化按 fence policy 重建，不遍历静态 ordered list。
- [ ] acquire lease 后只 resolve 选中 candidate 的 endpoint/credential；revision mismatch 释放 lease、记录 stale-target evidence 并有界重规划。
- [ ] Decision store 写 policy/profile/snapshot/runtime/quality revisions、factor contributions、tier/rejections、failure domains、band/dispatch/exploration、seed commitment、rounds/attempt links。
- [ ] recent decisions 使用 stable cursor；trace 按 ID；not-found/retained-away/corrupt/legacy-opaque 分开。
- [ ] production-like qualification loopback 使用同一 Planner/Coordinator/TargetResolver/Classifier/Finalization，但尚不注册 production Proxy。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_planner_controller -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_decision_store -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test execution_target_resolver -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
node scripts/routing-operational-loopback-contract.test.mjs
node scripts/routing-query-service.test.mjs
```

**Exit gate：** qualification loopback 覆盖 success、capacity miss、target stale、correlated failure、fallback、stream drop、no candidate、config required；trace 可直接查询和 replay。

**Commit：** `feat: complete intelligent routing coordinator loopback`

### Task 16：原子切换 Production Router、Policy、IPC 与 Routing 页面

**Review addendum:** include an explicit UI migration table from every `LocalRoutingWorkspace` consumer (status/edit tabs, diagnostics, cache keys, API gateway) to `RoutingWorkspaceSnapshot`. Add staged checkpoints for Rust lib, Tauri command registry, generated TypeScript bindings, and routing feature compilation; a failed checkpoint blocks the cutover revision rather than leaving a half-wired tree.

**依赖：** Tasks 0-15 全部 exit gate 通过。本 Task 与 Tasks 17-18 不可独立合并或发布。

**Files：**

- Modify: production composition / `AppServices` / command registration
- Rewrite: `src-tauri/src/services/proxy/{routing_repository,execution,runtime,attempt}.rs`
- Rewrite: routing commands / IPC DTO / registry / generated descriptors
- Rewrite: `src/features/routing/**`
- Rewrite: routing query gateway/cache keys
- Modify: Settings UI/types/store boundary to remove routing ownership
- Modify: production composition, loopback, startup and IPC tests

**Steps：**

- [ ] 先锁定 staged cutover manifest；列出 production symbols 和 frontend commands 的 old/new exact replacement。
- [ ] production Proxy 只构造新 PlanningSnapshotBuilder/Planner/RuntimeState/Coordinator/TargetResolver/Finalization chain；Request/Monitoring producer 同时切换到 Task 6-7 已资格化的唯一 Observation/Quality writer。
- [ ] admission 在 policy required/invalid、fact unavailable、writer unavailable 等矩阵下 fail closed；不 fallback old selector/default config。
- [ ] routing save 使用 complete config + expected revision；Routing 编辑页只显示四目标、必要 constraints、affinity 与 system policy，不显示旧 score fields。
- [ ] status/simulation/trace 使用唯一 Routing Workspace query family；durable/runtime join 验证 identity/revisions。
- [ ] regenerate bindings；new commands 进入 registry/capability；old command 不再注册。
- [ ] production composition test 证明新 Planner 真实可达，qualification-only fake 不参与 production。
- [ ] 不在本 Task 结束处交付；立即进入 Task 17。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_composition -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_startup_shutdown -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
pnpm.cmd generate:bindings
pnpm.cmd test -- src/features/routing
pnpm.cmd exec tsc --noEmit
node scripts/intelligent-routing-architecture.test.mjs
```

**Exit gate：** 新 production chain 和 UI 已接通，但只有 Tasks 17-18 完成旧链与旧 schema/contract 删除后才形成可接受 cutover candidate。

**Commit：** 与 Tasks 17-18 合并为 `feat: cut over intelligent routing engine`

### Task 17：删除全部旧后端、前端、Wrapper、Gate 和 Qualification Wiring

**Files：**

- Delete: `runtime_candidate_adapter.rs`、old selector/controller/snapshot/types/health modules 中被新 owner 取代的文件
- Delete: LocalRoutingWorkspace domain/API/query/cache/synchronization/UI remnants
- Delete: old SchedulerAdvancedSettings、policy literals/parsers/defaults、Settings routing fields
- Delete: old frontend projections/hash/view-model reducers and tests that require them
- Delete/merge: no-invariant Routing/Channel/Key command/query facades and API wrappers
- Delete: `models/shared_capabilities.rs` after types move to owners
- Delete: qualification-only wiring and temporary manifest entry
- Rewrite: all affected mocks, DemoBackend, generated binding, fixtures, manifests and contracts runner
- Update: deletion ledger entry-by-entry with evidence

**Required absence：**

```text
RuntimeRoutingCandidate
runtime_candidate_adapter
RouteCandidateProjection as Planner input
PriorityFirst / CostFirst old selector
SchedulerAdvancedSettings and old weight names
LocalRoutingWorkspace and localRoutingWorkspace query key
default_routing_strategy production parser
frontend buildCurrentStationGroupFacts / buildPricingGroupCandidates
canonicalizePricingGroupRefs / hashCanonicalPricingGroupRefs
bulk load_runtime_secrets candidate path
decision trace recent-500 scan
stations.status / station_keys.status production health writeback
positive gate requirements for V2/legacy/compat symbols
intelligent_routing_qualification temporary boundary
```

**Steps：**

- [ ] 每删一个 owner 同步删 import/re-export/DTO/command/mock/fixture/test/doc 正向要求，不保留 dead file。
- [ ] 保留历史 migration/red fixture 中旧文本时放入精确 allowlist；production/source docs 不解析旧值。
- [ ] gate 必须证明 new owner 存在和 old owner 不存在；不能用 empty search 作为唯一成功条件。
- [ ] dead-code inventory production 为 0；无新增 allow/expect 掩盖本升级代码。

**Run：**

```powershell
rg -n "RuntimeRoutingCandidate|runtime_candidate_adapter|SchedulerAdvancedSettings|LocalRoutingWorkspace|buildCurrentStationGroupFacts|buildPricingGroupCandidates|hashCanonicalPricingGroupRefs|default_routing_strategy" src-tauri/src src scripts
node scripts/intelligent-routing-architecture.test.mjs
node scripts/routing-single-owner.test.mjs
node scripts/routing-read-model-architecture.test.mjs
node scripts/dead-code-inventory.mjs --mode ci --scope production
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
pnpm.cmd exec tsc --noEmit
pnpm.cmd test:contracts
```

**Exit gate：** deletion ledger 无 open/temporary/compat-until-later；所有 required absence 只存在于允许的 historical migration/red fixture；新 production loopback 仍通过。

**Commit：** 与 Tasks 16、18 合并为 `feat: cut over intelligent routing engine`

---

## Workstream F：破坏性清理、资格与关闭

### Task 18：执行 destructive schema、generated contract 和文档门禁清理

**Review addendum:** update the portable migration catalog, fingerprints, released fixtures, and import/export redaction contract for every removed health-writeback/settings/legacy-workspace field. Add an explicit zero-reference check for `health_writeback_mode`, `health_writeback_decision`, `health_writeback_reason`, and all old status columns before dropping them.

**依赖：** Task 16 已接线且 Task 17 已删除 source owner；仍处于同一不可交付 cutover snapshot。

**Files：**

- Create: `src-tauri/src/persistence/migrations/00NN_intelligent_routing_cutover_cleanup.sql`
- Modify: schema registry / portable profiles / released fixtures
- Modify: IPC registry/descriptors/generated TypeScript
- Delete or rename: old `routing-task24-predeletion` / migration-readiness scripts and old fixture contracts
- Modify: old routing plan/spec headers to `Superseded` with link to approved spec/plan
- Modify: `docs/README.md`, `docs/PROJECT_PLAN.md`, `docs/PRODUCT_MODEL.md`

**Cleanup contract：**

- drop/rebuild schema to remove old active routing settings and derived station/key status columns;
- remove old candidate/health compatibility columns only after source/read/write references are zero;
- preserve canonical assets, immutable historical request evidence and released migration readability as specified;
- ambiguous legacy route config does not seed quality/policy; reset/import ends in configuration-required;
- old command descriptors/types disappear from generated output; generated hash updated once;
- old gate names do not remain in contracts runner as “pre-deletion” permanent vocabulary。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_startup_cutover -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture
pnpm.cmd generate:bindings
node scripts/routing-dto-completeness.test.mjs
node scripts/intelligent-routing-architecture.test.mjs
pnpm.cmd test:contracts
git diff --check
```

**Exit gate：** fresh/reset/import/current-schema upgrade 只产生新合同；active docs 只有批准 spec + 本计划；generated/registry/runner 无 dead command；此时 Tasks 16-18 才共同形成 cutover candidate。

**Commit：** 与 Tasks 16-17 合并为 `feat: cut over intelligent routing engine`

### Task 19：完整算法、故障、性能、安全与运行资格

**Files：**

- Create: `scripts/intelligent-routing-qualification.mjs`
- Create: `scripts/run-intelligent-routing-soak.ps1`
- Create: `docs/superpowers/audits/intelligent-routing-qualification.md`
- Create/update: deterministic fixture/performance datasets; generated outputs remain ignored unless explicitly versioned summary

**Qualification matrix：**

- deterministic replay：fixed input/profile/seed/output bytes；
- property：hard gates never overridden、tier monotonicity、bounds/overflow、duplicate/out-of-order equivalence；
- distribution：weighted dispatch tolerance、exploration budget/share/starvation；
- concurrency：capacity/retry/exploration admission、policy CAS、runtime restart/shutdown；
- fault：DB busy/commit unknown/projector lag/writer down/monitor down/secret resolver fail/target stale/stream drop；
- performance：100 stations、1,000 keys、100k observations、100k decisions、500k request facts；snapshot p95、plan p95、query p95、writer regression、memory/query bound；
- security：API key/cookie/auth/full seed/full URL/prompt canary scans、IPC redaction、trace export；
- operational：configured/unconfigured startup、Proxy restart、reset/reimport、fresh DB、bounded soak、all leases/gauges zero after shutdown。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml
pnpm.cmd test
pnpm.cmd build
pnpm.cmd test:contracts
pnpm.cmd architecture:fixtures
pnpm.cmd architecture:typescript
pnpm.cmd architecture:commands
pnpm.cmd architecture:security
pnpm.cmd architecture:artifacts
node scripts/dead-code-inventory.mjs --mode ci --scope production
node scripts/intelligent-routing-qualification.mjs
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-intelligent-routing-soak.ps1 -Smoke
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-local-self-check.ps1
git diff --check
```

真实 provider / CCSwitch / sleep-resume 只有用户显式授权和可用 secret 时运行；未授权记录 `not-run-without-user-authorization`，不能用 fixture 冒充，也不能因此恢复旧 router。

**Exit gate：** 所有自动化命令退出 0；性能达到 Task 0 冻结门槛；audit 记录环境、数据规模、p50/p95、失败注入、未授权观察和 artifact 路径。

**Commit：** `test: qualify intelligent routing engine`

### Task 20：关闭验收矩阵、删除台账和实施状态

**Files：**

- Modify: `docs/superpowers/audits/intelligent-routing-acceptance-matrix.md`
- Modify: `docs/superpowers/audits/intelligent-routing-deletion-ledger.md`
- Modify: `docs/superpowers/audits/intelligent-routing-boundary-manifest.json`
- Modify: `docs/superpowers/audits/intelligent-routing-qualification.md`
- Modify: approved spec / this plan status only after evidence exists
- Modify: `docs/README.md`

**Steps：**

- [ ] 逐条附上 78 条 acceptance 的 test/command/commit/artifact evidence；不得批量写“由 full suite 覆盖”。
- [ ] deletion ledger 所有项为 `deleted` 或明确属于 historical migration/red fixture；不得有 `temporary`、`ignored`、`later`。
- [ ] manifest source revision、generated hash、schema version、algorithm/profile/projector versions 与 binary 一致。
- [ ] 重新运行 source search，确认 active docs 不再描述旧 selector/weights/workspace 为当前能力。
- [ ] 只有全部 gate 通过后把 spec 改为 `Implemented`、plan 改为 `Completed`；否则保持 Planned/In progress 并列出真实未完成项。

**Run：**

```powershell
node scripts/intelligent-routing-architecture.test.mjs
node scripts/intelligent-routing-qualification.mjs
node scripts/dead-code-inventory.mjs --mode ci --scope production
pnpm.cmd test:contracts
git status --short
git diff --check
```

**Exit gate：** 78/78 acceptance 有独立证据；deletion ledger 为零；没有运行中的 required process；文档状态与实际实现一致。

**Commit：** `docs: close intelligent routing upgrade evidence`

## 8. 每 Task 通用交付模板

执行者在每个 Task 的 audit / commit message 中记录：

```text
Task:
Start HEAD / End HEAD:
Dirty paths preserved:
RED command and expected failure:
GREEN focused command:
Affected suite command:
Architecture gate:
Files added/modified/deleted:
Temporary entries introduced/deleted:
Security-sensitive paths reviewed:
Remaining blockers:
```

Task 完成说明必须包含：改了什么、如何验证、哪些外部观察未运行、是否存在未删除 temporary entry。不能只报告“测试通过”。

## 9. 禁止的计划偏移

- 不把 Tasks 16-18 拆成可独立发布的后端、前端或 schema 三期。
- 不为迁移方便恢复 production feature flag 或 old router fallback。
- 不先接新 score，再把 canonical facts / quality / read models 留到以后。
- 不把 frontend matcher 改名为 view model 后继续保留领域判断。
- 不让 Dashboard/Channel/Pricing workspace 成为 Router 输入。
- 不把 ordered projector runner 扩展成通用动态事件总线。
- 不让 Query repair write、页面 refresh 或 React Query invalidation 承担后端正确性。
- 不用 `V2` / `next` / `compat` 永久命名目标模块。
- 不用 `allow(dead_code)`、test-only production equivalent、空 trait default 或 source regex 假证据完成 gate。
- 不保留旧文件“以防回滚”；恢复依赖 Git 历史和 reset/reimport，不依赖生产双实现。
- 不在未冻结 profile version 和数值参数时开始 production cutover。
- 不以真实 provider 未授权为由跳过 deterministic/fault/performance 自动化。
