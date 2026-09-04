# 冷却结束后逻辑 Half-Open 修复验收记录

状态：Implementation complete；验证进行中

日期：2026-09-04

关联计划：[`../plans/2026-09-04-routing-half-open-eligibility-reliability.md`](../plans/2026-09-04-routing-half-open-eligibility-reliability.md)

## 已实现事实

- Circuit reducer 在冷却截止前返回 cooldown 拒绝，截止时刻起直接原子创建 Half-Open lease；Half-Open idle 可以取得下一次 lease。
- Proxy 执行不再为恢复评分预读全部 circuit status；出站前 durable circuit admission 是唯一事实门，持久化失败保持 fail-closed。
- Planner 的确定性顺序保持不变，低分恢复候选不会抢占高分 Closed 候选，但高分候选被排除后仍可成为后续候选。
- 工作区使用 `routing_workspace_read_model_v4` 和 `circuit_recovery_ready`，冷却结束的 Open Key 计为 conditionally eligible。
- UI 显示“已结束”“半开待探测”“半开待下次探测”和恢复成功次数，诊断说明由后端 participation reason 驱动。
- 架构契约扫描生产 Rust、当前 IPC DTO、生成绑定和相关前端源文件，阻止恢复专属评分准入重新出现。

## 验证证据

验证命令和最终结果将在本轮实现结束时写入本节。
