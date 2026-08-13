# 上游错误分类与重试：剩余实质工作计划

状态：Ready for implementation

日期：2026-08-13

适用范围：本计划汇总会话 `019ff0a9-246c-7972-9048-d69d81e998c9` 与其续接会话 `019ff60e-2d4d-70b0-8b8b-708a17fef8bf` 的已完成证据和仍未完成的实质工作。它补充而不替代 [`2026-08-13-upstream-error-classification-retry-closure.md`](2026-08-13-upstream-error-classification-retry-closure.md) 的不变量、任务定义和最终验收门禁。

## 1. 事实来源与边界

实施判断顺序为：仓库 `AGENTS.md`、当前代码和自动化契约、`docs/README.md` 标记的当前规范、原专项闭环计划，最后才是本计划。`proposals/`、归档和会话记录只提供背景，不能单独覆盖当前实现事实。

当前工作区存在大量并行未提交改动。每批开始和结束记录 `git status --short`；只能编辑该批所需文件；不得重置、回退、清理或覆盖未知来源的改动。未经明确要求，不执行 stage、commit、push 或建分支。

本计划不把真实 provider/Codex 验证混入本地工程完成条件。它需要用户另行授权、隔离测试账号及脱敏证据。

## 2. 已完成基线

以下内容已有实现和 focused test/`verify:fast` 证据，不应再作为剩余开发重复实现：

- durable routing outcome：canonical failure 的 classification、confidence、evidence、send phase、replay/billing/retry disposition 与 failure-domain digest 经 request finalization 写入持久化 summary；重复 terminal 仍做 equality/collision 校验。
- decision trace 查询：重启后以 durable summary 为权威，并在同进程可用时附加 bounded runtime timeline；IPC TypeScript union、binding generation 和 portable migration catalog 已同步。
- trusted capacity-domain：同域 sibling 抑制、至多一次实际跨可信域 outbound，以及仅在真实选中跨域 target 后记录 `capacity_cross_domain_fallback` 已有 production 路径和回归。
- 保守 transport 边界：当前 reqwest 生产路径只报告 `NotConnected`、`ResponseStarted` 或 `Unknown`；不确定的非幂等请求 fail closed。原因及兼容性要求记录在 [`2026-08-13-reliable-transport-send-phase-spike.md`](../specs/2026-08-13-reliable-transport-send-phase-spike.md)。
- 最近一次 `cargo check --locked`、`cargo fmt -- --check`、focused Cargo tests、`pnpm.cmd generate:bindings`、相关 integration tests 和 `pnpm.cmd verify:fast` 已通过。最终交付仍须重跑与最终 diff 对应的检查。

## 3. 剩余批次与依赖

```text
Batch A: transport adapter 决策与实现
  -> Batch B: fault / concurrency / protocol / recovery matrix
  -> Batch C: 删除旧 owner、架构门禁、全量验证和文档收口
  -> Batch D: 真实 provider/Codex smoke（需外部授权）
```

Batch B 的独立测试准备可先进行，但不能把生产中间 send phase 标为完成，也不能放宽 replay gate，直到 Batch A 具备 transport owner 的可靠事实。Batch C 只能在 A、B 的生产调用链和 E2E 证据齐全后开始删除 owner。Batch D 依赖 Batch C 和明确授权。

## 4. Batch A：可靠 transport send phase

目标：仅将 transport owner 可证明的连接、headers、partial/full body 与 response-started 阶段用于 acceptance/replay；不能证明时维持 `Unknown`。

实施步骤：

1. 审计当前 `Cargo.toml` / `Cargo.lock` 中 reqwest、hyper、hyper-util、TLS 与 proxy feature，确认直接、system、HTTP、SOCKS proxy、Rustls、HTTP/2、timeout、pooling、buffered/streaming response 的现有契约。
2. 选择一个能拥有连接、TLS 与请求序列化 write future 的低层 adapter；在新增依赖或替换 transport 前，记录许可证、锁文件变化和 Windows 兼容矩阵。若没有兼容方案，保持 reqwest + `Unknown`，将本批标为技术阻塞而不是伪造 phase。
3. 用同一个 production/test reporter 单调报告 `NotConnected`、`ConnectedNoHeaders`、`HeadersSent`、`BodyPartiallySent`、`BodyFullySent`、`ResponseStarted`。只有相关 write future 成功完成后才能前进；body 被 poll、HTTP status 或 downstream commit 都不是 write 证据。
4. 让 `ExecutionEngine` 的 canonical failure、acceptance/replay gate 使用该 reporter。未知、部分发送或可能已接受的非幂等请求均不得透明重放。
5. 建立确定性本地 TCP/HTTP harness，覆盖 connect、TLS、headers、partial/full body、response headers、mid-stream failure、timeout 与取消，并从真实 execution 路径断言 phase 单调性和 replay 结果。

退出条件：生产 `upstream.rs` 的中间 phase 全来自 transport write owner；不支持的 protocol/proxy 组合明确为 `Unknown`；production 与 test 没有两套 phase 合同；全部测试本地可重复运行。

## 5. Batch B：故障、并发、协议与恢复矩阵

目标：以确定性测试证明资源、终态与 exactly-once 不变量在压力、取消、协议异常和持久化故障下成立。

实施步骤：

1. 扩展 diagnostic memory 测试：100 并发 HTTP error body/SSE bootstrap、共享 32 MiB admission、FIFO、deadline、cancel、panic、shutdown 和 owned-allocation 回收。验证 scratch 也被保守计入预算。
2. 补 parser/protocol 矩阵：gzip/deflate、压缩膨胀与解压失败、JSON 深度/node/token/string 上限、malformed UTF-8、任意 chunk split、256 KiB SSE event、control-only EOF、语义事件后无 success terminal EOF、合法空 completed，以及同 chunk content/error 顺序。
3. 补流式生命周期：慢客户端背压、downstream disconnect、terminal writer failure、precommit/postcommit persistence failure、writer unavailable、outbox/checkpoint replay、duplicate/late terminal 与不同 payload collision。committed 路径不得 retry 或写第二终态。
4. 补运行时恢复：classifier/projector/profile mismatch fail closed；热更新只影响新请求；runtime 重启不泄漏旧 cooldown、permit 或 half-open probe；最大 request body 的 retry 共用 backing storage 而不按 attempt 深拷贝。
5. 优先扩展既有 `routing_capacity_faults`、`routing_stream_finalization_faults`、`persistence_fault_matrix`、`proxy_lifecycle_*` 和 loopback harness；只有不能表达的共享故障才新增 fixture。

退出条件：所有故障测试使用 fake clock 或本地 loopback；成功、错误、取消、panic、shutdown 后资源计数回到基线；持久化恢复结果有明确断言且无敏感数据进入 fixture、日志或 DTO。

## 6. Batch C：删除旧 owner、资格验证与文档收口

目标：在新链证据齐全后删除重复解释和兼容写回，并使代码、门禁与文档处于同一 revision。

实施步骤：

1. 搜索并删除旧 classifier、HTTP status/public error 反推 effect、Execution 本地 retry/health 推导、同域 sibling fallback、重复 OpenAI/Responses error extractor、generic SSE terminal 降级和 scoped effect 向当前 station key 的兼容写回。仅删除已被新 production chain 覆盖的 owner。
2. 收紧架构/契约检查，拒绝：provider message/status 二次分类、committed retry、无 phase 的非幂等 replay、raw message/secret/URL 进入 durable/IPC/metrics、无 permit buffer、sleep 持 lease、测试与生产不同核心合同，以及无 producer/consumer 的 trace/metric 类型。
3. 更新 deletion ledger、acceptance matrix、qualification、boundary manifests、原专项第 11 节和 `docs/README.md`。每项明确写 `done with evidence`、`partial`、`pending` 或 `externally blocked`，不得把 transport、fault 或真实 smoke 提前标为完成。
4. 运行最终资格门禁：

```powershell
git diff --check
node scripts/upstream-error-contract.test.mjs
node scripts/intelligent-routing-architecture.test.mjs
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract
cargo test --locked --manifest-path src-tauri/Cargo.toml --test upstream_error_contract
cargo test --locked --manifest-path src-tauri/Cargo.toml
pnpm.cmd test
pnpm.cmd build
pnpm.cmd verify:fast
pnpm.cmd verify:full
```

退出条件：上述门禁全部退出码 0，或未完成项有可复现的失败证据和明确范围；无旧 owner 的生产引用；生成 binding/ACL/schema 产物经官方命令第二次运行无 diff。

## 7. Batch D：真实 provider 与 Codex smoke（外部授权）

前置条件：Batch C 已完成；用户明确授权；使用专用测试账号、最小权限凭据和假业务输入；所有输出先脱敏，且不持久化原始认证数据。

验证：OpenAI-compatible HTTP/SSE capacity、Sub2API 成功/401/429/5xx、Codex 对最终 `server_error` 的实际行为、capacity 期间 target/key 不切换且不写 credential failure，以及 trace/log/diagnostic artifact 无 secret。

当前状态：`pending external authorization/evidence`。未取得授权时，此项不执行，不能以 fixture 替代，也不应阻塞本地 engineering cutover 的结论。

## 8. 交付记录

每批结束记录：变更文件、实际命令与退出码、未验证范围、残余风险和下一批是否可开始。最终交付必须区分：本地 engineering cutover 是否完成，以及 release qualification 是否仍受真实 smoke 授权限制。
