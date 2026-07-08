# Relay Pool 数据架构 Stage 8 进度审计

日期：2026-07-08

## 范围

Stage 8 复查 Stage 0-7 迁移后的兼容字段状态。范围限定为字段归属清单、source guard 和审计；未删除字段、未改 schema、未合并字段语义。

## Drift Intake

- 主 checkout 路径：`D:\Dev\Projects\relay-pool-desktop`
- 主 checkout HEAD：`ca0d9a7 fix: refine sub2api group displays`
- 主 checkout dirty paths：未观察到 tracked / untracked dirty path；`git status --short` 仍输出 `.pnpm-store` 缺失目录 warning。
- 工作树路径：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
- 工作树分支：`codex/data-architecture-stage0`
- Stage 8 起点 HEAD：`f155001 docs: summarize data architecture stage7 progress`
- 当前工作树 HEAD：`8fe6850 docs: review compatibility field ownership`

## 已完成

- `518e187 docs: add data architecture stage8 plan`
- `c08274f test: guard compatibility field review`
- `8fe6850 docs: review compatibility field ownership`
- 新增 `scripts/compatibility-field-review.test.mjs`。
- `docs/superpowers/audits/relay-pool-field-ownership-ledger.md` 追加 `Stage 8 兼容字段复查结论`。
- 明确本轮无 removable candidate 字段。
- `station_keys.group_name`、`station_keys.group_id_hash`、`station_keys.rate_multiplier`、`station_keys.rate_source`、`station_keys.rate_collected_at` 继续保留为 compatibility cache。
- `stations.balance_raw`、`stations.balance_cny`、`stations.last_pricing_fetched_at` 继续保留为 compatibility cache。
- 确认 runtime snapshot 不消费 UI view model、不携带明文 secret。

## 字段审计

- 本阶段未新增 schema 字段。
- 本阶段未批准任何字段删除。
- 本阶段未把任何 compatibility cache 标记为 removable candidate。
- 老数据库、preview fallback、远端 Key 兼容路径、Rust models、collector/database 写入路径仍依赖兼容字段读写边界。
- 后续若要降级或删除字段，必须另开 migration/rollback note，并补老数据库、preview fallback、runtime route 和 Rust persistence 测试。

## 验证

已在 `8fe6850` 后运行：

```powershell
node scripts/compatibility-field-review.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
node scripts/runtime-snapshot-boundary.test.mjs
node scripts/runtime-snapshot-projection.test.mjs
pnpm.cmd build
git status --short
```

结果：

- 上述 Node 脚本全部 exit 0。
- `pnpm.cmd build` exit 0，仅保留既有 Vite chunk-size warning。
- `git status --short` 在写入本审计前为 clean。

## 整体升级状态

Stage 0/1/2/3/4/5/6/7/8 均已完成并提交。当前升级已建立：

- 字段所有权 ledger 与 compatibility guard。
- Query services raw facts 边界。
- Current group facts、pricing facts、balance facts、runtime route snapshot projections。
- 价格页、站点详情、站点资产、Key Pool、Edit Key、Add Provider 的 current facts 消费路径。
- Runtime snapshot 的 no UI view model / no plaintext secret boundary。
- 兼容字段复查结论：本轮不删除字段、不改 schema、不标记 removable candidate。

## 后续建议

- 合并前再次执行 drift intake，确认主 checkout 是否已有新提交需要接入。
- 若要把 runtime snapshot 接入 Rust proxy，另开阶段保护 secret manager 注入边界，先写 Rust/TS focused tests。
- 若要真正删除兼容字段，另开 migration 阶段并准备 rollback note。
