# 智能路由评分、重试与 Key 熔断 v3 实施审计

状态：Completed（本轮交互发布范围；v3 运行链、代际切换和兼容边界清理已完成，最终重试语义已通过聚焦验证）

日期：2026-08-30

## 规范和基线

- 当前规范：[`../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md`](../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md)
- 规范 SHA-256（最终实施版本）：`092c84e3ee73504da7310bef2c05a99cbcaae9005fddc75d42018eb28b2fa1df`
- 迁移占用：`0060`--`0070` 已登记且未发现重复编号；`0060`--`0063` 是 v3 policy、observation、circuit 和 runtime generation schema，`0064` 保存代际 qualification report，`0065` 固定 raw event retention，`0066`--`0069` 依次完成 observation contract hardening、generation resume/qualification、circuit persistence gate 和 qualification v2 report schema；`0070` 增加 circuit applied-event 标记，使 generation rebuild 只重放已应用的 reducer 事件。
- Portable migration 基线：catalog 固定为 `110` 张用户表；fixture fingerprint 由 schema reader 的受信对象集合维护。
- 启动状态：迁移只创建 `pre_cutover` marker，不伪造 active generation；当前生产是否启用 v3 由 marker 和完整 runtime generation registry 决定。

## 已落地

- planner 在同一硬层内按 `effective_score DESC, station_key_id ASC` 确定性排序；生产路径不再使用随机探索或 rendezvous 分流。
- `maxRetryCount` 是首把 Key 之外可尝试的额外不同 Key 数量；同一 Key 在连续失败阈值内的重试、容量准入、Half-Open 竞争和快照重读都不消耗该数量；`429` 与其它上游单 Key 故障相同处理。
- station-key circuit 已实现 `Closed -> Open -> Half-Open -> Closed`、连续失败阈值、递增冷却、单 Half-Open lease、恢复成功阈值、generation/revision fence 和迟到结果保护。
- 真实请求和可比监控分别计算最近/历史可靠性，使用固定点时间衰减、最近/历史最小样本门槛、乐观可靠性/延迟值和默认 70/30 来源混合；真实请求/监控样本按 correlation 去重。
- 质量投影、circuit 重建和 runtime generation 使用 generation-scoped 表、checkpoint、输入水位、content hash、qualification report 和原子 pointer cutover；增量 quality revision 只单调推进，不修改 generation activation hash。
- 崩溃遗留 attempt 会标记为 `local_abandoned` 或 `upstream_uncertain`；恢复补写不会隐式再次发送请求。
- rollback 在 fence 期间发现新 circuit event 时会创建 replacement generation，重新完成质量/circuit tail replay、资格校验和激活；被尾部替代的旧 generation 会标记为 `failed`，从 retired policy 重建的 generation 会在激活时原子恢复 policy 状态。
- 生产 planner/admission/execution 已不读取容量域身份、跨域回退或作用域级 breaker；本地容量 registry 仍作为后置硬准入，`CapacityLease`/`CapacityWaitPermit` 通过 RAII 幂等释放。
- 设置页已改为评分、样本、超时、熔断器、重试和会话亲和分组；中转站编辑页不再挂载容量域身份编辑区块。
- 公共无可用 Key 终态为 `no_available_key`（HTTP 503）；容量终态仍使用 `route_capacity_exhausted`。

## 本次修正

- 熔断 lease reaper 采用 `lease_expires_at_ms <= now_ms`，在请求 deadline 恰好到达时即可回收，而不是多等一个周期。
- `attempted_count` 只在 durable attempt 成功标记 outbound boundary 后递增；本地目标解析、取消和 boundary 前 deadline 不再被计为 outbound attempt。普通代理与 `/models` 聚合路径保持一致。
- v3 active generation 后旧 error-rate reducer 被明确禁用；旧 `error_rate_protection`/`health_protection` API 仅作为兼容测试、迁移和诊断适配保留。
- 旧 `routing_failure_contract` 曾断言 `429` 使用 `Retry-After` 专用冷却；该契约已改为 `HealthEffect::ObserveFailure`，与“429 是普通单 Key 故障并进入统一熔断器”的 v3 语义一致。

## 验证证据

- `node scripts/upstream-error-contract.test.mjs`：通过。
- `node scripts/routing-single-owner.test.mjs`：通过。
- `node scripts/intelligent-routing-architecture.test.mjs`：通过。
- `node scripts/request-lifecycle-architecture.test.mjs`：通过。
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`：通过。
- `cargo check --locked --manifest-path src-tauri/Cargo.toml`：通过。
- `pnpm.cmd test:contracts`：通过。
- 较早实现基线的 `pnpm.cmd verify:fast`：通过（退出码 0；ESLint 仅有既有 warning）。最终重试语义调整后的复跑已到达 Rust 架构 fixture，但因正在运行的 `src-tauri/target/debug/relay-pool-desktop.exe` 被 Windows 锁定、无法覆盖而中止；未擅自关闭用户正在体验的桌面程序。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_failure_contract -- --nocapture`：`2 passed, 0 failed`。
- `$env:CARGO_BUILD_JOBS = '1'; pnpm.cmd verify:full`：最终退出码 `0`，Rust 库测试 `1512 passed`，Rust 全部测试通过；前端 `139` 个测试文件、`624` 个测试通过；生产构建、两轮 IPC bindings 确定性检查、runtime event catalog、ESLint、TypeScript、Rust fmt/clippy/check、架构、安全、迁移和许可证门禁均通过。
- schema `0070` 与最终失败作用域收口后又执行了聚焦回归：portable schema fingerprint `1 passed`、station-key circuit store `10 passed`、v3 migration `6 passed`、routing outcome domain `16 passed`、真实代理 502 写入 Key circuit 闭环 `1 passed`；`upstream-error-contract`、`intelligent-routing-architecture`、`request-lifecycle-architecture`、`cargo fmt --check`、`cargo check --locked` 和 `git diff --check` 均通过。
- 上述 `verify:full` 发生在 schema `0070` 和最终 `CurrentKey` 失败作用域调整之前；最终调整由后一条聚焦回归覆盖。本轮不重复执行全量门禁，也不执行压测、长时间 soak 或安装包资源画像。
- 第一次不限制 Cargo 并发的全量验证因 Windows `os error 1455`（页面文件不足）中止；这是构建资源限制，不是测试失败。限制 `CARGO_BUILD_JOBS=1` 后从头重跑并通过。

## 已知边界和残余风险

1. generation rebuild、普通 checkpoint/重启、tail replay 和候选上限有自动化行为证据。压测、长时间真实流量 soak 和安装包级资源画像不在本计划范围内；本轮完成后直接进入真人试用和反馈微调。
2. 普通容量 lease 是进程内 RAII 资源，进程崩溃时随 registry 丢失；它不会在 SQLite 中留下悬挂占用。若未来改成持久化容量租约，需要另立 schema/恢复规范，不能把当前内存计数器伪装成 durable lease。
3. `RoutingService`、`MonitoringService`、`RequestFinalizationService`、`RoutingDiagnosticsReader` 及旧 error-rate/health 模块仍承担兼容查询、迁移或测试注入；架构门禁保证它们不再进入 v3 production admission。物理删除应在兼容调用方退役时单独完成，不能仅按名称删除。
4. 全量验证存在既有 warning 和 Vite 大 chunk 提示；本次没有为隐藏这些非阻断项而放宽门禁。

## 结论

核心行为已从“失败但评分不写回、卡在单 Key”收敛为可解释的评分降序、请求预算、Key 熔断、真实样本闭环和可恢复的 generation cutover/rollback。v3 已达到本轮交互发布的实现完成门；后续以真人反馈微调参数和边界，兼容模块的物理删除按实际维护需要另立任务。
