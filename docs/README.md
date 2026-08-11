# 文档导航

本文档用于区分当前规范、工程记录与历史资料。开发前应优先阅读当前规范；历史计划只用于理解演进背景，不能直接作为当前实现依据。

## 当前规范

- [`PRICING_MULTIPLIER_MODEL.md`](PRICING_MULTIPLIER_MODEL.md)：原始倍率、兑换率与实际倍率的统一模型及消费边界。
- [`PROJECT_PLAN.md`](PROJECT_PLAN.md)：项目定位、能力边界与当前阶段方向。
- [`PRODUCT_MODEL.md`](PRODUCT_MODEL.md)：核心领域术语与对象职责。
- [`SECURITY_EXPORT_IMPORT.md`](SECURITY_EXPORT_IMPORT.md)：导入、导出与敏感数据边界。
- [`SCHEMA_UPGRADE_AUTHORING.md`](SCHEMA_UPGRADE_AUTHORING.md)：schema `15` 之后的数据升级 authoring contract。
- [`../AGENTS.md`](../AGENTS.md)：仓库级开发、验证与交付规则。

智能路由升级已进入批准设计与实施阶段：

- [`proposals/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](proposals/INTELLIGENT_ROUTING_ENGINE_SPEC.md)：目标架构与验收合同。
- [`superpowers/plans/2026-08-05-intelligent-routing-engine-upgrade.md`](superpowers/plans/2026-08-05-intelligent-routing-engine-upgrade.md)：唯一当前实施计划。
- 智能路由本地产品边界已完成：[`superpowers/audits/intelligent-routing-acceptance-matrix.md`](superpowers/audits/intelligent-routing-acceptance-matrix.md)、[`superpowers/audits/intelligent-routing-qualification.md`](superpowers/audits/intelligent-routing-qualification.md)、[`superpowers/audits/intelligent-routing-deletion-ledger.md`](superpowers/audits/intelligent-routing-deletion-ledger.md) 和 [`superpowers/audits/intelligent-routing-boundary-manifest.json`](superpowers/audits/intelligent-routing-boundary-manifest.json) 是同一 revision 的证据闭环。真实 provider、外部监控和发布机 soak 仍属于独立发布门禁。

价格 / 倍率页与渠道状态页的只读联动已进入当前实现基线：

- [`proposals/PRICING_MONITORING_INTEGRATION_SPEC.md`](proposals/PRICING_MONITORING_INTEGRATION_SPEC.md)：行为契约与跨层边界。
- [`superpowers/plans/2026-08-03-pricing-monitoring-integration.md`](superpowers/plans/2026-08-03-pricing-monitoring-integration.md)：实施步骤、验证命令与回滚边界。

发生冲突时，优先级依次为：`AGENTS.md`、当前代码与自动化约束、上述当前规范、带日期的设计记录、历史阶段计划。

## 提案与待排期规格

- [`proposals/`](proposals/)：尚未成为当前实现基线的 Draft、RFC 和待排期规格。
- [`proposals/CHANGE_CENTER_ALERTING_UPGRADE_SPEC.md`](proposals/CHANGE_CENTER_ALERTING_UPGRADE_SPEC.md)：变更中心告警闭环、恢复状态机、提醒策略与“设置 → 提醒与告警”入口 Draft。
- [`superpowers/plans/2026-08-08-change-center-alerting-upgrade.md`](superpowers/plans/2026-08-08-change-center-alerting-upgrade.md)：上述 Draft 的完整执行计划；待规格批准后方可实施。
- [`superpowers/audits/change-center-alerting-baseline.md`](superpowers/audits/change-center-alerting-baseline.md)：Task 0 基线事实、已完成证据与未完成范围。
- [`superpowers/audits/change-center-alerting-deletion-ledger.md`](superpowers/audits/change-center-alerting-deletion-ledger.md)：旧变更中心代码、IPC、表和前端入口的 owner、删除前置条件与状态。
- [`superpowers/audits/change-center-alerting-boundary-manifest.json`](superpowers/audits/change-center-alerting-boundary-manifest.json)：legacy allowlist、新领域边界和受保护用户文件清单。
- [`proposals/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](proposals/INTELLIGENT_ROUTING_ENGINE_SPEC.md)：已完成设计评审的智能路由、共享后端事实、评分、置信度、监控反馈与解释合同。
- [`superpowers/plans/2026-08-05-intelligent-routing-engine-upgrade.md`](superpowers/plans/2026-08-05-intelligent-routing-engine-upgrade.md)：上述智能路由规范的详细实施任务、原子 cutover、删除合同与验证命令。
- [`proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md`](proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md)：跨设备加密迁移 Draft。
- [`proposals/STATUS_MONITORING_REFACTOR_SPEC.md`](proposals/STATUS_MONITORING_REFACTOR_SPEC.md)：状态监控 V2 架构和实现参考。

## 跨设备加密迁移发布状态

跨设备数据搬家 capability 已允许进入 `codex/cross-device-encrypted-migration` 集成分支；发布晋级仍必须基于目标 release revision 完成两机 smoke、签名包门禁和 artifact/canary 审计。默认导出、本机备份和同机 data-dir relocation 都不是 `.rpd-move` 跨设备迁移包。

## 状态监控 V2 记录

- [`superpowers/plans/2026-07-29-status-monitoring-refactor.md`](superpowers/plans/2026-07-29-status-monitoring-refactor.md)：状态监控 V2 全流程重构计划。
- [`superpowers/audits/status-monitoring-qualification.md`](superpowers/audits/status-monitoring-qualification.md)：确定性验证、真实 provider 授权门禁与发布资格清单。
- [`release/status-monitoring-v2-qualification.md`](release/status-monitoring-v2-qualification.md)：发布侧资格说明。
- [`superpowers/plans/2026-07-29-status-monitoring-legacy-table-removal.md`](superpowers/plans/2026-07-29-status-monitoring-legacy-table-removal.md)：一个发布观察周期后删除只读 `channel_monitor_runs` 兼容层的后续票据。

## 工程记录

- [`superpowers/specs/`](superpowers/specs/)：带日期的设计决策快照。
- [`superpowers/plans/`](superpowers/plans/)：一次性实施计划和任务拆分，不代表仍有待执行。
- [`superpowers/audits/`](superpowers/audits/)：实施审计、验收记录与架构清单；其中部分 JSON 被自动化检查直接使用，不应作为普通历史文档移动或删除。
- [`release/`](release/)：版本说明和发布检查清单。

## 参考资料

- [`research/`](research/)：外部项目调研、源码审阅和 UI 参考，只提供背景与可借鉴结论。
- [`archive/early-phase-plans/`](archive/early-phase-plans/)：P1-P8 早期阶段计划。它们记录当时范围和实现状态，已不再是当前开发入口。

## 维护约定

- 长期有效的产品或安全约束放在 `docs/` 根目录，并在本页登记。
- 有明确日期和交付目标的设计、计划、审计分别放入 `superpowers/specs/`、`superpowers/plans/`、`superpowers/audits/`。
- 外部项目分析与视觉参考放入 `research/`。
- Draft、RFC 和待排期实现规格放入 `proposals/`；进入实施后按项目流程更新状态和归属。
- 已完成且容易被误读为待办的阶段计划放入 `archive/`，保留原文，不持续更新实现细节。
- 新文档应说明状态、适用范围；被取代时应移入归档或在文首标明替代文档。
