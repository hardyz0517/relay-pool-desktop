# `RoutingService` caller inventory

状态：Phase 0 baseline（只读扫描；不代表任何 wrapper 可以立即删除）  
日期：2026-09-06  
关联计划：[`../plans/2026-09-06-routing-reliability-correction.md`](../plans/2026-09-06-routing-reliability-correction.md)

## 扫描范围与方法

- 目标文件：`src-tauri/src/application/routing.rs`。
- 纳入 `RoutingService` `impl` 中所有 `pub`/`pub(crate)` 方法，以及该文件中供生产路径调用的 `pub(crate)` policy/document helper。
- 调用方通过 `rg` 扫描 `src-tauri/src` 的 Rust 生产代码，并单独标注 `#[cfg(test)]`、`differential_tests` 和 `test_support` 调用。
- 分类沿用计划：`P` policy mutation/activation、`E` proxy execution、`Q` query/read、`X` endpoint operation、`T` test-only compatibility。
- 本清单只记录 caller、事实类型、生命周期和删除门槛；没有修改生产代码，也没有把测试调用误判成生产 producer。

## 结论摘要

1. 当前仍有 10 组生产职责通过 `RoutingService`：policy document/CAS、circuit admission/reaper、proxy execution read、workspace/runtime/simulation query，以及 endpoint port adapter。
2. endpoint/monitoring 的生产 caller 已迁移到窄 port；`RoutingService` 对应的 `station_endpoint_probe_target`、`load_monitoring_target_snapshots`、`record_station_endpoint_health` 目前是 adapter 实现，尚不能删除，直到 adapter 改由独立 endpoint owner 提供。
3. proxy 不再直接 import `RoutingService`，但 `RoutingExecutionReader` 仍以它作为过渡 adapter；因此 execution 相关 wrapper（planning/settings/balance/target/admission/generation/boundary）均保留。
4. `RoutingCommandFacade` 已改为注入 workspace、runtime overlay、simulation、protection/circuit read 等窄 query ports；这些 port 当前由 `RoutingService` 提供兼容 adapter。不得因此误判宽 adapter 已可删除。
5. `load_workspace_candidates_with_request_pricing` 与 `load_runtime_candidates_with_request_pricing` 仅由 differential test 使用，属于可在测试迁移后删除的 compatibility helper，不是生产事实来源。
6. 没有发现新的旧 health/error-rate/capacity writer；本清单不改变 P6 no-go。

## 人读清单

| 方法（位置） | 类别 / 读写 | 当前生产 caller（测试 caller 另列） | 事实与生命周期 | 目标 owner（当前/后续） | 删除条件 |
|---|---|---|---|---|---|
| `persistence_handle` (`routing.rs:118`) | P / 只读句柄 | `RoutingPolicyMutationCoordinator::{apply,reconcile}` (`routing_policy_control_plane.rs:155,174,203`) | policy/document fast activation 和 mirror 同一持久化 runtime；应用控制面生命周期 | 当前 `RoutingService` adapter；后续 `RoutingPolicyPersistencePort` 或 coordinator 内部 runtime | coordinator 不再需要从 routing service 取得 runtime，且所有 sync/fast-activation caller 改注入窄 port |
| `routing_policy_config_directory` (`:122`) | P / 只读路径 | `RoutingPolicyMutationCoordinator::config_directory` → `policy_document_runner` | managed routing-policy 文件 watcher 启动和恢复 | 当前 coordinator 代理；后续 `RoutingDocumentStore`/document coordinator | document runner 直接依赖 document coordinator 的目录 port，且无其它 caller |
| `load_execution_settings` (`:129`) | E / 只读 | `RoutingExecutionReader` (`routing_execution_reader.rs:205`) → `RoutingExecutionRepository`/proxy execution (`execution.rs:524,561`) | 每个 proxy request 的 immutable execution settings 快照 | 当前 reader adapter；后续 `RoutingExecutionSettingsReadPort` | reader 改由 settings/execution read port 实现并通过 proxy 回归；全仓无 routing wrapper caller |
| `list_balance_snapshots` (`:143`) | E / 只读 | `RoutingExecutionReader` (`:220`) → proxy local usage response；非 proxy pricing facade 使用独立 `PricingService` | balance read 只用于请求执行兼容响应，不是 routing health | 当前 reader adapter；后续 pricing/balance read port | proxy usage response 改接 PricingService port，并通过 balance scope 回归；删除 reader 方法和 wrapper |
| `new` (`:160`) | T / 构造 | 无生产 caller；仅 compatibility unit fixtures | 测试方便构造，使用共享 circuit gate 默认值 | 测试支持 API | 所有测试改用 `new_with_circuit_persistence_gate` 或 fixture builder 后删除；生产编译由 `expect(dead_code)` 守护 |
| `new_with_circuit_persistence_gate` (`:164`) | P/E / 构造 | `AppServices::for_runtime` (`app_services.rs:93`)；policy/reaper/proxy 测试 | composition root 注入共享 persistence gate | `AppServices` composition owner | 只有在 RoutingService 被完全拆除或替换后才删除；不应在中途移除 |
| `health_check_station_key_circuit_persistence` (`:214`) | E / 写（恢复 gate） | `station_key_circuit_reaper::reap_once` (`:88`) | reaper 周期清理 persistence gate；不可放行时 fail-closed | 当前 `RoutingService`→`StationKeyCircuitStore` adapter；后续 `CircuitPersistenceHealthPort` | reaper 注入独立 circuit health/recovery port，并有 unavailable/commit-unknown 回归 |
| `load_routing_policy` (`:240`) | P/Q / 只读 aggregate | 生产：`RoutingPolicyMutationCoordinator` fast activation/reconcile (`routing_policy_control_plane.rs:167,223`)；command facade 的 IPC 读取已用 `RoutingPolicyReadService` | active/staged policy aggregate，供 mutation 后确认和 publication | 当前 routing policy aggregate adapter；后续 coordinator 内部 policy read port | coordinator fast lane/reconcile 改用 `RoutingPolicyReadService` 或窄 aggregate port；测试及 loopback 一并迁移 |
| `get_routing_protection_status` (`:254`) | Q / 只读 projection | `RoutingCommandFacade` (`command_facades/routing.rs:193`) → `commands/routing_health.rs:get_routing_protection_status` | V3 circuit-backed protection DTO；capacity 参数生产固定为空且不作 runtime authority | 当前 `RoutingService` query adapter；后续 `RoutingProtectionReadService` | facade 注入独立 protection reader，capacity 死分支完成 producer 证据/删除决策并通过 IPC 回归 |
| `admit_station_key_circuit_with_attempt` (`:272`) | E / 写（CAS admission） | `RoutingExecutionReader` (`:265`) → proxy execution admission (`execution.rs:780,1472`) | outbound 前 durable station-key circuit + attempt admission，携带 generation/revision/deadline | 当前 reader adapter；后续 `CircuitAdmissionPort` + `AttemptAdmissionPort` | execution reader 不再引用 routing service，两个 port 均有 CAS/stale/persistence unavailable 回归 |
| `load_station_key_circuit_read_snapshot` (`:410`) | E/Q / 只读 projection | 生产：`RoutingCommandFacade::get_routing_circuit_status` (`:206`)；workspace query (`routing.rs:1010`) | durable circuit rows 与 process/durable persistence gate 的一致性快照 | 当前 routing adapter；后续 `StationKeyCircuitReadService` | command/workspace 均改用独立 read service，且 revision/torn snapshot 测试通过 |
| `load_routing_generation_admission_guard` (`:437`) | E / 只读 gate | `RoutingExecutionReader` (`:298`) → proxy execution (`execution.rs:1940`) | generation fencing，阻止 stale runtime generation admission | 当前 reader adapter；后续 `RoutingGenerationReadPort` | reader 改用 generation port，proxy 架构门禁确认无 routing import |
| `mark_station_key_attempt_boundary` (`:451`) | E / 写（CAS boundary） | `RoutingExecutionReader` (`:315`) → proxy execution (`execution.rs:2038`) | outbound boundary ledger mark；Half-Open lease 同事务 CAS | 当前 reader adapter；后续 `AttemptBoundaryWritePort` | attempt lifecycle/circuit writer 独立注入并覆盖 cancellation/commit unknown；全仓无 wrapper caller |
| `apply_routing_policy_document_v3` (`:518`) | P / 写（policy CAS/stage） | `RoutingPolicyMutationCoordinator::apply` (`routing_policy_control_plane.rs:146`)；其余 command 经过 coordinator | UI/file-watch 完整文档校验、CAS stage、source provenance | 当前 coordinator 调用 routing aggregate；后续 coordinator 直接持有 policy mutation port | coordinator 的 mutation port 完成 CAS、activation/mirror 行为等价回归；删除 routing wrapper 后 architecture gate 保持单一 writer |
| `reconcile_external_routing_policy_document`（方法 `:541`） | P / 写（外部文档导入） | `RoutingPolicyMutationCoordinator::reconcile_external` (`:193`) | 稳定文件 → 校验 → 同一 policy CAS；无变化只更新 document-sync 诊断 | 当前 coordinator + routing helper；后续 `RoutingDocumentCoordinator` | coordinator 不再传 `&RoutingService` 给 helper，且 file-watch stale/invalid/unstable 回归通过 |
| `load_intelligent_planning_snapshot` (`:554`) | E/Q / 只读 planning | `RoutingExecutionReader` (`:190`) → proxy；`simulate_route` 与 differential tests 直接调用 | caller-owned deadline 下构建 immutable V3 planning snapshot；proxy admission 的唯一候选事实 | 当前 `RoutingService` planner implementation；后续 `RoutingPlannerReadPort` | execution reader、simulation/query 各注入目标 planner port，golden fixture/排序/截止时间回归一致 |
| `load_monitoring_target_snapshots` (`:759`) | X / 只读 | `RoutingMonitoringTargetReadPort for RoutingService` → `MonitoringRunner` (`runner.rs:63`) | monitoring 周期目标快照（endpoint revision/capability），不是 proxy health | 当前 port adapter；后续 `MonitoringTargetReadService` | runner 注入独立 target owner 且无 routing adapter caller；保留独立 monitoring lifecycle |
| `load_workspace_candidates_with_request_pricing` (`:787`, `cfg(test)`) | T / 只读 compatibility | `persistence/differential_tests.rs:293`；内部 test-only wrapper | 旧 workspace candidate+pricing 对照 fixture；不进入生产 proxy | 测试 fixture/differential harness | differential tests 改用 canonical planner snapshot 后，`rg` 无生产/测试 caller |
| `load_runtime_candidates_with_request_pricing` (`:841`, `cfg(test)`) | T / 只读 compatibility | `persistence/differential_tests.rs:1153` | 旧 runtime candidate pricing 对照 fixture；不进入生产 | 测试 fixture/differential harness | 对照测试迁移完成并保留 V3 parity evidence 后删除 |
| `load_operational_execution_target_refs` (`:849`) | E / 只读 | `RoutingExecutionReader` (`:236`) → `RoutingExecutionRepository` (`routing_repository.rs:192`) → proxy | outbound 前解析 endpoint/credential/revision 的执行目标引用 | 当前 reader adapter；后续 `ExecutionTargetReadPort` | reader 直接依赖 target resolver port，且 credential/endpoint revision integrity 回归通过 |
| `load_routing_workspace_snapshot` (`:899`) | Q / 只读 read model | `RoutingCommandFacade` (`command_facades/routing.rs:242`) → `commands/routing_health.rs:353` | workspace 候选、planner assessment、quality、circuit revision 的同读事务投影 | 当前 `RoutingService` query owner；后续 `RoutingWorkspaceReadService` | facade 改注入独立 workspace reader；golden fixture 和 timeout/unavailable DTO 回归一致 |
| `load_routing_runtime_overlay` (`:1212`) | Q / 只读 runtime overlay | `RoutingCommandFacade` (`:249`) → `commands/routing_health.rs:379` | proxy active counts 等 runtime-only overlay；不写 durable health | 当前 routing query adapter；后续 `RoutingRuntimeOverlayReadService` | facade 直接注入 overlay reader，窄窗口/空态/runtime unavailable 回归通过 |
| `station_endpoint_probe_target` (`:1247`) | X / 只读 | `RoutingEndpointTargetReadPort for RoutingService` (`:1499`)；port tests only | endpoint origin + revision，出站前重新 normalize；不含 credential | 当前 endpoint port adapter；后续 `EndpointTargetReadService` | endpoint target owner 实现 port，生产 `rg` 无 `RoutingService::station_endpoint_probe_target` |
| `record_station_endpoint_health` (`:1259`) | X / 写（CAS snapshot） | `RoutingEndpointHealthWritePort for RoutingService` (`:1536`)；manual ping/monitor/connectivity 通过 port | endpoint snapshot，必须携带 expected revision；stale 不得覆盖新 revision | 当前唯一 writer adapter（底层 `RoutingStore` CAS）；后续 `EndpointHealthSnapshotWriter` | 独立 writer 接管底层 CAS，manual/monitor/connectivity 全部使用它并通过 stale/unknown 回归；删除 wrapper |
| `simulate_route` (`:1291`) | Q / 只读 simulation | `RoutingCommandFacade` (`command_facades/routing.rs:301`) → `commands/routing_health.rs:483`；routing unit tests | 本地 preview，使用 canonical planner；不获取 capacity lease、不写 runtime | 当前 routing query/planning owner；后续 `RouteSimulationQuery`（可复用 planner port） | simulation facade 注入独立 query owner，preview policy/version 与 rejection explanation fixture 不变 |

## 文件级 `pub(crate)` helper（非 `RoutingService` 方法）

| helper（位置） | 类别 | 生产 caller | 事实/生命周期 | 删除/迁移条件 |
|---|---|---|---|---|
| `routing_policy_v3_from_stored` (`routing.rs:1610`) | P/Q decoder | `commands/routing_health.rs:35`、`RoutingPolicyMutationCoordinator::publish_active_policy` (`:238`)、policy tests | V1/V2/V3 stored config → V3 typed policy；兼容解码，不是 writer | 迁移到 `routing_policy_read`/policy module，保留 decoder compatibility tests；无跨层 caller 后删除 routing re-export |
| `sync_routing_policy_file` (`routing.rs:1723`) | P / 文件 mirror 写 | `RoutingPolicyMutationCoordinator::apply` (`:173`)；`routing_generation_cutover_runner.rs:2075` | active policy → desired/materialized managed document；CAS 与文件系统双资源协调 | coordinator/document sync owner 提供等价 API，generation runner 改注入 port；在此之前不得删除 |
| `initialize_routing_policy_document_sync` (`routing.rs:1858`) | P / 启动 mirror | `lib.rs:1194` startup | 启动时创建/恢复 managed mirror，不反向替换 SQLite active aggregate | startup 注入 document sync owner 并完成 missing/invalid/external-change 回归后迁移 |
| `reconcile_external_routing_policy_document` (`routing.rs:1875`) | P / 文件导入 helper | 仅 `RoutingService::reconcile_external_routing_policy_document` wrapper (`:548`) | 文件稳定性、digest、revision CAS 和诊断写回 | coordinator 直接拥有 helper 后，wrapper caller 为零即可删除；保留 file-watch compatibility tests |

## 生产依赖图与删除顺序

```text
IPC routing commands ──┬─> RoutingCommandFacade ──> workspace / overlay / simulation / protection (Q)
                       └─> endpoint ports ────────> RoutingService adapter ──> RoutingStore (X, transitional)

proxy execution ───────> RoutingRepository ────────> RoutingExecutionReader ──> RoutingService (E, transitional)
policy UI/file watch ──> RoutingPolicyMutationCoordinator ──> RoutingService policy/CAS (P, transitional)
generation cutover ────> sync_routing_policy_file helper (P)
circuit reaper ────────> health_check... ──────────> RoutingService (E, transitional)
```

建议删除顺序：

1. 先迁移 endpoint target/health adapter 到独立 writer；确认手动 ping、monitoring、connectivity 三个生命周期仍隔离后删除 X wrappers。
2. 再迁移 execution reader 的 settings/balance/target/planning/circuit 方法；每组 caller 清零并通过 proxy 回归后删除 E wrappers。
3. 将 workspace/runtime overlay/simulation/protection query 抽为 read owner；`RoutingCommandFacade` 不再持有宽 service 后删除 Q wrappers。
4. 最后迁移 policy aggregate/document helper 与 reaper gate；确认 mutation、mirror、generation 和 fail-closed 语义后删除 P wrappers。
5. `new`、两个 test-only pricing helper 和只为测试保留的构造路径最后清理；不得用测试删除掩盖生产 caller。

## 机器可读清单

下面 JSON 与上表保持一一对应（`kind=method` 为 `RoutingService` 方法，`kind=helper` 为文件级 helper）。`callers` 只列调用符号；测试 caller 单独放在 `test_callers`。

```json
[
  {"kind":"method","name":"persistence_handle","line":118,"class":"P","access":"pub(crate)","callers":["RoutingPolicyMutationCoordinator::apply/reconcile"],"owner_now":"RoutingService","target_owner":"RoutingPolicyPersistencePort","lifecycle":"policy mutation/managed-document","delete_when":"coordinator no longer obtains runtime through RoutingService"},
  {"kind":"method","name":"routing_policy_config_directory","line":122,"class":"P","access":"pub(crate)","callers":["RoutingPolicyMutationCoordinator::config_directory -> policy_document_runner"],"owner_now":"RoutingService adapter","target_owner":"RoutingDocumentStore","lifecycle":"managed-document watcher","delete_when":"document runner uses directory port directly"},
  {"kind":"method","name":"load_execution_settings","line":129,"class":"E","access":"pub(crate)","callers":["RoutingExecutionReader -> proxy execution"],"owner_now":"RoutingExecutionReader adapter","target_owner":"RoutingExecutionSettingsReadPort","lifecycle":"per-request execution snapshot","delete_when":"execution reader no longer calls RoutingService"},
  {"kind":"method","name":"list_balance_snapshots","line":143,"class":"E","access":"pub(crate)","callers":["RoutingExecutionReader -> proxy usage response"],"owner_now":"RoutingExecutionReader adapter","target_owner":"Pricing/BalanceReadPort","lifecycle":"request usage compatibility read","delete_when":"proxy usage response uses pricing port"},
  {"kind":"method","name":"new","line":160,"class":"T","access":"pub(crate)","callers":[],"test_callers":["routing/policy/differential fixtures"],"owner_now":"test compatibility constructor","target_owner":"test fixture builder","lifecycle":"test setup","delete_when":"all tests use explicit shared gate constructor"},
  {"kind":"method","name":"new_with_circuit_persistence_gate","line":164,"class":"P/E","access":"pub(crate)","callers":["AppServices::for_runtime"],"test_callers":["routing/policy/reaper/proxy tests"],"owner_now":"composition root","target_owner":"replacement service composition","lifecycle":"application construction","delete_when":"RoutingService removed/replaced"},
  {"kind":"method","name":"health_check_station_key_circuit_persistence","line":214,"class":"E","access":"pub(crate)","callers":["station_key_circuit_reaper::reap_once"],"owner_now":"RoutingService circuit adapter","target_owner":"CircuitPersistenceHealthPort","lifecycle":"reaper recovery","delete_when":"reaper injects dedicated circuit recovery port"},
  {"kind":"method","name":"load_routing_policy","line":240,"class":"P/Q","access":"pub(crate)","callers":["RoutingPolicyMutationCoordinator fast activation/reconcile"],"test_callers":["routing_loopback; policy tests"],"owner_now":"RoutingService policy adapter","target_owner":"RoutingPolicyReadPort","lifecycle":"policy CAS confirmation","delete_when":"coordinator uses dedicated policy read port"},
  {"kind":"method","name":"get_routing_protection_status","line":254,"class":"Q","access":"pub(crate)","callers":["RoutingCommandFacade -> get_routing_protection_status"],"owner_now":"RoutingService protection query","target_owner":"RoutingProtectionReadService","lifecycle":"IPC read","delete_when":"facade injects protection reader and capacity branch decision is recorded"},
  {"kind":"method","name":"admit_station_key_circuit_with_attempt","line":272,"class":"E","access":"pub(crate)","callers":["RoutingExecutionReader -> proxy admission"],"owner_now":"RoutingService CAS adapter","target_owner":"CircuitAdmissionPort + AttemptAdmissionPort","lifecycle":"pre-outbound admission","delete_when":"reader uses dedicated CAS ports"},
  {"kind":"method","name":"load_station_key_circuit_read_snapshot","line":410,"class":"E/Q","access":"pub(crate)","callers":["RoutingCommandFacade::get_routing_circuit_status","RoutingService::load_routing_workspace_snapshot"],"owner_now":"RoutingService circuit read adapter","target_owner":"StationKeyCircuitReadService","lifecycle":"versioned circuit read model","delete_when":"all query callers use read service"},
  {"kind":"method","name":"load_routing_generation_admission_guard","line":437,"class":"E","access":"pub(crate)","callers":["RoutingExecutionReader -> proxy generation fence"],"owner_now":"RoutingService generation adapter","target_owner":"RoutingGenerationReadPort","lifecycle":"request admission","delete_when":"reader uses generation port"},
  {"kind":"method","name":"mark_station_key_attempt_boundary","line":451,"class":"E","access":"pub(crate)","callers":["RoutingExecutionReader -> proxy outbound boundary"],"owner_now":"RoutingService attempt/CAS adapter","target_owner":"AttemptBoundaryWritePort","lifecycle":"outbound boundary","delete_when":"attempt writer is independently injected"},
  {"kind":"method","name":"apply_routing_policy_document_v3","line":518,"class":"P","access":"pub(crate)","callers":["RoutingPolicyMutationCoordinator::apply"],"test_callers":["routing_loopback; policy tests"],"owner_now":"RoutingService policy aggregate","target_owner":"RoutingPolicyMutationCoordinator + mutation port","lifecycle":"UI/file-watch policy mutation","delete_when":"coordinator owns CAS implementation"},
  {"kind":"method","name":"reconcile_external_routing_policy_document","line":541,"class":"P","access":"pub(crate)","callers":["RoutingPolicyMutationCoordinator::reconcile_external"],"owner_now":"RoutingService document adapter","target_owner":"RoutingDocumentCoordinator","lifecycle":"external file reconciliation","delete_when":"coordinator no longer passes RoutingService to helper"},
  {"kind":"method","name":"load_intelligent_planning_snapshot","line":554,"class":"E/Q","access":"pub","callers":["RoutingExecutionReader -> proxy","RoutingService::simulate_route"],"test_callers":["persistence differential; routing tests"],"owner_now":"RoutingService planner","target_owner":"RoutingPlannerReadPort","lifecycle":"caller-owned planning deadline","delete_when":"execution/query callers use planner port"},
  {"kind":"method","name":"load_monitoring_target_snapshots","line":759,"class":"X","access":"pub(crate)","callers":["RoutingMonitoringTargetReadPort -> MonitoringRunner"],"owner_now":"RoutingService monitoring adapter","target_owner":"MonitoringTargetReadService","lifecycle":"monitor execution preparation","delete_when":"runner has independent target owner"},
  {"kind":"method","name":"load_workspace_candidates_with_request_pricing","line":787,"class":"T","access":"pub(crate), cfg(test)","callers":[],"test_callers":["persistence differential"],"owner_now":"test-only compatibility","target_owner":"canonical planner fixture","lifecycle":"differential comparison","delete_when":"differential test migrated"},
  {"kind":"method","name":"load_runtime_candidates_with_request_pricing","line":841,"class":"T","access":"pub(crate), cfg(test)","callers":[],"test_callers":["persistence differential"],"owner_now":"test-only compatibility","target_owner":"canonical planner fixture","lifecycle":"differential comparison","delete_when":"differential test migrated"},
  {"kind":"method","name":"load_operational_execution_target_refs","line":849,"class":"E","access":"pub(crate)","callers":["RoutingExecutionReader -> RoutingExecutionRepository -> proxy"],"owner_now":"RoutingService target adapter","target_owner":"ExecutionTargetReadPort","lifecycle":"pre-outbound target resolution","delete_when":"reader uses target resolver port"},
  {"kind":"method","name":"load_routing_workspace_snapshot","line":899,"class":"Q","access":"pub(crate)","callers":["RoutingCommandFacade -> IPC workspace command"],"owner_now":"RoutingService workspace query","target_owner":"RoutingWorkspaceReadService","lifecycle":"bounded workspace read model","delete_when":"facade injects workspace reader"},
  {"kind":"method","name":"load_routing_runtime_overlay","line":1212,"class":"Q","access":"pub(crate)","callers":["RoutingCommandFacade -> IPC runtime overlay command"],"owner_now":"RoutingService runtime query","target_owner":"RoutingRuntimeOverlayReadService","lifecycle":"runtime-only overlay","delete_when":"facade injects overlay reader"},
  {"kind":"method","name":"station_endpoint_probe_target","line":1247,"class":"X","access":"pub(crate)","callers":["RoutingEndpointTargetReadPort adapter"],"owner_now":"RoutingService endpoint adapter","target_owner":"EndpointTargetReadService","lifecycle":"probe target capture","delete_when":"independent endpoint target owner implements port"},
  {"kind":"method","name":"record_station_endpoint_health","line":1259,"class":"X","access":"pub(crate)","callers":["RoutingEndpointHealthWritePort adapter"],"owner_now":"RoutingService endpoint CAS adapter","target_owner":"EndpointHealthSnapshotWriter","lifecycle":"revision-fenced endpoint write","delete_when":"independent writer owns CAS and all callers use port"},
  {"kind":"method","name":"simulate_route","line":1291,"class":"Q","access":"pub(crate)","callers":["RoutingCommandFacade -> IPC simulation command"],"test_callers":["routing tests"],"owner_now":"RoutingService simulation query","target_owner":"RouteSimulationQuery","lifecycle":"read-only local preview","delete_when":"simulation facade injects query owner"},
  {"kind":"helper","name":"routing_policy_v3_from_stored","line":1610,"class":"P/Q","access":"pub(crate)","callers":["routing_health command; policy coordinator"],"test_callers":["routing_loopback; policy tests"],"owner_now":"routing decoder helper","target_owner":"routing_policy_read decoder","lifecycle":"V1/V2/V3 compatibility decode","delete_when":"no cross-module routing re-export callers"},
  {"kind":"helper","name":"sync_routing_policy_file","line":1723,"class":"P","access":"pub(crate)","callers":["RoutingPolicyMutationCoordinator; routing_generation_cutover_runner"],"test_callers":["routing tests"],"owner_now":"routing document sync helper","target_owner":"RoutingDocumentCoordinator","lifecycle":"desired/materialized mirror","delete_when":"generation/coordinator use document sync port"},
  {"kind":"helper","name":"initialize_routing_policy_document_sync","line":1858,"class":"P","access":"pub(crate)","callers":["lib startup"],"owner_now":"routing startup sync helper","target_owner":"RoutingDocumentCoordinator","lifecycle":"startup mirror hydration","delete_when":"startup injects document sync owner"},
  {"kind":"helper","name":"reconcile_external_routing_policy_document","line":1875,"class":"P","access":"pub(crate)","callers":["RoutingService method wrapper only"],"owner_now":"routing document helper","target_owner":"RoutingDocumentCoordinator","lifecycle":"stable external document import","delete_when":"wrapper caller count reaches zero"}
]
```

## 审计限制与暂停条件

- 本 inventory 是当前工作树扫描结果；新增生产 caller、改变 `cfg`、生成绑定或跨层 composition 后必须重新生成并审阅。
- 未把 `RoutingStore` 的同名方法当成 `RoutingService` caller；它们是下游 persistence owner，只有通过上表路径才计入依赖。
- `commands/*`、`RoutingCommandFacade`、`RoutingExecutionReader` 等调用层的同名方法必须继续按其 owner 区分，不能因名称相同而误删。
- 任何删除动作前都必须满足对应行的删除条件，并重新运行 `rg`、相关 focused test、`routing-single-owner` 和 `git diff --check`。
- 发现 stale revision 被接受、旧 health/error-rate/capacity writer 复活、或 proxy/monitoring 重新持有宽 `RoutingService` 时，立即停止迁移，回到本清单重新审计。
