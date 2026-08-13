# 上游错误分类与重试：剩余实质工作执行计划

状态：执行中

日期：2026-08-13

适用范围：汇总会话 `019ff0a9-246c-7972-9048-d69d81e998c9` 及其续接会话 `019ff60e-2d4d-70b0-8b8b-708a17fef8bf` 的已完成证据和仍需完成的实质工作。本文件补充而不替代 [`2026-08-13-upstream-error-classification-retry-closure.md`](2026-08-13-upstream-error-classification-retry-closure.md)。实施以 `AGENTS.md`、当前代码、自动化契约和 [`docs/README.md`](../../README.md) 为准。

## 已完成基线

下列内容已有代码和针对性验证，不再作为剩余工作重复实现：

- canonical error classification、bounded HTTP/SSE evidence、public adapter、retry/replay gate 与双终态 fail-closed 生产链。
- trusted capacity-domain 的 migration、store、service、IPC、binding 和 UI；同域 sibling 抑制、最多一次可信跨域回退及缺失身份 fail closed。
- durable outcome summary、terminal outbox/replay、bounded decision trace、IPC 查询和低基数 metrics；重复、碰撞、过期租约重放均有回归。
- retry 重用 ingress `Bytes` backing storage；target commitment 已包含 station type，station type 变更会提高 endpoint revision 并强制重校验。
- model-not-found 与 Sub2API `SUBSCRIPTION_NOT_FOUND` 的真实 proxy E2E：均验证写入精确 verdict、下一 snapshot 仅排除相关 subject、无关 model/group 可用、对应 revision 恢复。
- 首个 Chat/Responses SSE error 的任意双 chunk 边界穷尽覆盖。
- HTTP diagnostic capture 显式关闭 gzip/deflate/brotli 自动解压，并验证三种 Content-Encoding 仍按 wire bytes 有界保留。
- `pnpm.cmd verify:fast`、前端测试与构建、bindings check、专项 contract/architecture checks、`cargo fmt --check`、`cargo check --locked` 和相关 focused Cargo tests 已实际通过。最终交付仍须按最终 diff 重跑。

## 依赖顺序

```text
scoped verdict 重建、崩溃与维度正交矩阵

transport owner 可证明 send phase（架构决策/实现）
        |
        +--> fault、并发、背压与协议矩阵
                 |
                 +--> 删除旧 owner、全量资格验证、文档收口
                                      |
                                      +--> 真实 provider/Codex smoke（需授权）
```

## 1. 补 scoped verdict 的持久化恢复证据

1. balance/account 的同 subject 并存与独立恢复已有回归；补 credential、group、quota、rate-limit、capability 等剩余组合，保证一个 dimension 因 revision 恢复不得清除其他 dimension。
2. shadow rebuild 的 checkpoint/swap 已使用完整 `(ingested_at_ms, ingestion_sequence, observation_id)` cursor，且 stale proof 会被拒绝；仍需审计其 immutable typed outcome 输入，禁止用当前 message 或 rule set 重分类历史。
3. 保留 immutable observations 而重置 runtime projection/checkpoint 的 portable restore/restart 已自动 shadow rebuild，第二次启动 no-op；补失败回滚、duplicate/late terminal/payload collision 的端到端 crash parity，保证 terminal、observation、verdict、checkpoint 的 exactly-once 事务边界。
4. 运行完整 migration/reimport 故障注入路径，确认 0035、0037、0039 与 portable catalog 一致。

验收：group 和 model capability 两条生产组合 E2E 均存在，且 scoped verdict 的恢复、重建和崩溃路径有确定性测试。

## 2. 解决可靠 transport send phase 的架构门槛

状态：外部/架构阻塞，不是“仅缺测试”。现有 reqwest owner 只能可靠报告 `NotConnected`、`ResponseStarted`、`Unknown`；未知非幂等请求已 fail closed。调研结论见 [`2026-08-13-reliable-transport-send-phase-spike.md`](../specs/2026-08-13-reliable-transport-send-phase-spike.md)。

必须先完成 Windows transport 的设计与可行性证明：保留 direct/system/HTTP/SOCKS proxy、TLS、HTTP/2、timeout、pooling、streaming 的现有契约，确认许可证和锁文件影响。只有新 transport owner 能在真实 write future 上单调报告 `ConnectedNoHeaders`、`HeadersSent`、`BodyPartiallySent`、`BodyFullySent` 时，才可接入 replay 判断。不得以 body poll、HTTP status 或 downstream commit 伪造 phase。

验收：production/test 共用同一 phase reporter；受控本地 TCP/HTTP harness 覆盖 connect、TLS、headers、partial/full body、response、timeout、取消和 mid-stream failure；不支持路径明确保留 `Unknown`。

## 3. 补 fault、并发、协议与资源回收矩阵

优先扩展既有 `routing_capacity_faults`、`routing_stream_finalization_faults`、`proxy_lifecycle_*`、`persistence_fault_matrix` 和 loopback harness：

1. 100 并发 capacity permit、SSE bootstrap、decoder/scratch 的共享 32 MiB 上限和 RAII 回收已有定向回归；32 个真实 proxy HTTP no-candidate 请求也验证 lifecycle terminal 与 active-request 回收。仍需真实 HTTP/SSE execution 路径的 100 请求 FIFO、队列上限、deadline、cancel、shutdown、half-open race 资格。
2. gzip/deflate 解压错误与压缩膨胀，JSON 深度/node/token/string 上限，malformed UTF-8，任意 chunk split，256 KiB SSE event、control-only EOF、缺 success terminal EOF、合法空 completed 及 content/error 顺序。
3. 慢客户端背压、downstream disconnect、terminal writer unavailable、pre/post-commit persistence failure、outbox/checkpoint replay、重复/迟到 terminal 和 payload collision。
4. profile mismatch fail closed、热更新快照隔离、runtime restart 后 cooldown/permit/half-open 不泄漏，以及最大 request body 在 retry 间不深拷贝。

验收：测试均使用 fake clock 或本地 loopback；成功、失败、取消、panic、shutdown 后资源计数回到基线；committed 路径绝不 retry 或生成第二终态。

## 4. 删除旧 owner、收紧门禁并收口文档

前置条件：第 1-3 节的新链证据齐全。

删除已经被新生产链覆盖的旧 classifier、基于 HTTP status/public error 的 effect 反推、Execution 本地 retry/health 推导、同域 sibling fallback、重复 OpenAI/Responses extractor、generic SSE terminal 降级和 scoped effect 兼容写回。先用 architecture/contract 搜索证明没有 production consumer，再删除。

收紧门禁，拒绝二次分类、committed retry、无可靠 phase 的非幂等 replay、敏感数据进入 durable/IPC/metrics、sleep 持有 lease 和 production/test 两套核心合同。同步更新 deletion ledger、acceptance matrix、qualification、boundary manifests、原专项台账与 `docs/README.md`，如实标记 `done with evidence`、`partial`、`pending`、`externally blocked`。

## 5. 全量工程资格验证

最终 diff 稳定后依次运行：

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

当前限制：完整 `cargo test` 与 `pnpm.cmd verify:full` 都超过本会话单次命令的 124 秒硬时限，尚未取得 exit code 0，也没有取得根因失败。后续需在可等待完整命令的执行环境运行，或分解为等价且可证明的阶段；在此之前不得宣称工程闭环完成。

最新已通过门禁：`git diff --check`、upstream-error contract、intelligent-routing architecture gate、`cargo fmt --check`、`cargo check --locked`、`pnpm.cmd test`、`pnpm.cmd build`、`pnpm.cmd verify:fast`。这些不替代完整 Cargo suite 或 `verify:full`。

## 6. 真实 provider/Codex smoke

状态：`pending external authorization/evidence`。仅在本地工程收口、用户提供隔离测试账号、最小权限凭据和明确范围后执行。使用假业务输入并先脱敏产物；验证 OpenAI-compatible HTTP/SSE capacity、Sub2API 成功/401/429/5xx、Codex 对最终 `server_error` 的行为，以及 capacity 期间不切换 target/key 且不写 credential failure。fixture 不能替代该证据。

## 执行纪律

每批结束记录变更文件、实际命令与退出码、未验证范围、残余风险与下一批前置条件。工作区当前有大量并行未提交改动；每批前后检查 `git status --short`，不得清理、回退或覆盖来源不明的文件。未经明确要求，不执行 stage、commit、push、建分支或创建 PR。
