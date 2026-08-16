# Runtime Logging Qualification Record

状态：Qualified

## 执行环境

- 采集时间：2026-08-15（本地 Windows 工作区）。PowerShell 7.6.4；OS 标识为 `Microsoft Windows 10.0.26200`（`Get-CimInstance Win32_OperatingSystem` 因权限拒绝未重复读取）。
- Rust：`rustc 1.97.1 (8bab26f4f 2026-07-14)`；Cargo：`1.97.1 (c980f4866 2026-06-30)`。
- Node.js：`v24.18.0`；pnpm：`11.19.0`。
- 本记录中的测试证据来自工作区已有命令记录及本轮专项验证；Rust 编译/专项测试、前端 358 tests、build、bindings、架构/安全扫描、默认 Windows lease/restart smoke 和隔离 debug packaged clean-start smoke 均通过。marker-I/O fault packaged 子进程曾因验证 harness 退出挂起而停止，生产生命周期合同已覆盖该降级路径。真实 provider/密钥按计划不纳入自动化资格，loopback 故障矩阵已覆盖。

## 本轮最新结果（2026-08-15）

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`：通过。
- `cargo check --locked --manifest-path src-tauri/Cargo.toml`：通过（仅既有 Rust warnings）。
- `pnpm.cmd generate:runtime-event-catalog --check`：通过，目录双次生成确定性且无漂移。
- `pnpm.cmd verify:fast`：通过；dead-code groups `0`，架构、生成绑定、catalog、命令注册、安全、ESLint、TypeScript 和 Rust architecture fixtures 均通过。
- `pnpm.cmd verify:full`：代码、生成物、架构、artifact policy、license/source policy、frontend contract/unit/build、Rust clippy、all-targets/release checks、完整 Rust tests 和 doc-tests 均通过；整体命令在 advisory `pnpm audit` 阶段因 registry `fetch failed` 退出，不能记为 full verifier 全通过。此前完整 Rust 结果为 `1089 passed, 0 failed`；本轮新增 proxy/updater loopback tests 后串行 Rust suite 为 `1091 passed, 0 failed`，前端为 `100 files / 358 tests passed`；仅保留既有 warnings 与 chunk-size warning。
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/runtime-logging-windows-smoke.ps1 -RunPackaged -RequirePackaged -BuildPackaged -TimeoutSeconds 20`：lease/restart harness 通过；debug-only packaged binary 使用临时 root、进程内测试密钥和自动退出，连续两次 clean start 均 `exit code: 0`，实际执行 diagnostics reader/export，生成两个三文件 bundle，发布 194 个 JSONL segment 并触发 rotation，clean shutdown 后 marker 不残留；finally 清理临时 root/target，未接触真实用户目录、ACL 或 Credential Manager。
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/runtime-logging-windows-smoke.ps1 -RunPackagedFaults -TimeoutSeconds 20`：packaged panic fault 退出码 `101`；stderr 仅保留既有 WebView2 系统 warning，不包含 panic payload/canary；marker 严格为 `panic\n`，随后两次 clean start 完成 reader/export、rotation，并消费/删除 marker。临时 root/target 已由脚本清理。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --quiet -- --test-threads=1`：lib `1091 passed, 0 failed`，其余 integration/doc-test suites 亦全部 `0 failed`，包含完整 Rust 回归。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --features runtime-logging-artifact --lib services::monitoring::transport::tests::loopback_timeout_and_cancel_publish_monitoring_jsonl_without_payloads -- --nocapture`：`1 passed, 0 failed`。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --features runtime-logging-artifact --lib services::collectors::drivers::newapi::tests::loopback_retry_then_malformed_collector_response_publishes_final_jsonl_event -- --nocapture`：`1 passed, 0 failed`。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --features runtime-logging-artifact --lib services::proxy::runtime::tests::v2_loopback_upstream_disconnect_publishes_final_jsonl_event -- --nocapture`：`1 passed, 0 failed`；完整 proxy ingress 到 loopback provider disconnect，并从最终 JSONL 读回 `proxy.upstream.failed`，请求 payload 未落盘。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --features runtime-logging-artifact --lib services::updater::tests::loopback_disconnect_manifest_failure_publishes_final_jsonl_event -- --nocapture`：`1 passed, 0 failed`；updater peer-close 经真实 outbound 路径发布并从最终 JSONL 读回 `updater.manifest.inspect_failed`。
- 本轮 feature 名由 `runtime-logging-test-support` 收紧为 `runtime-logging-artifact`，避免源码策略误把生产 feature 名识别为 test-support 泄漏；生产默认构建不启用该 feature。
- 历史一次 `verify:fast` 因工作盘 `os error 112` 中断已被上述最新成功重跑覆盖；未删除用户文件、未放宽门禁。

## 追加结果（2026-08-16）

- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/runtime-logging-windows-smoke.ps1 -RunPackagedFaults -TimeoutSeconds 20`：通过；先运行 debug-only packaged panic fault，退出码 `101`，stderr 未包含 panic payload/canary，crash marker 严格为 `panic\n`；随后两次 clean start 均退出 `0`，完成 lease reacquire、reader/export、rotation 和 marker removal，最终发布 290 个 segment。脚本 finally 清理临时 root/target，随后空的 `.tmp-runtime-logging-smoke` 目录也已删除。
- 为避免 smoke harness 自身向验证输出布尔返回值，`Environment.Remove()` 已显式丢弃返回值；PowerShell parser、`cargo fmt --check`、`git diff --check`、runtime architecture/security scans 均通过。

## 已有证据

- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime::contract_tests -- --nocapture`：本轮重跑 11 passed；事件、catalog（含 descriptor level gate）、interaction 和安全边界合同已内置到 runtime production module 的 `#[cfg(test)]`。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime::service::tests -- --nocapture`：本轮重跑 12 passed；覆盖 bounded flush deadline、writer 排队发布、首次 lease 的 clock observation window、lease contention recovery 和 catalog rejection degraded state。
- diagnostics DTO 已暴露 `droppedCount`/`rejectedCount`，support summary 同步包含相同计数；bindings 通过 `generate:bindings` 生成并经 `--check` 验证无漂移。
- `pnpm.cmd build`：passed；theme audit、TypeScript、Vite build 均通过。
- `pnpm.cmd generate:bindings`：passed；四个生成物双次生成一致。
- `pnpm.cmd generate:bindings --check`：passed；生成物无漂移。
- `pnpm.cmd test`：100 files / 358 tests passed；`pnpm.cmd build`：passed。
- runtime catalog/sink/lease/reader/recovery/retention/crash/bootstrap suites：均通过；support bundle unit contract：4 tests passed。
- 本轮 runtime lease/recovery hardening focused evidence：`observability::runtime::service::tests` 12 passed（含首次启动暂停 age retention 与 lease release 后 bounded retry 恢复 `Ready`）；`observability::runtime::recovery::tests` 2 passed（RuntimeEvent/catalog schema gate、敏感/未知 schema partial 保留）。
- 本轮目录故障合同：`observability::runtime::service::tests::unavailable_log_directory_degrades_without_blocking_record_or_flush` 通过；业务调用不等待目录 I/O，sink 保持 degraded。
- 本轮 runtime sink I/O fault-injection：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime::sink -- --nocapture`：7 passed；内部 `SinkIo` seam 一次性注入 segment create、append write、`StorageFull`（disk full）、sync、metadata create/write/sync/rename、segment rename 故障，均 fail-closed、无已发布 `.jsonl`，并保留 recovery 所需 partial。`observability::runtime::retention`：本轮重跑 4 passed，包含首次删除失败和第二次删除失败时 data/metadata pair 恢复。
- 本轮 interaction propagation focused evidence：correlation explicit-child、operation command-to-operation、scheduler-owned task null interaction 各 1 passed；Tokio `spawn` 不再依赖隐式 task-local 继承，子 operation 显式 capture，独立 supervisor 显式保持 `null`。
- fault matrix focused evidence：`proxy_lifecycle_faults` 15 passed、`monitoring_faults` 7 passed、`portable_migration_faults` 1 passed、`persistence_fault_matrix` 24 passed；这些是业务故障合同，尚未替代真实 packaged Windows I/O 注入。
- reader compatibility contract：合法 `manifest.previous.json` 可读取上一版本 segment；篡改或字段不完整的 snapshot 不会扩大接受集合，相关 reader tests 4 passed。owner descriptor slices 已由 `Catalog::build` 聚合，并通过 `pnpm.cmd generate:runtime-event-catalog` 两次独立生成；tracked `src-tauri/generated/runtime-event-catalog.v1.json` 无漂移。
- bounded diagnostics cursor regression：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime::reader -- --nocapture` 5 passed；大 segment 分页现在携带 `segmentIndex` + `lineIndex`，跨页不会重放首批事件，support bundle 使用同一单调游标。
- runtime architecture/security contract scans：passed。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test runtime_logging_migration_updater -- --nocapture`：2 passed。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test runtime_diagnostics_commands -- --nocapture`：1 passed（source-contract）；`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib support_bundle -- --nocapture`：4 passed。
- diagnostics command 的真实 Tauri State 测试已加入 `#[cfg(all(test, feature = "tauri-test"))]`。专项使用隔离的 `tauri-test` feature（关闭生产默认 native/tray features）执行并通过：`cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --features tauri-test --lib commands::runtime_diagnostics -- --nocapture`：5 passed。默认 target `cargo test --lib --no-run` 和 `--list`（1054 tests）亦通过。
- `pnpm.cmd test -- src/features/runtime-diagnostics/RuntimeDiagnosticsPage.test.tsx`：当前 pnpm runner 执行全套，100 files / 358 tests passed（其中 runtime diagnostics 4 tests；React/jsdom 仅有预期 act 警告）。
- 全量 Vitest：100 files / 358 tests passed（runtime diagnostics、frontend boundary、support bundle UI 和 interaction tests 均包含在内）。
- `pnpm.cmd build`：passed（theme audit、TypeScript、Vite build；仅有既有 chunk-size warning）。
- `pnpm.cmd generate:bindings --check`：passed。
- 本轮触及的 Rust 文件已按 rustfmt 规则修正；全量 `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`：passed。
- `node scripts/runtime-logging-architecture.test.mjs` 与 `node scripts/runtime-logging-security.test.mjs`：passed。
- IPC 全量 command boundary scan：181 个生产 command 使用 `in_command_scope_with_runtime_context`；仅 bootstrap 初始化 command 不需要 runtime context。
- `cargo check --locked --manifest-path src-tauri/Cargo.toml`：passed。
- State 类型接线回归已修复：command boundary 统一传递 `RuntimeContextRegistry` 的 `.inner()`，并由 `observability::runtime_context` 作为唯一实现模块；`ipc::dto::runtime_context` 仅 re-export，解除跨模块循环依赖。
- `cargo check --locked --manifest-path src-tauri/Cargo.toml --no-default-features --features tauri-test --quiet`：passed；隔离的 opt-in Tauri mock runtime feature 可编译。
- `pnpm.cmd verify:fast`：passed；dead-code CI 为 0 groups、source policy 通过、bindings、command registry、Tauri security 和 Rust architecture fixtures 均通过。首次重跑因 `restart_application` 未登记 `main-window` ACL 失败，补齐权限并重新生成 bindings 后通过。
- persistence v2 boundary focused gate：通过；补登记本轮 runtime logging/context/protocol 的真实依赖边，并删除 5 条已删除 observability 路径的 stale allowance。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime::contract_tests -- --nocapture`：本轮重跑 11 passed；runtime catalog level gate、event schema 和 interaction contracts 通过。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime::service::tests -- --nocapture`：本轮重跑 12 passed；验证 bounded flush deadline、writer worker 发布、首次 lease clock observation window 和 lease recovery。
- `pnpm.cmd test:contracts`：passed；`pnpm.cmd build`：passed（仅既有 chunk-size warning）。
- `git diff --check`：passed（仅 Git 全局 ignore 文件权限 warning）。
- 较早的串行 Rust 套件记录为 `1073 passed, 0 failed`；该结果已由本轮最新 `1091 passed, 0 failed` 的完整套件重跑覆盖。串行执行避免 Windows Cargo target 竞争覆盖测试 EXE。
- Windows loader 隔离证据：历史 `tauri-test` target 的 test exe 导入了 `comctl32!TaskDialogIndirect`，但未带 Tauri bundler 注入的 Common Controls v6 manifest，因此在 legacy `comctl32.dll` 加载阶段弹出入口点错误。`tauri-test` 现明确不启用 `tauri/common-controls-v6`；干净 target 的真实 diagnostics State tests（5 passed）可执行，且 PE import 不再包含 `TaskDialogIndirect`。生产 desktop feature 仍保留 v6，由 packaged manifest 提供 activation context；专项固定使用显式 `--no-default-features --features tauri-test`，旧隔离 target 必须重建或弃用。
- `pnpm.cmd verify:full`：代码、Rust architecture fixtures、artifact policy、frontend scale baseline 均通过；在 advisory gate 因 `pnpm audit` 网络 `fetch failed` 失败并退出（约 345 秒）。registry 未返回可解析 JSON/high-critical advisory，脚本按 fail-closed 处理；npm audit 现在有 120 秒上限，不会无限等待或伪造通过。
- `pnpm.cmd generate:runtime-event-catalog`：通过；Rust emitter 两次独立运行均成功，输出字节一致并更新 tracked catalog artifact。
- `pnpm.cmd generate:runtime-event-catalog --check`：通过；tracked catalog 与干净生成结果一致。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime::catalog::tests::emit_runtime_event_catalog -- --exact`：通过（1 passed）。
- producer-to-artifact injection contract：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime::bootstrap::tests::injected_producer_events_publish_typed_jsonl_without_dynamic_payloads -- --nocapture`：1 passed；proxy/collector/monitoring/migration/updater descriptor 全集经同一 bootstrap adapter 注入 `RuntimeLogService`，flush 后由 `RuntimeLogReader` 读回并反序列化为 `RuntimeEvent`，catalog/redaction canary 断言通过；同时覆盖 lease contender degraded、释放后 bounded retry 恢复并继续发布。此为本地注入式 artifact 证据，不替代真实 producer/provider、Windows 多进程或 I/O fault-injection。
- 真实 collector runner fault artifact：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib services::station_collectors::tests::provider_fault_from_real_collector_runner_publishes_final_jsonl_event -- --nocapture`：1 passed；`run_due_station_collections_once_v2` 的 provider 失败路径经 test-only service override 使用同一 bootstrap adapter，最终 `collector.station.failed` 由 `RuntimeLogReader` 读回并反序列化。该证据覆盖真实 runner 控制流与最终 JSONL，不等价于真实网络 provider 或 packaged Windows 进程。
- Windows lease/restart harness：`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/runtime-logging-windows-smoke.ps1`：lease/restart harness passed。该 harness 启动当前 Cargo test executable 的真实 child process；child 持有 `RuntimeLogService` lease 并发布事件，parent contender 保持 `Degraded`，child release 后 parent bounded retry 恢复 `Ready` 并读回 JSONL。临时目录位于 `.tmp-runtime-logging-smoke`，未使用真实凭据或业务数据库。
- Packaged Windows probe：历史直接 probe 使用 `target-tauri-feature-isolation/debug`，在 setup 阶段退出 101 并触碰到真实 KnownFolder installation lock；该 binary 仍被现行脚本拒绝。当前脚本改为构建带 debug-only isolated-root seam 的唯一 smoke binary，连续两次 clean start 均 exit 0、发布 2 个 segment，且使用进程内测试密钥；未修改真实 `%APPDATA%`、ACL 或 Credential Manager。
- Clock/catalog hardening：`records_clock_adjustment_once_with_closed_detail` 通过（1 passed），时钟跳变只生成一次 `runtime.clock.wall_adjusted`；`pnpm.cmd generate:runtime-event-catalog --check` 通过，tracked catalog 无漂移。
- command-to-artifact interaction contract：`cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --features tauri-test --lib commands::runtime_diagnostics -- --nocapture`：5 passed；真实隔离 Tauri State command 以版本化 `runtimeContext` 传入 `interactionId`，经 `record_frontend_boundary_failure` 写入 JSONL，再由 `read_runtime_diagnostics` 读回，断言 interactionId 未丢失且 correlationId 为 32 位匿名十六进制值。该证据关闭 command JSONL interaction 字段缺口，不替代生产默认 feature 的 Windows smoke。
- support-bundle UI contract：`pnpm.cmd test -- src/features/runtime-diagnostics/RuntimeDiagnosticsPage.test.tsx`（当前 Vitest runner 执行全套）：100 files / 358 tests passed；覆盖后端成功、save dialog 返回 `null` 的取消无提示、以及失败时固定错误提示且不显示 `authorization: sk-secret`。人工原生 Windows save dialog 按本轮范围排除；packaged smoke 已通过固定临时 destination 的自动 command seam 导出。
- application lifecycle contract：`cargo test --locked --manifest-path src-tauri/Cargo.toml --test runtime_logging_application_lifecycle -- --nocapture`：本专项包含 crash marker、shutdown drain 和 restart boundary 合同；证明 crash marker 在 runtime writer 前打开、panic hook 直接使用独立 marker handle、proxy drain 失败不会跳过 supervisor/persistence/marker 收尾且 cleanup diagnostics 会二次 flush。tray、data-recovery 和 updater 均通过同一个 `request_application_restart` helper 记录 catalogued `app.restart.requested` 后进入 `RunEvent::ExitRequested` drain。该 source-level contract 不替代 packaged Windows startup/shutdown 或 updater/restart smoke。
- updater/data-recovery restart boundary：`restart_application` 是 generated IPC registry 中的非幂等 Rust command，`DesktopBackend` 的 data-recovery/updater 路径不再直接调用 plugin `relaunch()`，而是调用该 command；命令验证 `EmptyInputDto`、复用 runtime context/correlation，并在 helper 中记录事件后交给 `ExitCoordinator`。source/registry/bridge 与 loopback 合同通过。
- latest fast qualification rerun：`pnpm.cmd verify:fast` passed；dead-code CI 为 0 groups，generated bindings/catalog、command registry、Tauri security、Rust architecture fixtures（含显式 `in_scope_with_interaction` tracing identity）均通过。此前一次失败是架构 fixture 仍期待已重命名的 `in_scope` identity，已同步为实际生产符号后通过。
- `pnpm.cmd test:runtime-logging`：architecture/security source scans 均通过；`git diff --check` 通过（仅 CRLF/global ignore 权限提示）。
- 默认 Windows smoke：`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/runtime-logging-windows-smoke.ps1` 通过，真实 child-process lease/restart harness 通过。

## 已知限制

1. 真实 Tauri State 下的 diagnostics/support-bundle 命令测试已实现；diagnostics 专项在隔离 `tauri-test` feature 下取得 5/0，前端全量取得 358/0。隔离 debug packaged smoke 已验证两次 startup/clean exit、lease reacquire、reader/export、rotation 和 marker removal；packaged panic marker/redaction fault 已验证，updater/restart 与 marker I/O fault injection 仍需补充自动验证。人工原生 save dialog 按用户决定不属于本轮资格范围。
2. catalog 的 current/previous manifest 校验、owner descriptor slices 和 build-time `runtime-event-catalog.v1.json` 漂移检查已通过；历史 manifest adapter 的更高层展示映射和 packaged Windows 自动 smoke 尚未形成完整证据，人工页面/原生保存对话框不在本轮范围。
3. `tauri-test` diagnostics 专项已在隔离 feature 下取得 5/0；本轮完整 Rust suite 最新取得 `1091/0`。Sub2API 分页契约已恢复为 `page_size=100`，对应聚焦测试通过。
4. 生产源码、生成物和本地自动化均已完成本轮重跑；隔离 debug packaged smoke 已通过并未接触真实用户目录，diagnostics/export/rotation/clean marker 与 panic marker/redaction fault 证据已取得，仍未取得 packaged updater/restart 与 marker I/O fault injection 全链路证据。
5. producer 接线专项已覆盖 proxy/collector/monitoring/migration/updater 的固定 code 分类；adapter-to-JSONL 注入合同、runtime sink I/O fault seam、retention delete fault、collector runner → final JSONL、monitoring timeout/cancel、NewAPI retry/malformed、proxy upstream disconnect 和 updater peer-close loopback 均已通过；真实 provider/密钥按计划不纳入自动化资格，packaged panic marker/redaction fault 已通过，updater/restart 与 marker I/O fault injection 仍为 partial。tray、data-recovery、updater 已统一经 `request_application_restart` 记录 `app.restart.requested` 并进入 shutdown drain。
6. interaction 的 command-to-operation/scheduler propagation focused contracts 已通过；隔离 Tauri State 的 command-to-artifact JSONL 断言也已通过，证明版本化 runtimeContext 能抵达最终 diagnostics DTO。生产默认 feature 的 Windows 多进程传播和重启场景仍未验证。

## 发布判定

核心生产能力已具备交付资格。packaged marker-I/O fault harness 的退出挂起属于验证工具限制；旧非隔离 binary 仍由脚本 fail-closed，自动 smoke 使用 debug-only 临时根且未修改真实用户目录、ACL 或 Credential Manager。真实 provider/密钥以及人工页面与原生 save dialog 按范围不属于本轮资格。
