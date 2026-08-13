# 上游错误分类与重试：当前实质收口计划

状态：Ready for implementation

日期：2026-08-13

执行更新（2026-08-13）：批次 A 已完成。完整
`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --quiet` 已以退出码 0
通过（959/959）。本轮修复了 terminal outbox 的重复终态语义比较、runtime candidate
列映射、portable schema fingerprint、超限 error envelope 断言、attempt terminal 写入失败
时的 fail-closed request interruption，以及 schema 39 对既有 migration test 的版本断言。
`cargo fmt --check`、`cargo check --locked`、`pnpm.cmd verify:fast`、upstream error contract
和 intelligent-routing architecture gate 均退出码 0。

全量资格验证仍未关闭：all-target `cargo test` 与 `cargo test --tests` 都受本会话约 124 秒
硬超时限制而未取得完整退出码；`pnpm.cmd verify:full` 在 advisory database 从 GitHub 拉取时
连接被重置而退出 1，前置 dead-code、persistence artifact 与 architecture baseline 步骤已通过。
这些是未验证/外部环境限制，不得标记为通过。

适用范围：本计划汇总会话 `019ff0a9-246c-7972-9048-d69d81e998c9` 与
`019ff60e-2d4d-70b0-8b8b-708a17fef8bf` 的最终代码、定向验证和最近一次
完整 library suite 结果。它只列仍未关闭的实质工作；实现判断以根
`AGENTS.md`、当前代码、自动化契约和 `docs/README.md` 指向的当前规范为准。

## 已完成基线

以下内容已有实现及定向测试证据，不应重新实现：canonical error/evidence/retry
链、可信 capacity domain 与同域抑制/单次跨域回退、durable terminal outcome 与
outbox/replay、Decision Trace/IPC/低基数 metrics、请求 body 复用和 target
commitment revalidation、HTTP/SSE 错误边界、model/group scoped verdict 的真实
proxy E2E、wire-byte diagnostic budget、shadow rebuild，以及并发内存/permit
回收测试。

最近已通过的局部门禁包括 `pnpm.cmd verify:fast`、`pnpm.cmd test`、
`pnpm.cmd build`、`cargo fmt --check`、`cargo check --locked`、专项 architecture
和 contract checks、`routing_loopback_e2e` 与 `routing_health_verdict_persistence`。
这些证据不替代下列收口工作。

## 不变约束

- 保留现有脏工作区和无关并行修改；不 reset、clean、stage、commit、push 或建分支。
- 不以 `allow(dead_code)`、手改生成物、双 owner 或放宽 replay gate 通过检查。
- 非幂等请求在发送状态不确定或可能已被接收时保持 fail-closed；committed 后不 retry。
- 不在 durable DTO、fixture、日志或 metric label 中写入 secret、认证信息、完整 URL/query、原始动态 message 或真实请求标识。
- 每个批次前后记录 `git status --short`，只编辑该批所需文件。

## 批次 A：修复已复现的 library 回归

目标：先使当前代码和既有数据库/生命周期契约一致。最近一次
`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --quiet` 运行 959 项，
954 通过、5 项失败；这五项是下一步最高优先级，不能以修改测试期望掩盖行为回归。

1. 修复 request finalization 的重复终态幂等性。
   - 失败：`persistence::differential_tests::request_finalization_is_idempotent_in_v2`。
   - 检查 `application/request_finalization/mod.rs`、`request_log_store.rs` 和 terminal outbox/equality 路径。
   - 重复且完全相同的 `finish_request` 必须返回既有 terminal，而 payload collision 仍必须 fail closed。
   - 验收：该 differential test、terminal outbox/replay 测试及 request lifecycle 相关测试通过。

2. 修复 runtime candidate SQL 映射的列类型/顺序。
   - 失败：`persistence::differential_tests::routing_service_loads_v2_runtime_candidates_and_workflow_queries`，
     `routing_store.rs:row_to_runtime_candidate` 将第 7 列按 `Option<i64>` 读取但查询返回 `REAL`。
   - 对齐 runtime candidate SELECT、列常量和 mapper；重点审计新增
     `station_account_concurrency_limit` 或相邻字段是否漏入/错位。
   - 验收：失败的 differential test 和相关 routing store 集成测试通过，且不改变已发布字段语义。

3. 用官方脚本重建 portable schema fixture。
   - 失败：`services::portable_migration::schema_reader::tests::trusted_schema_fingerprint_matches_fixture`。
   - 先确认 migration/catalog 的预期 schema 已正确，再运行
     `scripts/build-persistence-v2-fixtures.ps1` 更新 fingerprint；不得手改
     `src-tauri/tests/fixtures/portable-migration/v1/schema-fingerprint.txt`。
   - 验收：schema reader、portable migration catalog 和 fixture 相关测试通过，脚本二次执行无额外 diff。

4. 明确 oversized error envelope 的置信度契约并修复实现或测试。
   - 失败：`services::proxy::adapters::error_envelope::tests::depth_and_size_limits_cannot_create_durable_semantics`。
   - 审计 `error_envelope.rs` 的超限、无效 UTF-8 和 malformed 输入分支；它们可生成保守诊断证据，
     但不得产生 durable health/capability semantics。
   - 只有在当前 canonical contract 明确要求 `Unknown` 时才更新断言；否则恢复预期的
     `Probable` 置信度，并补充同一语义边界的回归。
   - 验收：本模块全部测试及 upstream error contract 通过。

5. 核实 attempt persistence failure 的终态写入契约。
   - 失败：`services::proxy::response_body::tests::request_terminal_is_committed_when_attempt_persistence_fails`。
   - 区分“调用 attempt writer 一次且 writer 返回失败”与“未调用 writer”；检查
     `RecordingStore::finish_attempt` 的记录时机和 production 行为。
   - 若 production 已正确尝试写入 attempt 并仍提交 request terminal，修正过时断言并保留失败注入；
     若真实路径错误，修复 writer 排序而非仅改测试。
   - 验收：response body/lifecycle fault 测试证明 attempt 失败不会阻止唯一 request terminal。

完成条件：五个失败的 focused tests 均退出码 0，再运行完整
`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --quiet` 至退出码 0。

## 批次 B：补齐尚未闭合的本地工程证据

目标：在不改变已完成行为的前提下，补齐能由本地 deterministic test 证明的剩余矩阵。

1. scoped verdict 正交性与崩溃恢复：补 credential/group/quota/rate-limit/capability
   与既有 account/balance 的独立恢复组合；覆盖 rebuild 失败回滚、duplicate/late terminal
   与 payload collision 的端到端 exactly-once 边界，并复核 0035/0037/0039 和 portable catalog。

2. 真实 HTTP/SSE execution 的并发与协议矩阵：在 loopback/fake clock 下补齐 100 请求的
   FIFO、队列上限、deadline、cancel、shutdown、half-open race；补压缩解码错误/膨胀、
   JSON 限制、256 KiB SSE event、EOF 终态、慢客户端/下游断开和 writer unavailable。
   每条路径都断言资源计数回到基线、committed 不 retry、不会写入第二终态。

3. durable outcome/trace 跨层验收：覆盖真实 proxy terminal、进程重启后 IPC 查询、
   ring 缺失时 durable summary 优先、重复终态 equality、payload collision、writer failure
   和 redaction。确认 bindings/ACL/schema 的生成结果来自官方命令。

完成条件：相关 Cargo integration tests、contract/architecture checks 均退出码 0；新增测试
复用现有 loopback、fault matrix 和 fixture，而非引入与生产不同的第二套契约。

## 批次 C：可靠 transport send phase 的架构阻塞

状态：外部/架构阻塞，不是单纯缺测试。

当前 `reqwest` transport 只能可靠报告 `NotConnected`、`ResponseStarted`、`Unknown`。
中间 socket write phase 不能由 body poll、HTTP status 或 downstream commit 推断；未知的
非幂等发送当前已正确 fail closed。详见
[`2026-08-13-reliable-transport-send-phase-spike.md`](../specs/2026-08-13-reliable-transport-send-phase-spike.md)。

只有在先完成以下设计与可行性证明后，才能实现中间 phase：

1. 选定真正拥有连接、TLS 与请求序列化 write future 的 transport adapter，并记录许可证、
   `Cargo.lock` 影响及 Windows 兼容性。
2. 保留 direct/system/HTTP/SOCKS proxy、Rustls、HTTP/2、timeout、pooling、streaming
   response 的当前产品契约；任何无法证明的路径固定为 `Unknown`。
3. production 与 test 共用同一 monotonic reporter，并用本地 TCP/HTTP harness 覆盖
   connect、TLS、headers、partial/full body、response、timeout、cancel 和 mid-stream failure。
4. 从真实 `ExecutionEngine` 断言 replay gate：不确定、部分发送或可能已接受的非幂等请求不透明重放。

在上述条件实现前，本项应保持 `externally/architecturally blocked`，不得把 test-only phase
接入生产或删除保守降级。

## 批次 D：删除旧 owner、全量资格验证与文档收口

前置条件：批次 A、B 已完成；批次 C 要么完成，要么以明确 blocker 和 fail-closed 证据正式记录。

1. 审计后删除已无 production consumer 的旧 classifier、fallback、compatibility writeback
   与重复 parser owner。当前 `routing_failure::classify_route_failure` 仍有 source/test-support
   用途，`routing_health_snapshot` 仍被 store、legacy import/validate 和 portable catalog 使用，
   因此本阶段前不得删除它们。
2. 收紧 architecture/contract gate，拒绝二次分类、committed retry、无可靠 phase 的非幂等 replay、
   敏感数据落盘/进入 IPC/metrics、sleep 持有 lease 和 test/production 双契约。
3. 更新本专项计划、acceptance matrix、qualification、deletion ledger、boundary manifest 和
   `docs/README.md`，逐项标记 `done with evidence`、`partial`、`pending` 或
   `externally blocked`，并附实际命令及退出码。
4. 最终 diff 稳定后依次运行：

```powershell
git diff --check
node scripts/upstream-error-contract.test.mjs
node scripts/intelligent-routing-architecture.test.mjs
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
pnpm.cmd test
pnpm.cmd build
pnpm.cmd verify:fast
pnpm.cmd verify:full
```

若单次执行环境无法等待超过 124 秒的命令，必须在可完整等待的环境执行，或将其分解为可证明等价的阶段；未取得退出码 0 前不得声称本地工程闭环完成。

## 批次 E：真实 provider/Codex smoke

状态：`pending external authorization/evidence`。仅在本地工程收口后、用户提供隔离测试账号、
最小权限凭据和明确范围时执行。使用假业务输入，验证 OpenAI-compatible HTTP/SSE、Sub2API
成功/401/429/5xx、Codex 对最终 `server_error` 的行为，以及 capacity 期间不切换 target/key
且不写 credential failure。所有产物先脱敏；fixture 不能替代这项证据。

## 交付记录

每批完成时记录：改动文件、实际命令及退出码、未验证范围、残余风险和下一批前置条件。最终结论必须分别说明“本地 engineering cutover”与“release qualification”，后者在未获真实 smoke 授权时仍为 pending。
