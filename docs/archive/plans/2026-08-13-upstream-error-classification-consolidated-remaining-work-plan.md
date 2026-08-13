# 上游错误分类与重试：剩余实质工作计划

状态：Ready for implementation

日期：2026-08-13

## 范围与事实来源

本计划汇总任务 `019ff0a9-246c-7972-9048-d69d81e998c9` 与续接任务
`019ff60e-2d4d-70b0-8b8b-708a17fef8bf` 的实际完成证据和未完成工作。它替代同日其他
“remaining/current/final”计划作为后续排期入口；安全不变量和原始退出门禁仍以
[`2026-08-13-upstream-error-classification-retry-closure.md`](../../plans/2026-08-13-upstream-error-classification-retry-closure.md)
为准。

实现以仓库 `AGENTS.md`、当前代码、自动化契约和 `docs/README.md` 指向的当前规范为准。
当前工作区有大量未提交的并行改动。后续工作不得清理、回退或覆盖来源不明的内容，也不得
未经明确要求执行 stage、commit、push 或建分支。

## 已完成且不应重复开发的基线

1. 有界 HTTP/SSE 证据采集、canonical upstream outcome、OpenAI-compatible 公共错误适配，
   包括 buffered `2xx` error envelope 识别。
2. 同一 resolved target 的 capacity 有界重试、可信 capacity domain 的同域 sibling 抑制，
   以及至多一次跨可信域的终局 outbound。
3. scoped health/capability verdict 的迁移、typed store、planner 消费、revision fence、
   shadow rebuild 维护入口和主要回归。
4. 受限 `DecisionTrace`、durable routing outcome summary、terminal equality/collision fence，
   以及对应 IPC/binding/portable catalog 链路。
5. 严格 UTF-8 SSE 解析、chunk-boundary、诊断内存上限、慢下游提交后的 lifecycle 保护；
   100 路 SSE decoder 与 parser scratch 的共享 32 MiB admission/reclaim 回归已存在。
6. `reqwest` 生产 transport 只可靠报告 `NotConnected`、`ResponseStarted`、`Unknown`。
   对可能已被接受的非幂等请求，`Unknown` 保持 fail-closed。这是已验证的安全边界。
7. 已完成的本地验证包括 `cargo fmt --check`、`cargo check --locked`、`pnpm.cmd test`、
   `pnpm.cmd build`、`pnpm.cmd verify:fast`、upstream error contract、architecture contract，
   以及主要 focused Rust 集成测试。完整 `cargo test` 曾在 124 秒后超时，不能标记为通过。

## 尚未完成的实质工作

### 1. 受控配置 trusted capacity domain

现状：`station_capacity_domains` 已由 migration `0038` 建表，读取、join 和 revision
revalidation 已接入 operational facts/routing；但没有写入 store、service、command、IPC 或 UI。
操作者目前无法安全配置可信容量身份，跨域 fallback 因而可能不可用。

实施：

1. 将 capacity domain 作为独立于普通 station update 的资源建模；使用其自身 `revision`
   作为 optimistic fence。station 删除依赖外键 cascade，station 更新不得隐式修改该身份。
2. 增加 typed store 和 application/service facade，提供受限 `upsert` 与 `clear` 语义。
   所有字段执行长度、空值、provider family/deployment/region 组合校验，并在 revision 冲突时
   返回明确、脱敏的并发错误。
3. 经现有 station command facade、命令注册、ACL、IPC DTO 和官方 binding generation 暴露；
   不复用未携带 revision 的 `CreateStationInput`/`UpdateStationInput`。
4. 在既有 station 编辑工作流中提供紧凑的 capacity-domain 编辑和清空控件，覆盖 loading、
   empty、error、disabled、窄窗口、键盘焦点与并发更新反馈。
5. 补 create/update/clear/delete-cascade/revision-conflict、snapshot hot-update、routing
   revalidation、导入导出和 portable migration 回归。

验收：经 UI 或 IPC 写入的身份在下一次新请求 snapshot 中生效；station 删除后不残留配置；
无可信身份时不发生跨域；旧 attempt 不能借 revision 漂移写入新身份。

### 2. Durable terminal outbox 与崩溃恢复

现状：`LifecycleWriter` 是内存 `mpsc` 加三次短重试。`CommitOutcomeUnknown` 会使 writer
不健康，重启后没有待投递终态。`finish_request` 可原子写入已进入 SQLite 事务的 terminal 和
outcome summary，但无法覆盖 writer unavailable、worker 崩溃、队列丢失或提交结果未知。
启动 reconciliation 只把未终态请求标为 `startup_interrupted`，不能重放原 canonical terminal。

实施：

1. 先冻结 durable terminal payload：仅含稳定枚举、版本、内部 identity reference 和已审计
   outcome summary；禁止 raw message、URL/query、authorization、secret 与原始 provider body。
2. 新增 additive migration：durable terminal outbox、claim/lease、attempt/retry/checkpoint 与
   payload fingerprint/collision fence。同步 portable migration catalog、schema tests 与导入导出
   契约。
3. 让 admission/terminal lifecycle 在同一事务内持久化可重放 payload 和 outbox 记录；由单一
   worker claim、投递、CAS 完成。不得以吞掉 effect、压制 terminal write 或无限重试代替恢复。
4. 在启动 reconciliation 中安全回收过期 lease、重放未完成 outbox，并保持 duplicate/late
   terminal exactly-once；不同 payload collision 必须 fail closed 并留下脱敏诊断。
5. 覆盖 writer unavailable、queue/worker 崩溃、事务前失败、事务后提交结果未知、进程崩溃、
   restart replay、重复/晚到 terminal、payload collision、cancel 和 shutdown。所有测试使用
   fake clock 或本地 SQLite/loopback。

验收：任一提交窗口失败后，要么 terminal/outcome 已原子完成，要么有可重放 outbox；重启不丢失
canonical terminal；committed 路径不重试 upstream、不写第二个终态。

### 3. Transport send phase 的技术决策

现状：不能从 `reqwest::RequestBuilder::send()` 证明 headers 或 body 实际写入 socket。body
poll、HTTP 状态和 downstream commit 都不是发送证据。现有保守降级正确，但无法实现精确的
headers/body partial/full phase。

实施：

1. 写明 Windows transport spike 的选择与结论：direct/system/HTTP/SOCKS proxy、Rustls、
   HTTP/2、timeout、pooling、buffered/streaming response、许可证及 `Cargo.lock` 影响。
2. 只有存在同时拥有连接、TLS 与序列化 write future 的兼容 transport owner 时，才把
   `ConnectedNoHeaders`、`HeadersSent`、`BodyPartiallySent`、`BodyFullySent` 升为生产信号；
   production/test 必须共用同一 reporter。
3. 使用真实 `ExecutionEngine` 的本地 TCP/HTTP/TLS harness 验证 phase 单调性、connect、
   headers、partial/full body、response、timeout、cancel 和 mid-stream failure。
4. 若选型不能完整保留现有 `reqwest` Windows 协议/代理契约，正式记录为技术阻塞，维持
   三态生产实现；不伪造中间 phase，也不放宽非幂等 replay gate。

验收：每个生产 phase 都有 socket write 证据；不支持路径返回 `Unknown`；所有不确定或可能已
接受的非幂等请求都不透明重试。

### 4. 剩余故障矩阵与组合 E2E

实施：

1. 补 group/subscription 与 `model_not_found` 的完整 E2E：terminal effect、下一 snapshot 的
   精确 scope 排除、无关 subject 可选、authority revision 后恢复，并验证 verdict 维度正交。
2. 审计四次总尝试预算及同 target retry、同域抑制、一次跨域终局对其他 failure class 的影响；
   增加无可信身份不跨域、identity/revision drift 阻止旧 attempt 写新事实的 composition 证据。
3. 补 100 并发 admission 的 FIFO、queue cap、deadline、cancel、shutdown、half-open、
   sleep/lease 回收和 runtime restart 无 cooldown/permit 泄漏；验证 retry 共用 request body
   backing storage。
4. 为 gzip/deflate 建立明确策略：继续不解压时测试 encoded wire-byte admission；若支持解压，
   必须加入 decoded-byte admission 及解压失败/zip bomb 回归。补 JSON 深度/node/token/string
   限制（如适用）、SSE EOF/顺序/大事件、慢客户端背压、downstream disconnect。
5. 将 profile/rule/provider/retry/public/trace 的热更新限定为新请求；在途 attempt 使用单一
   快照，并通过回归证明 fail-closed 的 classifier/projector/profile mismatch 不改写 canonical
   决策。

验收：所有测试确定性地使用 fake clock 或本地 loopback；成功、错误、取消、panic、shutdown 后
资源计数回到基线；DTO、fixture、日志和 metrics 不含敏感数据或高基数标识。

### 5. 旧 owner 删除、资格验证与文档收口

前置条件：第 1、2、4 项完成；第 3 项已实现或有正式的保守技术阻塞记录。

1. 搜索并删除已由 canonical chain 覆盖的旧 classifier、status/message 二次推导、local
   retry/health 推导、重复 parser、兼容写回和仅服务旧行为的 test API；保留 `Unknown`
   fail-closed 等安全路径。
2. 收紧 architecture/contract/dead-code gates，拒绝 committed retry、无 phase 的非幂等 replay、
   同域 sibling fallback、无界队列/缓冲、raw secret/message/URL 进入 durable/IPC/metrics，及
   production/test 核心合同分叉。
3. 依次运行：

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

4. 更新原专项实施台账、acceptance matrix、qualification、deletion ledger、boundary manifest、
   `docs/README.md` 和必要 release note；只按实际证据标记 `done with evidence`、`partial`、
   `pending` 或 `externally blocked`。

验收：本地可运行门禁均为退出码 0，或保留可复现失败及影响范围。此前完整 `cargo test` 的
124 秒超时和 `verify:full` 的 RustSec advisory 网络依赖，均不得写作通过。

### 6. 真实 provider/Codex smoke（外部授权）

仅在前述工程收口完成、用户明确授权并提供隔离测试账号、最小权限凭据和范围后执行。使用假
业务输入验证 OpenAI-compatible HTTP/SSE capacity、Sub2API 成功/401/429/5xx、Codex 对
最终 `server_error` 的行为，以及 capacity 时不错误切换 key 或写入 credential failure。

验收：只将脱敏证据写入 qualification；未获授权时保持
`pending external authorization/evidence`，不以 fixture 代替真实 smoke。

## 交付顺序

```text
capacity-domain configuration ----+
durable terminal outbox ----------+--> fault/composition E2E --> deletion, verification, docs
transport spike (decision gate) --+
                                                     |
                                                     +--> external provider smoke (authorization required)
```

每个批次开始和结束均记录 `git status --short`、实际改动、命令及退出码、失败归属和未验证范围。
