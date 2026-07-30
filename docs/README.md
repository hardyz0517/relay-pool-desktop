# 文档导航

本文档用于区分当前规范、工程记录与历史资料。开始开发前，应先阅读当前规范；历史计划只用于理解演进背景，不能直接作为当前实现依据。

## 当前规范

- [`PROJECT_PLAN.md`](PROJECT_PLAN.md)：项目定位、能力边界与当前阶段方向。
- [`PRODUCT_MODEL.md`](PRODUCT_MODEL.md)：核心领域术语与对象职责。
- [`SECURITY_EXPORT_IMPORT.md`](SECURITY_EXPORT_IMPORT.md)：导入、导出与敏感数据边界。
- [`../AGENTS.md`](../AGENTS.md)：仓库级开发、验证与交付规则。

发生冲突时，优先级依次为：`AGENTS.md`、当前代码与自动化约束、上述当前规范、带日期的设计记录、历史阶段计划。

## 提案与待排期规格

- [`proposals/`](proposals/)：尚未成为当前实现基线的 Draft、RFC 和待排期规格。
- [`proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md`](proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md)：跨设备加密迁移 Draft；在正式排期和评审前不代表已实现能力。
- [`proposals/STATUS_MONITORING_REFACTOR_SPEC.md`](proposals/STATUS_MONITORING_REFACTOR_SPEC.md)：状态监控内核、协议适配、CLI 请求画像、调度、健康联动、时间桶与横向 UI 的全面重构 Draft。
- [`superpowers/plans/2026-07-29-status-monitoring-refactor.md`](superpowers/plans/2026-07-29-status-monitoring-refactor.md)：上述状态监控重构的逐任务实施、验证、切换与旧代码删除计划。

## 工程记录

- [`superpowers/specs/`](superpowers/specs/)：带日期的设计决策快照。内容可能已被更新日期的设计或当前代码取代。
- [`superpowers/plans/`](superpowers/plans/)：一次性实施计划和任务拆分，不代表仍有待执行。
- [`superpowers/audits/`](superpowers/audits/)：实施审计、验收记录与架构清单。其中部分 JSON 被自动化检查直接使用，不应视为普通历史文档移动或删除。
- [`release/`](release/)：版本说明和发布检查清单。

## 参考资料

- [`research/`](research/)：外部项目调研、源码审计和 UI 参考，只提供背景与可借鉴结论。
- [`archive/early-phase-plans/`](archive/early-phase-plans/)：P1-P8 早期阶段计划。它们记录当时的范围和实现状态，已不再是当前开发入口。

## 维护约定

- 长期有效的产品或安全约束放在 `docs/` 根目录，并在本页登记。
- 有明确日期和交付目标的设计、计划、审计分别放入 `superpowers/specs/`、`superpowers/plans/`、`superpowers/audits/`。
- 外部项目分析与视觉参考放入 `research/`。
- Draft、RFC 和待排期实现规格放入 `proposals/`；进入实施后再按项目流程更新状态和归属。
- 已完成且容易被误读为待办的阶段计划放入 `archive/`，保留原文，不持续更新实现细节。
- 新文档应说明状态、适用范围；被取代时应移入归档或在文首标明替代文档。
