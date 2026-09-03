# Station 采集与授权可靠性资格审计

状态：本轮可交付实现完成；真实 WebView/账号验收与旧字段物理删除为发布/后续 no-go

日期：2026-09-03

关联计划：[`../plans/2026-09-02-station-collection-authorization-state-reliability.md`](../plans/2026-09-02-station-collection-authorization-state-reliability.md)

## 结论

本轮可交付实现已建立 typed collection/authorization projection、单事务 Full terminal commit、credential/endpoint/intent fence、capture 隔离和 revision notice/reconcile 链路。它修复了“重新授权成功但 Station 仍显示采集需关注”的根因：授权证据不再伪装成 Full 采集，旧 revision 的 401 不能覆盖当前授权投影，Station Asset/Detail 不再以 `stations.status` 作为 current authority。

普通（非 capture）`collector_operations` 也已纳入启动恢复。上次进程遗留的 `queued/running` operation 会在单一启动事务中按当前 fence 归档为 `interrupted` 或 `superseded`，相应遗留 running history 被关闭；scheduler 因而不会被孤儿 operation 永久阻塞。运行与 snapshot 历史读取已从 `CollectorService` 提取到只读 `CollectorHistoryQuery`，Station Detail 和 metadata facade 使用统一 owner，生产旧入口已删除并由负向架构门禁保护。

本轮不能宣称旧链已完全删除。调度和告警已迁移到最新终态 `collector_runs`，旧 `collector_task_state` writer 仅保留在 `cfg(test)` 兼容夹具。`parent_run_id` 已从请求校验、Full 子任务构造、run-key/幂等输入以及授权/告警副作用判断中移除；Full terminal commit 只把实际生成的 parent run id 写入子任务历史，供历史导航与兼容读取。测试兼容字段仅在 `cfg(test)` 存在且被 `serde(skip)` 排除。兼容 DTO/查询仍暴露 `status` 与旧时间字段，物理删除必须等下述资格条件全部满足后另行评审。

## 已验证的正向不变量

| 不变量 | 证据 | 结论 |
| --- | --- | --- |
| Full 终态原子可见 | `src-tauri/src/application/collectors.rs` 的 Full apply 在单个 `WriteSession` 提交 parent、children、facts、projection、alerting 和 revisions | 通过 |
| capture 与 collection 解耦 | capture 使用 `task_type = capture/recharge`，不更新 collection projection；授权成功需 authenticated self-probe | 通过 |
| Station Asset/Detail 读取 typed projection | `application/queries/station_assets.rs`、`station_detail.rs` 在同一 `ReadSession` 读取 projections 与 revision | 通过 |
| Station Detail 不回退旧状态 | 前端按真实 DTO 结构合并 sibling `collectionSummary` / `authorizationSummary`；冲突测试证明旧 `station.status=warning` 不会覆盖 typed healthy | 通过 |
| `stations.status` 非 current authority | 生产代码未发现 `UPDATE stations SET status`；catalog 写入不包含 status 列；状态读取由 typed read model 提供 | 通过（兼容字段仍存在） |
| 通知不承担正确性 | commit 后才发布 `DomainRevisionNotice`，前端丢通知可通过 scope reconcile 恢复 | 通过 |
| 普通 operation 启动可恢复且 owner 隔离 | app startup 调用 `recover_active_collector_operations`；store 仅对标准 collector 遗留 active operation 执行 fence-aware terminalization，明确排除 capture/post-authorization；回归测试验证普通孤儿恢复后重新准入，post-auth 再由专属恢复链完成 ledger/work 双成功 | 通过 |
| history query 职责单一 | `CollectorHistoryQuery` 只持有 read stores/`ReadSession`；Station Detail 和 metadata facade 经该 owner 读取，架构门禁禁止 `CollectorService` 恢复生产 history 方法 | 通过 |
| schema `0072 -> 0074` 升级链可恢复 | 首个开发版 `0072` 仅在精确 checksum 与精确结构均匹配时，于已验证备份后原子补齐 canonical postconditions；生产升级入口测试证明到 latest、checksum/结构、21 个 revision triggers、FK clean 及备份可恢复 | 通过 |

## 未清零的生产依赖

| 对象 | 当前生产消费者/写入者 | 风险 | 退出条件 | 责任 |
| --- | --- | --- | --- | --- |
| `collector_task_state` | 仅 `CollectorStore::update_task_state_for_test` 测试兼容夹具；生产调度、告警和终态提交已迁移到终态 `collector_runs`/typed projection | 历史表和测试夹具仍可能被误接回生产 | 保持生产源码零命中（migration/import/test 白名单除外）；完成 operation/task projection 的重启、并发和告警回归后评审物理删除 | CollectionOperationCoordinator owner |
| `collector_runs.parent_run_id` | Full terminal commit 写入实际 parent id；历史 runs/portable 读取保留该列 | 历史字段若重新接入控制流会绕过 operation ledger；物理删除还涉及旧安装兼容 | 保持生产源码零控制流引用；`parent_run_id` 仅限历史读取/导入；幂等哈希和 run key 不含该字段；完成 portable/备份/回滚资格后再评审 DROP | CollectionCommitService owner |
| `stations.status` / `last_*` DTO | `StationDto`、TS `Station` 与 catalog 兼容读取仍暴露字段；Station Asset/Detail current UI 不再消费其状态 authority | compatibility surface 若无门禁可能被新 caller 误用 | 完成 caller inventory、导入导出、版本窗口和 rollback 说明；再单独评审 DROP migration | Station read-model owner |
| `CollectorService` 剩余收敛 | run/snapshot history 已提取到 `CollectorHistoryQuery`；service 仍承担 collection command/terminal commit 及部分 metadata use case，大量同模块测试仍在 | 后续扩展若绕过 owner 仍可能重新堆积职责 | 保持 history owner 负向门禁；其余职责按真实 caller 渐进提取，不以文件行数或本轮旧字段 DROP 作为完成门槛 | Application architecture owner |

## 物理删除 no-go

以下条件任一未满足时，不得 DROP 列/表或改写历史 migration：

- 至少一个完整版本的生产读写扫描、portable migration、export/import 和旧 fixture 升级矩阵通过；
- `collector_task_state`、`parent_run_id`、`stations.status`、`stations.last_checked_at`、`stations.last_pricing_fetched_at` 的 production caller inventory 已签名，migration/import/test 白名单与运行时路径分离；
- 备份 manifest、隔离数据目录恢复和旧版本前滚方案通过；不提供 SQL downgrade；
- 真实 Tauri WebView 完成重新授权、self-probe、post-auth Full、离线失败、关闭/取消和重启恢复验收；
- 发布 owner 冻结 revision、rollback floor 和 go/no-go 结论。

## 验证记录

本审计只记录当前工作区已提供的自动化证据，不把未执行的真实账号/WebView 操作写成通过。相关 migration、collector 并发/故障注入、capture recovery、Station Asset/Detail、revision synchronizer 和生成契约测试应在每次旧链迁移后重跑；命令失败、执行数为 0 或仅命中 compatibility fixture 均不能作为完成证据。

最新工作区实际取得退出码 0 的验证包括：`pnpm verify:full`（`CARGO_BUILD_JOBS=1`、`RUST_TEST_THREADS=1`、`RELAY_POOL_NPM_AUDIT_TIMEOUT_MS=300000`；Rust 主库 1552 passed，前端 145 files/679 tests，完整集成矩阵通过）、前端生产构建、Rust formatting/clippy/all-targets/release checks、collector 定向测试、`station_collection_reliability_migration`、`station-collection-authority.test.mjs` 与 `station-auto-collector.test.mjs`。本次全量结果已经包含普通 operation 启动恢复、post-auth owner 隔离、`CollectorHistoryQuery` 职责提取和 Station Detail sibling summary 映射修复。输出中的既有 warning、React `act(...)` 提示和 Vite chunk-size 提示不改变退出码。

截图反馈后的 schema `0072` 兼容修复另有定向增量证据：canonical migration checksum 冻结测试和首个开发版 `0072 -> latest` 生产升级入口测试均通过。该测试不只改写 `_sqlx_migrations`，还验证精确历史结构、已验证升级前备份、派生 projection 重建、canonical postconditions、最终 schema 与 foreign-key 完整性；未知 checksum 或未知结构仍失败关闭。增量后的 `cargo fmt -- --check`、`cargo check --locked`、`station_collection_reliability_migration`（2 passed）、冻结 schema15 升级 fixture（3 passed）、startup upgrade 单元矩阵（15 passed）及 `pnpm verify:fast` 也均取得退出码 0。

同一修复版本在截图对应的本机数据库上完成启动 smoke：只读核验显示 compatibility schema 与已应用 migration 均为 `74`，collection projection 已具备 endpoint/credential/intent fence 列，Station Asset revision triggers 为 `21`，`pragma_foreign_key_check` 为 `0`；升级链复用了已校验的 `72 -> 74` 备份 manifest。该 smoke 仅证明数据库升级与结构完整性，不替代使用者对真实站点授权/采集行为的验收。

因此，本轮代码、迁移、契约、自动化验证和文档台账均已收口，可标记为“本轮可交付实现完成”。真实 WebView/账号验收和兼容字段物理 DROP 仍按上文 no-go 独立跟踪，不纳入本轮完成声明。
