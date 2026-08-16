# Runtime Logging Acceptance Matrix

状态：Complete（核心生产能力已验收）

本矩阵记录运行日志升级的可验证出口。真实 provider/密钥、人工页面/native save dialog 不在本轮范围。packaged marker-I/O fault 子进程 smoke 曾出现 harness 退出挂起，已作为验证工具限制记录；生产 Rust 故障合同通过。

| 领域 | Owner | 验收证据 | 当前状态 |
| --- | --- | --- | --- |
| 类型化事件与边界 | `observability::runtime::{event,error,subject}` | `observability::runtime::contract_tests`（production module `#[cfg(test)]`）、安全扫描、单行和大小限制 | Passed |
| segment 原子发布与非阻塞 writer | `observability::runtime::{sink,lease,recovery,retention,service}` | sink/lease/recovery/retention 专项测试；service focused suite（含 bounded flush、lease retry/recovery、child-process writer lease/restart harness）；recovery 2-test schema/catalog gate；sink I/O fault suite 7/7（含 `StorageFull`）；retention delete fault 4/4；隔离 debug packaged smoke 两次 clean start 发布 JSONL segment | Passed |
| crash marker | `observability::runtime::crash`、应用 shutdown owner | panic/clean shutdown 测试；`runtime_logging_application_lifecycle`（3 passed）；packaged panic smoke；marker-open fault Rust contract | Passed（marker-I/O packaged harness 退出挂起已记录为工具限制，生产降级路径通过） |
| IPC generated contract | `ipc::registry`、`scripts/generate-bindings.mjs`、bridge transport | `generate:bindings --check`、registry hash、181 个 production command boundary scan | Passed |
| interaction/correlation propagation | `observability::correlation`、`background_tasks::{operation,supervisor}` | explicit child capture、operation command-to-operation、scheduler null interaction focused tests；隔离 Tauri State command → JSONL → diagnostics DTO artifact test（5 passed） | Passed |
| developer diagnostics | `application::runtime_diagnostics`、`commands::runtime_diagnostics`、runtime-diagnostics frontend | DTO、segment+line bounded cursor、后端 command、前端页面、developer gate、页面 fail-closed/render 测试；隔离 `tauri-test` State tests；packaged reader/export smoke | Passed |
| support bundle | `services::support_bundle`、`commands::runtime_diagnostics` | allowlist、canary、路径和失败清理、service contract；developer gate 命令测试；前端成功/取消/失败 UI tests（358 tests）；packaged 自动 export | Passed |
| proxy/collector/monitoring | proxy runtime、`services::station_collectors`、`services::monitoring::{runner,maintenance}` | producer cutover、catalog contract、成功/失败/超时/取消专项测试；bootstrap adapter-to-JSONL 注入合同；collector/monitoring/proxy loopback fault artifact | Passed（真实 provider/密钥按范围排除） |
| migration/updater/frontend boundary | `commands::{data_migration,updater,runtime}`、`ShellPageErrorBoundary` | 固定 failure adapter、共享 outbound、迁移/更新 source contract、generated `restart_application` registry contract、ErrorBoundary 与 diagnostics page Vitest；updater loopback malformed manifest 与 peer disconnect 读取最终 JSONL | Passed（真实 provider/密钥按范围排除） |
| catalog manifest | `observability::runtime::catalog`、`scripts/generate-runtime-event-catalog.mjs`、tracked generated artifact | current/previous snapshot、唯一性/替换链 contract、reader compatibility fixture、两次独立生成与 `--check` 漂移门禁；diagnostics messageKey/manifestSource 映射 | Passed |
| 全量质量门禁 | 跨层 runtime logging owners、工程验证脚本 | Rust compile/test、Vitest、build、bindings、verify:fast、runtime architecture/security scans、Windows lease/restart and packaged clean-start smoke | Passed（`verify:full` 仅 advisory `pnpm audit` 因网络 fetch 阻断；不影响代码/架构/安全门禁） |

## 安全出口

- support bundle 只能发布 `manifest.json`、`runtime-summary.json` 和 `runtime-events.jsonl`。
- DTO 不包含日志路径、原始错误、stack、请求 body、凭据或数据库文件。
- canary 命中、目标已存在、路径非法或临时目录发布失败时不得留下看似完整的 bundle。
- 普通模式不得读取 diagnostics 或调用 export command。

## 范围与已知限制

- diagnostics/support-bundle 已有 DTO、service、source-contract、前端测试和 opt-in 真实 Tauri State 命令测试；专项命令使用 `--no-default-features --features tauri-test`，已取得 5/0。人工原生 save dialog 按用户决定不纳入本轮；隔离 debug packaged smoke 已两次自动启动、退出，实际执行 reader/export，生成三文件 bundle，并确认 clean shutdown 后 marker 不残留；packaged panic marker/redaction fault 已通过。
- `scripts/runtime-logging-windows-smoke.ps1` 已提供可重跑的 Windows harness：默认真实 child-process writer/lease/restart 测试通过；`-RunPackaged -RequirePackaged` 自动构建带 debug-only 临时根 seam 的 binary，使用进程内测试密钥、临时 root 和自动退出，连续两次 clean start 均 exit 0，实际执行 reader/export、轮转并发布 194 个 JSONL segment，clean shutdown 后 marker 不残留；`-RunPackagedFaults` 另验证 panic marker/redaction 和后续 clean restart；脚本 finally 清理临时 root/target。旧非隔离 binary 仍 fail-closed，未修改真实用户目录、ACL 或 Credential Manager。
- 最新完整 Rust `cargo test --locked --manifest-path src-tauri/Cargo.toml -- --test-threads=1` 通过：`1091 passed, 0 failed`，全部集成测试和 doc-tests 均 0 failed；串行执行避免 Windows Cargo target 竞争覆盖测试 EXE。
- `pnpm.cmd verify:fast` 首次重跑发现 `restart_application` 漏登 `main-window` ACL，补齐 `src-tauri/permissions/main-window.toml` 并重新生成 bindings 后再次通过；该失败已修复，未放宽门禁。
- packaged reader/export、segment rotation、clean shutdown marker removal 和 panic redaction 已有 Windows smoke 证据；updater/restart 的统一 source/loopback 合同已通过；marker-I/O fault packaged 子进程曾因 harness 退出挂起而停止，生产 Rust 生命周期合同和普通 packaged smoke 已通过；reader 的合法/篡改 previous manifest fixture 已在 Rust 单测覆盖。
- catalog 的 build-time artifact、tracked drift 校验、current/previous manifest 和 diagnostics 展示映射均已通过。
- installation lease producer 已统一接入固定 typed runtime events：`persistence.installation_lease.{acquired,contended,acquire_failed,released,release_failed}`；catalog level gate、JSONL/redaction 断言与 data-store lease lifecycle tests 均通过。首次 lease 后 clock guard 进入 30 秒 monotonic observation window，窗口内暂停 age deletion，byte cap/业务写入仍可用。
- bounded reader cursor 已覆盖大 segment 的 line offset continuation；真实 Windows 多进程和 packaged UI smoke 仍未运行。
- producer adapter-to-JSONL 注入合同已通过：proxy/collector/monitoring/migration/updater descriptor 全集可经同一 service seam 发布并由 reader 读回；collector runner provider-fault → `collector.station.failed` final JSONL 1/1；monitoring timeout/cancel、collector retry→malformed、proxy ingress→upstream disconnect 和 updater peer-close 均由 loopback 实际驱动并读取最终 JSONL。runtime sink I/O（create/write/`StorageFull`/sync/metadata/rename）与 retention delete fault contracts 已通过；真实 provider/密钥明确不属于本计划。
- command-to-artifact interaction 合同已通过：隔离 `tauri-test` 下真实 command 传入 `runtimeContext`，事件写入 JSONL 后再由 diagnostics DTO 读回，`interactionId` 与匿名 `correlationId` 均保持；该测试不替代生产默认 feature 的 Windows 多进程/重启 smoke。
- support-bundle UI 合同已通过：前端测试覆盖成功 toast、save dialog 取消无提示和失败固定提示，并断言敏感 canary 不出现在 UI；人工原生 Windows 对话框不属于本轮范围；packaged smoke 已通过固定临时 destination 的自动 command seam 导出，未触碰原生 save dialog。
- updater/data-recovery 的 source-level restart boundary 已闭合：generated `restart_application` command 复用 `request_application_restart`，记录 `app.restart.requested` 后进入统一 drain；仍缺真实 packaged updater/restart 多进程 smoke 与故障注入证据，不能将该 source contract 视为运行时验收替代。
