# 路由 V3 旧链退役实施审计

状态：P0-P5 代码升级与最终代码门禁已完成；P6 schema DROP 为 no-go；P7 未开始。本文区分当前实现、验证证据与发布资格，不把保留旧表等同于保留旧运行时语义。

日期：2026-08-31

关联计划：[`../plans/2026-08-31-routing-v3-legacy-chain-retirement.md`](../plans/2026-08-31-routing-v3-legacy-chain-retirement.md)

机器可读台账：[`routing-v3-legacy-retirement-ledger.json`](routing-v3-legacy-retirement-ledger.json)

## 固定基线

- source revision：`5e445d7a69eb96282f1c22a1640aa9a9909d921a`。
- 工作区状态：dirty worktree，未 commit；上述 HEAD 不包含本审计记录的未提交实现，证据必须结合当前 diff 和实际命令结果读取。
- schema version：`0071`；本次没有创建 DROP migration。
- portable catalog：`111` 张用户表。
- portable fingerprint：`bc8b675f90012fe6179bd489170e24937600de6f711b79cb83952a943efbec48`。
- read-owner version：`routing-v3-circuit-read-v1`。
- P4/P5 rollback floor：只允许回退到已完成 V3 read-side cutover 的构建；不得回退到以 `routing_health_snapshot` 为事实源的版本。
- P6/P7 rollback：尚未进入。旧表存在只提供兼容窗口，不允许重新接回生产 reader/writer。

## 实施结论

### V3 单一读链

- Routing Workspace、runtime overlay、Key Pool 与 protection 统一读取 mutable station-key circuit state 和 `CircuitPersistenceGate` 只读快照。
- 读模型在 gate revision 变化时有界重读；第二次变化或持久化不可用返回 typed unavailable，不回退旧 health snapshot。
- workspace 在分页前计算完整候选集合的 participation 与 aggregates，前端只展示后端 typed status/reason，不重算资格。
- endpoint ping 继续使用独立 `endpoint_health_snapshot`，不与 Key circuit 合并。

### 停止旧副写

- 请求终态保留 attempt、outbox、V3 quality、cost、capability 与 circuit effect，删除旧 health observation/snapshot 副写。
- monitoring 直接构造 canonical `RoutingObservation` 并经 `ObservationIngestion` 写入，不再经过 transient `HealthObservation` adapter，也不再推动旧 health reducer。
- station-key connectivity 保留 progress/cancel/result/error 合同，不再修改旧 health 或 V3 circuit/quality。
- 429、502 与可归责 pre-commit failure 继续遵循 V3 consecutive-failure threshold：同 Key 重试至阈值，circuit 打开后才尝试下一 score-ordered Key。
- `RetrySameTarget` 与 `TryNextCandidate` 的 V3 circuit disposition 均为 `retryable_before_commit`，不恢复旧 cooldown 语义。

### P5 表面收口

- 删除 station-key health/operational-detail、error-rate history 与 capacity-domain 的无 consumer IPC、facade、registry、ACL、binding 和 bridge surface。
- 删除 `HealthTransitionService`、`HealthObservationStore`、legacy error-rate reducer/store 与 capacity-domain 代码 owner；兼容表和历史 migration 保留。
- `RoutingHealthVerdictStore` 收敛为 capability-only，保留 durable `model_on_key / unsupported_model`。
- 删除 `RoutingPolicyStore::save_compare_and_swap`、专用旧输入验证和无 caller 的 unversioned observation readers。
- `RuntimeRoutingSettings` 删除 `policy` 与 `scheduler_config`；proxy ordering、trace label 和 failure context 直接取请求级 V3 planning snapshot。
- 删除 test-only `coordinator`、`eligibility`、`hierarchical_preview` 与旧 strata planner；生产 `RoutePlanCandidate`、V3 intelligent planner 和 admission 保留。
- admission 的 `planning_snapshot` 改为类型级必选引用，删除不可达 `planning_snapshot_required` fallback；删除 planning 链上的 probe 参数和无 caller circuit facade。
- 删除 `failure_domains.rs`、runtime `ProviderCapacity` failure target/commitment 解析和 `AttemptBudgetProfileV1` 的 capacity-domain shadow；provider-capacity 上游错误仍按当前 Key 归责并保留 V3 retry/circuit 语义。
- 删除 `ProbeDiscoveryOnly`、`RoutingScoreStatus::ProbeDiscovery` 及 Rust/DTO/generated/frontend 的 `probe_discovery` 表面。
- 删除 operational fact 的旧 health counter/latency/cooldown/error shadow；`CanonicalRoutingCandidate.health` 暂只保留在 `cfg(test)` 回归面，旧 scoped-health reducer/probe token 已完全删除。
- 删除 test-only scheduler compatibility types 和 RoutingService 三个恒空 health/probe facade。
- machine ledger 升级为 schema v2；每项都有稳定 ID、consumer、删除条件、验证命令与证据，architecture gate 检查状态语义和关键旧代码不得复活。

### 二次审计边界

- `allow_cross_capacity_domain_fallback` 只保留在受支持的 V1/V2 policy decoder、IPC DTO 和生成兼容类型中；V3 compiler 产出的 `AttemptBudgetProfileV1` 与 execution 不携带、不读取该字段。
- `routing_health_snapshot` 等旧表不再有 runtime authority 或 writer。生产源码中的剩余引用只能属于 migration、legacy import/upgrade、delete cleanup、schema/portable compatibility；测试和 fixture 只用于兼容验证。
- `get_routing_protection_status` 不是旧 health reader：它已由 versioned V3 circuit read model 支撑，当前仍有桌面消费者，因此作为兼容命令保留至 P7，不能在 P5 误删。
- `HealthProtectionProbe`、`HealthProtectionReducer`、`HealthProbeAdmissionMode` 和 production planning 参数链已删除；只保留 V1 policy profile 与历史 `probe_scope` 反序列化类型。

## 保留边界

| 对象 | 结论 | 保留原因 |
| --- | --- | --- |
| `routing_health_observations/routing_health_verdicts` | capability-only 保留 | unsupported model 必须跨重启生效。 |
| `endpoint_health_snapshot` | 完整保留 | endpoint ping 的独立事实，不是 station-key circuit。 |
| `probe_state_revision` | 保留 decoder | 已存在于历史 evidence 和受支持 fixture。 |
| 历史 migration、schema/portable reader、legacy import/upgrade、delete cleanup 与 fixture | 保留 | 旧数据库、导入包和 downgrade/恢复窗口仍需读取；这些路径不得成为 runtime authority 或 writer。 |
| `routing_health_snapshot`、`station_key_health_observations`、`routing_error_rate_history` | 表暂留、生产语义退役 | P6 资格不足，禁止提前 DROP；生产引用仅允许 migration/import/upgrade/delete-cleanup/schema/portable 白名单。 |
| `station_capacity_domains` | 表暂留、代码 owner 退役 | 需要独立 portable/import/降级资格，不能与 health 表强绑删除。 |
| V1/V2 `allow_cross_capacity_domain_fallback` decoder/DTO | 有界保留 | 继续接受受支持的旧 policy payload；字段不得进入 V3 attempt budget/runtime。 |
| `get_routing_protection_status` | V3-backed compatibility 保留至 P7 | 当前桌面消费者仍使用既有命令契约，底层已切 versioned V3 circuit read model。 |
| `health_protection.rs` 的 profile/scope 类型 | 有界保留 | V3 policy upgrade 仍编译 V1 profile，历史 observation evidence 仍需反序列化 `probe_scope`；旧 reducer/probe 状态机不再保留。 |

## 阶段结果

| 阶段 | 状态 | 结果 |
| --- | --- | --- |
| P0 caller/能力/事务基线 | 完成 | ledger v2、schema/portable 基线、caller 分类和防复活 architecture gate 已建立。 |
| P1 V3 circuit read model | 完成 | mutable circuit + gate snapshot、revision consistency、typed unavailable 和无写副作用已接入。 |
| P2 后端切读 | 完成 | workspace、runtime overlay、Key Pool、protection 与 versioned DTO 已切 V3。 |
| P3 前端切读 | 完成 | typed participation、full-set aggregates、unknown/stale/gate 状态已接入，旧 fallback/重算已删除。 |
| P4 legacy writer | 完成 | request、monitor、manual connectivity 停旧写，V3 circuit/quality/capability/terminal 保持。 |
| P5 dead surface | 完成 | IPC/settings、health adapter、probe/scheduler/planner、capacity-domain runtime shadow、operational health shadow 与无 caller API 已收口；退役合同和全量代码门禁通过。 |
| P6 schema DROP | no-go | 不满足兼容窗口、soak、备份恢复、桌面验收和 release go 条件；未创建 migration。 |
| P7 post-drop | 未开始 | 仅在 P6 独立 release 成功后进入。 |

## 验证记录

最终代码冻结验证于 `2026-08-31T23:24:04Z` 至 `2026-08-31T23:42:16Z`（UTC）完成。Windows 环境使用单任务 Cargo 编译/测试参数，避免并发编译触发页面文件不足；该参数不改变测试语义。所有结果均来自当前 dirty worktree，未执行 stage/commit/push。

| 命令/范围 | 当前状态 |
| --- | --- |
| frontend Vitest 与 `pnpm build` | 通过：`140` files、`647` tests；生产构建完成。 |
| `pnpm test:contracts` 与 retirement gate | 通过：全量 contract checks、`routing V3 legacy retirement contract` 均通过。 |
| `pnpm generate:bindings --check` | 通过：`4 artifacts`、two-run deterministic；仅有既存 Rust warning。 |
| monitoring write/fault、admission/execution、routing policy/error、operational facts/planning snapshot focused tests | 通过：包含在 Rust 全量回归与 contract suite 中。 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | 通过。 |
| `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets`、`cargo check`、all-targets check、release lib check | 通过；存在非阻断 lint/linker warning，无 error。 |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | 通过：`1471 passed, 0 failed, 0 ignored`；串行 `CARGO_BUILD_JOBS=1`、`RUST_TEST_THREADS=1`。 |
| `node scripts/routing-v3-legacy-retirement.test.mjs` | 通过：`routing V3 legacy retirement contract passed`。 |
| `pnpm verify:full` | 通过；命令为 `$env:CARGO_BUILD_JOBS='1'; $env:RUST_TEST_THREADS='1'; pnpm verify:full`，耗时约 `1092.15s`。 |
| `git diff --check` | 通过。 |

说明：编译过程中仍有未使用 import/variable、clippy 建议和少量未满足的历史 lint expectation warning；它们未升级为 error，也未改变退役门禁结论。

首次使用默认并发执行 `pnpm verify:full` 时，Windows 曾出现页面文件不足 `OS error 1455`，并由失效 rlib 触发级联未解析导出；同一轮并发 Rust 测试还在首次创建临时数据库时出现一次 `DatabaseFailed`。该测试单独运行通过，随后独立串行 Cargo 全量回归通过 `1471` tests，最终上述单任务参数下的完整 `pnpm verify:full` 也通过。因此没有复现代码或数据库契约失败，但默认并发验证对本机内存/页面文件和测试资源竞争敏感，后续 CI 应保留受控 Cargo 并发。

`pnpm verify:release` 不在本次执行范围，因为没有进入 P6 schema-drop release qualification，也没有创建 release bundle 或 DROP migration。即使上述代码门全部通过，也不能替代兼容窗口、soak、备份恢复、桌面验收和人工 release go。

## P6 Go/No-Go

| 资格项 | 当前证据 | 结论 |
| --- | --- | --- |
| 不少于 7 个连续自然日的 V3-only 兼容窗口 | 无 | no-go |
| 至少 1000 个请求/监控事件的 deterministic soak，零未解释差异 | 无 | no-go |
| verified same-device backup manifest | 无 | no-go |
| 隔离 data directory 恢复演练 | 无 | no-go |
| pre-drop `verify:full`、schema15/startup upgrade 与桌面手测 | `verify:full` 与 schema15/startup 自动化已通过；桌面手测和完整 release qualification 尚未完成 | no-go |
| release revision、rollback floor 与人工 go 决策 | 未冻结 | no-go |

因此最高 migration 保持 `0071`，不得删除旧 health/error-rate/capacity-domain schema。达到全部资格后必须重新扫描 caller inventory、portable/import 支持矩阵和当前 migration 编号，再单独评审 append-only DROP migration；不能从本次代码完成状态自动推导 P6 go。
