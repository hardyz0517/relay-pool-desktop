# 智能路由重试与故障转移升级基线

状态：Working baseline；本文记录 Task 0 的代码事实，不宣称升级已完成。

日期：2026-08-21

目标规格：[`../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md`](../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md)

实施计划：[`../plans/2026-08-20-intelligent-routing-retry-failover-configuration.md`](../plans/2026-08-20-intelligent-routing-retry-failover-configuration.md)

## 当前事实

| 范围 | 当前 owner/行为 | 证据位置 | 升级处理 |
| --- | --- | --- | --- |
| 策略文档 | `RoutingPolicyDocumentV1` 包含 `RoutingPolicyConfigV1`；envelope 已有 camelCase/unknown-field 约束，但嵌套 policy 仍是 V1 storage shape | `src-tauri/src/models/routing_policy.rs`、`src-tauri/src/services/policy_documents.rs` | V1 作为 upgrade input，新增 V2 public/domain contract；严格 decoder 继续复用 document service 的重复键检测能力 |
| 策略 CAS | routing policy 通过 aggregate revision 与 `apply_routing_policy_document` 做 CAS；旧 `save_routing_policy` 仍是兼容 facade | `src-tauri/src/application/routing.rs`、`src-tauri/src/persistence/stores/routing_policy_store.rs`、`src-tauri/src/commands/routing_health.rs` | 新字段只进入完整 document apply；后续删除 production direct update consumer |
| 总尝试上限 | admission 使用 `DEFAULT_MAX_ATTEMPTS = 4`；execution `RetryPolicy` 使用 `max_candidate_attempts = 4`；capacity profile 还有独立 `max_upstream_attempts = 4` | `src-tauri/src/application/routing_engine/admission.rs`、`src-tauri/src/services/proxy/execution.rs`、`src-tauri/src/services/proxy/routing_runtime.rs` | 编译成单一 `AttemptBudgetProfileV1`，删除无消费者字段和重复 hard-code |
| 同目标容量 retry | capacity runtime profile 默认 `same_target_retries = 2`、等待 `2000 ms`，有确定性 jitter、cooldown、waiter 和 Half-Open probe | `src-tauri/src/services/proxy/routing_runtime.rs`、`src-tauri/src/application/routing_engine/capacity.rs` | 四个首版字段只参数化已存在的容量 envelope，保留内部并发/队列安全边界 |
| canonical action | canonical failure 具备 `RetrySameTarget`、`WaitThenReplan`、`TryDifferentFailureDomain`、`StopRequest`；execution 通过二值 `RetryDecision` 将前三者压成 `NextCandidate` | `src-tauri/src/services/proxy/execution.rs` | 用单一 typed `RetryActionPlanner` 替换二值 decision，不新增平行 planner |
| replay gate | committed、已发送/响应开始和非幂等 Unknown 等边界 fail-closed；无 canonical producer 的失败也停止 | `src-tauri/src/services/proxy/execution.rs`、`src-tauri/src/services/proxy/request_send.rs` | 继续由 classifier/replay gate 唯一授权，设置不得绕过 |
| 跨请求保护 | durable scoped verdict 有 Degraded/Cooldown/Blocked；capacity registry 有进程内 Open/Half-Open；两者生命周期不同 | `src-tauri/src/models/health.rs`、`src-tauri/src/persistence/stores/routing_health_verdict_store.rs`、`src-tauri/src/services/proxy/routing_runtime.rs` | 首版只提供 `ProtectionStatus` read model；不把 test-only outlier 当生产 breaker |
| trace | runtime decision trace 有界且进程内，terminal outcome 可持久化摘要；重启后不保证完整逐步时间线 | `src-tauri/src/observability/decision_trace.rs`、`src-tauri/src/application/routing.rs` | 记录 effective policy/profile/action；UI 明确 summary-only，不扩大内存 ring |
| 设置页 | `LocalRoutingSettingsEditor` 自行维护 config/saved/revision/save state，以 JSON.stringify 判断 dirty | `src/features/routing/LocalRoutingSettingsEditor.tsx` | 先迁移 shared query/draft/typed conflict，再增加四个字段 |
| test-only outlier | `RuntimeOutlierPolicyV1`、窗口和 Half-Open 逻辑受 `cfg(test)` 保护 | `src-tauri/src/application/routing_engine/runtime_metrics.rs` | 禁止进入 active policy、IPC 或 UI；Phase 4 重新设计生产 observation 链 |

## 已确认的默认基线

- 总尝试边界：4。
- 同目标容量重试：2 次额外重试。
- 容量等待总预算：2000 ms。
- 容量耗尽后允许跨独立 capacity domain fallback：当前 admission 路径为 true。
- `Retry-After` 只参与容量路径等待裁剪；stream 已提交/响应开始不触发普通重放。

## Task 0 验证证据

已运行并通过：

```text
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib precommit_chat_capacity_event_enters_same_target_retry_path
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib cooldown_allows_only_one_half_open_probe_and_reopens_on_failure
```

两条命令各通过 1 项测试。编译输出包含约 280 条既有 warning，本基线不批量清理无关 warning。

尚未证明：V2 document、统一 profile、typed RetryAction、ProtectionStatus IPC 和设置页参数生效。这些是后续 Task 的验收内容。
