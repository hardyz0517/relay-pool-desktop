# Relay Pool Desktop 文档

这里维护项目当前规范、设计决策、实施记录和发布资料。开发前先从本页确认文档状态，再阅读相关代码、测试与自动化契约。

发生冲突时，事实优先级依次为：[`AGENTS.md`](../AGENTS.md)、当前代码与自动化契约、本页列出的当前规范、已批准设计、带日期的工程记录、历史归档。

## 当前规范

以下文档长期有效，是当前实现与评审的主要入口：

- [`PROJECT_PLAN.md`](PROJECT_PLAN.md)：项目定位、能力边界与当前阶段方向。
- [`PRODUCT_MODEL.md`](PRODUCT_MODEL.md)：核心领域术语与对象职责。
- [`PRICING_MULTIPLIER_MODEL.md`](PRICING_MULTIPLIER_MODEL.md)：原始倍率、兑换率、实际倍率与消费边界。
- [`SECURITY_EXPORT_IMPORT.md`](SECURITY_EXPORT_IMPORT.md)：导入、导出与敏感数据边界。
- [`SCHEMA_UPGRADE_AUTHORING.md`](SCHEMA_UPGRADE_AUTHORING.md)：schema `15` 之后的数据升级编写契约。

智能路由当前以 [`specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md) 为目标规格，以 [`plans/2026-08-05-intelligent-routing-engine-upgrade.md`](plans/2026-08-05-intelligent-routing-engine-upgrade.md) 为总体实施记录。上游错误分类与重试的收口范围见 [`plans/2026-08-13-upstream-error-classification-retry-closure.md`](plans/2026-08-13-upstream-error-classification-retry-closure.md)，传输边界见 [`specs/2026-08-13-reliable-transport-send-phase-spike.md`](specs/2026-08-13-reliable-transport-send-phase-spike.md)。

变更中心告警仍以 [`proposals/CHANGE_CENTER_ALERTING_UPGRADE_SPEC.md`](proposals/CHANGE_CENTER_ALERTING_UPGRADE_SPEC.md) 记录目标设计；基线、边界清单与实施证据位于 [`audits/`](audits/)。价格监控联动和状态监控 V2 的设计入口分别为 [`specs/PRICING_MONITORING_INTEGRATION_SPEC.md`](specs/PRICING_MONITORING_INTEGRATION_SPEC.md) 与 [`specs/STATUS_MONITORING_REFACTOR_SPEC.md`](specs/STATUS_MONITORING_REFACTOR_SPEC.md)。

## 目录分类

| 目录 | 内容 | 是否可作为当前实现依据 |
| --- | --- | --- |
| [`adrs/`](adrs/) | 已接受的架构决策及其取舍 | 是；被替代时新增 ADR，不改写历史决策 |
| [`specs/`](specs/) | 已形成的设计快照、技术契约和专项分析 | 需结合状态、日期及当前代码判断 |
| [`proposals/`](proposals/) | Draft、RFC 和尚未成为默认基线的目标规格 | 否，除非本页明确列为已批准入口 |
| [`plans/`](plans/) | 一次性实施计划、任务拆分和迁移步骤 | 否；它们是执行记录，不是当前待办列表 |
| [`audits/`](audits/) | 基线、验收、资格记录、台账和机器可读清单 | 证据类可参考；其中部分 JSON 是自动化契约 |
| [`release/`](release/) | 版本说明、升级恢复说明和发布检查清单 | 仅适用于对应版本或发布流程 |
| [`research/`](research/) | 外部项目调研、源码审阅和 UI 参考 | 否，只提供背景与可借鉴结论 |
| [`archive/`](archive/) | 已结束阶段和被替代资料；按 plans、specs、audits 分类 | 否，只用于追溯历史 |
| [`assets/`](assets/) | README 和其他文档引用的图片等静态资源 | 不适用 |

## 维护约定

- `docs/` 根目录只放长期有效、跨模块的项目级规范和本导航；不要把临时设计或单次任务计划放在根目录。
- 新文档应在开头说明状态、适用范围和关联入口。被替代后移入 `archive/`，或在文首明确标注替代文档。
- ADR 只记录已经接受的关键架构选择；普通方案说明放 `specs/`，尚待批准的方案放 `proposals/`。
- `plans/` 文件保留日期，用于复盘实施过程；完成后不持续改写成当前事实。
- `audits/` 中的清单和 JSON 可能被测试或发布门禁直接读取，移动、重命名或删除前必须同步自动化引用。
- 外部代码、产品分析和视觉参考统一放入 `research/`，不得直接视为项目实现规范。
