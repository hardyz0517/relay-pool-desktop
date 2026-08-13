# 状态监控内核重构实施计划

状态：Proposed，等待按任务执行
日期：2026-07-29
目标规范：`docs/specs/STATUS_MONITORING_REFACTOR_SPEC.md`
参考基线：Relay Pulse commit `c62537085f4202f6f1f28716f45c107303f2836f`，MIT License

## 1. 目标与架构决策

本计划实施一次监控内核重构，不以现有 `ChannelMonitorRun`、30 秒轮询和前端拼装统计为长期基础。保留 Tauri IPC、Application Service、Persistence Runtime、SQLite/SQLx、SecretManager、AsyncOutboundClient、TaskSupervisor、React Query 和 Station/Station Key 所有权模型；重建监控领域、协议适配、请求画像、执行与目标结果、调度、健康 observation、rollup/read model 和横向状态 UI。

最终事实链固定为：

```text
MonitorDefinition
  -> MonitorExecution
    -> MonitorTargetResult (每个 Station Key 唯一终态)
      -> ProbeAttempt (网络事实，可有 retry/fallback)
    -> HealthObservation (以 target_result_id 幂等)
  -> BucketRollup (可重建派生数据)
  -> ChannelStatusWorkspace (后端读模型)
```

关键决策：

- availability、趋势桶和当前 synthetic 状态只消费 `MonitorTargetResult`，不直接消费 attempts。
- attempt append、target finalization、execution finalization 是三个独立事务边界；rollup 不阻塞 execution 完成。
- 标准 OpenAI、Anthropic、Gemini、xAI/Grok adapter 全部实现；CLI compatibility profile 可选、版本化、受控。
- `grok_cli_compat` 在没有可验证 fixture 和授权实测前保持 disabled，xAI/Grok 标准 adapter 正常交付。
- 手动和定时执行只进入同一个 `MonitorOrchestrator`。
- 当前散落的 proxy/request-log/monitor 健康写入收敛到一个 `HealthTransitionService`。
- 最终删除旧 runner、旧 run write path、旧前端趋势生成和旧业务 facade；不保留长期 runtime selector 或 dual write。

## 2. 执行规则

1. 每个 Task 开始前运行 `git status --short`，记录与当前任务重叠的用户改动。当前已知冲突热点包括 `src-tauri/src/persistence/stores/request_log_store.rs`、`src-tauri/src/application/app_services.rs`、`src-tauri/src/app_composition.rs`、`src-tauri/src/runtime_composition.rs`、`src/lib/bridge/BackendClient.ts`、生成绑定与命令 registry；只合并任务 hunk，不覆盖现有改动。
2. migration 文件名使用执行时的下一可用编号。当前工作区已有 `0009_provider_drafts.sql`，不得在计划或代码里假定固定 `0010`；先枚举目录，再选择编号。
3. 不使用 `git add .` 或 `git add -A`。需要提交时只 stage 当前 Task 的明确路径，并检查 `git diff --cached --check` 与 `git diff --cached`。
4. 采用 RED-GREEN-REFACTOR。每个行为任务必须先看到指定测试因缺失能力而失败，再做最小完整实现，最后运行该任务的回归命令。
5. fixture、日志、测试报告和截图不得包含 API key、Cookie、Authorization、完整 prompt/response 或可还原账号身份的 CLI metadata。
6. 默认测试只使用本地 fixture 和 loopback server。真实 provider 验证必须显式授权、低频、有预算、从环境解析 secret，且不进入默认 CI。
7. 任何新 background task 必须由现有 `TaskSupervisor` 管理，有 cancellation、bounded drain、状态与指标；禁止 detached spawn 和无界 channel。
8. production 切换前可以保留只读 legacy 数据和机械适配器，但禁止 dual write。切换完成的同一阶段删除旧生产 authority。
9. 任何 Task 的退出命令没有真实退出 0，状态保持未完成。超时、只跑部分测试或只看日志均不算通过。

## 3. 目标文件地图

| 路径 | 完成后职责 |
|---|---|
| `src-tauri/src/models/monitoring/` | 纯领域类型、不变量、策略与 reducer；不依赖 Reqwest/SQLx/Tauri |
| `src-tauri/src/application/monitoring/` | definition commands、orchestrator、planner、recorder、queries |
| `src-tauri/src/application/health_transitions.rs` | 所有来源唯一健康状态转换入口 |
| `src-tauri/src/services/monitoring/adapters/` | OpenAI/Anthropic/Gemini/xAI/Generic 协议请求与解析 |
| `src-tauri/src/services/monitoring/profiles/` | 标准及 CLI compatibility profile registry/golden definitions |
| `src-tauri/src/services/monitoring/transport.rs` | 受限请求发送、阶段计时、流读取、取消与响应大小边界 |
| `src-tauri/src/services/monitoring/scheduler.rs` | nearest-due queue、通知、并发许可与生命周期 |
| `src-tauri/src/persistence/stores/monitoring/` | definition/execution/status/retention/budget repositories |
| `src-tauri/src/application/queries/channel_status.rs` | 参数化 workspace read model，不再混合 raw request logs |
| `src/features/channels/` | 横向状态工作区、详情、配置与前端状态控制 |

## 4. 类型引入顺序

| 类型 | 首次引入 Task | 最终 owner |
|---|---:|---|
| `ProtocolKind`, `ProbeOutcome`, `FailureKind` | 1 | `models/monitoring/outcome.rs` |
| `MonitorDefinition`, `DefinitionRevision` | 1 | `models/monitoring/definition.rs` |
| `MonitorExecution`, `MonitorTargetResult`, `ProbeAttempt` | 1 | `models/monitoring/execution.rs` |
| `RetryPolicy`, `RiskPolicy`, `HealthPolicy`, `SchedulePolicy` | 1 | `models/monitoring/policy.rs` |
| `Challenge`, `ValidationStrategy`, `ParsedProbeResponse` | 2 | `services/monitoring/challenge.rs`, `adapters/contract.rs` |
| `ClientProfile`, `ProfileVersion`, `AuthStrategy` | 5 | `services/monitoring/profiles/` |
| `ProbePlan`, `TargetPlan`, `AttemptPlan` | 8 | `application/monitoring/planner.rs` |
| `HealthObservation`, `HealthTransition` | 10 | `models/health.rs`, `application/health_transitions.rs` |
| `ChannelStatusWorkspaceV2`, `StatusBucket` | 12 | `models/shared_capabilities.rs` 或专用 monitoring read-model 模块 |

## 5. Task 0：冻结基线、协议依据和删除账本

**Files:**

- Create: `docs/archive/audits/2026-07-29-status-monitoring-baseline.md`
- Create: `docs/audits/status-monitoring-boundary-manifest.json`
- Create: `docs/audits/status-monitoring-deletion-ledger.md`
- Create: `docs/audits/status-monitoring-protocol-sources.md`
- Read only: `src-tauri/src/services/channel_monitors/**`
- Read only: `src-tauri/src/application/monitoring.rs`
- Read only: `src-tauri/src/application/queries/channel_status.rs`
- Read only: `src-tauri/src/persistence/stores/{monitoring_store,request_log_store,routing_store}.rs`
- Read only: `src/features/channels/**`

**Steps:**

- [ ] 记录 `git status --short --branch`、当前 commit、所有重叠 dirty hunk owner，不修复无关改动。
- [ ] 运行并记录当前 monitoring Rust tests、前端 tests、contract tests 和构建的精确通过/失败数量及耗时。
- [ ] 记录旧符号及所有 production consumers：`ChannelMonitorRun`、`CompletedMonitorProbe`、`run_monitor_probe`、`RUNNER_POLL_INTERVAL`、`ACTIVE_MONITOR_RUNS`、`record_probe_outcome`、`channel_monitor_runs`、前端 `buildRecentOutcomes`/`healthToRecentOutcomes`。
- [ ] boundary manifest 列出允许依赖方向、现有例外、目标 owner、generated files 和所有跨层 SQL/Reqwest/Tauri import。
- [ ] protocol sources 固定 OpenAI Chat/Responses、Anthropic Messages、Gemini generateContent/streamGenerateContent、xAI Chat 的官方文档 URL、验证日期和支持范围；Relay Pulse 只记录概念映射，不作为官方协议替代。
- [ ] 对用户提供的横向 UI 截图记录结构观察：工具栏筛选、稳定列宽、状态/可用率/最近检测、固定趋势格；明确不复制深色视觉、品牌、文字或源码。

**Run:**

```powershell
git status --short --branch
git log -5 --oneline
rg -n "ChannelMonitorRun|CompletedMonitorProbe|run_monitor_probe|RUNNER_POLL_INTERVAL|ACTIVE_MONITOR_RUNS|record_probe_outcome|channel_monitor_runs|buildRecentOutcomes|healthToRecentOutcomes" src-tauri/src src scripts
cargo test --manifest-path src-tauri/Cargo.toml --lib channel_monitor -- --nocapture
pnpm.cmd test -- src/features/channels src/lib/api/channelMonitors.test.ts src/lib/queries/channelQueries.test.ts
pnpm.cmd test:contracts
pnpm.cmd build
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 基线必须可复现；任何已有红项已归因并单独处理，不能把红基线默认为本重构可接受状态。删除账本的每个旧 symbol 都有 owner、consumer 和最终删除 Task。

## 6. Task 1：建立纯监控领域模型与架构门禁

**Files:**

- Create: `src-tauri/src/models/monitoring/mod.rs`
- Create: `src-tauri/src/models/monitoring/definition.rs`
- Create: `src-tauri/src/models/monitoring/execution.rs`
- Create: `src-tauri/src/models/monitoring/outcome.rs`
- Create: `src-tauri/src/models/monitoring/policy.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/tests/monitoring_domain.rs`
- Create: `scripts/monitoring-architecture.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`
- Modify: `docs/audits/status-monitoring-boundary-manifest.json`

**RED:**

- [ ] 测试配置拒绝 scope/key 不一致、空 primary model、重复/超过 3 个 fallback、profile/protocol 不兼容、无可行 primary attempt、负值或越界调度配置。
- [ ] 测试 `Execution -> TargetResult -> Attempt` 唯一关系、decisive attempt 所有权、零 attempt 只允许 skipped。
- [ ] 测试 retry/fallback 后成功降级、skipped 不进分母、execution summary 不受 target 写入顺序影响。
- [ ] 架构测试禁止 `models/monitoring` import `tauri`、`sqlx`、`reqwest`、persistence 或 services。

**GREEN:**

- [ ] 用 closed enum/newtype 表达 protocol、outcome、failure、trigger/status、revision、timeout 和 policy；不暴露任意业务字符串。
- [ ] reducer 使用纯函数：attempts -> target result、target results -> execution summary、results -> availability。
- [ ] 为 legacy HTTP-only 数据定义显式 `SemanticConfidence::LegacyHttpOnly`，不让它参与新健康写回。
- [ ] `models/channel_monitors.rs` 暂时只保留旧 DTO，禁止新内核反向依赖它。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_domain -- --nocapture
node scripts/monitoring-architecture.test.mjs
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 领域不变量全部由构造器/reducer 保证；不存在依赖网络、数据库或 UI 的领域测试。

## 7. Task 2：协议契约、challenge 与增量解析地基

**Files:**

- Create: `src-tauri/src/services/monitoring/mod.rs`
- Create: `src-tauri/src/services/monitoring/challenge.rs`
- Create: `src-tauri/src/services/monitoring/adapters/mod.rs`
- Create: `src-tauri/src/services/monitoring/adapters/contract.rs`
- Create: `src-tauri/src/services/monitoring/adapters/http_mapping.rs`
- Create: `src-tauri/src/services/monitoring/adapters/sse.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Create: `src-tauri/tests/monitoring_adapter_contracts.rs`
- Create: `src-tauri/tests/fixtures/monitoring/common/**`

**RED:**

- [ ] 为普通 JSON、SSE 随机 chunk、CRLF/LF、UTF-8 跨 chunk、协议 error、提前 EOF、缺 completion、空输出、body 超限写 table-driven tests。
- [ ] HTTP 200 HTML/error JSON/空 body 必须失败；2xx 只有 parser 完整且 validator 命中才 available。
- [ ] challenge token 使用 CSPRNG、短、低 token；validator 只比较规范化预期内容，不持久化 expected answer。

**GREEN:**

- [ ] `ProtocolAdapter` 只负责能力校验、请求描述、响应增量解析和错误映射，不读 secret/DB，不决定 retry。
- [ ] SSE parser 有硬 response-byte/output-byte/event-count 上限，保留必要计时但不保留正文。
- [ ] 建立 `FailureKind` 的 HTTP/transport/protocol 映射表；未知错误 fail closed 为 `protocol_mismatch` 或 `internal`，不标绿。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts common_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::monitoring -- --nocapture
```

**Exit gate:** parser 可在任意合法 chunk 边界下得到相同结论；没有 EOF 即成功、status-only 成功或无限响应读取。

## 8. Task 3：OpenAI、Responses、Generic 与 xAI/Grok adapters

**Files:**

- Create: `src-tauri/src/services/monitoring/adapters/openai_chat.rs`
- Create: `src-tauri/src/services/monitoring/adapters/openai_responses.rs`
- Create: `src-tauri/src/services/monitoring/adapters/generic_openai.rs`
- Create: `src-tauri/src/services/monitoring/adapters/xai_grok.rs`
- Modify: `src-tauri/src/services/monitoring/adapters/mod.rs`
- Create: `src-tauri/tests/fixtures/monitoring/{openai_chat,openai_responses,generic_openai,xai_grok}/**`
- Modify: `src-tauri/tests/monitoring_adapter_contracts.rs`

**Steps:**

- [ ] 分别实现 Chat Completions 与 Responses 普通/流式 terminal 语义，不用一个松散 parser 猜两套 envelope。
- [ ] Responses 覆盖 `response.completed`、`response.failed`、`response.incomplete`、output text delta 和 usage/model extraction。
- [ ] Chat 覆盖 delta、finish reason、`[DONE]`、流内 error 和无 content completion。
- [ ] Generic 只实现明确最小 OpenAI-compatible 交集，vendor dialect 必须由 capability 明示。
- [ ] xAI/Grok 使用独立 adapter/capability 声明，即使 wire shape 与 OpenAI 接近也不以 URL 猜测。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts openai_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts generic_openai_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts xai_grok_ -- --nocapture
```

**Exit gate:** 四个 adapter 的普通/流式、200 假成功、401/403/429/400/422/5xx、usage/model、redaction fixtures 全绿。

## 9. Task 4：Anthropic 与 Gemini Native adapters

**Files:**

- Create: `src-tauri/src/services/monitoring/adapters/anthropic_messages.rs`
- Create: `src-tauri/src/services/monitoring/adapters/gemini_native.rs`
- Modify: `src-tauri/src/services/monitoring/adapters/mod.rs`
- Create: `src-tauri/tests/fixtures/monitoring/{anthropic_messages,gemini_native}/**`
- Modify: `src-tauri/tests/monitoring_adapter_contracts.rs`

**Steps:**

- [ ] Anthropic 分离 API version、Messages body、content block delta、message stop、流内 error 和 usage。
- [ ] Gemini Native 分离 `generateContent`/`streamGenerateContent`、candidate parts、finish reason、blocked/safety、API error 和 usageMetadata。
- [ ] Gemini OpenAI-compatible 只通过 Generic/OpenAI adapter + 已持久化 dialect 进入，不与 Gemini Native 自动互试。
- [ ] `protocol_kind=auto` 只读 capability facts；未知或冲突返回 `needs_configuration`，网络调用数为 0。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts anthropic_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts gemini_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts protocol_auto_ -- --nocapture
```

**Exit gate:** Anthropic/Gemini 协议错误和 safety/block 不会误判可用；auto 不产生逐协议试探。

## 10. Task 5：标准与 CLI compatibility profile registry

**Files:**

- Create: `src-tauri/src/services/monitoring/profiles/mod.rs`
- Create: `src-tauri/src/services/monitoring/profiles/registry.rs`
- Create: `src-tauri/src/services/monitoring/profiles/standard.rs`
- Create: `src-tauri/src/services/monitoring/profiles/codex_cli.rs`
- Create: `src-tauri/src/services/monitoring/profiles/claude_code.rs`
- Create: `src-tauri/src/services/monitoring/profiles/gemini_cli.rs`
- Create: `src-tauri/src/services/monitoring/auth.rs`
- Create: `src-tauri/tests/monitoring_profile_golden.rs`
- Create: `src-tauri/tests/fixtures/monitoring/profiles/**`
- Modify: `docs/audits/status-monitoring-protocol-sources.md`

**RED:**

- [ ] golden tests 比较 method/path/header names/body shape/defaults/hash，不包含 header secret values。
- [ ] profile 不能覆盖 Authorization/API key/Cookie、完整 URL、host、TLS、proxy、body limit 或 redaction。
- [ ] profile 与 adapter capability 不兼容时保存/执行均失败。
- [ ] installation/station/profile scoped local identity 稳定、可重置且不含真实账号或设备标识。

**GREEN:**

- [ ] 注册 `standard_api`、`codex_cli_compat`、`claude_code_compat`、`gemini_cli_compat` 的明确版本。
- [ ] CLI profile 只实现经 fixture 证明且必要的 User-Agent、受控 header/body defaults；不复制大型 system prompt、tools 或 OAuth/device identity。
- [ ] registry 暴露 capability、版本、弃用/升级信息；execution snapshot 固定 profile version/hash。
- [ ] 保留 `grok_cli_compat` capability ID 但 `enabled=false`，直到单独的验证变更具备证据。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_profile_golden -- --nocapture
rg -n "Authorization|Bearer |api[_-]?key|Cookie|sk-[A-Za-z0-9]" src-tauri/tests/fixtures/monitoring
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** profile 变化必须显式改版本和 golden；安全扫描无命中，Grok CLI 未被无证据启用。

## 11. Checkpoint A：协议地基评审

继续前必须同时满足：

- [ ] Domain、common parser、五类 provider adapter、Generic 和 profiles 全部测试通过。
- [ ] 任何 HTTP 200 假成功 fixture 都不能 available。
- [ ] 官方协议依据、profile 来源和 Relay Pulse attribution 已记录。
- [ ] architecture gate 证明 domain/adapters 不依赖 persistence、scheduler 或 Tauri。

若仍需在 Generic parser 内按 provider 名/URL 堆条件，停止实施并回到 adapter/dialect 设计；不得带着模糊协议进入数据库和调度器。

## 12. Task 6：数据库 migration、backfill 与恢复证明

**Files:**

- Create: `src-tauri/src/persistence/migrations/<NEXT>_status_monitoring_v2.sql`
- Modify: `src-tauri/src/persistence/migrations.rs`
- Modify: `src-tauri/tests/persistence_pricing_monitoring.rs`
- Create: `src-tauri/tests/monitoring_migration.rs`
- Modify: `src-tauri/tests/persistence_upgrade/fixtures/profile_001/expected_manifest.json`
- Modify/create released-schema fixtures according to existing persistence V2 workflow
- Create: `docs/audits/status-monitoring-migration-manifest.md`

**Schema requirements:**

- [ ] 演进 `channel_monitors`：primary/fallback、protocol/profile version、retry/risk/health policy、attempt/execution timeout、schedule revision、INTEGER millisecond due time；旧 latest fields 降为非权威或迁移后停止写。
- [ ] 新建 `channel_monitor_executions`、`channel_monitor_target_results`、`channel_monitor_attempts`。
- [ ] 新建 `channel_monitor_bucket_rollups`、`channel_monitor_rollup_dirty_ranges`。
- [ ] 新建 `station_key_health_observations` 和 `channel_monitor_probe_budget_usage`。
- [ ] custom profiles 如进入本阶段，独立版本化表且 JSON 有 schema version；内置 profiles 不以可编辑 row 覆盖代码 registry。
- [ ] outcome/failure/status/trigger 使用 CHECK；业务查询时间全部为 INTEGER Unix ms。
- [ ] 加入 spec 16 章定义的 unique/index/foreign key；用测试验证 decisive attempt 的跨列所有权。

**Backfill requirements:**

- [ ] 旧 `fallback_models[0]` -> primary，其余去重后 -> fallback；空值使用明确默认并记录 warning。
- [ ] 每个旧 run -> legacy execution + target result + attempt；只有可证明的批次才合并 station-wide execution。
- [ ] legacy result 标记 `LegacyHttpOnly`，只读展示、不生成 health observation。
- [ ] 对比 definition/run 前后数量、orphan、重复 identity、null/invalid timestamp；任一不符回滚 migration。
- [ ] 迁移前走既有 V2 backup/recovery 机制；不自行发明第二套数据库备份目录。

**Run:**

```powershell
Get-ChildItem src-tauri/src/persistence/migrations | Sort-Object Name
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_migration -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_pricing_monitoring -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** fresh DB、released fixture upgrade、migration rollback/retry 全部通过；manifest 计数一致；新代码尚未激活 production write。

## 13. Task 7：拆分 monitoring repositories 与三段事务

**Files:**

- Create: `src-tauri/src/persistence/stores/monitoring/mod.rs`
- Create: `src-tauri/src/persistence/stores/monitoring/definitions.rs`
- Create: `src-tauri/src/persistence/stores/monitoring/executions.rs`
- Create: `src-tauri/src/persistence/stores/monitoring/status_queries.rs`
- Create: `src-tauri/src/persistence/stores/monitoring/retention.rs`
- Create: `src-tauri/src/persistence/stores/monitoring/budgets.rs`
- Delete after consumers move: `src-tauri/src/persistence/stores/monitoring_store.rs`
- Modify: `src-tauri/src/persistence/stores/mod.rs`
- Create: `src-tauri/tests/monitoring_persistence.rs`

**RED:**

- [ ] attempt append 的 `(execution,target,model_index,attempt_number)` 和 ID 重放不重复。
- [ ] target finalization 对相同 target/result ID 幂等，attempt_count/decisive ownership 不符时完整 rollback。
- [ ] health observation `(source,source_event_id)` exactly-once；重放不增加健康计数。
- [ ] execution 只有全部 target results 终态才能完成；summary 和 next due 重放一致。
- [ ] rollup failure 只写/合并 dirty range，不回滚 execution；dirty marker 数量有界。
- [ ] budget reservation 在网络前原子计数，并发与重启测试不能越过日上限。

**GREEN:**

- [ ] repositories 只映射 SQL/rows，不做 retry、fallback、健康阈值或 protocol 判断。
- [ ] application recorder 拥有三段 transaction 编排；commit outcome unknown 通过稳定 ID 对账。
- [ ] 所有 list API 使用 bounded limit/cursor 和稳定 ID 次排序。
- [ ] 为 recent、due、execution detail、bucket workspace、retention 写 `EXPLAIN QUERY PLAN` 断言。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_persistence -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_pricing_monitoring -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 三段事务、幂等和 query plan 有真实 SQLite integration test；没有 store 内隐藏业务 reducer。

## 14. Task 8：ProbePlanner、retry/fallback 与 execution reducer

**Files:**

- Create: `src-tauri/src/application/monitoring/mod.rs`
- Create: `src-tauri/src/application/monitoring/commands.rs`
- Create: `src-tauri/src/application/monitoring/planner.rs`
- Create: `src-tauri/src/application/monitoring/orchestrator.rs`
- Create: `src-tauri/src/application/monitoring/recorder.rs`
- Delete after moving old reads: `src-tauri/src/application/monitoring.rs`
- Create: `src-tauri/tests/monitoring_orchestrator.rs`

**RED:**

- [ ] station scope 在 execution snapshot 时展开 Key；编辑 definition 不改变运行中计划。
- [ ] auto protocol 从 persisted capability 唯一解析，未知为 skipped/needs_configuration，调用 transport 0 次。
- [ ] retry 只对 retryable failure；auth/TLS/invalid/profile rejection 不 retry；429 尊重 Retry-After 和 deadline。
- [ ] primary 完成允许 attempts 后才 fallback；auth、profile rejection、target-level 429 不用换模型掩盖。
- [ ] deadline 覆盖 permit/backoff/attempt/fallback；剩余时间不足不启动请求。
- [ ] retry/fallback 成功得到 degraded target result；attempt 顺序完整但 availability 分母只加 1。
- [ ] station-wide target 执行/写入顺序随机时 execution summary 相同。

**GREEN:**

- [ ] `ProbePlanner` 生成不可变、有上限 `ProbePlan`，保存 resolved adapter/dialect/profile hash/endpoint revision。
- [ ] `MonitorOrchestrator::request_execution` 是 manual/scheduled 唯一入口，手动幂等键返回已有 execution。
- [ ] recorder 每个 attempt 即时 append，每个 target 单独 finalization，最后 execution finalization。
- [ ] 单 target 内 attempt 串行；station targets 可并发但全部受 cancellation/deadline。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_orchestrator -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib application::monitoring -- --nocapture
```

**Exit gate:** 使用 fake clock/fake transport/fake repository 的 deterministic tests 证明所有决策；不接 production runner。

## 15. Task 9：受限 Transport、Secret 与真实请求执行链

**Files:**

- Create: `src-tauri/src/services/monitoring/transport.rs`
- Create: `src-tauri/src/services/monitoring/executor.rs`
- Modify: `src-tauri/src/services/monitoring/mod.rs`
- Modify if required: `src-tauri/src/outbound/mod.rs`
- Modify if required: `src-tauri/src/outbound/client.rs`
- Modify if required: `src-tauri/src/outbound/policy.rs`
- Modify: `src-tauri/src/application/credentials.rs`（仅复用/补齐 scoped secret resolve）
- Create: `src-tauri/tests/monitoring_transport.rs`
- Create: `src-tauri/tests/monitoring_execution_integration.rs`

**RED:**

- [ ] DNS/connect/TLS/header timeout/body timeout/cancel/body limit/redirect/HTTP proxy/SOCKS 的 typed mapping。
- [ ] warm client 复用连接；cold diagnostic 独立 client 且不得成为默认健康 authority。
- [ ] secret 只在发送前解析，request/profile/debug 输出均无 secret；endpoint revision 在发送前和写回前校验。
- [ ] first headers、first content、total latency 在 buffered/streaming 下定义一致。
- [ ] 取消发生在 secret resolve、permit、connect、stream、backoff 任一阶段均有明确 terminal。

**GREEN:**

- [ ] 复用 `AsyncOutboundClient`/Reqwest pool，不为每 attempt 默认新建 client。
- [ ] adapter 产出受控 request descriptor，AuthStrategy 最后注入 secret；redirect/target host 遵循现有 outbound security policy。
- [ ] response bytes、events、output tokens、attempt timeout 和 execution deadline 全部硬限制。
- [ ] request observation 仅存 method、相对 path、协议、profile hash、计时、usage/cost 和脱敏错误。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_transport -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_execution_integration -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** loopback server 证明协议、transport、secret 和 recorder 联通；测试输出/DB 经过 canary scan 无泄漏。

## 16. Task 10：统一 HealthObservation 与 HealthTransitionService

**Files:**

- Create: `src-tauri/src/models/health.rs`
- Create: `src-tauri/src/application/health_transitions.rs`
- Modify: `src-tauri/src/application/mod.rs`
- Modify: `src-tauri/src/application/routing_engine/routing_health.rs`
- Modify: `src-tauri/src/application/request_finalization.rs`
- Modify: `src-tauri/src/persistence/stores/request_log_store.rs`
- Modify: `src-tauri/src/persistence/stores/routing_store.rs`
- Create: `src-tauri/src/persistence/stores/health_observation_store.rs`
- Modify: `src-tauri/src/persistence/stores/mod.rs`
- Create: `src-tauri/tests/station_key_health_transitions.rs`

**RED:**

- [ ] proxy request、synthetic monitor、manual connectivity 映射到同一 observation contract。
- [ ] duplicate source event 不重复 success/failure/cooldown 计数。
- [ ] available/degraded/unavailable/skipped、failure/recovery threshold、429 Retry-After、auth/revoked、endpoint revision、traffic equivalence/writeback mode 矩阵。
- [ ] diagnostic/observe-only 不影响路由；CLI profile 与真实流量不等价时 auth failure 不 hard-disable Key。
- [ ] proxy attempt 和对应 health transition 仍保持原有 exactly-once transaction，不因抽 service 降级。

**GREEN:**

- [ ] `routing_health.rs` 保留纯 reducer；`HealthTransitionService` 负责 load/reduce/observation/upsert transaction。
- [ ] `RequestFinalizationService` 和 monitoring recorder 调用同一 service/port。
- [ ] 从 `request_log_store.rs` 删除 `apply_attempt_health` 及其私有状态转换；store 只做持久化映射。
- [ ] 审计 proxy runtime、routing store、connectivity path，所有 production health write 都登记并收敛；迁移适配器不得双写。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test station_key_health_transitions -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib application::request_finalization -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib application::routing_engine::routing_health -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_persistence -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 仓库只有一个健康业务 reducer 和一个 application 写入口；request lifecycle 原有持久化/幂等测试不回归。

## 17. Task 11：nearest-due scheduler、并发与生命周期

**Files:**

- Create: `src-tauri/src/services/monitoring/scheduler.rs`
- Create: `src-tauri/src/services/monitoring/runtime.rs`
- Modify: `src-tauri/src/services/monitoring/mod.rs`
- Modify: `src-tauri/src/app_composition.rs`
- Modify: `src-tauri/src/application/app_services.rs`
- Modify: `src-tauri/src/runtime_composition.rs`
- Create: `src-tauri/tests/monitoring_scheduler.rs`

**RED with Tokio paused time:**

- [ ] 最近到期唤醒、definition edit notify、startup stagger、正向 jitter、至少 interval。
- [ ] 系统休眠/长任务/重启不 catch-up storm；manual 不移动 scheduled baseline。
- [ ] global -> station -> key 固定 permit 顺序，global/station/key 上限均生效，无 permit leak。
- [ ] 同 monitor execution single-flight、同 Key synthetic single-flight、manual/scheduled 重叠返回同 execution。
- [ ] queue 满记录 lag 且不 busy-loop；等待 queue/permit/backoff 均可取消且受 deadline 控制。
- [ ] shutdown：停止 admission -> 取消 queued -> bounded drain running -> interrupted -> join。

**GREEN:**

- [ ] priority queue 或等价 timer 等待 `next_due_at`，不固定扫描全部 monitor。
- [ ] bounded queue、semaphore 和 RAII guards；没有 per-execution detached fallback spawn。
- [ ] 通过现有 `TaskSupervisor` 注册 scheduler/runtime，暴露 queue depth、active、permit、lag、restart/terminal diagnostics。
- [ ] 启动恢复只把旧 queued/running 标 interrupted 并重算调度，不自动补发网络请求。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_scheduler -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test observability_contract -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** deterministic time/concurrency/shutdown tests 全绿；新 scheduler 此时可接 application composition，但旧 runner 尚未删除。

## 18. Checkpoint B：写路径切换评审

继续到 read model/UI 前必须满足：

- [ ] migration、repositories、orchestrator、transport、health 和 scheduler integration 全绿。
- [ ] 手动与定时请求都已能走 orchestrator；若仍有旧手动直跑，仅允许 facade 转发，不能保留独立业务。
- [ ] 目标级 exactly-once、budget 跨重启、endpoint revision 和 health exactly-once 均有 fault test。
- [ ] 新 production write 只写 execution/target result/attempt/observation；`channel_monitor_runs` 不再新增。

若需要 dual write 才能让 UI 工作，停止并先完成 read model；不得让 dual write 进入观察期。

## 19. Task 12：固定桶、时区、rollup repair 与 retention

**Files:**

- Create: `src-tauri/src/application/monitoring/buckets.rs`
- Create: `src-tauri/src/application/monitoring/retention.rs`
- Create: `src-tauri/src/services/monitoring/maintenance.rs`
- Modify: `src-tauri/src/persistence/stores/monitoring/status_queries.rs`
- Modify: `src-tauri/src/persistence/stores/monitoring/retention.rs`
- Modify: `src-tauri/src/application/app_services.rs`
- Modify: `src-tauri/src/runtime_composition.rs`
- Modify: `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`（仅在确需 IANA system-timezone 库时）
- Create: `src-tauri/tests/monitoring_buckets_retention.rs`

**RED:**

- [ ] recent 恰为每行最新 60 target results；attempt retry 不增加格子。
- [ ] 24h 返回 24 个含当前小时的滚动小时桶；7d/30d 返回包含当前日的 7/30 个本地自然日桶。
- [ ] DST forward/backward、跨月/年、UTC fallback、系统时区变化测试；每桶由后端返回 start/end/label。
- [ ] missing、skipped-only、unavailable 严格区分；strict/effective availability 和 degraded weight 正确。
- [ ] failure-count JSON schema 验证失败标 dirty，不返回损坏计数。
- [ ] rollup 写失败不影响 execution；repair 重建幂等并合并 dirty ranges。
- [ ] retention 先确保 rollup，再按 age/per-monitor/global 三类上限分批删除；dirty raw source 不删除。

**GREEN:**

- [ ] bucket boundary 与 aggregation 在后端完成，UI 不用 `Date.now() - 7 * day` 重建自然日。
- [ ] maintenance task 有 startup delay+jitter、防重入、每轮 row/time budget、取消和 metrics。
- [ ] 生成 100k attempts/500k target results 数据，保存 query plan、workspace latency、repair/cleanup throughput。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_buckets_retention -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_persistence -- query_plan --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 所有窗口固定、DST 正确、rollup 可重建、清理有界；性能数据达到 spec 目标或先调整 schema/index 再继续。

## 20. Task 13：参数化 Read Model、IPC 与生成绑定

**Files:**

- Create: `src-tauri/src/application/monitoring/queries.rs`
- Rewrite: `src-tauri/src/application/queries/channel_status.rs`
- Modify: `src-tauri/src/models/shared_capabilities.rs` 或拆出 `src-tauri/src/models/monitoring/read_model.rs`
- Modify: `src-tauri/src/application/command_facades/channel_status.rs`
- Modify: `src-tauri/src/application/command_facades/channel_monitoring.rs`
- Modify: `src-tauri/src/commands/channel_status.rs`
- Modify: `src-tauri/src/commands/channel_monitoring.rs`
- Modify: `src-tauri/src/ipc/dto/channel_monitor_reads.rs`
- Modify: `src-tauri/src/ipc/dto/channel_monitor_operations.rs`
- Modify: `src-tauri/src/ipc/registry.rs`
- Modify generated: `src-tauri/src/ipc/dto/*.typescript.txt`, `src/lib/bridge/generated.ts`, `src-tauri/generated/command-registry.json`, schemas/permissions as produced by repository tooling
- Modify: `src/lib/bridge/BackendClient.ts`
- Modify: `src/lib/bridge/DesktopBackend.ts`
- Modify: `src/lib/bridge/DemoBackend.ts`
- Modify: `src/lib/types/channelMonitors.ts`
- Modify: `src/lib/api/channelMonitors.ts`
- Modify: `src/lib/query/resourceQueries.ts`
- Create: `src-tauri/tests/monitoring_read_model.rs`

**Contract:**

- [ ] `load_channel_status_workspace` 输入 window/filter/sort/cursor/limit，limit 有硬上限；返回 row summaries、bucket boundaries、aggregate、running state、generated/freshness、timezone id/source。
- [ ] `run_channel_monitor` 接收 `trigger_request_id`，只返回 execution identity/status，不同步等待并返回 runs 数组。
- [ ] 增加 execution get/list/cancel、definitions CRUD、profile/capability list；旧命令 facade 只做迁移转发。
- [ ] execution/attempt history 独立 cursor query，不塞入 workspace 首屏。
- [ ] read model 不读取 raw request logs 拼 synthetic status，不返回 secret/error body。

**Run:**

```powershell
pnpm.cmd generate:bindings
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_read_model -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib ipc::dto -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test operation_registry -- --nocapture
pnpm.cmd test -- src/lib/bridge/generated.test.ts src/lib/api/channelMonitors.test.ts src/lib/queries/channelQueries.test.ts
pnpm.cmd architecture:commands
pnpm.cmd build
```

**Exit gate:** Rust DTO、serialization fixture、registry、generated TypeScript、BackendClient 完全一致；workspace 在 500 行上有界且没有 raw history fan-out。

## 21. Task 14：前端数据控制器与 view model 换源

**Files:**

- Rewrite: `src/features/channels/channelStatusViewModel.ts`
- Expand: `src/features/channels/channelStatusViewModel.test.ts`
- Create: `src/features/channels/useChannelStatusController.ts`
- Modify: `src/lib/query/resourceQueries.ts`
- Modify: `src/lib/queries/channelQueries.ts`
- Modify: `src/features/channels/ChannelStatusTab.tsx`（先接 controller，不完成视觉）
- Delete/replace: `scripts/channel-status-view-model.test.mjs`
- Delete/replace: `scripts/tests/channelMonitorViewModel.test.mjs`
- Delete/replace: `scripts/channel-status-backend-rollup-contract.test.mjs`

**RED:**

- [ ] window/filter/sort/cursor 形成稳定 query key；页面 inactive 时遵循现有 query enabled 约定。
- [ ] running refresh 不覆盖 last terminal；workspace error 保留 last successful data 并显示 freshness。
- [ ] recent/24h/7d/30d 直接映射后端 buckets，missing 不补成失败或按百分比伪造。
- [ ] Run Now 重复点击复用 trigger request/execution，cancel 和终态 invalidation 正确。

**GREEN:**

- [ ] 删除 request logs + key health + monitor runs 的前端多源合并。
- [ ] 删除 `filterLogsByWindow`、`healthToRecentOutcomes`、按成功率生成 60 格、浏览器自然日计算。
- [ ] controller 拥有 query/mutation/selection/pagination；view model 只做纯展示映射。

**Run:**

```powershell
pnpm.cmd test -- src/features/channels/channelStatusViewModel.test.ts src/lib/queries/channelQueries.test.ts
pnpm.cmd lint
pnpm.cmd build
```

**Exit gate:** 前端只消费 V2 workspace；没有伪造趋势、raw request log fallback 或重复时间窗口算法。

## 22. Task 15：横向状态工作区、详情与配置 UI

**Files:**

- Rewrite/split: `src/features/channels/ChannelStatusTab.tsx`
- Create: `src/features/channels/components/ChannelStatusToolbar.tsx`
- Create: `src/features/channels/components/ChannelStatusTable.tsx`
- Create: `src/features/channels/components/StatusTrend.tsx`
- Create: `src/features/channels/components/MonitorExecutionDrawer.tsx`
- Create: `src/features/channels/components/MonitorDefinitionDialog.tsx`
- Create: `src/features/channels/components/MonitorProfileSelector.tsx`
- Create: `src/features/channels/components/*.test.tsx`
- Replace obsolete static layout scripts: `scripts/channel-status-card-layout.test.mjs`, `scripts/channel-monitoring-layout.test.mjs`, `scripts/channel-status-drag-transform.test.mjs`

**UI requirements:**

- [ ] 首屏直接是浅色紧凑工作区：搜索、Station、status、protocol/model/profile、window、sort、refresh、新建。
- [ ] 横表稳定列：Key、Station、Model/Protocol、Profile、Current、Availability、Last Probe/Latency、Trend、Actions。
- [ ] 趋势格固定尺寸，available/degraded/unavailable/missing 使用低饱和绿/黄/红/灰；tooltip 显示后端边界、计数、失败分类和延迟。
- [ ] station-wide monitor 可分组但每 Key 仍是独立事实行；不使用嵌套卡片。
- [ ] 详情 drawer 展示 execution -> target result -> attempts、requested/effective model、retry/fallback、profile snapshot、health writeback reason。
- [ ] 配置 UI 提供标准/CLI profile、协议、primary/fallback、频率、attempt/execution timeout、预算、writeback mode；高频/authoritative 有明确风险确认。
- [ ] 宽桌面优先；窄窗口冻结关键列并允许趋势横向滚动。长名称、中文/英文、125%/150% Windows scaling 不重叠。
- [ ] familiar actions 使用 Lucide icon + tooltip，键盘焦点、表头语义、状态非仅颜色表达。

**Visual verification:**

```powershell
pnpm.cmd dev
```

在另一个终端或浏览器测试工具验证至少 `1440x900`、`1280x720`、`1024x768` 和窄窗口 `720x900`；保存脱敏截图到 `output/`（不提交），逐项检查无横向不可达操作、无文本遮挡、无趋势跳动、无嵌套卡片。数据集覆盖 500 行、超长 Station/Key/model、四种 outcome、running/disabled/stale/error。

**Run:**

```powershell
pnpm.cmd test -- src/features/channels
pnpm.cmd theme:audit
pnpm.cmd lint
pnpm.cmd build
```

**Exit gate:** UI 与参考项目学习的是横向信息架构，不复制深色网站风；真实后端桶驱动，所有目标 viewport 和交互可用。

## 23. Checkpoint C：用户工作流与数据事实评审

- [ ] 创建/编辑/启停/删除 monitor、Run Now、cancel、筛选、四窗口、详情 history 完整可用。
- [ ] UI、workspace、SQLite 对同一 execution/target result 的 outcome、模型、attempt_count、时间边界一致。
- [ ] 500 行性能达到目标，workspace 没有 N+1 query，DTO 大小有记录。
- [ ] CLI profile 默认关闭或按用户选择启用；UI 不暗示它能规避所有上游风控。

不满足任一项时不删除 legacy read 数据，先回到最早出现分歧的 read/write boundary 修复。

## 24. Task 16：production 单路径切换与旧代码删除

**Files:**

- Delete: `src-tauri/src/services/channel_monitors/mod.rs`
- Delete/replace: `src-tauri/src/services/channel_monitors/probe.rs`
- Delete/relocate reusable tests only: `src-tauri/src/services/channel_monitors/templates.rs`, `redaction.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Delete: legacy DTO/types in `src-tauri/src/models/channel_monitors.rs` after consumers migrate
- Modify: `src-tauri/src/models/mod.rs`
- Delete: old run methods/SQL from former `monitoring_store.rs`
- Modify: `src-tauri/src/application/command_facades/channel_monitoring.rs`
- Modify: `src-tauri/src/commands/channel_monitoring.rs`
- Modify: `src-tauri/src/app_composition.rs`, `application/app_services.rs`, `runtime_composition.rs`
- Modify: `src-tauri/src/ipc/registry.rs` and generated bindings after command removal
- Modify: `scripts/run-contract-tests.mjs`
- Modify: `docs/audits/status-monitoring-boundary-manifest.json`
- Modify: `docs/audits/status-monitoring-deletion-ledger.md`

**Deletion gate:**

- [ ] 删除 30 秒全表 poll、`ACTIVE_MONITOR_RUNS` 静态 guard、旧 `ChannelMonitorRunnerPort` 和 manual bypass。
- [ ] 删除 status-only `MonitorProbeResult.ok`、旧 `CompletedMonitorProbe`/`ChannelMonitorRun` production write。
- [ ] 删除 `fallback_models[0]` 隐式 primary、逐子 run 更新 monitor latest、未使用 threshold/config。
- [ ] 删除 UI 对 request logs/health/runs 的临时拼装和 60 格伪造。
- [ ] `channel_monitor_runs` 只读兼容保留一个发布观察周期；所有代码通过专用 legacy reader 访问，禁止新写。后续明确 migration 删除表与 reader。
- [ ] production composition 只能构造 V2 orchestrator/scheduler/read model，不加 runtime selector。
- [ ] architecture test 搜索所有旧 symbol/SQL；任何未登记 consumer 阻塞完成。

**Run:**

```powershell
rg -n "RUNNER_POLL_INTERVAL|ACTIVE_MONITOR_RUNS|ChannelMonitorRunnerPort|CompletedMonitorProbe|record_probe_outcome|insert_run_and_advance_monitor|healthToRecentOutcomes" src-tauri/src src scripts
node scripts/monitoring-architecture.test.mjs
pnpm.cmd generate:bindings
pnpm.cmd test:contracts
pnpm.cmd test
pnpm.cmd build
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 旧 symbol 搜索只命中 deletion ledger/legacy read adapter/历史文档；production 只有一套执行、统计和健康路径。

## 25. Task 17：故障、并发、性能与真实授权验证

**Files:**

- Create: `src-tauri/tests/monitoring_faults.rs`
- Create: `src-tauri/tests/monitoring_concurrency.rs`
- Create: `scripts/run-monitoring-soak.ps1`
- Create: `scripts/verify-monitoring-live.ps1`
- Create: `scripts/verify-monitoring-db.ps1`
- Create: `docs/audits/status-monitoring-qualification.md`

**Fault matrix:**

- [ ] attempt append、target finalization、health observation、execution finalization、rollup、budget reservation 分别注入 busy/locked/rollback/permanent/commit-outcome-unknown。
- [ ] DNS/TLS/connect/header/body/idle/protocol/content/cancel；permit wait、backoff、shutdown 各阶段取消。
- [ ] scheduler queue saturation、worker panic、clock jump、sleep/resume、definition edit/delete、endpoint revision race。
- [ ] 100 monitors 同时到期、同 Station/Key 竞争、manual storm；证明并发上限、single-flight、budget 和无死锁。
- [ ] hard kill/restart 后 running -> interrupted，不伪造失败、不重复网络请求、不返还已预留预算。

**Performance/soak:**

- [ ] release build 下 1 小时 mixed provider/stream/retry/fallback/missing workload。
- [ ] 结束后 queue、permits、active executions/attempts、repair ranges、task handles 回到稳态，无持续增长。
- [ ] 100k attempts/500k target results、500 行 workspace p95 < 250 ms；scheduler 正常容量 p95 lag < 2 s。
- [ ] retention/repair 与正常执行并发时不长期占用 write coordinator；记录 p50/p95/p99 和 DB 大小。

**Optional live matrix（必须用户明确授权）:**

- [ ] 各使用一个用户许可的 OpenAI、Anthropic、Gemini、xAI/Grok compatible target，标准 profile 先验证。
- [ ] Codex/Claude Code/Gemini CLI profile 分别低频验证 request acceptance 和语义内容；不验证未授权 Grok CLI profile。
- [ ] secret 只来自环境/SecretManager，脚本输出只包含 execution ID、枚举、status、latency 和 hash。
- [ ] 对每个 execution 三方核对：client-visible terminal、runtime sanitized diagnostics、SQLite execution/target/attempt/observation。

**Run:**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_faults -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_concurrency -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-monitoring-soak.ps1 -DurationMinutes 60
```

授权后才运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-monitoring-live.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-monitoring-db.ps1
```

**Exit gate:** qualification 报告包含命令、版本、数据规模、原始分位数、失败注入结论和脱敏扫描；mock 绿色不能替代可执行的本地 E2E，真实 provider 验证则是发布前授权 gate，不是默认 CI gate。

## 26. Task 18：完整验收、文档晋级与后续 legacy 表删除票据

**Files:**

- Modify: `docs/README.md`
- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/PRODUCT_MODEL.md`
- Modify: root `README.md`（仅用户可见能力确已交付时）
- Modify: `docs/specs/STATUS_MONITORING_REFACTOR_SPEC.md` status/implementation references
- Create: release note/qualification link under existing release process
- Create: 一个明确后续 migration 任务，用于观察周期后删除 `channel_monitor_runs` 和 legacy reader

**Full validation order:**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm.cmd architecture:fixtures
pnpm.cmd architecture:typescript
pnpm.cmd architecture:commands
pnpm.cmd architecture:security
pnpm.cmd test:contracts
pnpm.cmd test
pnpm.cmd lint
pnpm.cmd build
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_domain -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_profile_golden -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_migration -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_persistence -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_orchestrator -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_transport -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_scheduler -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test station_key_health_transitions -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_buckets_retention -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_read_model -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_execution_integration -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_faults -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_concurrency -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
pnpm.cmd tauri:build
```

**Release gate:**

- [ ] signed Windows package fresh install、从上一正式版本升级、退出/重启/系统休眠恢复通过。
- [ ] migration backup/recovery、scheduler shutdown drain、persistence close 顺序通过。
- [ ] UI 在真实 Tauri WebView 而不只是 Vite 浏览器中完成截图和交互检查。
- [ ] 对源码、fixtures、logs、output、DB verification report、bundle 做 secret/artifact scan。
- [ ] `git diff --name-only` 仅包含任务范围，没有 SQLite、日志、key、环境文件、截图或无关 provider draft 改动。
- [ ] 文档只宣称实际完成能力；可选真实 provider 未获授权时明确标为未执行，不伪造通过。

## 27. Checkpoint D：最终完成定义

- [ ] `MonitorExecution -> MonitorTargetResult -> ProbeAttempt` 是唯一 synthetic 事实模型。
- [ ] retry/fallback 可追踪但不放大 availability；skipped/missing/unavailable 语义严格。
- [ ] 五类 provider 标准 adapter、普通/stream、语义 challenge 和 fake-200 拒绝均有 contract evidence。
- [ ] CLI profiles 可选、版本化、受控；Grok CLI 未经验证不启用。
- [ ] manual/scheduled 共用 orchestrator；scheduler nearest-due、有界、可取消、可 drain。
- [ ] attempt、target、execution、observation、budget、rollup/repair 的事务与幂等边界经过 fault/restart test。
- [ ] proxy/monitor/connectivity 共享唯一健康状态机，诊断探针不误伤路由。
- [ ] recent/24h/7d/30d 完全由后端 target result/bucket 驱动，IANA/DST 边界正确。
- [ ] 横向浅色桌面 UI 在 500 行与目标 viewport 下可用，无伪造趋势、遮挡或嵌套卡片。
- [ ] 旧 runner、旧 write path、旧 status 拼装和无效配置已删除；legacy 表只有有期限的只读观察与明确删除任务。
- [ ] 全量 test/build/Tauri package、性能、soak、升级与安全扫描有可审计结果。

只有以上全部满足，规范状态才能从 Draft 更新为 Implemented。发布、push 和删除观察期 legacy 表分别需要对应授权，不由本计划自动扩大权限。
