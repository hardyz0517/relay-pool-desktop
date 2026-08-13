# Relay Pool 数据架构 Stage 7 进度审计

日期：2026-07-08

## 范围

Stage 7 新增 runtime route snapshot 纯投影，为本地 proxy 编译稳定运行时输入。范围限定在 TypeScript projection 与 focused tests；未改 Rust proxy、未改 secret storage、未改数据库 schema。

## Drift Intake

- 主 checkout 路径：`D:\Dev\Projects\relay-pool-desktop`
- 主 checkout HEAD：`ca0d9a7 fix: refine sub2api group displays`
- 主 checkout dirty paths：未观察到 tracked / untracked dirty path；`git status --short` 仍输出 `.pnpm-store` 缺失目录 warning。
- 工作树路径：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
- 工作树分支：`codex/data-architecture-stage0`
- Stage 7 起点 HEAD：`1253887 docs: summarize data architecture stage6 progress`
- 当前工作树 HEAD：`c67cc09 test: guard runtime snapshot projection boundary`

## 已完成

- `cb6e91d docs: add data architecture stage7 plan`
- `a4fea16 test: guard runtime route snapshot projection`
- `3773cb4 refactor: add runtime route snapshot projection`
- `c67cc09 test: guard runtime snapshot projection boundary`
- 新增 `src/lib/projections/runtimeSnapshot.ts`。
- 新增 `buildRuntimeRouteSnapshot()`，输出 `RuntimeRouteSnapshot`、`RuntimeRouteSnapshotCandidate` 和 `RuntimeRouteSecretRef`。
- Snapshot candidate 包含 station key、station、priority、upstream base URL、upstream API format、group binding id、current group identity、倍率、model policy、pricing status、balance status、health/cooldown 和 evidence。
- Snapshot candidate 只包含 `secretRef`，不包含明文 key 字段；`secretRef` 仅记录 station key id、present 状态和 masked value。
- Snapshot projection 消费 `buildCurrentStationGroupFacts()`、`buildCurrentStationBalanceFacts()`、`buildPricingGroupCandidates()`，不导入 UI feature、API/query、Tauri 或 secret modules。
- 候选过滤 disabled station、disabled key 和 `apiKeyPresent = false` 的 key。

## 字段审计

- 本阶段未新增 schema 字段。
- `station_keys.group_binding_id` 继续作为 runtime candidate 的本地 durable group binding identity。
- `station_group_bindings.effective_rate_multiplier` 等 current group facts 继续提供 runtime 倍率来源。
- `group_rate_records` 继续是 evidence/history，只通过 current group projection 进入 snapshot。
- `balance_snapshots.scope = station` 继续通过 current balance projection 进入 runtime balance status。
- `station_keys.apiKeyMasked` / `apiKeyPresent` 只用于 secret reference display 和 presence，不等同于明文 secret。
- 明文 secret 不进入 TypeScript snapshot；后续 Rust proxy 注入真实 secret 时必须保持 secret manager 边界。
- 字段归属清单无需新增 `unknown pending audit` 行。

## 验证

已在 `c67cc09` 后运行：

```powershell
node scripts/runtime-snapshot-projection.test.mjs
node scripts/runtime-snapshot-boundary.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/pricing-facts-projection.test.mjs
node scripts/station-current-balance-projection.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
git status --short
```

结果：

- 上述 Node 脚本全部 exit 0。
- `pnpm.cmd build` exit 0，仅保留既有 Vite chunk-size warning。
- `git status --short` 在写入本审计前为 clean。

## 下一步

- 提交本审计后更新 rolling heartbeat。
- 进入 Stage 8：兼容字段复查。
- Stage 8 只做字段读写清单、状态标注和 migration/rollback note；老数据库、preview fallback、runtime 任一路径仍依赖时，不删除字段。
