# Relay Pool Data Architecture Stage 0 Baseline

日期：2026-07-07
工作树：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
分支：`codex/data-architecture-stage0`

## 基线命令

- `node scripts/pricing-comparison-view-model.test.mjs`: PASS
- `node scripts/add-provider-key-groups.test.mjs`: PASS
- `node scripts/test-dashboard-balance-summary.mjs`: PASS
- `node scripts/edit-key-page-flow.test.mjs`: PASS
- `node scripts/shared-capabilities-contract.test.mjs`: PASS

## Stage 0 边界

- 不删除字段。
- 不合并 `group_key_hash` 和 `group_id_hash`。
- 不把 `group_name` 当作首选 identity。
- 不迁移 UI 布局。
- 不引入新的 Tauri command。
- 不改数据库 schema。

## 主线同步点

- 主工作区 HEAD：`7e7d567`
- 工作树 HEAD：`1fee5a7`
- 工作树分支：`codex/data-architecture-stage0`
- 集成分支 upstream：`origin/master`
- 接入状态：未 merge 主工作区 dirty files；当前工作树只包含已提交的总 SPEC/总计划/字段清单。

## 未提交主线改动

主工作区存在未提交改动，不能通过 `git merge master` 自动接入。当前 Stage 0 记录为未接入风险：

- `scripts/pricing-comparison-view-model.test.mjs`
- `src-tauri/src/services/collectors/adapters/sub2api.rs`
- `src-tauri/src/services/collectors/apply.rs`
- `src/features/pricing/pricingComparisonViewModel.ts`
- `src/features/stations/components/StationDetailContent.tsx`
- `src/features/stations/stationDetailViewModels.ts`
- `src/lib/types/groupFacts.ts`
- `scripts/station-detail-group-source.test.mjs`
- `scripts/station-group-visual-meta.test.mjs`
- `src/features/stations/components/Sub2ApiPlatformIcon.tsx`
- `src/features/stations/groupVisualMeta.ts`

处理原则：

- 不复制、不回滚、不 stage 主工作区文件。
- 若这些改动后续提交到 `master`，工作树再做 drift intake 并 merge。
- 若需要提前接入，必须由用户明确指定 patch 范围。

## 字段归属清单

- 字段归属清单路径：`docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
- 当前状态：已建立核心字段初始登记。
- 新增字段策略：默认进入 `unknown pending audit`，补测试后再改分类。

## 兼容字段白名单

Stage 0 新增 `scripts/data-architecture-field-ownership.test.mjs`，用于阻止新增页面或 runtime 路径直接消费兼容字段。当前白名单代表尚未迁移的旧消费者，不代表推荐架构。

允许直接消费兼容字段的路径包括：

- 类型定义、API 边界、mock 数据、未来 query/projection 边界。
- 旧页面消费者：价格页、站点详情、站点资产、Stations、Key Pool、Add Provider、变更中心、监控、采集、日志、路由。
- Rust 持久化/采集/远端 Key/proxy runtime 等当前事实写入和 runtime 兼容路径。

收窄规则：

- 每迁走一个消费者，必须从白名单移除对应路径。
- 新增路径命中兼容字段时，默认失败；只有登记到字段归属清单并写明迁移理由后才允许临时白名单。
- Query service 可以搬运 raw facts，但不能把兼容字段解释为权威事实。

## 保护对象

- 价格页同一个当前分组不能因为 binding 和 rate record 同时存在而显示两次。
- 同站点同模型下多个真实不同分组必须保留。
- station-scope 余额快照优先于 station-key 快照。
- Key 编辑无关字段时必须保留当前 group binding；清除分组必须是显式动作。
- shared capabilities 保存 Key 时必须携带 group binding 选择，不能把上游 group id 当成本地 binding id。
- 兼容字段只能在白名单路径内直接消费，新代码应通过 projection/query 层消费。
