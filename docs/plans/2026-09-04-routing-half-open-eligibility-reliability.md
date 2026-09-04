# 冷却结束后逻辑 Half-Open 可靠性修复

状态：Implemented

日期：2026-09-04

适用范围：Key 级 circuit 准入、Proxy 路由执行、路由工作区 read model、IPC 绑定、状态界面和当前智能路由规范。

关联入口：

- [`../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md`](../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md)
- [`../audits/2026-09-04-routing-half-open-eligibility-reliability.md`](../audits/2026-09-04-routing-half-open-eligibility-reliability.md)

## 目标不变量

熔断状态和冷却决定候选是否具备恢复资格；评分只决定具备资格的候选何时被轮到，不能取消恢复资格。

Open Key 冷却结束后，持久状态在被选中前仍保持 `Open(cooldown_elapsed)`，read model 将其投影为 `conditionally_eligible / circuit_recovery_ready`。它与 Closed Key 使用相同的目标 rank、可用层级、有效分数和稳定身份排序。真正被选中并通过容量准入后，出站前的原子 circuit admission 才写入 Half-Open lease 和 attempt slot。

## 实施范围

1. 从 reducer、持久化 store、应用服务、执行端口、Proxy repository 和 planner admission 数据中删除恢复专属评分准入参数及状态全量预读。
2. 保留 durable CAS、唯一 lease、lease revision、deadline、迟到结果保护、pre-boundary 释放、reaper、成功阈值和递增冷却。
3. 将冷却结束的 Open Key 投影为条件可参与；Half-Open idle 保留条件资格，lease occupied 排除，持久化不可用 fail-closed。
4. 将工作区 read model 提升到 `routing_workspace_read_model_v4`，公共诊断仅暴露 circuit 事实和 typed participation reason。
5. 将状态界面改为“已结束”“半开待探测”“半开待下次探测”和恢复成功进度，并从 typed reason 生成恢复说明。
6. 更新当前规范和架构删除门；历史 proposal、旧计划和旧审计保持原样。

## 明确不增加的行为

- 不强制轮换低分恢复候选。
- 不设置最长等待时间。
- 不发送 synthetic 探测。
- 不增加策略字段、数据库 schema、迁移或人工重置入口。

## 验收边界

- 冷却截止前拒绝，截止时刻起可以取得唯一 Half-Open lease。
- 高分 Closed Key 成功时不额外请求低分恢复 Key；高分候选被安全排除后，低分恢复 Key 仍能接管。
- 准入拒绝发生在 outbound boundary 前，不消费不同 Key 重试预算，也不写质量或失败样本。
- Half-Open 成功、失败、取消、deadline、目标失效、lease 竞争和 reaper 沿用 durable 状态机语义。
- circuit persistence 错误在任何出站前 fail-closed，并最终得到 `no_available_key` 与低基数 trace。
