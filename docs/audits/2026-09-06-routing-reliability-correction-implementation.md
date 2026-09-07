# 路由可靠性修正实施证据

状态：Implemented（Phase 1/2 endpoint 范围、Phase 3 query caller 首批窄 port、Phase 4 最小生产调用收口、Phase 5 空壳与 routing test-only health 清理已完成；Phase 3 宽 adapter 迁移、剩余 test-only 迁移和 Phase 6 仍未完成）

日期：2026-09-06

关联计划：[`../plans/2026-09-06-routing-reliability-correction.md`](../plans/2026-09-06-routing-reliability-correction.md)

## 已实施范围

- 新增 `application/routing_endpoint_ports.rs`，建立 endpoint target read、monitoring target read、endpoint health revision-fenced write 三个窄 port；station-key diagnostic port 仅保留为带删除条件的契约，当前 facade 仍使用 bounded in-memory diagnostic store。
- `RoutingCommandFacade::ping_station_endpoint` 改为“读取目标并捕获 revision → 无状态 probe → typed status + expected revision 写回”，仅 `CommitOutcomeUnknown` 映射为 `ResultUnknown`。
- `RoutingCommandFacade` 的 workspace、runtime overlay、protection、circuit status、simulation query 改为注入窄 read ports；该 facade 已不再持有完整 `RoutingService`。
- `services/endpoint_ping.rs` 的 HEAD→GET fallback 改为共享绝对调用级 deadline；预算耗尽时不再启动第二次请求，避免单次 probe 超过 caller timeout。
- `StationKeyConnectivityCommandFacade` 和 `MonitoringRunner` 改用 endpoint health write port；`MonitoringRunner` 不再持有或导入完整 `RoutingService`。
- `RoutingService` 暂作为 adapter 实现窄 port，并在 target read 边界重新校验 API base URL 和 endpoint revision；底层原子 CAS/revision fence 保持不变。
- protection command 的生产调用改为显式 V3 circuit-only 参数；保留兼容 DTO/projection 分支，未提前删除 capacity schema 或旧表。
- protection capacity 分支证据结论：全仓生产源码未发现 `CapacityProtectionFact` 构造点；当前唯一生产调用位于 `RoutingService::get_routing_protection_status`，显式传入空 capacity 列表并使用 V3 circuit read model。该分支因此不是当前 runtime authority，但暂保留 `CapacityProtectionFact`/`capacity_entry` 作为兼容投影，待前端消费、历史 decoder 与 portable 契约完成独立审计后再删除。
- 删除四个仅含 placeholder 的 service 空壳模块，以及 `routing_protection.rs` 中永不编译的 `#[cfg(all(test, any()))]` 旧测试块。
- 删除仅由旧 test-only health projection 使用的 `routing_engine/routing_health.rs`、`StationKeyHealth` 和 candidate health 字段；保留仍被集成测试直接引用的 `runtime_health_port.rs`、`health_projector.rs` 与 operational health models。
- 架构脚本新增 endpoint ownership boundary，防止 monitoring/facade 重新依赖宽 service 或绕过 revision-fenced port。
- 同一架构门禁固定检查 endpoint probe 使用绝对 deadline，避免 HEAD→GET fallback 回归为双重计时。

## 证据与验证

通过：

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- `cargo check --locked --manifest-path src-tauri/Cargo.toml --target-dir $env:TEMP/relay-pool-routing-reliability-target`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --target-dir $env:TEMP/relay-pool-routing-reliability-target --lib routing -- --nocapture`（213 passed）
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --target-dir $env:TEMP/relay-pool-routing-reliability-target --lib routing_endpoint_ports -- --nocapture`（4 passed）
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --target-dir $env:TEMP/relay-pool-routing-reliability-target --lib endpoint_ping -- --nocapture`（7 passed）
- `node scripts/routing-single-owner.test.mjs`
- `node scripts/routing-v3-legacy-retirement.test.mjs`
- `node scripts/routing-projection-runner.test.mjs`
- `node scripts/request-lifecycle-architecture.test.mjs`
- `pnpm.cmd exec vitest run src/features/routing/LocalRoutingStatusCandidateRow.test.tsx src/features/routing/RoutingCandidateOrderPanel.test.tsx`（23 passed）
- `pnpm.cmd build`
- `git diff --check`（仅 CRLF 转换提示，无 whitespace error）
- `pnpm.cmd verify:full`（使用 `CARGO_TARGET_DIR=D:\Temp\relay-pool-verify-full-target`、`CARGO_BUILD_JOBS=1`；全部 profile 门禁通过，Rust 单元/集成/文档测试通过，Rust tests 阶段耗时约 1100 秒）
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture`（42 passed；新 port 依赖边界已登记到 `persistence-v2-boundary-manifest.json`）

未通过/未完成：

- `pnpm.cmd verify:fast` 使用隔离 `CARGO_TARGET_DIR=$env:TEMP/relay-pool-verify-fast-target` 通过。默认 target 曾因并行桌面进程占用 linker 文件而失败；隔离 target 后所有 fast profile 门禁通过。
- 首次 `pnpm.cmd verify:full` 在临时 C 盘 target 因 `no space on device` 失败；迁移到 D 盘后，默认并行 Rust tests 曾触发 Windows `STATUS_STACK_BUFFER_OVERRUN`，单独复核 `operation_registry`（9 passed）后，以 `CARGO_BUILD_JOBS=1` 完成全量验证并通过。该失败属于构建环境/并发参数，不是测试断言失败。
- 未运行发布级 `verify:release`、真实桌面 WebView/Provider 验收、P6 schema DROP 资格流程。

## 保留的边界与后续条件

- `RoutingExecutionReader`、policy coordinator、reaper 仍通过 `RoutingService` 过渡 adapter；这些宽职责已在 caller inventory 中登记，必须等待对应删除条件满足后再迁移。
- `health_projector.rs`、`runtime_health_port.rs` 和 operational health models 保留为 `operational_health_projection.rs` 的直接测试契约；删除前需先完成等价 V3 集成测试迁移。
- protection projection 的 `CapacityProtectionFact`、`capacity_entry` 和兼容 DTO 仍保留，直到生产 producer、前端消费和历史 decoder 完成独立证据审计。
- 旧 health/error-rate/capacity 表继续遵守 P6 no-go；本次没有新增 writer、migration DROP 或清理本地数据库。
- 工作区中原有的 dashboard/routing 前端修改与 `src-tauri/target-login-fix/` 未跟踪目录保持不变；本次未执行 stage、commit、push、建分支或破坏性回退。
