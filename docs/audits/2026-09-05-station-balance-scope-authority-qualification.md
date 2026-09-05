# Station Balance Scope Authority Qualification

状态：代码修正与自动化资格记录；发布 no-go（真实 KedayA 账号验收未执行）

日期：2026-09-05

## 结论

同一 Sub2API 账户的多把 Key 不再被推导为 Station 账户余额。直接账号事实使用 `account_balance`，Key 额度使用 `station_key_quota`；`subscription_quota`、`usage_summary` 与历史 `legacy_derived_aggregate` 保持独立，不能进入账号余额 current projection。没有 confirmed + authoritative 且未过期的事实时，投影返回 missing、untrusted、stale 或 depleted，路由资格 fail closed。

## 代码证据

- `src-tauri/src/services/collectors/collector_apply.rs` 不再包含 station balance aggregate writer；双 Key apply 回归保留两条 Key 事实，不生成 5.6。
- `src-tauri/src/persistence/migrations/0076_balance_kind_contract.sql` 为 kind/scope 增加约束、索引，并把历史 aggregate 分类为 `legacy_derived_aggregate`，不提升 authority。
- `src-tauri/src/persistence/stores/operational_facts/queries.rs` 批量读取 typed balance evidence；Key quota 优先，只有对应 Key 没有事实时才回退 Station account。
- `src-tauri/src/application/operational_facts/assembler.rs` 与 planning snapshot 校验 kind、scope、currency、authority 和 valid-until；未知、未确认、过期和缺失事实拒绝路由。
- `src-tauri/src/persistence/stores/routing_store.rs` 仅读取 `station_key_quota` 或 `account_balance`，不按金额比较 scope。
- `src/lib/projections/balanceFacts.ts` 和 Dashboard 只选择 account balance；Key quota、订阅额度、legacy aggregate 不计入 Station 总额。
- `scripts/balance-scope-authority.test.mjs` 禁止生产 aggregate 引用、通用 Key 求和和 compatibility cache current fallback。

## 自动化验证

以下命令在本轮执行；最终结果以命令退出码为准：

| 命令 | 结果 |
| --- | --- |
| `pnpm vitest run src/features/dashboard/dashboardBalanceSummary.test.ts src/lib/projections/balanceFacts.test.ts` | 通过，8 tests |
| `node scripts/balance-scope-authority.test.mjs` | 通过 |
| `node scripts/station-collection-authority.test.mjs` | 通过 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml` | 通过 |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml` | 通过，有仓库既有 warning |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader` | 通过（使用临时 Cargo target，避免默认 target 的 displaydoc DLL 锁定） |
| `pnpm generate:bindings --check` | 通过 |
| `$env:CARGO_TARGET_DIR='output/cargo/verify-fast-final'; pnpm verify:fast` | 通过（隔离 Cargo target；架构、余额静态门禁、生成绑定、ESLint、TypeScript 与 Rust 架构 fixture 全部通过） |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture` | 通过，14 tests；共享 loopback 夹具已补充明确 confirmed/authoritative `account_balance`，生产 fail-closed 规则不变 |
| `$env:CARGO_TARGET_DIR='output/cargo/schema-upgrade-final'; pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/run-schema-upgrade-modules.ps1 -StartModule migrations` | 通过：migrations 28、secret baseline 4、sanitizer 41、routing v3 8、schema15 fixture 3；persistence artifact 与 install contract 均通过 |
| `$env:CARGO_TARGET_DIR='output/cargo/verify-full-lowjobs'; $env:CARGO_BUILD_JOBS='1'; pnpm verify:full` | 通过（单次低并发运行；前端、架构/静态门禁、生成绑定、schema/portable/backup 专项与 Rust 全部通过；Rust 主套件及集成测试无失败） |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml v2_streaming_request_lease_survives_handler_return_until_body_drop --lib -- --nocapture` | 通过，1 test；作为既有流式租约时序回归的补充验证 |

临时 Cargo target 仅用于测试编译，未纳入仓库；不得提交数据库、日志、原始响应或凭据。为规避 Windows 分页文件不足，本次 `verify:full` 使用 `CARGO_BUILD_JOBS=1`，不改变测试内容。

## 未完成与发布边界

- 未执行真实 KedayA 账号验收，因此不能声称 Station Detail、Dashboard、重启和第三把 Key 操作已在线上账号通过。
- schema upgrade、portable migration、checksum/fingerprint、backup/recovery、restart/replay 相关自动化专项已完成并通过；真实安装包 old/new bundle 矩阵仍受发布环境依赖约束，安装契约检查已通过。
- `verify:full` 已取得单次全绿退出结果；低并发仅是构建资源约束下的执行参数。
- 发布前必须在无真实凭据落盘的前提下完成真实账号验收；在此之前本修正保持发布 no-go。
