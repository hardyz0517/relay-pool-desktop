# 上游错误分类与重试：协调后的剩余实质工作计划

状态：Ready for implementation

日期：2026-08-13

适用范围：本计划汇总任务 `019ff0a9-246c-7972-9048-d69d81e998c9` 与
`019ff60e-2d4d-70b0-8b8b-708a17fef8bf` 的实现和验证记录，列出仍未由当前代码
与可复现证据关闭的工作。它补充而不替代
[`2026-08-12-upstream-error-classification-retry-upgrade.md`](../../plans/2026-08-12-upstream-error-classification-retry-upgrade.md)
及 [`2026-08-13-upstream-error-classification-retry-closure.md`](../../plans/2026-08-13-upstream-error-classification-retry-closure.md)。实现以当前代码、自动化契约和
[`docs/README.md`](../../README.md) 所列当前规范为准。

## 已完成基线

以下内容已有生产实现及定向验证，不再作为功能开发任务重复拆分：

1. canonical 上游错误分类、受限 HTTP/SSE error envelope、公开 OpenAI-compatible
   error adapter、同目标 capacity 重试、同域 sibling 抑制、请求级预算和一次可信跨域
   终局路径。
2. `0035` scoped health verdict、`0037` routing outcome summary、`0038` trusted
   capacity domain 与 `0039` terminal outbox migration；终态 outbox 已具备 lease
   恢复、重放和 payload collision fail-closed 语义。
3. capacity identity 已有受控的 store/service/IPC/UI 写入路径，缺失 identity 时仍
   fail closed；不从 station 名称、URL、key 或错误文本推断 domain。
4. runtime `DecisionTrace`、有界 diagnostic memory、2xx semantic error envelope、
   retry body backing allocation 复用及 scoped verdict 的基础生产消费链。
5. transport spike 已记录当前 `reqwest` 的可靠事实只有 `NotConnected`、
   `ResponseStarted` 和 `Unknown`；`Unknown` 对非幂等 replay 保持 fail closed。

已通过的代表性门禁包括 `pnpm.cmd verify:fast`、`pnpm.cmd test`、`cargo fmt --check`、
`cargo check --locked`、bindings generation check、专项 contract/architecture test 及
outbox、capacity、scoped verdict 的定向 Cargo 测试。完整 `cargo test` 曾因执行环境的
122 秒输出/运行时限中断，不能标记为已通过。

## 不变量

- 非幂等请求在上游接受状态不确定时绝不透明重放；committed 后不 retry、不产生第二
  terminal。
- capacity 不写 credential/account hard failure；同一可信 capacity domain 不轮换
  sibling key；跨域分支至多一次且其 terminal 结束该请求 retry 链。
- durable record、IPC、日志、fixture 和 metrics 不保存原始 secret、认证数据、完整
  URL/query、动态 message 或真实 request identifier。
- 终态先经 durable outbox，再经既有 CAS/outcome transaction 应用；重复投递只接受
  完全相同 payload，冲突 fail closed。
- 保留脏工作区的并行改动；不通过扩大 retry、放宽 replay gate、手改生成物或 suppress
  门禁来取得绿色结果。

## 剩余本地工程工作

### 1. 补强 profile/revision 热更新的端到端证据

目标：证明规则、provider、retry 与 public profile 在运行中的请求使用固定 snapshot，
而后续新请求使用权威 revision 的新值。

1. 建立 loopback/production-composition 场景：在首个 attempt 后修改 profile 或
   station capacity identity，断言当前请求不改变其 classification、retry、domain 或
   public mapping。
2. 对新请求断言新的 snapshot 立即生效；对同目标 retry 断言 resolver 重新读取权威
   facts，revision 漂移时不会把旧 commitment 当作同一 target。
3. 覆盖 profile/classifier/projector 不匹配时的 fail-closed 行为，并确认 trace/outcome
   只记录闭合版本和枚举值。

验收：场景不依赖真实 provider；current-request snapshot 与 new-request reload 均有
确定性断言，且不引入第二套 profile owner。

### 2. 完成尚缺的故障、并发与协议矩阵

目标：把已有定向回归扩展到计划要求的资源释放、单 terminal 和边界协议证据。

1. capacity/admission：以 fake clock 或本地 loopback 覆盖 100 并发、active/waiter
   上限、FIFO、queue full、deadline、cancel、shutdown、HalfOpen race，以及 sleep 前
   释放和醒来后重取 lease。
2. HTTP/SSE：覆盖 gzip/deflate 失败与压缩膨胀、JSON 深度/token/string 限额、非法
   UTF-8、任意 chunk split、control-only EOF、缺成功 terminal 的 EOF、256 KiB event
   上限、慢客户端背压和下游 drop。
3. durable finalization：覆盖 writer unavailable、pre/post-commit failure、crash 后
   expired lease reclaim、duplicate/late terminal、payload collision、outbox tampering，
   并确认所有 permit/计数回到基线。
4. 验证最大请求体在同目标 retry 中复用 backing storage；mapped-model 转换每目标
   重新序列化是当前正确但未优化的行为，除非先定义缓存 identity、生命周期和内存上限，
   不在本批引入缓存。

验收：测试全部为确定性本地场景；committed 分支不 retry、不双 terminal；所有资源
计数归零或回到预期稳态。

### 3. 完成全量资格验证并记录真实结果

目标：用独立的退出码证据关闭本地工程资格，而不把超时或外部网络问题误报为通过。

1. 单独运行 `pnpm.cmd build`，取得不受并行 Cargo 输出中断影响的结果。
2. 运行 `pnpm.cmd verify:full`；若 RustSec/advisory 数据库访问失败，保留完整命令、
   退出码、时间和失败范围，标为未验证。
3. 尝试完整 `cargo test --locked --manifest-path src-tauri/Cargo.toml`。若环境限制仍无法
   取得退出码 0，则按 test target 分组补充覆盖并明确说明这不等价于完整 suite。
4. 在所有本批修复后重跑 `git diff --check`、`cargo fmt --check`、`cargo check --locked`、
   `pnpm.cmd test`、`pnpm.cmd build`、`pnpm.cmd verify:fast` 和 bindings check。

验收：每项只能以实际退出码 0 记为通过；任何未运行、超时或依赖外部网络的项都有可复现
记录与影响说明。

### 4. 原子删除审计与文档收口

前置条件：第 1-3 项完成或已按失败证据明确阻断范围。

1. 对旧 classifier、status/message fallback、compatibility writeback、parser owner、
   历史 `*-red*` 脚本和仅供旧行为的测试 API 做生产引用审计；仅在实际零引用后删除。
2. 更新原专项计划第 11 节、acceptance matrix、qualification、deletion ledger、
   boundary manifests 和 `docs/README.md`，逐项采用 `done with evidence`、`partial`、
   `pending` 或 `externally blocked`。
3. 明确记录两项不应被掩盖的限制：transport 精确 phase 的架构 blocker，以及 legacy
   portable import/export 不携带 capacity identity 时的安全默认值（缺失即不跨域）。

验收：文档、manifest 与代码处于同一 revision；没有将未完成 transport/fault/full-suite
证据写成 done，也没有把已完成 outbox/capacity configuration 再写成 pending。

## 需产品决策或外部授权的事项

### 精确 transport send phase

当前不是“补测试即可完成”的缺口。若要区分 headers、partial body 和 full body 的实际
写入，必须由 transport owner 在 Windows 环境中证明兼容 direct/system/HTTP/SOCKS proxy、
Rustls、HTTP/2、pooling、timeout 与 streaming 合同。未完成该替换/包装前，维持现有三态
保守实现；`Unknown` 对非幂等请求禁止透明重试。

### legacy identity 的可移植性

当前 portable migration/import 不携带 capacity identity，导入后缺失 identity 会禁止跨域
fallback。这是安全默认值。若产品要求跨设备保持该运营断言，需要单独确认是否允许迁移该
配置、其加密/导出边界及 UI 解释，不能在本专项中自行扩大导出范围。

### 真实 provider / Codex smoke

仅在用户明确授权、提供隔离测试账号和最小权限凭据后执行。验证 OpenAI-compatible HTTP
与 SSE capacity、Sub2API 成功/401/429/5xx、Codex 对 `server_error` 的实际行为、capacity
期间不切换 key 且不写 credential failure，以及所有 artifact 脱敏。未授权时状态固定为
`pending external authorization/evidence`，不阻塞本地工程结论，也不得以 fixture 替代。

## 执行顺序

```text
profile/revision E2E ─┐
fault/concurrency/protocol matrix ─┼─> full qualification ─> deletion/document closure
                                  │
transport architecture decision ──┘  (保守实现可并行保留)

真实 provider/Codex smoke：文档收口后且取得外部授权才执行
```

每批结束更新本计划的状态、变更范围、实际命令与退出码；未经明确要求不 stage、commit、push、
创建分支或 PR。
