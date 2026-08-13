# 上游错误分类与重试：剩余实质工作执行计划

状态：待执行（以当前工作区和自动化契约为准）

日期：2026-08-13

## 目的与边界

本计划只覆盖会话 `019ff0a9-246c-7972-9048-d69d81e998c9` 与
`019ff60e-2d4d-70b0-8b8b-708a17fef8bf` 留下的实质工作。它替代同日多个
“remaining work”文档作为后续执行顺序；实现依据仍是 `AGENTS.md`、当前代码、自动化
契约及 `docs/README.md` 指向的当前规范，而不是本计划本身。

已完成且不应重复实现的部分：canonical error/evidence/retry 链、capacity-domain
选择和同域抑制、durable terminal outcome/outbox、decision trace/IPC/低基数指标、请求
body 复用与 commitment revalidation，以及对应的定向测试。当前已取得以下成功证据：

- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --quiet`：959/959；
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml`；
- `pnpm.cmd verify:fast`、`pnpm.cmd test`、`pnpm.cmd build`；
- `node scripts/upstream-error-contract.test.mjs`、`node scripts/intelligent-routing-architecture.test.mjs`；
- 已分批通过的 routing、monitoring、operational、request-terminal 相关 integration tests。

任何执行批次开始和结束时均记录 `git status --short`。不得 reset、clean、stage、commit、
push 或创建分支；不得以放宽 replay gate、`allow(dead_code)`、手改生成物或保留双 owner
来换取通过。

## 批次 1：闭合持久化架构门禁与本地集成测试

### 1.1 审核并更新 persistence boundary manifest

当前 `persistence_architecture` 报告的未登记依赖边必须逐条审核后登记到
`docs/superpowers/audits/persistence-v2-boundary-manifest.json` 的 `allowed_edges`；仅登记
当前架构明确拥有的生产组合和 test-support 边。重点包括：

- station capacity domain 的 model、store、facade、DTO；
- routing failure-domain 被 planner、admission、proxy error 和 operational snapshot 消费；
- request outcome、terminal outbox 与 decision trace 的持久化/查询链；
- runtime metrics，以及必要的 test-support scenario 依赖。

若任何边代表跨层反向依赖、旧 owner 回流或可消除的重复职责，先重构该依赖，不能把它加入
allowlist。验收：

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_architecture --quiet
node scripts/intelligent-routing-architecture.test.mjs
```

### 1.2 完成按 binary 分批的 integration sweep

本会话单命令的约 124 秒上限会中断全量 `cargo test`，因此按独立 integration binary
分批运行，从下列未确认集合继续，保留每批退出码：

```text
persistence_fault_matrix, persistence_installation_lease,
persistence_pricing_monitoring, persistence_runtime, persistence_sessions,
persistence_startup_cutover, persistence_upgrade, persistence_upgrade_recovery,
portable_migration_e2e, portable_migration_faults, portable_migration_malicious,
pricing_group_monitor_status, provider_conformance, proxy_lifecycle_concurrency,
proxy_lifecycle_domain, proxy_lifecycle_faults, proxy_lifecycle_persistence,
proxy_protocol_contracts, request_log_http_status_migration, request_terminal_outbox,
route_candidate_projection, routing_capacity, routing_capacity_faults,
routing_catalog_loopback, routing_decision_store, routing_dual_terminal_lifecycle,
routing_failure_contract, routing_health_verdict_persistence,
routing_lifecycle_reconciliation, routing_loopback_e2e, routing_outcome_domain,
routing_outcome_persistence, routing_policy_field_e2e,
routing_production_composition, routing_production_startup_shutdown,
routing_runtime_state, routing_security_boundaries,
routing_stream_finalization_faults, routing_url_sanitizer_migration,
schema15_upgrade_fixture, secret_rekey, station_key_health_transitions, task_supervisor
```

每批使用显式 `--test <binary>`，控制在宿主时限内；失败时先修根因、重跑该 binary，再继续。
验收：每一个 listed binary 都有退出码 0，或保留可复现失败和明确归属。不得把宿主强杀的
全量命令解释为测试失败或测试通过。

## 批次 2：补足仍缺的闭环故障与组合 E2E

先盘点现有测试是否已覆盖；只为真实缺口增加测试，优先复用 fake clock、SQLite、loopback
与既有 fault matrix。

### 2.1 Scoped verdict 和恢复正交性

补足从真实 proxy terminal 到下一次 planning snapshot 的生产组合测试：

- group/subscription failure 与 `model_not_found` capability failure；
- credential、account、group、balance、quota、rate-limit、capability 维度并存；
- 无关 subject 不受影响，authority/profile/subject revision 后仅相关 verdict 恢复；
- rebuild 失败回滚、duplicate/late terminal exactly-once、payload collision fail-closed；
- migration `0035`、`0037`、`0039` 与 portable catalog/reimport/restart 一致。

验收：至少两条 production-composition E2E 均满足“写入 -> 下一 snapshot 精确排除 -> 无关对象
不受影响 -> revision 恢复”，并通过对应 persistence、operational fact reader 和 proxy lifecycle tests。

### 2.2 并发、资源和协议故障矩阵

用确定性 harness 补足尚无证据的组合：100 请求 FIFO/queue cap/deadline/cancel/shutdown/
half-open race；gzip/deflate 策略、JSON 限制、SSE EOF/大事件/慢下游/断开；writer unavailable、
crash-restart outbox replay 与 commit 前后失败。

每条路径必须断言：资源计数回到基线、committed 后不 retry、不产生第二个 terminal，且 durable
DTO、日志、fixture 和 metric 无 secret、原始认证信息、完整 URL/query 或高基数标识。

验收：扩展既有 `routing_capacity_faults`、`routing_stream_finalization_faults`、
`proxy_lifecycle_faults/concurrency`、`persistence_fault_matrix` 后，相关 tests 退出码 0。

## 批次 3：旧 owner 删除与门禁收紧

在批次 2 的 E2E 证据齐备后，审计并删除已经被 canonical 链完全取代的旧 owner；删除前用
`rg` 建立生产调用清单，删除后要求零生产引用。当前不得预先删除：

- `routing_failure::classify_route_failure`，仍有 source/test-support 使用；
- `routing_health_snapshot`，仍有生产 store、legacy import/validate 和 portable catalog 消费者。

待迁移证据证明可安全移除后，删除重复 status/message 分类、execution 局部 retry/health 推导、
旧 SSE/error extractor、兼容写回和只服务旧行为的测试 API；同步更新 deletion ledger 与边界
manifest。架构门禁必须继续拒绝 committed retry、无 phase 的非幂等 replay、同域 sibling
fallback、敏感数据进入 durable/IPC/metrics、无界队列和 production/test 核心契约分叉。

验收：删除目标零生产引用，focused behavior tests、architecture/contract/artifact gates 均退出码 0。

## 批次 4：资格验证与文档收口

### 4.1 本地资格验证

在全部本地实质改动后依次运行：

```powershell
git diff --check
node scripts/upstream-error-contract.test.mjs
node scripts/intelligent-routing-architecture.test.mjs
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
pnpm.cmd test
pnpm.cmd build
pnpm.cmd verify:fast
pnpm.cmd verify:full
```

全量 Cargo 仍须以批次 1 的逐 binary 证据闭合，除非执行环境允许获得未被中断的完整退出码。
当前 `verify:full` 的 RustSec advisory database 下载被网络重置；可重试，但网络恢复前标为
“未验证，外部网络阻塞”，不得标记通过。

### 4.2 文档和账本

只按实际证据更新原专项计划、acceptance matrix、qualification、deletion ledger、boundary
manifest 和必要 release note。每项标记为 `done with evidence`、`partial`、`pending` 或
`externally blocked`，并记录命令、退出码、schema/profile 版本和残余风险。

## 外部/架构阻塞项

### Reliable transport send phase

`reqwest` 当前只能可靠提供 `NotConnected`、`ResponseStarted` 和 `Unknown`；不能从 body poll、
HTTP status 或 downstream commit 推断 socket write 的中间 phase。非幂等未知请求已正确
fail-closed，不能伪造 `HeadersSent` 或 body phase。

只有先完成 `docs/superpowers/specs/2026-08-13-reliable-transport-send-phase-spike.md` 中的可行性
证明，选定同时拥有连接、TLS 与序列化 write future 的 transport owner，并验证 Windows
direct/system/HTTP/SOCKS proxy、Rustls、HTTP/2、pooling、timeout 和 streaming compatibility，
才能启动生产替换。若无法保持这些契约，正式维持三态信号并记录为架构阻塞。

### 真实 provider/Codex smoke

需要用户明确授权、隔离测试账号、最小权限凭据和假业务输入后才可执行。届时验证
OpenAI-compatible HTTP/SSE、Sub2API success/401/429/5xx、Codex 对最终 `server_error` 的行为，
以及 capacity 不错误切 key 或写 credential failure。未获授权时保留 `pending external evidence`；
fixture 不能替代真实 smoke。

## 完成定义

本计划完成的最低条件是：批次 1 的 manifest 与所有可运行 integration binary 已闭合；批次 2
真实缺口已有确定性 E2E 证据；批次 3 的旧 owner 仅在零引用条件下移除；批次 4 的本地门禁有
真实退出码和文档账本一致。transport phase 与真实 smoke 仅在各自外部前置满足后才能由
`externally blocked/pending` 转为完成。
