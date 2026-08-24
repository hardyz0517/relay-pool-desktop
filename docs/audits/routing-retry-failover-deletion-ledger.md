# 重试与故障转移 owner 删除台账

状态：Phase 0-1 ledger；已清理项保留证据，Phase 2-4 遗留项继续跟踪。

日期：2026-08-21

| 遗留项 | 当前位置 | 处理 | 完成条件 |
| --- | --- | --- | --- |
| admission 总尝试 hard-code `DEFAULT_MAX_ATTEMPTS` | `src-tauri/src/application/routing_engine/admission.rs` | replace | 从 compiled `AttemptBudgetProfileV1` 注入，source-absence test 通过 |
| execution `RetryPolicy.max_candidate_attempts` 默认 4 | `src-tauri/src/services/proxy/execution.rs` | removed | 生产 RetryPolicy 仅拥有 transport timeout；总尝试由 request snapshot/profile 提供 |
| capacity `CapacityRetryProfileV1.max_upstream_attempts` | `src-tauri/src/services/proxy/routing_runtime.rs` | removed | 无 production consumer；字段、初始化和测试引用已删除 |
| canonical -> 二值 `RetryDecision` | `src-tauri/src/services/proxy/execution.rs` | replaced | 唯一 `RetryActionPlanner` 保留 action、reason、scope、wait、budget |
| `RoutingPolicyConfigV1` 作为 active runtime config | `src-tauri/src/models/routing_policy.rs`、`src-tauri/src/application/routing_policy.rs` | retain as migration input | active aggregate/compiler 只消费 V2；V1 仅 decoder/legacy fixture |
| legacy `save_routing_policy` / `update_routing_policy` facade | `src-tauri/src/application/routing.rs`、facade/command、IPC/bridge、`src-tauri/permissions/main-window.toml`、compiled ACL | source/registry/frontend/ACL removed | 所有 production caller 已改用完整 document CAS；源码 command、DTO、binding、前端 adapter 和窗口 ACL 已删除，当前 `apply_routing_policy_document`、`get_routing_protection_status`、`list_error_rate_history` 已在 ACL allow-list；历史 migration 不受影响 |
| 页面局部 server state 与 JSON dirty compare | `src/features/routing/LocalRoutingSettingsEditor.tsx` | replaced | shared query/draft/revision/conflict owner 完成，组件不直接维护 revision |
| durable verdict、legacy snapshot、capacity runtime 多读模型 | routing health/query/runtime modules | retained behind projector | UI/IPC 只读取 `ProtectionStatus`，各生命周期显式区分 |
| `RuntimeOutlierPolicyV1` | `src-tauri/src/application/routing_engine/runtime_metrics.rs` | retain test-only | 不进入 production snapshot/document/UI；Phase 4 另行设计 |
| 手写前端 routing policy DTO | `src/lib/types/routing.ts`、bridge generated consumer | remove after binding update | `generate:bindings` 后前端仅使用生成类型 |
| lifecycle `TryNextCandidate` projection | `src-tauri/src/application/request_lifecycle/attempt.rs`、`request_finalization/effect_planner.rs` | retain compatibility projection, not an action owner | `RetryAction`/`RetryActionPlanner` 是唯一实际动作 owner；`TryNextCandidate` 仅用于兼容生命周期/终态记录，UI、trace 和重试执行不得据此反推 Stop/Wait/SameTarget/OtherDomain |

## 删除验证

每次 Task 结束时运行：

```powershell
rg -n "RetryDecision|max_upstream_attempts|DEFAULT_MAX_ATTEMPTS" src-tauri/src
```

Phase 0-1 验证结果：`RetryDecision`、`max_upstream_attempts`、`DEFAULT_MAX_ATTEMPTS` 在 `src-tauri/src` production path 中无残留；`max_candidate_attempts` 也已从 production RetryPolicy 删除。`models/monitoring::RetryPolicy::theoretical_max_attempts` 属于独立监控领域，不是路由执行 owner，保留并不改变本表结论。`TryNextCandidate` 仍有上述生命周期兼容投影；它不构成第二个 planner。旧 routing mutation 已从源码、registry、frontend binding 和窗口 ACL 完全移除；public apply 的 duplicate-key 保证仅适用于 raw/file decoder，已解析 IPC `Value` 不承诺原始重复键检测。
