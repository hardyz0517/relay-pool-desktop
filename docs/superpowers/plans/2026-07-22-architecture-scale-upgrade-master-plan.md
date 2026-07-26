# Relay Pool Desktop 规模化架构升级总实施计划

> **执行者必读：** 本计划是 `docs/superpowers/specs/2026-07-22-architecture-scale-upgrade-design.md` 的唯一总实施入口。开始任何 Task 前必须先读设计 spec、本文、当前 Task 引用的 ADR/清单，并核对工作树。不得跳过前置门禁后直接拆文件。

**目标：** 在不改 Persistence V2、不重写已验证的 proxy request lifecycle、不引入微服务或运行时插件的前提下，把当前已经落后于项目规模的 IPC、composition、前端数据所有权、页面生命周期、后台工作、outbound 和 provider 扩展方式升级为可靠、可维护、可拓展的模块化单体。

**目标架构：** Rust 是 IPC 契约和公开错误的唯一权威；前端由显式 `DesktopBackend` / `DemoBackend` 组合领域 client；command 只依赖窄 application service/facade；TanStack Query 是 server state 唯一 owner；`PageVisibility` 是页面活跃性的唯一 owner；`TaskSupervisor`、`OperationRegistry` 和 `BlockingExecutor` 分别治理 daemon、前台长操作和真实阻塞工作；共享 `AsyncOutboundClient` 先落地，再迁移 capability-based `ProviderRegistry`；最后才按已稳定的 owner 拆巨型文件并删除旧路径。

**技术栈：** Tauri 2、React 18、TypeScript、Vite、TanStack Query 5、Rust、Tokio、reqwest、`tracing`、SQLx（只作为 Persistence V2 已提供的外部能力）、specta/tauri-specta 候选、TypeScript Compiler API、`syn`、Vitest、Cargo tests、GitHub Actions。

**批准的设计：** `docs/superpowers/specs/2026-07-22-architecture-scale-upgrade-design.md`

---

## 1. 文档权威与冲突规则

执行时按以下优先级解释：

1. 已批准的 architecture scale upgrade spec 决定目标边界和禁止项。
2. 本总计划决定顺序、Task 边界、cutover、删除点和验证证据。
3. 各 Task 新增的 ADR 只能补充实现选择，不能放宽 spec。
4. 现有行为测试、正式发布行为和用户明确要求决定兼容语义。
5. 若当前代码与本文路径或数量漂移，先更新 inventory 和 Task 证据，不凭旧计数继续。

冲突时不得自行选择更省事的双轨实现。目标边界不明确就停止该 Task，修订 spec/plan 并评审后再继续。

## 2. 前置条件、范围与非目标

### 2.1 硬前置条件

- Persistence V2 必须已完成其 production cutover、删除门禁和独立验证，或至少有一个明确、可复现、已提交的稳定集成点。
- 本 design spec、总计划及其正式审阅必须先形成 durable local commit；implementation worktree 记录该 commit hash。不得从未跟踪/未提交文档启动长期重构，否则压缩、切线程或主线 drift 后无法证明执行的是哪个版本。
- 本升级不得编辑 `src-tauri/src/persistence/**`、迁移 SQL、generation upgrade、schema manifest 或 Persistence V2 fixture。
- 当前主 checkout 的 Persistence V2 未提交改动不能被复制、stash、重置或吸收到本升级提交。
- 实施分支建议为 `codex/architecture-scale-upgrade`，在独立 worktree 中执行；创建 worktree 前先记录主 checkout 和目标 base revision。
- 若 Persistence V2 最终 composition API 与 spec 假设不同，只允许在 Stage 0 更新 composition ADR 和窄 adapter 计划，不修改 persistence 内核。

### 2.2 包含范围

- typed IPC、公开错误、generated TypeScript bindings、ACL/registry 一致性和 runtime handshake。
- Rust application composition、窄 command facade/state、frontend backend mode 和 demo 隔离。
- Query ownership、aggregate read models、页面可见性、缓存和 mutation 收敛。
- 后台 task、前台 operation、blocking work、shutdown 和本地诊断。
- provider/probe/management async outbound、provider registry 和 capability drivers。
- command、页面、provider adapter 的职责拆分和旧路径删除。
- parser-backed architecture fitness、PR/release CI、artifact/index hygiene 和 release qualification。

### 2.3 明确非目标

- 不重做 Persistence V2。
- 不改变数据库 schema，除非另立 spec 且不属于本计划。
- 不把模块化单体拆成微服务。
- 不引入动态插件 ABI、通用 DI 容器、事件总线、actor framework、workflow DSL 或 generic repository。
- 不重写 proxy request/attempt lifecycle；只通过其公开 runtime facade 协调启动、drain 和 shutdown。
- 不重做视觉设计、不引入暗色主题、不借重构修改无关产品文案。
- 不在架构 owner/cutover shard 中替换 React、Tauri、TanStack Query、Tokio、reqwest 或 SQLx 的大版本；版本升级走独立 prerequisite shard。Stage 0 仍必须核对支持窗口和安全状态，unsupported/EOL 或不可接受高危风险会阻塞后续 release，不能用本条规避。

## 3. 执行纪律

- 每个 Task/shard 开始和结束都记录：主 checkout HEAD、升级 worktree HEAD、`git status --short`、最近 8 个提交、当前 shard 允许文件。
- RED-GREEN-REFACTOR 必须可复核：先看到测试因目标能力缺失而失败，再实现，再跑 focused、affected、stage gate。
- 每个 Task 只允许一个 production cutover。新旧路径不得同时写同一状态，也不得互相 fallback。
- temporary adapter 必须在引入时登记 owner、调用者、删除 Task 和禁止扩张规则。
- 不使用 `git add .`、`git add -A`、`git commit -a`。只 stage Task 列出的明确路径；混合文件使用 `git add -p`。
- 每次提交前检查 `git diff --cached --name-only`、`git diff --cached --check` 和完整 staged diff。
- Windows 上 Cargo build/test 串行执行，并把 `CARGO_TARGET_DIR` 指向 worktree 外或统一 `output/cargo/<task>`，避免 watcher、CodeGraph 和 Cargo lock 互相污染。
- 任何 live provider 验证必须与 fixture 验证分开记录；HTTP 200、debug build 或单个 fixture 通过都不能替代 release gate。
- 若 baseline 已红、行为来源不清、需要改 persistence、出现跨 Task 大面积 drift 或回滚点失效，立即停止，不继续堆兼容层。
- push 和 PR 不属于默认执行动作；每个原子 shard 默认形成一个本地、可 review 的提交。

### 3.1 Task shard 与精确文件协议

本文中的 Task 是依赖和验收单位，不代表允许把整个 Task 塞进一个巨大提交。凡是包含多个 command group、feature、page、runner、provider 或基础设施内核的 Task，必须拆成 `Task <n>.<shard>` 原子执行：

- 一个 shard 只迁移一个 command group、一个 feature/page、一个 runner、一个 provider capability，或一个独立基础设施内核。
- 每个 shard 单独经历 RED、GREEN、focused/affected gate、production cutover、旧路径删除和 Checkpoint；前一 shard 未通过不得开始后一 shard。
- foundation、adapter、cutover、delete 不得合并成一个不可回滚提交。推荐提交顺序为 `contract/gate -> implementation -> one-owner cutover -> deletion`。
- 本文出现 `matching ... files`、glob 或“剩余页面”时，Task 0 必须在 `architecture-scale-upgrade-inventory.json` 的 `execution_shards` 中展开精确路径、owner、依赖 Task、cutover 和 delete Task。没有精确 manifest entry 的 shard 不得开工。
- shard 不能跨越 Stage Gate，也不能借“同一 Task”吸收无关主线 drift。

至少强制拆分：Task 5 按 command group；Task 7 按 facade/domain；Task 9 按 feature；Task 11-13 按页面/read model；Task 14 按 lifecycle/blocking/outbound；Task 17 按 monitor/capture/shutdown；Task 19 按 contract/conformance/reference driver；Task 20-22 按 capability/HTTP consumer；Task 23-25 按领域/页面/provider/删除类别。

## 4. 总体依赖与切换顺序

```text
Stage 0 可信门禁和基线
  -> Stage 1 Typed IPC / Error / Handshake
    -> Stage 2 Narrow Facades / Explicit Backend
      -> Stage 3 Query Ownership / Page Visibility
        -> Stage 4 Work Lifecycle / Async Outbound
          -> Stage 5 Provider Capability Drivers
            -> Stage 6 Physical Decomposition / Legacy Deletion
              -> Stage 7 Release Qualification
```

强制顺序理由：

- 没有 Stage 0 的门禁，迁移期间会继续新增旧式 `invoke`、fallback、thread 和 `ureq`。
- 没有 Stage 1 的 typed contract，Stage 2 的 backend abstraction 只会把字符串调用再包一层。
- 没有 Stage 2 的窄 composition，Stage 3/4 会继续依赖 frontend/Rust service locator。
- 没有 Stage 3 的单一状态 owner，页面拆分会保留多套 loader、event 和缓存副本。
- 没有 Stage 4 的 async outbound/cancellation，Stage 5 的 driver 只会把 `ureq + spawn_blocking` 搬进新 trait。
- 没有 owner 和依赖方向稳定，Stage 6 的拆文件只是移动屎山。
- Stage 7 只验收同一个 revision，不允许在 qualification 阶段临时修业务逻辑后跳过前置 Task 证据。

## 5. 最终模块与文件责任图

| 路径 | 最终责任 |
|---|---|
| `src-tauri/src/application/command_facades/*.rs` | command 可调用的窄领域用例；不公开内部 service 字段 |
| `src-tauri/src/commands/mod.rs` | module declarations、re-export、generated registry；无业务实现 |
| `src-tauri/src/commands/error.rs` | internal/application/work/driver error 到 `CommandError` 的唯一映射 |
| `src-tauri/src/commands/*.rs` | transport validation、一个 facade 调用、DTO conversion |
| `src-tauri/src/background_tasks/supervisor.rs` | daemon metadata、token、join、restart/backoff；无业务 service lookup |
| `src-tauri/src/background_tasks/operation.rs` | foreground operation status、cancel、terminal；无完整 payload history |
| `src-tauri/src/background_tasks/blocking.rs` | 有界真实阻塞工作和 orphan diagnostics |
| `src-tauri/src/background_tasks/exit.rs` | tray/window/updater/OS exit 的唯一幂等协调和 bounded drain |
| `src-tauri/src/outbound/*.rs` | 中立 async HTTP、proxy route、budget、timeout、redaction、typed failure |
| `src-tauri/src/observability/*.rs` | correlation、structured tracing、bounded local metrics 和统一 redaction；不拥有业务状态 |
| `src-tauri/src/services/collectors/orchestration.rs` | collector workflow；不解析 provider payload |
| `src-tauri/src/services/collectors/drivers/**` | provider-specific auth/client/parser/mapping 和 capability implementation |
| `src/app/bootstrap/**` | runtime mode、handshake、backend client 的唯一 composition root |
| `src/demo.tsx`, `demo.html`, `vite.demo.config.ts` | 独立 browser preview/test composition；production build 不可达 |
| `src/lib/bridge/generated.ts` | 由 Rust 契约确定性生成；禁止手改 |
| `src/lib/bridge/DesktopBackend.ts` | generated binding 和 streaming adapter 的 desktop 实现 |
| `src/lib/bridge/DemoBackend.ts` | 确定性、隔离的 preview 实现；不访问真实系统能力 |
| `src/features/*/api.ts` | 领域 client 合同或必要 transport policy；不直接 invoke |
| `src/features/*/queries.ts` | canonical keys、query options、workspace read models |
| `src/features/*/mutations.ts` | authoritative cache transition/invalidation |
| `src/features/*/pages/**` | layout 和 feature composition，不拥有 transport/stream parser |
| `scripts/architecture/**` | parser-backed dependency/registry/artifact gates |
| `scripts/verify.ps1` | PR 和 release 共用的 fail-closed 验证入口 |
| `src-tauri/tauri.conf.json`, `src-tauri/capabilities/**` | production CSP、window/capability least privilege；由 compiled registry/security gate 校验 |

## 6. 共享类型与所有权台账

| 类型/合同 | 引入 Task | 唯一 owner |
|---|---:|---|
| `CommandError`, `CommandErrorCode`, `PublicErrorDetails` | 4 | `commands/error.rs` / IPC contract module |
| `RuntimeContractInfo`, contract hash/version | 4 | Rust binding registry |
| generated command groups | 3-6 | Rust registry + generator |
| `BackendError` | 4 | `src/lib/bridge/errors.ts` |
| `BackendMode`, `BackendClient` | 8 | app bootstrap/bridge |
| domain frontend clients | 8-9 | matching feature `api.ts` |
| narrow command facades | 7 | `application/command_facades/**` |
| canonical query keys/options | 10-13 | matching feature `queries.ts` |
| `PageVisibility`, retention policy | 10 | `ShellPageHost` / navigation host |
| `TaskSpec`, `TaskStatus`, `RestartPolicy` | 14 | `background_tasks/task.rs` |
| `OperationSpec`, `OperationTerminal::ResultUnknown` | 15 | `background_tasks/operation.rs` |
| `BlockingExecutor` | 14 | `background_tasks/blocking.rs` |
| `RequestBudget`, `OutboundFailure` | 14 | `outbound/**` |
| `ExitCoordinator`, `ShutdownReport` | 17 | `background_tasks/exit.rs`, `shutdown.rs` |
| `CorrelationId`, local metric schemas, redaction contract | 4, 18 | `observability/**` |
| `RuntimeTaskSummary` | 18 | `background_tasks/status.rs` + narrow runtime status facade |
| `ProviderKind`, `ProviderEntry` | 19 | `services/collectors/drivers/mod.rs` |
| capability traits | 19 | matching capability contract module |
| `DriverFailure`, evidence | 19 | collector contract/failure modules |

## 7. 程序级退出门禁

| 合同 | 主任务 | 必需证据 |
|---|---:|---|
| Desktop 不会 fallback 成模拟成功 | 8-9 | bootstrap/component/Tauri smoke failure matrix |
| IPC 跨语言契约唯一 | 3-6 | deterministic generation、serialization fixture、zero diff |
| command 依赖半径受控 | 7, 23 | AST graph、无 `State<AppServices>`、facade fan-out ledger |
| server state 单一 owner | 10-13 | cache transition tests、无长期副本/DOM data event |
| 列表刷新 O(1) command 数 | 11-12, 26 | 固定规模 command-count/payload benchmark |
| hidden page 主动 query 为 0 | 10-13, 26 | lifecycle component test 和 runtime metric |
| daemon 可 cancel/join/backoff | 14, 16-17 | state-machine、fault、shutdown tests |
| foreground operation 唯一终态 | 15 | cancel/detach/timeout/commit-barrier matrix |
| blocking work 有界 | 14, 17 | saturation、queue timeout、orphan/shutdown diagnostics |
| provider 网络异步且有 budget | 14, 19-22 | outbound policy fixtures、cancellation、client reuse |
| provider 扩展局部化 | 19-22 | conformance suite、registry gate、无字符串 dispatcher |
| production `ureq` 为零 | 22 | dependency removal、AST import/construction gate |
| correlation/diagnostic 可用且不泄密 | 4, 18 | continuity、canary redaction、bounded-cardinality tests |
| Tauri/WebView 最小权限 | 0, 2, 8, 17 | threat model、CSP、compiled ACL/capability、exact-origin、bundle graph |
| work lifecycle 不自造 runtime | 1, 14-17 | ADR primitive choice、no custom executor/thread pool、ExitCoordinator matrix |
| PR/release 验证同源 | 2, 28 | shared entrypoint、workflow contract test |
| 产物不污染源码/index | 2, 25, 28 | ignore parity、artifact provenance、clean source scan |

### 7.1 当前热点到迁移 Task 的映射

以下是 2026-07-22 审计快照，不作为永久硬编码计数。Task 0 必须用当前 revision 重算并覆盖 inventory；此表用于防止大问题在执行中失踪。

| 当前热点 | 当前问题 | 主要迁移 Task | 最终删除/验收 Task |
|---|---|---:|---:|
| `src-tauri/src/commands/mod.rs`（约 4180 行） | transport、connectivity、SSE、HTTP、blocking 和业务编排混合 | 3-7, 15 | 23 |
| 约 103 个 `State<AppServices>` command signature | runtime service locator、依赖半径不可见 | 7 | 23/26 |
| 约 14 个 API 文件、118 个 invoke-unavailable 分支 | production failure 静默回退 mock/default | 8-9 | 25/26 |
| `KeyPoolPage.tsx` | Query、本地 server-state 副本、event/manual refresh 多 owner | 10-11 | 24/26 |
| `StationsPage.tsx` 的 per-station `useQueries` | IPC/query fan-out 随站点数增长 | 11 | 24/26 |
| `ShellPageHost` + `PageActivity` 多活跃信号 | hidden 页面仍可能刷新或响应副作用 | 10 | 25/26 |
| station/channel runners | 自有 thread/atomic stop/block_on，shutdown 语义分散 | 14, 16-17 | 18/27 |
| connectivity/scan/capture 长操作 | 前端 run token 不等于后端取消 | 6, 15, 17 | 18/27 |
| 约 12 个 production Rust 文件引用 `ureq` | 同步网络、blocking 扩散、policy 分裂 | 14-22 | 22/26 |
| Sub2API/NewAPI 巨型 adapter | auth/client/parser/mapping/orchestration 耦合 | 19-21 | 25/26 |
| remote-key/auth provider string dispatch | 新 provider 需要修改多个无关 service | 19-22 | 22/26 |
| 167 个 `.test.mjs` 中大量源码文本断言 | 结构门禁可能产生假阳性/假安全 | 2 | 25/26 |
| 只有 tag release workflow | PR 阶段不能 fail closed | 2 | 28 |
| 分散 target/output 目录 | watcher/index/provenance 污染 | 2 | 25/28 |
| `tauri.conf.json` 当前 `csp: null` | main WebView 缺少 production content boundary | 0, 2, 8 | 26/28 |
| `capture.json` remote shell 覆盖 `http://*`/`https://*` | 必须依靠 window/station/revision/exact-origin 二次校验和最小 command | 0, 17 | 26/28 |
| tray/window 直接 `app.exit`，主要 shutdown 在 `RunEvent::Exit + block_on` | async drain 过晚、多个退出源可能重复或漏关 | 1, 17 | 27/28 |

### 7.2 Design spec 到实施 Task 的追踪矩阵

Task 0 复核本矩阵；任何 design requirement 没有 implementation owner、test owner 和 delete/qualification owner 时，Stage 0 不得通过。

| Design spec 章节 | Implementation owner | 验证/删除 owner |
|---|---|---|
| §6 模块边界与 composition root | 7-9, 14, 19 | 23-26 |
| §7 Typed IPC、公开错误、registry/ACL、handshake | 3-6 | 26, 28 |
| §8 Desktop/Demo backend | 8-9 | 25-26 |
| §9 Query ownership、aggregate、PageVisibility | 10-13 | 24-26 |
| §10 TaskSupervisor、OperationRegistry、BlockingExecutor | 14-18 | 26-27 |
| §11 Provider drivers、registry、async outbound、connectivity | 14-22 | 25-27 |
| §12 command/application boundary | 7, 15 | 23, 26 |
| §13 correlation、tracing、本地指标与脱敏 | 4, 18 | 26-28 |
| §14 parser-backed gates、CI、artifact hygiene | 2 | 25-28 |
| §3.4/§14.6 Tauri/WebView/secret 安全边界 | 0, 2, 8, 14, 17, 19 | 26-28 |
| §5.6 成熟基础设施复用 | 1, 2, 14 | 18, 26-28 |
| §15 性能与容量合同 | 1, 10-22 | 26-27 |
| §18 回滚与提交策略 | 所有 Task/shard | 17-18 节 Checkpoint 审计 |
| §19 风险缓解与 §20 禁止反模式 | 0-2 architecture ledger | 25-28 零例外审计 |

## 8. Stage 0：冻结边界、预算与可信门禁

### Task 0：建立隔离工作树和可复核基线

**文件：**

- Create: `docs/superpowers/audits/2026-07-22-architecture-scale-upgrade-baseline.md`
- Create: `docs/superpowers/audits/architecture-scale-upgrade-inventory.json`
- Create: `docs/superpowers/audits/architecture-scale-boundary-manifest.json`
- Create: `docs/superpowers/audits/architecture-scale-threat-model.md`
- Create: `docs/superpowers/audits/architecture-scale-tauri-security-manifest.json`
- Create: `docs/superpowers/audits/architecture-scale-dependency-lifecycle.json`
- Read only: `docs/superpowers/specs/2026-07-22-architecture-scale-upgrade-design.md`
- Read only: 当前 production/frontend/runtime/provider 文件

- [x] **Step 1：确认 Persistence V2 稳定基点**

记录主 checkout dirty paths 和 stable base commit。任何 persistence 未提交文件只记路径，不读取后顺手修改。创建 worktree 后证明工作树不包含这些 dirty hunks。

- [x] **Step 2：生成机器可读 inventory**

inventory 至少列出 command、公开错误、DTO、command registry/ACL/capability/window、CSP/build entry、`State<AppServices>`、feature direct invoke、fallback、DOM data event、`useQueries` fan-out、runner/thread、spawn、exit source、operation-like flow、`ureq`/HTTP client construction、provider string dispatch、secret-bearing type、source-regex tests、target/output 目录。每项包含 owner、callers、迁移 Task、删除 Task、例外理由和证据命令。

inventory 使用稳定顶层字段，后续 gate 不得各自发明清单：

```json
{
  "schema_version": 1,
  "source_revision": "<git sha>",
  "inventories": {},
  "execution_shards": [],
  "temporary_adapters": [],
  "temporary_architecture_allowlist": [],
  "behavior_baselines": [],
  "performance_baselines": [],
  "spec_traceability": []
}
```

`execution_shards` 每项必须包含 `id`、`stage`、`paths`、`owner`、`depends_on`、`cutover`、`delete_paths`、`focused_gates` 和 `rollback_revision`；`temporary_*` 每项必须有引入 shard、删除 shard、owner 和过期 Stage，禁止无期限例外。

boundary manifest 是 parser/compiled gate 的唯一 allowlist owner，至少包含：`allowed_exports`、`allowed_edges`、`forbidden_edges`、`temporary_edges`、`command_state_allowlist`、`spawn_allowlist`、`http_client_construction_allowlist`、`fan_in_baseline` 和 `fan_out_baseline`。每条 temporary entry 包含 source/target symbol identity、reason、owner、introduced shard、delete shard 和 expiry stage；只写文件名或自由文本不算有效 identity。

Tauri security manifest 至少包含 production/dev/preview config paths、CSP、window label pattern、local/remote capability、command permission、allowed remote URL shell、application exact-origin validator、external navigation owner、demo entry reachability、owner 和 expiry shard。当前 `csp: null` 与宽 remote capture URL 必须显式记录风险；宽 URL shell 只有在 application 二次校验存在时才可暂留，production `csp: null` 必须在 Task 8 消除。

- [x] **Step 3：跑 baseline**

```powershell
pnpm.cmd install --frozen-lockfile
pnpm.cmd test:contracts
pnpm.cmd test
pnpm.cmd build
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

记录精确 revision、profile、通过/失败数、耗时和既有失败。未分类的红 baseline 阻止 Task 1。

- [x] **Step 4：冻结行为样本**

保存 Desktop read/write/error、station/key CRUD、collector facts、monitor state、proxy startup/drain、page navigation、demo/browser preview 的最小 characterization 清单。所有样本只存脱敏数据。

Threat model 至少覆盖恶意/失陷 provider、lookalike/cross-station origin、redirect/header 泄露、remote capture window IPC abuse、main renderer compromise、stale WebView assets、secret 出现在日志/fixture/bundle、update/exit race、重复 remote create 和强制终止。每项记录 asset、trust boundary、abuse case、现有控制、目标控制、owner shard 和验证证据；不扩张到账号/云平台威胁。

- [x] **Step 5：冻结依赖生命周期基线**

基于官方支持政策、安全公告和实际 lockfile，记录当前 React 18、Vite 6、Tauri 2、TanStack Query 5、Tokio 1、reqwest、Axum、SQLx、Rust toolchain/edition、Node/pnpm 以及 binding/architecture build tools。每项包含 resolved version、support/EOL 状态、advisory、MSRV/Node/Windows/Tauri 兼容性、source URL/check date、keep/upgrade/block 决策、owner、prerequisite shard 和下次复查日期。未知支持状态不能默认判定安全；大版本升级不与架构 owner cutover 混合，但 unsupported/EOL 或不可接受高危风险必须在受影响 Stage 前完成独立升级。

- [x] **Step 6：冻结性能、容量与生命周期基线**

在同一 release/debug profile 分别记录 10/100/500 数据集的 IPC count、backend query/read-port count、payload bytes、query/page commit duration；记录 runner/thread/spawn、HTTP client、operation-like work、页面 observer/listener 和 idle/shutdown 状态。每条 baseline 包含 dataset hash、binary/source revision、profile、machine fingerprint、warm-up/sample 方法和原始结果路径，不用单次耗时下结论。

Stage 0 必须实际生成固定 seed、稳定排序、脱敏的 10/100/500 stations/keys fixture，并取得 frontend invoke count、projected response JSON bytes、TanStack Query lifecycle 和 data-ready React commit 的原始样本。当前没有 runtime measurement owner 的 `backend_read_port_round_trips`、`backend_sql_statement_count_runtime`、`backend_query_duration_ms`、`sqlite_query_plan`、`real_tauri_ipc_payload_bytes`、`real_tauri_command_duration_ms` 和 `webview2_page_commit_ms` 必须写成 `value: null, qualification: blocked`，分别登记 owner Task 11 和 release gate Task 26；禁止填 0 或用源码静态计数冒充实测。Stage 0 可在这些 blocker 有唯一 owner/关闭 Task 时建立基线，但 Stage 3/7 对应 Gate 在 blocker 关闭前不得通过。

**退出：** inventory 无 `unknown owner`；所有迁移对象都有删除 Task；baseline 可复现；Persistence V2 文件零改动。

### Task 1：冻结七个 ADR 和容量预算

**文件：**

- Create: `docs/superpowers/adrs/architecture-scale/0001-ipc-contract-and-error.md`
- Create: `docs/superpowers/adrs/architecture-scale/0002-application-composition.md`
- Create: `docs/superpowers/adrs/architecture-scale/0003-backend-mode.md`
- Create: `docs/superpowers/adrs/architecture-scale/0004-query-and-page-visibility.md`
- Create: `docs/superpowers/adrs/architecture-scale/0005-work-lifecycle.md`
- Create: `docs/superpowers/adrs/architecture-scale/0006-provider-registry-and-outbound.md`
- Create: `docs/superpowers/adrs/architecture-scale/0007-ci-and-artifacts.md`
- Create: `docs/superpowers/audits/architecture-scale-capacity-budgets.json`

- [x] 对 specta/tauri-specta 的 command、serde rename、enum、nullable、Channel 支持做最小 spike；同时记录维护活跃度、最近兼容 release、Tauri 2/Rust MSRV、Windows CI、deterministic output 和 transitive dependency 风险。只有全部通过才选用并固定版本；否则使用窄 repository build-time generator，runtime/domain contract 不依赖具体工具。
- [x] 冻结 command timeout、aggregate page limit/payload limit、Query stale/gc policy 与 cache memory budget、retained-page 最大数、task queue、operation queue、operation progress ring、terminal TTL/最大保留数/GC 周期、blocking queue、provider endpoint fan-out、outbound body size、diagnostic buffer/log rotation、per-kind shutdown timeout、global shutdown budget 的具体数值和 owner。
- [x] budget ledger 定义父子预算传播和不变量：子步骤只消费 remaining budget，所有 per-kind shutdown 等待受 global deadline 截断，queue wait + execution + retry/fallback 不得各自重新取得完整 timeout。
- [x] 冻结性能资格方法：10/100/500 固定数据集 hash、warm-up 次数、sample 数、同机/同 profile 要求、p50/p95 统计、绝对 SLO、允许的相对回归阈值、soak 时长和内存/handle/client/listener 增长上限。单次最快值和跨机器比较不能作为 gate。
- [x] ADR 明确 `AppServices` 只可作为 construction bundle，command facade 不是全方法转发层。
- [x] work-lifecycle ADR 在 `TaskTracker` 与 bounded `JoinSet` 中选定一个主要 join owner，证明 panic/join error、close/wait、admission race、重复 shutdown 和 Tauri lifecycle 集成；禁止长期两套并存或自定义 executor/thread pool。
- [x] Backend mode ADR 明确 production Tauri entry 与 browser preview/test Vite entry 物理分离、production build 不可达 demo、DemoBackend unsupported policy、deterministic clock/data、reset 行为和可见标识。
- [x] ADR 明确 source-regex test 的淘汰范围、parser/ESLint/cargo-deny 等 CI 工具的固定版本与安装校验、advisory 例外格式和 artifact root；CI 禁止无版本 `cargo install` 或下载 latest。

**退出：** 所有“有界”“及时”“较大”都已变成可执行数值或清楚的测量公式；ADR 不放宽 spec 禁止项。

### Task 2：建立 parser-backed fitness、PR CI 和统一验证入口

**文件：**

- Create: `src-tauri/tests/architecture_scale_boundaries.rs`（`syn` visitor）
- Create: `src-tauri/tests/fixtures/architecture_scale/**`
- Create: `scripts/architecture/check-typescript-boundaries.mjs`
- Create: `scripts/architecture/fixtures/typescript/**`
- Create: `scripts/architecture/check-command-registry.mjs`
- Create: `scripts/architecture/check-artifact-policy.mjs`
- Create: `scripts/architecture/check-tauri-security.mjs`
- Create: `scripts/architecture/check-build-entries.mjs`
- Create: `scripts/verify.ps1`
- Create: `scripts/check-advisories.ps1`
- Create: `scripts/architecture/check-dependency-lifecycle.mjs`
- Create: `deny.toml`
- Create: `docs/superpowers/audits/dependency-advisory-exceptions.json`
- Create: `eslint.config.mjs`
- Create: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Modify: `.gitignore`
- Modify: `vite.config.ts`（仅统一 `output/**` watcher ignore；不得夹带构建重构）

- [x] Rust gate 使用 `cargo metadata`/目标 cfg + `syn` module visitor，RED fixtures 必须覆盖 qualified/ordinary path、`use foo::{self,*}`、alias、re-export、inline/out-of-line module、nested module、`cfg`/`cfg_attr` target semantics、macro/generated registry、same-name symbols、descendant fan-out、forbidden public export 和 dependency cycle。无法可靠解析的 construct 必须 fail closed 或进入有 owner/expiry 的精确例外，不能静默跳过。
- [x] TypeScript gate 用真实 project `tsconfig` 创建 Compiler Program，RED fixtures 覆盖 path alias、barrel/re-export、type-only import、dynamic import、同名 symbol、跨 feature descendant import 和循环依赖；不以字符串包含关系推断 ownership。
- [x] 能由 ESLint `no-restricted-imports`/标准规则表达的直接 import 边界优先用 ESLint；Compiler API 只负责标准规则无法表达的 resolved graph、barrel/dynamic/fan-out 约束，禁止维护两份冲突规则。
- [x] command registration/ACL/binding 一致性使用编译后的 registry/serialization fixture 校验，不能让 `syn` 猜 `generate_handler!` 宏展开。
- [x] 建立初始 allowlist：旧债可暂时存在但数量只能下降；新文件/新 edge 不得加入旧式模式。
- [x] gate 对 stale allowlist fail closed：manifest entry 没有匹配真实 edge、已过 expiry stage、symbol identity 变更或 fan-in/fan-out baseline 为空都失败；删除旧债时必须同步删除例外。
- [x] 建立 `pnpm lint`，覆盖 production TypeScript/TSX 和 architecture-specific rules；lint 配置缺失或只扫描部分 feature 时 fail closed。
- [x] `scripts/verify.ps1` 提供 fast、full、release profile；相同底层命令由 PR 和 release workflow 调用，workflow 不复制业务验证列表。
- [x] CI 安装严格使用 `pnpm install --frozen-lockfile`；Cargo check/clippy/test/build 使用 `--locked`。lockfile 漂移、缺失或安装脚本绕过 frozen/locked 都必须失败。
- [x] PR 至少执行 binding/check placeholder、frontend lint/test/build、Rust fmt/clippy/check/test、architecture gates 和 dependency advisory/license/source gate；缺少脚本或工具时 fail closed。
- [x] `dependency-advisory-exceptions.json` 的每个例外必须包含 ecosystem、package、advisory id、受影响判断、owner、批准日期和到期日期；未知字段、过期例外或全局 ignore 均失败。
- [x] dependency lifecycle gate 校验台账覆盖实际 lockfile/toolchain 中的关键组件、复查日期未过期、source 可追溯且没有 unresolved/unsupported/EOL/block 状态；版本 prerequisite shard 必须拥有独立 compatibility matrix、rollback 和 qualification，不能借架构测试代替 major upgrade 验收。
- [x] 建立 10/100/500 deterministic frontend scale baseline：fixture 双次生成 hash 一致，mock IPC 记录 command/count/projected response JSON bytes，独立 QueryClient 记录 lifecycle，React Profiler/浏览器时间线记录 data-ready commit；warm-up/sample/provenance 完整。未具备 backend/Tauri/WebView runtime owner 的指标保持 typed blocked，并由 Task 11/26 关闭。
- [x] release 在同一入口上增加 `--locked` release build、Tauri bundle、签名、artifact scan 和 provenance。
- [x] 统一 `output/<purpose>`，让 Git、Vite watch、CodeGraph、test discovery 同步排除；检查现存 source-tree target/output 只登记，不在脏主 checkout 删除。
- [x] Tauri security gate 解析 production/dev/preview config、compiled command registry、capability manifests 和 window patterns；新增 `csp: null`、capture 调用 main command、registered/authorized mismatch、production demo entry 可达或无 application exact-origin owner 均失败。当前债务只能使用 security manifest 中有 owner/expiry 的临时 entry。

**退出：** 新增旧模式会让 CI 红；PR workflow 已实际可运行；release 调用相同 entrypoint；不依赖结构 regex。

**Stage 0 Gate：** Task 0-2 证据和 docs/scripts 独立提交；production behavior 未改变；从此所有新功能必须遵守目标边界。Threat/security manifest 完整，现有 `csp: null`、capture remote shell、direct exit/block_on 和 demo/mock 路径均有精确 owner/expiry shard，新增或扩大立即失败。Generator、Tokio join owner 和 build entry 的成熟度 ADR 已冻结；未完成 spike 不得进入实现。Dependency lifecycle ledger 覆盖实际 resolved versions；unsupported/EOL、不可接受高危风险或未知关键支持状态已转成有 owner 的 prerequisite blocker，不能带入受影响 Stage。10/100/500 frontend baseline 的 fixture hash和原始样本必须存在；backend/Tauri/WebView 指标允许暂为 typed blocked，但必须有 Task 11/26 owner，且不得被表述为性能通过。

## 9. Stage 1：Typed IPC、统一错误与运行时握手

### Task 3：固定 binding generator 和 command registry

**文件：**

- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- Create: `src-tauri/src/ipc/mod.rs`
- Create: `src-tauri/src/ipc/registry.rs`
- Create: `src-tauri/src/ipc/dto/**`
- Create: `src/lib/bridge/generated.ts`
- Create: `scripts/generate-bindings.mjs`（固定的 repository-owned generator wrapper）
- Modify: `package.json`

- [x] 以 settings/stations 的只读 command 建立 RED generation/serialization fixture。
- [x] Rust registry 同时驱动普通 command binding、Tauri registration ledger 和 ACL consistency manifest；禁止维护三份手写名字。
- [x] generated 文件头包含 generator version、IPC contract version 和 canonical hash。
- [x] ADR/registry 定义 additive/breaking contract change policy：command/input/output/error/event schema 的 breaking change 必须新增 versioned command/event 或提升 IPC contract version；只改 doc/comment 不得改变 canonical hash。删除旧 version 只能在调用者/ACL/asset inventory 为零后执行。
- [x] `pnpm generate:bindings` 只生成确定性内容；`pnpm generate:bindings --check` 在生成后要求零 diff。
- [x] Channel 暂不能生成时，只登记一个 typed streaming adapter surface，不能因此放弃普通 command 生成。

**Cutover：** 只有试点 command 的 frontend 调用切到 generated binding；其他 command 仍保持旧 transport，但不得 fallback 到试点旧路径。

**退出：** 连续生成两次 hash 相同；registry/ACL/binding 缺任一项均测试失败。

### Task 4：建立公开错误、frontend BackendError 和 runtime handshake

**文件：**

- Create: `src-tauri/src/commands/error.rs`
- Create: `src-tauri/src/ipc/runtime_contract.rs`
- Create: `src/lib/bridge/errors.ts`
- Create: `src/app/bootstrap/runtimeContract.ts`

- [ ] 按 spec 实现封闭 `CommandErrorCode`、`PublicErrorDetails` 和构造 invariant；未知 internal error 默认不可重试且脱敏。
- [ ] CommandError constructor 对 message、field errors、provider/status details 和 correlation id 设置长度/数量上限；无约束 JSON、nested error chain、完整 URL/query/path 和 secret-bearing source 永远不能进入 IPC。
- [ ] 建立 application/driver/outbound/work error 到 public error 的单向映射测试；command 不做补偿。
- [ ] frontend 只按 `code`/typed details 决策，message 仅展示；加入未知 code 和恶意 details fixture。
- [ ] Rust `get_runtime_contract_info` 只返回 app version、contract version、binding hash 和 capabilities；frontend `runtimeContract.ts` 是纯校验器，不读取 window/env、不发第二个业务 command。
- [ ] 纯 validator/serialization tests 覆盖 match、version/hash mismatch、unknown capability、恶意/超大 payload；UI 启动状态机和 recovery screen 延后到 Task 8，避免在 backend mode owner 建立前引入隐式模式探测。
- [ ] correlation id 跨 command/application/outbound span 传播，公开错误只暴露短安全标识。

**退出：** 错误 envelope 的 nullable/enum/rename/redaction fixtures 双端一致；runtime contract command/binding/validator 可独立验证，但尚不改变 `main.tsx -> DataStoreBootstrap -> App` production startup chain。

### Task 5：迁移普通 read/mutation command groups

**顺序：** settings/stations -> station keys -> changes/logs -> collectors -> routing/proxy -> updater/data recovery。

**文件：**

- Modify: matching Rust command/DTO modules
- Modify: matching generated binding output
- Modify: exact legacy transport wrappers under `src/lib/api/**` and `src/lib/queries/**` listed by Task 0; feature components are read-only in this Task
- Modify: ACL/capability manifests

每个 command group 执行：

- [ ] 记录输入/output/error/ACL/调用者和 mutation idempotency 分类。
- [ ] 先为 serialization、typed failure、registration 写 RED test。
- [ ] 每个 command group 冻结 transport validation：字符串/数组/page/body 上限、enum/URL/ID 语法和未知字段策略；Rust 在进入 application use case 前拒绝超限/畸形输入，TypeScript 类型不能替代 runtime validation。
- [ ] read command 迁移后再迁 mutation；mutation transport 不自动 retry。
- [ ] `IdempotentWithKey` 在一次用户意图中复用 operation key，由 application owner 去重；重新点击生成新 key。
- [ ] `NonIdempotent` 丢失响应返回 result unknown，不通过重复 invoke 猜测。
- [ ] 删除该 group 的 TypeScript 手写 DTO/command string/error-message parser。
- [ ] generated binding 暂时只能被 legacy transport wrapper 调用；`features/**` 不得直接 import `bridge/generated.ts`。Task 8/9 建立 domain client 后再迁 feature，避免 Stage 1/2 两次改同一组件调用面。
- [ ] 运行 group focused tests、generator drift 和 Tauri smoke。

**Cutover：** 以 command group 为单位，在现有 API wrapper 后一次只保留 generated 调用为 production transport path。旧 command 若暂留，只能作为同一 application use case 的版本 adapter，并登记 Task 6/9 删除。

**退出：** 所有非 streaming production command 均由 Rust 生成调用和类型；public `Result<T, String>` 为零。

### Task 6：迁移 streaming Channel 合同并删除 IPC 双轨

**文件：**

- Create: `src/lib/bridge/streamingAdapter.ts`
- Modify: connectivity/capture/scan streaming commands and controllers
- Modify: registry/ACL/architecture manifests
- Delete: obsolete handwritten invoke wrappers and error parsers

- [ ] 为 event envelope version、transport/run id、sequence/progress、显式 terminal 和 channel-close-without-terminal 写 RED tests。
- [ ] generated 能覆盖 Channel 时直接生成；不能覆盖时，手写 adapter 只能接收 generated input/output/event DTO。
- [ ] unknown event version 终止本次 stream 并返回 incompatible error。
- [ ] Channel close 不等于 completed。若现有实现没有真实 backend cancellation，合同必须显式标记不可取消，UI 只能 detach，不能展示 `Cancelled`；Task 15 再切换到 `OperationRegistry` 的 operation id、cancel 和 exactly-one-terminal 合同。
- [ ] 删除 command-not-found 文本探测和版本 fallback。

**Stage 1 Gate：** 全部 production commands typed；binding/registry/ACL/handshake 一致；前端不解析错误字符串；streaming 的当前可取消能力表达真实；临时 IPC adapter 有明确删除 owner。

## 10. Stage 2：窄 Command Facade、显式 Backend 与 mock 隔离

### Task 7：建立窄 command facade 和原子 composition

**文件：**

- Create: `src-tauri/src/application/command_facades/mod.rs`
- Create: matching domain facade files
- Modify: `src-tauri/src/app_composition.rs`
- Modify: `src-tauri/src/runtime_composition.rs`
- Modify: `src-tauri/src/application/app_services.rs`
- Modify: Rust commands incrementally

- [ ] 从 inventory 生成 command-to-use-case-to-dependency matrix，先写 AST gate：command 只能注入 allowlist 中一个领域 facade/state。
- [ ] 已经单领域且接口合适的 application service 可直接注册；只有真实跨 service use case 才建 facade。
- [ ] facade 只固定最小依赖并提供 use-case 方法，不公开字段、不复制业务、不镜像全部 service 方法。
- [ ] composition 先 preflight 所有 concrete Tauri state slot，再原子 register；失败不能留下半注册 runtime。
- [ ] `AppServices` 若保留，只在 construction 期间使用，不作为 Tauri managed state。
- [ ] 按 Task 5 相同 group 顺序迁 command，逐组删除 `State<AppServices>`。

**Cutover：** 每个 command group 注册窄 state 后立即删除该组对 `AppServices` 的 runtime access；不得同时注入二者。

**退出：** production command 中 `State<AppServices>` 为零；新增 facade 不扩大无关 command transitive fan-out。

### Task 8：建立显式 DesktopBackend、DemoBackend 和领域 client

**文件：**

- Create: `src/app/bootstrap/backendMode.ts`
- Create: `src/app/bootstrap/createBackendClient.ts`
- Create: `src/lib/bridge/BackendClient.ts`
- Create: `src/lib/bridge/DesktopBackend.ts`
- Create: `src/lib/bridge/DemoBackend.ts`
- Create: `src/app/bootstrap/BackendBootstrap.tsx`
- Create: `src/app/IncompatibleRuntimeScreen.tsx`
- Create: `src/demo.tsx`, `demo.html`, `vite.demo.config.ts`（独立 preview/test entry）
- Modify: `src/main.tsx`
- Modify: `vite.config.ts`, `package.json`
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`
- Create/Modify: `src/features/*/api.ts`

- [x] bootstrap 只选择一次 `desktop | demo`：打包的 Tauri entry 固定请求 desktop handshake，显式 browser-preview/test entry 才注入 demo；未知/缺失 mode fail closed。mode 不由任意 invoke error、command-not-found、`window.__TAURI__` 或 feature 环境变量运行时猜测。
- [x] `BackendBootstrap` 使用显式状态机：`SelectingMode -> HandshakingDesktop -> DataStoreBootstrapping -> Ready`，或 `SelectingMode -> DemoReady`。desktop 必须 handshake 成功后才挂载 `DataStoreBootstrap`；demo 不挂载 `DataStoreBootstrap`，不调用任何 data-recovery/真实系统 command。
- [x] desktop contract mismatch 进入 `IncompatibleRuntimeScreen`，普通 runtime unavailable 进入可重试 fatal recovery；二者都不创建 DemoBackend、不挂载业务页面。重试复用同一 fixed desktop mode，不重新猜 mode。
- [x] `src/main.tsx` 只组合 theme/query/toast/backend bootstrap providers，不直接调用 command 或读取 backend mode；App/feature 通过 typed provider 获得窄 domain client。
- [x] `pnpm build` 只构建 production desktop entry，`pnpm build:demo`/`dev:demo` 使用独立 config/HTML；production `dist`/Tauri bundle scan 不含 demo bootstrap marker、fixture dataset 或可触发 demo 的 query/env/localStorage branch。
- [x] 按 threat model 为 production `tauri.conf.json` 建立非空 CSP，默认禁止 remote script、`unsafe-eval` 和 main-window remote navigation；dev/demo 需要的放宽只存在独立 Vite/Tauri test config，不进入 release provenance。
- [x] main capability 保持 least privilege，compiled registry/ACL/capability gate 证明 main command 授权精确；Task 17.B 再收紧 `capture-*` remote capability，不在此处把 capture 权限并入 main。
- [x] DesktopBackend 只调用 generated binding/stream adapter，不把 transport failure 变成默认值。
- [x] DemoBackend 使用固定 clock、seed 和 resettable store；真实 keyring/database/file/network/updater 全部不可达。
- [x] demo mode 在 shell/bootstrap 上有稳定可测试的可见标识和显式 reset command；Desktop handshake 失败时该标识绝不能出现。
- [x] 不支持的 demo capability 返回 typed `Unsupported`；不得复制真实 provider 登录、采集或路由决策。
- [x] architecture gate 禁止 DemoBackend/demo fixtures import DesktopBackend、generated invoke、Tauri API、credential/data-recovery/network/file adapter；contract test 以 fail-fast spies 证明 demo workflow 零真实调用。
- [x] feature 只接收 matching domain client/query hook；完整 `BackendClient` 只在 bootstrap composition 出现。
- [x] contract tests 对同一领域 client 验证成功/typed failure/unsupported shape。

**退出：** architecture gate 阻止 feature import Tauri core、完整 BackendClient 和 desktop binding。

### Task 9：按 feature 切换 backend 并删除业务 fallback

**顺序：** settings/stations -> key pool -> changes/logs -> collectors -> routing/proxy -> updater/data recovery -> pricing/economics/channel monitoring。

**当前 checkpoint：** settings/stations、key-pool/stationKeys、changeEvents/collectorRuns、external URL、proxy/localRouting、dataRecovery、economics、groupFacts、pricing workspace、routing/health、channel monitoring/status、collectors 与 updater 子切片已建立 `DesktopBackend` domain client；legacy `src/lib/api/settings.ts`、`src/lib/api/stations.ts`、`src/lib/api/stationKeys.ts`、`src/lib/api/changeEvents.ts`、`src/lib/api/collectorRuns.ts`、`src/lib/api/external.ts`、`src/lib/api/proxy.ts`、`src/lib/api/localRouting.ts`、`src/lib/api/dataRecovery.ts`、`src/lib/api/economics.ts`、`src/lib/api/groupFacts.ts`、`src/lib/queries/pricingQueries.ts`、`src/lib/api/routing.ts`、`src/lib/api/channelMonitors.ts`、`src/lib/queries/channelQueries.ts`、`src/lib/api/collector.ts` 与 `src/lib/api/updater.ts` 已删除 `isTauriInvokeUnavailable`、browser-preview memory fallback、内存 production state 和直接 generated/transport/streaming/native updater adapter import；data-recovery feature 的 restart/error display 也改走 backend client/pure error formatter，不再直接 import Tauri；DemoBackend 对真实桌面能力返回 typed unsupported，station-key connectivity 在 demo 下不得 fake success。`src/features`、`src/lib/api` 与 `src/lib/queries` production direct Tauri/generated/fallback surface 扫描为空；Stage 2 Gate 在 2026-07-25 closeout 审计通过。

**文件：**

- Modify: `src/lib/bridge/**`, `src/app/bootstrap/**` only for composition contracts
- Modify: exact `src/features/**` domain client/consumer paths in Task 0 feature shards
- Delete/Modify: exact legacy `src/lib/api/**`, `src/lib/queries/**`, mock store and fallback paths mapped to each feature shard
- Modify: colocated bridge/feature component tests and architecture inventory

每个 feature：

- [x] 写 Desktop failure 和 Demo unsupported RED component test。
- [x] 注入窄 client，保持现有 loading/error/partial 体验。
- [x] 删除 `isTauriInvokeUnavailable`、command-not-found 业务 fallback、内存 production state 和 mock success。
- [x] 删除 feature 直接 `invoke` 与手写 command name。
- [x] 验证 desktop runtime unavailable 显示 recovery/error，不显示空成功。
- [x] 验证 demo 不调用任何真实 adapter。

**Stage 2 Gate：** runtime mode 只有 bootstrap owner；production API 隐式 fallback 为零；feature 不持有完整 BackendClient；command 不持有完整 AppServices。Production/demo build entry 物理分离，packaged bundle 不可达 DemoBackend，production CSP 非空且 main capability 通过 least-privilege/registry gate。

**Gate evidence（2026-07-25）：** `pnpm.cmd exec vitest run ...` 覆盖 16 个 Task 9 API/query/bootstrap/demo tests（31 tests）；`pnpm.cmd run build` 通过 `theme:audit && tsc --noEmit && vite build`；`pnpm.cmd run architecture:typescript` 通过 904 resolved edges；`pnpm.cmd run architecture:security` 通过 2 capabilities 和 416 production / 245 demo modules；`node scripts/architecture/check-command-state-boundaries.mjs` 通过 103 migrated commands；updater 四个 source-contract scripts 通过；`cargo check --manifest-path src-tauri\Cargo.toml` 使用 `output/cargo/stage2-gate` 通过；`rg` 扫描确认 `src/features`、`src/lib/api`、`src/lib/queries` 无 production direct Tauri/generated/fallback surface；Persistence V2 protected paths 零 diff。

## 11. Stage 3：Query ownership、Aggregate Read Models 与页面生命周期

### Task 10：建立 canonical query policy 和唯一 PageVisibility

**当前 checkpoint：** `S3-T10-page-visibility-foundation` 已建立 `PageVisibility` canonical provider、shell/transient visibility mapping 和 `pageRetentionPolicy` migration allowlist；`ShellPageHost`/`TransientPageHost` 只向旧 `PageActivity` adapter 传 visibility，`refreshRouteId` 二级刷新 owner 已删除。`S3-T10-activity-query-visibility` 已让 `useActivityQuery` 自己读取 canonical query visibility，页面不再把 `refreshEnabled` 传给普通 activity query。`S3-T10-retention-allowlist-pruned` 已移除全部 shell 页面 legacy retention allowlist，默认只保留 active + transition 两个 shell 页面；Stations per-row `useQueries` 和少量 legacy activation callback 尚未删除。

**文件：**

- Create: `src/app/navigation/PageVisibility.tsx`
- Create: `src/app/navigation/pageRetentionPolicy.ts`
- Modify: `src/app/ShellPageHost.tsx`
- Modify: `src/components/shell/PageActivity.tsx`
- Create/Modify: shared query policy helpers

- [ ] 写 lifecycle RED tests：只挂 current/previous during transition/transient；background page 零 polling、零 focus refetch、零 activation loader。
- [ ] host 唯一产生 `foreground | background`；feature 不再组合 interactive/refreshEnabled/activation callback。
- [ ] current/entering page 才是 foreground；previous/leaving/retained/被 transient 覆盖的 shell page 均为 background，并同步设置 inert/focus/keyboard ownership。transition test 覆盖快速往返、reduced motion、transient open/close 和焦点恢复。
- [ ] 默认离开页面后 unmount；昂贵不可序列化 draft 只能进入显式 retention allowlist，并具有内存上限。
- [ ] prefetch 使用 QueryClient，不通过预挂载隐藏页面。
- [x] `PageActivity` 只可作为迁移 adapter，输入必须收敛到 visibility；最终删除第二套 refresh 语义。
- [ ] 增加 hidden-query-start metric，invariant 违反时测试 fail，不用补偿 refresh 掩盖。
- [ ] `PageVisibility` 只控制订阅/refetch policy，不清空 cache 或提交业务状态；短 command 的 AbortSignal 只能阻止过期结果进入 cache，不能谎称后端已取消。需要真实取消/progress 的工作必须进入 OperationRegistry。

**退出：** 页面活跃性 owner 唯一；host 不知道 feature query key/stale time；页面返回依靠 cache/stale policy。

### Task 11：迁移 Key Pool 与 Stations 并建立 aggregate workspace

**当前 checkpoint：** `S3-T11-key-pool-query-owner` 已把 Key Pool 页面、Add/Edit Key transient 页的 Key Pool/Stations 读源收敛到 canonical React Query options，并删除 `stationKeys` API 层的 key-pool DOM update event；Key Pool、Stations 和 AddProvider 的 key mutation 成功路径改为显式 `QueryClient` cache update/invalidation。`S3-T11-key-pool-monitor-query-owner` 已把 KeyPoolPage 的 channel monitors/templates 辅助读迁移到 `channelMonitoringQueryOptions/useActivityQuery`，删除本地 monitor server-state 与 activation loader；监控开关 mutation 成功后失效 `queryKeys.channelMonitoring` 与 `queryKeys.channelStatus`。`S3-T11-stations-bounded-snapshot-read-blocker` 已确认 Stations 列表的 per-row `getLatestCollectorSnapshot(stationId)` 不能在不新增 Persistence/read-port 能力时改成真实 bounded aggregate；最新快照影响采集失败/需登录/未采集标签和 rate chips，不得用空快照或前端 wrapper 伪装通过。Task 11 仍未完成：Stations per-row `useQueries`、bounded aggregate workspace/read-model、backend query-count evidence、partial semantics 和 mutation race tests 尚未关闭，且不得通过隐藏 N+1 或修改 Persistence V2 绕过。

**文件：**

- Create/Modify: `src/features/key-pool/{api,queries,mutations,models,viewModels}.ts`
- Create/Modify: `src/features/stations/{api,queries,mutations,models,viewModels}.ts`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`
- Create: matching Rust application query/read-model and command DTO modules

- [ ] 先冻结当前 row facts、partial semantics、group identity、balance precedence、sort/filter 和 mutation behavior。
- [ ] 后端 workspace command 一次返回分页/上限内列表所需事实；partial 字段显式带 availability/errorCode。
- [ ] aggregate read model 在一个明确的 consistent read snapshot 内构造，并使用稳定排序和 cursor/page contract；并发 mutation 下不能重复/漏行或混合两个 revision 的事实。
- [ ] 写 command-count 和 backend-query-count test，10/100/500 条数据时正常刷新 IPC command 数和 SQL/read-port round trip 数都不随行数增长；同时记录 payload bytes、query duration 和 query-plan evidence。只把 N+1 藏到一个 command 后面不算通过。
- [ ] 若 Persistence V2 的已批准公开 read/query port 无法支持 bounded aggregate，立即把所需 input/output/consistency/performance contract登记为外部 prerequisite 并阻塞该 shard；不得修改 V2 schema/session/upgrade 内核，也不得在 application 层用 N+1 临时绕过后声称完成。
- [ ] 删除 Stations per-row `useQueries` 和 Key Pool 本地 server-state 副本。
- [ ] 每个资源只有 canonical key factory、queryFn、stale/refetch/timeout/partial policy。
- [ ] query policy 显式定义 mount/focus/reconnect/interval retry；Tauri window focus 与 browser preview online state 使用统一 adapter。background 页面在 focus/reconnect 时仍为零主动 query，foreground 页面只按 stale policy refetch。
- [ ] mutation 开始取消同资源过期 refetch，成功先应用 authoritative result，再执行精确 invalidation；较旧 response 不得覆盖 mutation。
- [ ] 删除 key-pool update CustomEvent 和手工 refresh owner。
- [ ] draft 与 server state 分离，未提交表单不写 Query Cache。

**Cutover：** 单页面 workspace query 成为唯一读源时立即删除 per-row loader/本地副本；不保留双读比较在 production。

**退出：** Stations/Key Pool O(1) command 数；无空数组/0 伪装 partial failure；回归覆盖 group/balance/key editing 语义。

### Task 12：迁移 Dashboard、Logs/Changes、Pricing/Routing

**文件：**

- Modify: `src/features/dashboard/DashboardPage.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/changes/ChangeCenterPage.tsx`
- Modify: `src/features/pricing/{PricingPage,ModelBasePricesPage}.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Create/Modify: each feature's `api.ts`, `queries.ts`, `mutations.ts`, models/view models and colocated tests
- Create/Modify: exact Rust application read-model/command DTO paths expanded by Task 0 for each page shard

**当前 checkpoint：** `S3-T12-model-base-prices-query-owner` 已把 ModelBasePricesPage 的模型基准价格列表从本地 `rows/loading` server state 和 mount-time `listModelBasePrices()` loader 迁移到 canonical `modelBasePricesQueryOptions/useActivityQuery`；reset/upsert mutation owner 成功后直接更新 `queryKeys.modelBasePrices` 并失效 `queryKeys.pricing`。`S3-T12-change-events-query-sync` 已删除 change-events API/AppShell/ChangeCenter 的业务 DOM event 同步，ChangeCenter 和 AppShell read-on-entry mutation owners 直接更新 `queryKeys.changeEvents`。`S3-T12-proxy-status-query-sync` 已删除 proxy API/AppShell 的业务 DOM status event；Dashboard/Routing/Settings/Updater lifecycle owners 成功后写入 `queryKeys.proxyStatus`。`S3-T12-logs-query-refresh-owner` 已删除 LogsPage 的 legacy page refresh owner 和 page-local refresh fan-out，刷新按钮只失效 `queryKeys.requestLogs`，key/settings 辅助读继续由各自 activity query owner 管理。`S3-T12-dashboard-routing-visibility-owner` 已删除 Dashboard 的死 `usePageRefreshEnabled` adapter，并让 Routing cooldown clock 直接读取 canonical `usePageQueryEnabled`；两个页面的 server-state query 仍由 `useActivityQuery` 统一控制。Task 12 仍未完成：Changes/Pricing 的剩余页面级 server-state、bounded read-model、hidden-query 和 observer/query-budget Gate 仍需逐片验证。

每个页面：

- [ ] 分类 server/derived/view/draft/operation state，并登记 owner。
- [ ] 建 canonical query keys/options；移除 `useState` server truth 和 `Promise.all` 页面编排。
- [ ] 删除 DOM data event；mutation owner 更新/invalidate cache。
- [ ] 聚合字段使用 bounded backend read model，详情/大日志按需分页。
- [ ] 对 loading/error/empty/partial/stale/mutation race 写 component tests。
- [ ] 验证 hidden page query 为零、返回页面读取最新 authoritative cache。

**退出：** shell 不重复页面 workspace query；read model 不携带 secret/raw payload；跨资源 mutation 等待契约内 invalidation 后再 settled。

### Task 13：迁移 Settings、Collectors 和剩余页面

**当前 checkpoint：** `S3-T13-settings-cache-owner` 已把 SettingsPage 的 settings/proxyStatus server state 收敛到 canonical query options，删除 settings API/AppShell/RoutingPage 的业务 DOM event 同步；Settings、Collector advanced 和 LocalRouting settings mutation owners 成功后直接更新 `queryKeys.settings`，LocalRouting settings 额外失效 `queryKeys.localRoutingWorkspace`。`S3-T13-collectors-query-owner` 已把 CollectorsPage 的 stations/latest snapshot/history/runs/capture status server state 收敛到 canonical query options，并删除页面 activation loader/local server-state 副本；collector task/capture mutation owner 成功后直接更新或失效对应 Query Cache。`S3-T13-channel-monitoring-query-owner` 已把 ChannelMonitoringTab 的 monitoring workspace 收敛到 `channelMonitoringQueryOptions/useActivityQuery`，删除 local workspace server-state 和 activation loader；monitor/template mutation owner 成功后失效 `queryKeys.channelMonitoring`。`S3-T13-channel-status-refresh-owner` 已删除 ChannelStatusPage/Tab 的 prop refresh token，监控运行完成后直接失效 `queryKeys.channelStatus`，Status Tab 只保留 canonical status query owner 和本页 UI draft 排序。Task 13 仍未完成：DataRecovery/Updater 等剩余页面的 local server-state、operation owner 和 retention allowlist 仍需逐片审计。

**文件：**

- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/features/channels/ChannelStatusPage.tsx`
- Modify: `src/features/data-recovery/**`, `src/features/updater/**` only where they own server state
- Modify: `src/components/shell/AppShell.tsx`
- Create/Modify: exact feature query/mutation/model/tests and Rust read models expanded by Task 0

- [ ] 用同一 owner 分类和迁移步骤处理剩余 server state。
- [ ] operation progress 不塞入普通 resource Query Cache；只由 operation controller keyed by id 持有。
- [ ] 删除所有业务 `CustomEvent` cache synchronization、activation loader 和 query/local-copy 双 owner。
- [ ] 对 proxy/runtime/collector 等真实后端 out-of-band event，建立 bridge-owned typed event adapter：校验 schema version、resource revision/sequence 后只调用 QueryClient atomic update/invalidate；组件不直接订阅 Tauri/DOM event。event gap/lag 触发一次 bounded canonical refetch，不能按 message 猜状态或无限补刷。
- [ ] 对 retention allowlist 做逐项审查；没有明确必要性的页面移除保活。
- [ ] 运行 frontend architecture gate，阻止新增直接 invoke、BackendClient locator、DOM data event 和隐藏 query。

**Stage 3 Gate：** Query Cache 是 server state 唯一 owner；aggregate refresh 有界/O(1)；页面可见性唯一；所有长期 draft/operation owner 明确。

## 12. Stage 4：Work Lifecycle、Foreground Operation 与 Async Outbound

### Task 14：建立 lifecycle、blocking 与 outbound 三个独立基础内核

Task 14 必须按 14.A -> 14.B -> 14.C -> 14.D 四个 shard 提交；三个内核禁止共享万能 manager、任意 service registry 或通用 `Context: Any`。

#### Task 14.A：纯 TaskSupervisor lifecycle kernel

**文件：**

- Create: `src-tauri/src/background_tasks/{mod,task,supervisor,status,shutdown}.rs`
- Create: `src-tauri/tests/task_supervisor.rs`

- [ ] 用纯状态机 RED tests 覆盖 register/start/run/backoff/stop/fail/panic/cancel/shutdown-timeout、重复 task id 和 concurrency-key non-reentry。
- [ ] 实际执行只使用 ADR 选定的 Tokio/Tokio-util primitives：`CancellationToken`、一个主要 `TaskTracker` 或 bounded `JoinSet`、`Semaphore`、bounded `mpsc`/`watch`；Supervisor 不实现 executor、线程池、通用 mailbox、actor address 或 future polling loop。
- [ ] supervisor 只持有 immutable spec、status、child cancellation token 和 join handle；task body 构造时注入业务依赖。
- [ ] restart classifier 只允许 transient 自动重试；configuration/auth/invariant/panic 保持可见 terminal，cancelled 不计业务失败。
- [ ] backoff 使用有上限 jitter，并使用 deterministic clock/random source 测试；测试不得真实 sleep。
- [ ] restart 前必须确认上一 join handle 已终结并释放 concurrency key；stable-success window 后才重置 consecutive failure/backoff，短暂成功不能制造无限快速重试。
- [ ] status transition 非法、join handle 遗失或重复 terminal 必须触发 invariant failure，不能静默覆盖。

#### Task 14.B：有界 BlockingExecutor

**文件：**

- Create: `src-tauri/src/background_tasks/blocking.rs`
- Create: `src-tauri/tests/blocking_executor.rs`

- [ ] 按 Task 1 数值实现 semaphore、bounded queue、queue timeout、deadline、late-result discard 和 orphan metric。
- [ ] job 必须带 kind、operation/correlation id 和 cancellation disposition；禁止捕获完整 service bundle、database transaction 或 async mutex guard。
- [ ] RED tests 覆盖 queue full、queue timeout、cancel-before-start、cancel-during-uncancellable-call、late completion、panic 和 shutdown orphan report。
- [ ] 取消后无法强停的 job 只能完成物理调用，结果不得触发 retry、cache update、event 或业务 commit。

#### Task 14.C：共享 AsyncOutboundClient

**文件：**

- Create: `src-tauri/src/outbound/{mod,client,policy,proxy,error}.rs`
- Create: `src-tauri/tests/async_outbound.rs`

- [ ] RED fixtures 覆盖 direct/system/manual HTTP/SOCKS、redirect、connect/first-byte/body/total timeout、body limit、Retry-After、remaining budget、cancel、header allowlist 和 URL/header/body redaction。
- [ ] URL userinfo 和 control characters 拒绝；redirect 默认受 endpoint-role policy 约束。跨 origin/scheme redirect 不携带 Authorization/Cookie/provider headers，HTTPS 不静默降级 HTTP；redirect loop/limit 返回 typed failure 并保留脱敏 evidence。
- [ ] client pool key 只使用稳定且不含 secret 的 transport policy；测试证明 request 数增长不会线性创建 client，proxy credential 不出现在 Debug/metrics/cache key。
- [ ] sensitive headers 使用不可 Debug/Display 的 secret wrapper，构建 request 后最短生命周期持有；error chain、retry clone、redirect history 和 response evidence 均不得复制 secret。能零化的临时 secret buffer 在 drop 时 zeroize。
- [ ] outbound 不解析 provider JSON、不决定 auth refresh/health/task success；driver 不自行建 reqwest client。
- [ ] retry 消耗同一 deadline/budget，response body streaming 在超过上限或取消时停止读取并释放资源。
- [ ] TLS/redirect/proxy policy 的不兼容变更必须返回 typed failure，不降级到更宽松 transport。

#### Task 14.D：原子 composition 与临时 allowlist

**文件：**

- Modify: `src-tauri/src/app_composition.rs`
- Modify: `src-tauri/src/runtime_composition.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: architecture inventory/allowlist

- [ ] composition preflight 后原子注册 supervisor/executor/outbound；任一依赖构造失败时不启动 runner、不接纳 operation。
- [ ] production direct `spawn_blocking` / HTTP client construction 进入 parser allowlist，只允许尚未迁移且登记精确删除 shard 的路径。
- [ ] 分别运行三个内核 focused tests，再运行 composition fault matrix；一个内核失败不能留下另外两个半可用的 managed state。

**退出：** 三个内核独立可测、独立依赖、独立容量；saturation 产生 typed overload；取消后 late result 不 commit；Task 15 只能依赖其公开窄合同。

### Task 15：建立 OperationRegistry 并迁移 connectivity probe

**文件：**

- Create: `src-tauri/src/background_tasks/operation.rs`
- Create: `src-tauri/src/application/connectivity_probe/**`
- Create: `src/features/key-pool/connectivityOperationController.ts`
- Create: `src/features/key-pool/useConnectivityOperation.ts`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src-tauri/src/commands/mod.rs`（仅 connectivity transport adapter，Task 23 再移动文件）

- [ ] RED tests 覆盖 start/id、bounded progress、`Completed`/`Failed`/`Cancelled`/`TimedOut`/`ResultUnknown`、channel close 和 exactly-one terminal；越过不可逆 commit barrier 后不能谎报 `Cancelled`。
- [ ] operation id 在进程生命周期内不可复用；registry 只保留有界 progress ring 和 terminal summary，不保存 secret/body。Task 1 冻结 terminal TTL、最大条数和 GC 周期，容量达到时只淘汰已 terminal 的最老记录，绝不丢弃 running handle。
- [ ] concurrency-key admission、operation id allocation 和 handle registration 是一个原子步骤；重复 start 在 capacity 满时返回 typed Overloaded/Conflict，不允许工作已启动但 registry 无 handle。
- [ ] GC 后 status/cancel 返回 typed NotFound/Expired；应用重启不伪装恢复 in-flight operation，除非该 operation kind 另有 durable owner 和独立恢复合同。
- [ ] `cancel_operation(id)` 推进 backend token 并等待或报告仍在停止；页面关闭按 fixed policy cancel/detach。
- [ ] connectivity 模型候选、Responses/Chat fallback、SSE terminal decode 和 envelope validation 移出 command。
- [ ] protocol probe kernel 不依赖 Tauri Channel、Query、registry、routing scheduler 或 persistence；与 proxy 相同语义共享 fixture/decoder contract。
- [ ] 迁移到 AsyncOutboundClient；取消后禁止新 retry/fallback/commit。
- [ ] UI 只订阅 operation controller，不用 run token 忽略真实后台工作。
- [ ] progress 乱序、重复和 terminal-after-terminal 被 controller/registry 拒绝或幂等忽略，并记录 invariant metric；不得让较旧 stream 覆盖新 operation。
- [ ] progress transport 使用有界缓冲并允许按 kind 合并/丢弃中间进度；terminal 走不可被 progress 挤掉的独立交付/查询路径。lagged subscriber 重新读取 bounded status/terminal，不通过猜测 channel close 补终态。
- [ ] detach 后可用 operation id 查询脱敏 terminal/result projection；result 过期返回 typed Expired。包含外部不可逆副作用的 operation 必须定义 idempotency/reconciliation key 和 commit barrier，UI 重试不能重复创建远端资源。

**Cutover：** start/cancel/status command 成为唯一 connectivity production path 后删除旧同步/阻塞实现。

**退出：** network stop、retry stop、UI terminal 和无后续持久化副作用全部可证明。

### Task 16：迁移 station collector runner

**文件：**

- Modify: `src-tauri/src/services/station_collectors.rs`
- Modify/Create: collector task adapter and status projection
- Modify: startup/shutdown composition

- [ ] Characterize current schedule、single-instance、manual trigger、failure effect 和 persistence side effect。
- [ ] runner 改为 Tokio interval/select cancellation；每 tick 独立 run id、budget、processed count 和 duration。
- [ ] 同 concurrency key 不重入；transient only backoff，auth/config/invariant/panic 可见且不无限重启。
- [ ] supervisor 统一持有 join handle；删除自有 OS thread、atomic stop flag 和 block_on loop。
- [ ] manual collection 是 operation 或受监督 one-shot，不绕过 capacity。
- [ ] shutdown 在预算内等待 in-flight run，timeout 写入 final report。

**退出：** station collector 的旧 thread runner 删除；success/partial/failure 事实和 change event 无漂移。

### Task 17：分片迁移 monitor、真实 blocking ports 和 app shutdown

Task 17 必须拆成三个互不混合的 production cutover。

#### Task 17.A：channel monitor runner/probe

**文件：**

- Modify: `src-tauri/src/services/channel_monitors/mod.rs`
- Modify: `src-tauri/src/services/channel_monitors/probe.rs`
- Modify: matching monitor composition/tests from Task 0 shard manifest

- [ ] channel monitor 按 Task 16 同一 task contract 迁移，probe HTTP 使用 AsyncOutboundClient。
- [ ] characterization 保持 template、latency、streaming terminal、failure classification 和 status projection；每个 probe 共享同一 run budget，不按 endpoint 重置 timeout。
- [ ] cutover 后删除 monitor 自有 thread/atomic stop/spawn_blocking network path，再运行单实例、取消、退避和 shutdown tests。

#### Task 17.B：capture、dialog、keyring 等真实 blocking ports

**文件：**

- Modify: `src-tauri/src/services/capture/**`
- Modify: `src-tauri/src/application/data_directory.rs`
- Modify: `src-tauri/capabilities/capture.json`
- Modify: capture custom command permission definitions/generated ACL inputs identified by Task 0
- Modify: Task 0 inventory 中批准的 OS/WebView/keyring blocking callers

- [ ] WebView cookie、folder dialog、keyring 和仅同步 filesystem compatibility 能力进入 BlockingExecutor；网络 I/O 明确排除。
- [ ] capture 得到的 cookie/session/token 立即进入既有 credential/vault secret boundary；普通 struct、operation progress、diagnostic event 和 frontend DTO 只传 handle/masked metadata。验证失败、取消和 late result 都会 zeroize/drop 临时 secret。
- [ ] `capture-*` remote window 使用独立 least-privilege capability，只允许 capture-specific sanitized commands；不能调用 station/key/settings/proxy/updater/data-recovery/main runtime commands。
- [ ] 每个 capture invoke 同时校验 window label/session owner、station id、endpoint revision、exact scheme/host/effective port 和允许 request path；跨 station、stale revision、lookalike host、userinfo、非法 scheme 和 window 重用全部 fail closed。
- [ ] window create/navigate/close 由单一 capture owner 管理；外部普通网站默认系统浏览器打开，main WebView 不导航 remote URL。宽 `http://*`/`https://*` capability shell 只有在上述 application gate 和 smoke test存在时才可保留。
- [ ] 每个 port 定义 cancel-before-start、late-result、side-effect barrier 和 shutdown policy；不能取消的 job 在取消后丢弃结果。
- [ ] 不在持有 async mutex、write/read session 或 business lock 时提交 blocking job；parser/lock-order test 覆盖此约束。

#### Task 17.C：updater coordination、proxy drain 与 app shutdown

**文件：**

- Modify: `src-tauri/src/services/updater.rs`
- Modify: `src-tauri/src/services/proxy/**`（只允许公开 drain/status facade 和 composition adapter）
- Create: `src-tauri/src/background_tasks/exit.rs`
- Modify: `src-tauri/src/app_composition.rs`
- Modify: `src-tauri/src/runtime_composition.rs`
- Modify: `src-tauri/src/lib.rs` and every tray/window/updater/OS exit caller identified by Task 0

- [ ] updater prepare 先停止接纳新 operation，再按固定顺序停 scheduler、请求 proxy drain、等待 task、flush diagnostics、释放 runtime。
- [ ] tray Quit、window true-close、updater restart、OS `ExitRequested` 和 test shutdown 只调用幂等 `ExitCoordinator::request_exit(reason)`；close/minimize-to-tray 只隐藏窗口，不启动 shutdown。
- [ ] 在 Tauri 可 `prevent_exit` 的 `ExitRequested` 阶段启动 bounded async drain；完成或 global deadline 后由 coordinator 只调用一次 final `app.exit`。`RunEvent::Exit` 仅做无异步依赖的最后记录，禁止 `block_on` 主要 shutdown。
- [ ] supervisor 只调用既有 ProxyRuntime drain/shutdown facade，不接管 request/attempt/body lifecycle。
- [ ] fault injection 覆盖 task panic、hung blocking job、proxy drain timeout、runtime release failure、重复 shutdown 调用和多失败聚合。
- [ ] shutdown 是幂等状态机；第二次调用返回相同/更完整报告，不重新启动 drain，也不吞 join error。
- [ ] 强制 kill/crash 无法保证 graceful cleanup；下次启动只报告 unclean previous exit 并交给各 durable owner 恢复，不能补写虚假的 Cancelled/Stopped terminal。
- [ ] public shutdown report 只含稳定 code、elapsed/timeout 和脱敏 owner，不含内部 error chain、secret 或数据库路径。

**退出：** monitor、blocking ports、shutdown 三个 shard 均已单独 cutover；proxy request lifecycle 未被 supervisor 接管；旧 runner/blocking caller 清单只剩 Task 18/22 明确允许项。

### Task 18：建立 structured observability 并收紧 work/outbound gates

**文件：**

- Create: `src-tauri/src/observability/{mod,correlation,metrics,redaction}.rs`
- Create: `src-tauri/tests/observability_contract.rs`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`（固定 `tracing` 相关依赖）
- Modify: command/work/outbound/collector composition points
- Create/Modify: frontend local diagnostic metric adapter and tests from Task 0 shard manifest

- [ ] 建立 command、task run、operation、collector run、monitor run、outbound request 和 proxy request 的 correlation propagation test；跨层不生成无关联新 id。
- [ ] 结构化 span/event 只使用稳定 code、kind、duration、result、redacted resource id；完整 key/cookie/token/Authorization、prompt/response body、provider payload、URL query 和数据库路径在 Debug/Display/event/metric 中均不可达。
- [ ] 本地 bounded metrics 至少覆盖 command latency/error、workspace latency/payload/IPC count、task status/backoff/shutdown timeout、operation terminal/cancel latency、blocking saturation/orphan、collector failure class、hidden query start 和 binding drift。
- [ ] metrics label 必须是封闭低基数 enum/normalized id；禁止 station id、operation id、URL、model 自由文本作为无界 label。诊断 buffer 具有 Task 1 冻结的容量/TTL，并验证 GC。
- [ ] 通过窄 runtime status facade 和 generated read command 暴露 `RuntimeTaskSummary { id, status, last_started_at, last_succeeded_at, last_failure_code, consecutive_failures, next_retry_at }`；字段来自 supervisor status projection，不从日志反推。
- [ ] runtime status/read model 只暴露用户可行动状态；完整本地诊断仅开发者模式可读，仍经过同一 redaction contract。本升级不引入云遥测。
- [ ] production 永久 runner 中 `thread::spawn + block_on` 为零；fire-and-forget spawn 只有带 owner/理由/删除期限的最小 allowlist。
- [ ] production network I/O 不允许进入 BlockingExecutor；operation event 必须带 id/version/terminal contract，cancel command 必须有 registry owner。
- [ ] 运行混合 saturation：periodic task、manual operation、blocking jobs 同时达到容量时，拒绝/排队可预测、指标可见且 UI 不假成功。

**退出：** redaction canary、bounded-cardinality、correlation continuity、runtime status 和 architecture gates 全部通过；日志/指标不承担业务决策或状态 owner。

**Stage 4 Gate：** daemon、operation、blocking 三类 owner 分离；所有纳入治理的工作有界、可取消、可等待、可诊断；共享 async outbound 可供 Stage 5 使用。Supervisor 只复用 Tokio primitives 而非自造 runtime；所有真实退出入口统一到 ExitCoordinator；`RunEvent::Exit + block_on` 主要关机路径为零；capture capability 与 main 分离并通过 window/station/revision/exact-origin gate。

## 13. Stage 5：ProviderRegistry 与 Capability Drivers

### Task 19：冻结 capability contracts、registry 和 conformance suite

Task 19 按 contract/registry、conformance harness、reference driver 三个 shard 执行，禁止在 trait 仍变化时同时迁 NewAPI/Sub2API。

#### Task 19.A：capability contracts 与 ProviderRegistry

**文件：**

- Create: `src-tauri/src/services/collectors/{contract,failure,evidence,orchestration}.rs`
- Create: `src-tauri/src/services/collectors/drivers/mod.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/app_composition.rs`

- [ ] 在 `contract.rs` 明确 `CollectorDriver`、`RemoteKeyDriver`、`AuthorizationDriver`、ProviderKind/Descriptor/Entry、capability descriptors 和 canonical input/output；trait 不接受 Tauri/Application/Persistence DTO。
- [ ] Provider/Collector context 只接受 opaque credential handle 或短生命周期 secret accessor，不接受可 Clone/Debug 的 raw key/cookie/token；driver 不能缓存 secret，auth/client helper 的错误与 evidence 经过统一 redaction。
- [ ] `ProviderKind` 是封闭 enum；历史未知 provider 可只读保留并标记 unsupported，不映射到 custom，也不参与路由/采集。
- [ ] `ProviderEntry` 只组合 descriptor 和三个可选 capability；三类 trait 各自保持最小能力面，禁止合并成要求所有 provider 实现空方法的万能 trait。
- [ ] registry 只按 kind 返回 descriptor/capability object，不执行 network/retry/persistence/schedule；duplicate kind、descriptor/capability mismatch 和缺少 registration 在 composition 时 fail closed。
- [ ] capability 缺失返回 typed Unsupported；不按 provider string、错误 message 或 endpoint shape 猜默认实现。
- [ ] DriverFailure 固定分类 retry/auth effect/endpoint/evidence，sanitized detail 不承担决策；evidence 数量、字段和文本长度有上限。
- [ ] auth/session refresh 的并发 owner 和 single-flight key 明确：同 station/credential revision 只允许一个 refresh，等待者共享剩余 budget；refresh side effect、stale credential revision 和取消均有 conformance test，禁止重试风暴。

#### Task 19.B：provider conformance harness

**文件：**

- Create: `src-tauri/tests/provider_conformance.rs`
- Create: `src-tauri/tests/fixtures/providers/**`
- Create: `docs/superpowers/audits/provider-capability-matrix.json`

- [ ] harness 对任一 registered capability 运行 success、partial、auth failure、rate limit、server failure、malformed、unknown shape、cancel、budget exhaustion、stale endpoint revision 和 redaction fixtures。
- [ ] fixture manifest 记录 provider kind、capability、endpoint role、request/response schema、redaction status、source/provenance 和预期 canonical facts/failure；禁止只存无来源 happy path JSON。
- [ ] matrix 中声明“不支持”必须与 registry descriptor 和 runtime typed Unsupported 一致；缺 fixture 的已声明 capability fail closed。

#### Task 19.C：OpenAI-compatible reference driver

**文件：**

- Refactor: `src-tauri/src/services/collectors/adapters/openai_compatible.rs` -> `src-tauri/src/services/collectors/drivers/openai_compatible/**`
- Modify: provider registry composition and exact callers from Task 0 shard manifest

- [ ] OpenAI-compatible 作为最小 reference driver，只依赖 AsyncOutboundClient 和 capability contracts。
- [ ] reference driver 通过完整 applicable conformance suite，并证明新增该 provider 不改 orchestrator/supervisor/query/persistence workflow。
- [ ] production cutover 后立即删除其旧字符串 dispatcher/ureq path；不等待 NewAPI/Sub2API 一起删除。

**退出：** registry/god-object gate 通过；driver 无 Tauri/Query/persistence store 依赖。

### Task 20：迁移 NewAPI capabilities

**文件：**

- Refactor: `src-tauri/src/services/collectors/adapters/newapi/**` -> `src-tauri/src/services/collectors/drivers/newapi/**`
- Modify: `src-tauri/src/services/remote_keys.rs`
- Modify: `src-tauri/src/services/capture/web_authorization.rs`
- Modify: provider registry composition and exact command/application callers from Task 0 shard manifest

- [ ] 先锁住 NewAPI endpoint/auth/task matrix、recovery、group/rate/balance facts 和 partial/error effects。
- [ ] 按 `auth.rs/client.rs/endpoints.rs/parsers.rs/mapping.rs/mod.rs` 分离，但先迁职责后做最终物理 rename。
- [ ] client 只构建 typed OutboundRequest；parser 纯解析；mapping 纯 canonical conversion；driver 不持久化。
- [ ] 按 collector -> remote-key -> authorization 三个 capability shard 分别 cutover；一个 shard 只改变一个 capability owner。
- [ ] collector、remote-key、authorization 分别实现所需 capability，不要求空方法。
- [ ] remote-key create 明确 upstream idempotency/reconciliation 合同：支持 idempotency key 时在一次用户意图内稳定复用；不支持时 response 丢失返回 `ResultUnknown`，后续先 list/reconcile 再允许新建，禁止 transport 自动重放。
- [ ] authorization capability 只验证 provider session/header；WebView window/cookie capture、secret storage 和用户关闭窗口仍由 capture/application owner 管理，driver 不创建/关闭窗口。
- [ ] 每个 shard cutover 后立即删除 NewAPI 在对应通用 service 中的字符串分支；同一 capability production path 不保留 ureq fallback。
- [ ] conformance、现有 collector fixture、remote-key/auth regression 和 live qualification 分开通过；live qualification 不阻塞确定性 fixture shard 的本地提交，但阻塞 Stage 7 release。

**退出：** NewAPI 三个 capability 各有唯一 owner、独立 conformance 证据和删除后的旧分发路径；未完成 capability 不影响已 cutover capability 回滚。

### Task 21：迁移 Sub2API capabilities

**文件：**

- Refactor: `src-tauri/src/services/collectors/adapters/sub2api.rs` -> `src-tauri/src/services/collectors/drivers/sub2api/**`
- Modify: `src-tauri/src/services/remote_keys.rs`
- Modify: provider registry composition and exact command/application callers from Task 0 shard manifest

- [ ] 锁住 Sub2API login/session、endpoint recovery、group rates、model/pricing、balance、partial facts 和 compatibility fixtures。
- [ ] 使用相同 capability/async outbound contract，不给 Sub2API 建第二套 retry/client/error policy。
- [ ] endpoint revision 和 evidence 显式传播；stale revision 不 commit。
- [ ] remote-key 与 collector capability 可共享 auth/client primitives，但不能互调 orchestration。
- [ ] Sub2API remote-key create 同样通过 Task 20 的 idempotency/result-unknown/reconciliation conformance；provider 不支持某项语义时显式 Unsupported，不在通用 service 猜测成功。
- [ ] 按 collector -> remote-key capability 两个 shard 分别 cutover，每个 shard 后删除对应通用 service/provider name match 和 ureq fallback。
- [ ] fixture 与 authenticated live verification 分离记录 provenance。

**退出：** Sub2API collector/remote-key capability 各有唯一 owner；旧 string/ureq path 已按 shard 删除；canonical facts/effects 与 baseline 一致。

### Task 22：迁移剩余 management/probe HTTP 并删除 production ureq

**文件：**

- Modify: `src-tauri/src/services/endpoint_ping.rs`
- Modify: `src-tauri/src/services/channel_monitors/probe.rs`
- Modify: `src-tauri/src/services/capture/web_authorization.rs`
- Modify: `src-tauri/src/services/updater.rs`（仅仍需 direct HTTP 的部分）
- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- Delete/replace: legacy `src-tauri/src/services/outbound.rs` ureq builder and provider dispatchers identified by Task 0

- [ ] 按 endpoint/ping、channel probe、authorization validation、updater inspection 四个 shard 顺序迁移，每项先锁定 proxy/redirect/timeout/body/redaction fixture 并独立 cutover/delete。
- [ ] 一个 operation 只使用一种 transport；禁止 async 失败后回退 ureq。
- [ ] 删除 per-request client construction，验证稳定 proxy route/policy 下 client pool 复用。
- [ ] production source AST gate 对 `ureq` import/type/construction 为零后移除 dependency。
- [ ] 测试 fixture server 可使用测试专属实现，但 production dependency graph 不包含 ureq。
- [ ] 删除 remote-key/authorization/collector provider 字符串 dispatcher。

**Stage 5 Gate：** provider-specific 代码局部化；capability conformance 完整；orchestrator 不解析 payload/message；production ureq 为零；新增 provider 只改 registry、driver 和 fixtures。

## 14. Stage 6：按已稳定 owner 物理拆分并删除旧路径

### Task 23：拆分 commands/mod.rs

**文件：**

- Create/Move: `src-tauri/src/commands/{stations,station_keys,collectors,connectivity,channel_monitors,remote_keys,capture,routing,proxy,settings,changes,logs,updater,data_recovery,runtime}.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] 先由 parser/registry 证明每个 command 已只有 transport validation、一个 facade call、error mapping、DTO conversion。
- [ ] 按领域移动，不在移动提交改变行为、DTO 或 error code。
- [ ] connectivity/remote-key/capture helper 必须已在 application/service owner 中，不能原样搬 1000 行 helper。
- [ ] `mod.rs` 最终只含 declarations/re-exports/registry hook。
- [ ] generated registry 中每个 production command 恰好归属一个领域 module；Task 0 inventory 中零 unclassified command，新增杂项 `misc.rs` 禁止通过。
- [ ] 每个领域移动后运行 registry/ACL/binding/focused tests，再提交；禁止一次移动全部领域后排错。

**退出：** registry 中零 unclassified/duplicate command；`commands/mod.rs` 无业务 helper/network/blocking/parser；各领域 module 只依赖允许 facade/error/DTO 边界。

### Task 24：拆分巨型页面

**顺序：** `AddProviderPage` -> `StationsPage` -> `KeyPoolPage` -> 其他同时拥有多职责的页面。

**文件：**

- Refactor: `src/features/stations/AddProviderPage.tsx` -> `src/features/stations/pages/**`, `components/**`, `formModel.ts`, `viewModels.ts`
- Refactor: `src/features/stations/StationsPage.tsx` -> matching stations page/components/controller modules
- Refactor: `src/features/key-pool/KeyPoolPage.tsx` -> matching key-pool page/components/controller modules
- Modify/Create: colocated Vitest/component tests for each extracted owner
- Modify: exact remaining page paths listed in Task 0 `execution_shards`; glob 不授权额外页面

- [ ] 先画 state ownership map；transport/query/mutation/form/operation/view model 已有 owner 才允许拆。
- [ ] 页面只保留 layout/composition；form reducer/validation、controller、dialog、list/table row、纯 view model 分离。
- [ ] 不按机械行数拆，也不创建 `utils.ts`/`helpers.ts` 垃圾桶。
- [ ] 每次抽取保持 loading/error/focus/draft/dialog/keyboard 行为，由 component tests 证明。
- [ ] 新组件不能接收完整 BackendClient、QueryClient 或无界 context 作为 locator。
- [ ] 每个 page shard 在抽取前后记录 React component tree、query/operation subscriptions 和 event listeners；拆分不得增加重复 owner、重复 request 或 retained listener。

**退出：** 三个已知巨型页面和 Task 0 追加页面均只负责 layout/composition；transport/query/form/operation/dialog/list owner 可独立测试，行为与订阅数量无回归。

### Task 25：拆 provider 文件、删除迁移 adapter 和清理 artifact policy

**文件：**

- Refactor/Delete: `src-tauri/src/services/collectors/adapters/**` and final `drivers/**` paths from Tasks 19-21
- Delete/Modify: legacy frontend API/event/PageActivity paths listed by Task 0
- Delete/Modify: legacy runner/outbound/provider dispatcher paths listed by Task 0
- Modify/Delete: structural source-contract tests listed by Task 0
- Modify: `.gitignore`, Vite/CodeGraph ignore configuration, artifact scripts and inventory

- [ ] **Task 25.A provider physical closeout：** provider 模块最终落实 auth/client/endpoints/parsers/mapping/mod 分层；逐 provider 删除旧 adapter re-export 和 temporary compatibility module。
- [ ] **Task 25.B legacy path deletion：** 按 frontend fallback/event、PageActivity、runner/stop flag、outbound builder/provider dispatcher 类别分提交删除，不将所有删除揉成一次不可定位变更。
- [ ] **Task 25.C test replacement：** 将仍有行为价值的 source-contract tests 先改为 parser/compile/behavior tests并观察 RED/GREEN，再删除声称验证结构却只读源码文本的测试。
- [ ] **Task 25.D artifact cleanup：** 清理分散 target/output 的策略和脚本；实际删除前解析并验证精确 absolute path 位于 worktree 或批准的 `output/`，拒绝 workspace root、`~`、环境变量未解析值和目录穿越。
- [ ] **Task 25.E final graph：** 逐条清空 Stage 0 temporary adapter/allowlist，重新生成 dependency/fan-in/fan-out inventory，检查没有 `RuntimeContext`/`AppManager`/万能 facade 替代旧 god object。

**Stage 6 Gate：** 目标模块图成立；旧路径/临时 adapter/allowlist 清零；文件变小是职责收敛的结果，不是唯一验收依据。

## 15. Stage 7：完整资格验证与交付

### Task 26：确定性功能、架构与性能验证

**文件/产物：**

- Create: `docs/superpowers/audits/2026-07-22-architecture-scale-deterministic-qualification.md`
- Create: `output/architecture-scale/qualification/deterministic/**`（gitignored evidence）
- Read/execute only: production source、tests、shared verification entrypoint；发现失败回到 owner shard 修复并重新取得新 revision 证据

- [ ] 运行 shared fast/full verification，生成 binding 后工作区零 diff。
- [ ] 运行所有 frontend unit/component/contract/build 和 Rust fmt/clippy/check/test。
- [ ] Tauri smoke 覆盖 desktop bootstrap、handshake、ACL denial、runtime unavailable、typed error、normal read/write 和 demo isolation。
- [ ] 按 Task 1 冻结的方法，以 10/100/500 stations/keys 固定 dataset 测 command count、backend query count、payload、query duration、page commit；同时满足绝对 SLO 和相对回归阈值。command/query 数保持 O(1)，payload 有上限，完整报告记录同机 provenance、sample 和 p50/p95。
- [ ] 页面切换循环验证 hidden query=0、cache freshness、draft retention allowlist 和无 listener 泄漏。
- [ ] parser gates 验证 dependency、state injection、spawn、HTTP client、provider registration、binding/ACL/registry 和 artifact policy。
- [ ] observability contract 验证 correlation continuity、redaction canary、低基数/容量和 `RuntimeTaskSummary`；dependency advisory/license/source/lifecycle gate 无过期例外、未知支持状态或未完成 blocker。
- [ ] Tauri security gate 验证 production CSP、main/capture window capability、compiled registry/ACL、exact-origin validator、external navigation owner、production/demo build graph 和 current threat-model controls。

**退出：** 同一 clean revision 的 deterministic/architecture/performance 报告完整且全绿；任何修复都会改变 revision 并使旧报告失效。

### Task 27：并发、取消、shutdown soak 与 provider qualification

**文件/产物：**

- Create: `docs/superpowers/audits/2026-07-22-architecture-scale-soak-live-qualification.md`
- Create: `output/architecture-scale/qualification/{soak,live}/**`（gitignored redacted evidence）
- Read/execute only: release candidate binary、fixture harness、approved live endpoints

- [ ] 同时运行 collector/monitor/manual connectivity/remote scan/blocking capture/proxy traffic，验证 capacity、backpressure 和无 starvation。
- [ ] 对每个 operation kind 做 cancel-before-request、during-body、during-retry、before-commit、after-commit-barrier；核对 terminal 和持久化副作用。
- [ ] 对 shutdown 做 idle、in-flight、hung external、hung blocking、task panic、proxy drain timeout 和 update preparation matrix。
- [ ] 对 process crash/restart 验证：daemon 从 composition 重新注册且不重复运行，非 durable operation 不伪装恢复/完成，越过外部 side-effect barrier 的 ResultUnknown 可通过 reconciliation 查询，bounded diagnostics 能说明上次异常终止但不泄密。
- [ ] Sub2API/NewAPI 用 redacted fixture 全覆盖；authenticated live run 单独记录 binary hash、endpoint role、selected credential mask、HTTP lifecycle、facts/DB result。
- [ ] soak 按 Task 1 时长和增长阈值采样 operation history、task/OS handles、HTTP clients/connections、memory、React listeners 和 query observers；终态 GC 后回落到稳定上限，不能只凭“进程未崩溃”判定通过。

**退出：** fault/cancel/shutdown/soak/live matrix 对同一 candidate binary 通过；live 不可用必须明确阻塞 release，不能用 fixture 替代后标记通过。

### Task 28：release/locked 构建、产物审计和最终快照

**文件/产物：**

- Create: `docs/superpowers/audits/2026-07-22-architecture-scale-upgrade-final-qualification.md`
- Create: `output/architecture-scale/qualification/release/**`（gitignored provenance/artifacts）
- Modify: qualification index/manifest only；production 修复必须返回 owner shard，不能在本 Task 顺手修改

- [ ] 在干净 worktree、固定 toolchain/lockfile、明确 target triple 上运行 release profile shared entrypoint。
- [ ] 构建 Tauri bundle，验证签名、updater preparation、artifact hash、revision、dirty=false 和 bundle 内容。
- [ ] 在签名 Windows bundle 上执行 fresh install、从当前受支持已发布版本升级、update preparation/drain/relaunch、offline startup、旧 WebView asset/new binary contract mismatch、single-instance second launch 和正常 tray/exit matrix；Persistence V2 自身 schema/import 结果只引用其独立资格报告，不在本计划重复实现。
- [ ] packaged production entry 必须固定 desktop mode；尝试注入 preview/demo mode、缺失 handshake capability 或 binding hash mismatch 均进入 fail-closed recovery，不能访问 DemoBackend 数据。
- [ ] bundle/security scan 解析最终 `tauri.conf`、capabilities、dist module graph 和 custom command permissions；`csp: null`、remote script/eval、capture 获得 main command、demo fixture/entry、secret canary 或 authorization mismatch 任一出现即失败。
- [ ] 扫描 source、logs、fixtures、generated bindings 和 bundle，确认无 key/cookie/token/db path/raw provider payload。
- [ ] 验证 PR 和 release workflow 对同一 revision 调用同一 repository entrypoint；所有 advisory 例外未过期且有 owner。
- [ ] 核对 release revision 的 dependency lifecycle ledger 与实际 lockfile/toolchain 一致；关键依赖无 unsupported/EOL、不可接受高危 advisory 或过期复查，独立 major-upgrade prerequisite 的资格报告均指向当前 revision。
- [ ] 精确 stage 本升级路径，检查 staged snapshot；从 staged tree 重跑关键 binding/architecture/build gate。
- [ ] final qualification 记录所有证据、已知限制、artifact hash、签名结果和 rollback revision；引用 Task 26/27 报告并验证 source/candidate revision 相同。

**Stage 7 Gate：** 同一 staged revision 通过 deterministic、Tauri smoke、soak、live、release/locked 和 artifact verification；否则不具备发布资格。

## 16. 标准验证命令合同

Stage 0 建立脚本后，执行者不得自己拼一套更弱的“等价验证”。统一入口的目标命令合同如下；实际参数由 `scripts/verify.ps1` 封装并在 CI contract test 中核对：

```powershell
# 依赖解析与供应链门禁
pnpm.cmd install --frozen-lockfile
powershell -ExecutionPolicy Bypass -File scripts/check-advisories.ps1
pnpm.cmd dependency:lifecycle:check

# 生成与前端确定性门禁
pnpm.cmd generate:bindings --check
pnpm.cmd lint
pnpm.cmd test:contracts
pnpm.cmd test
pnpm.cmd build

# Rust 确定性门禁；同一时间串行运行
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml

# Repository-owned profiles
powershell -ExecutionPolicy Bypass -File scripts/verify.ps1 -Profile fast
powershell -ExecutionPolicy Bypass -File scripts/verify.ps1 -Profile full
powershell -ExecutionPolicy Bypass -File scripts/verify.ps1 -Profile release
```

Profile 内容固定如下，禁止同名 profile 在本地与 CI 执行不同命令：

| Profile | 用途 | 必须包含 | 不包含 |
|---|---|---|---|
| `fast` | 单 shard 高频反馈 | generated check、lint、architecture gates、受影响 Vitest/Cargo targets、frontend typecheck | live provider、签名、长 soak |
| `full` | PR/主分支 deterministic gate | frozen install、advisory/license/source/lifecycle、全部 contract/unit/component/integration、build、fmt/clippy/check/test、Tauri smoke fixture、artifact policy | authenticated live、签名、长 soak |
| `release` | tag/candidate qualification | `full` 的严格超集，加 locked release/Tauri bundle、签名、bundle scan、provenance、Task 26-27 资格报告 revision check | 无；缺任一依赖工具/报告即失败 |

`verify.ps1` 必须 non-interactive、`$ErrorActionPreference = 'Stop'`、转发子进程非零退出码、打印 start/end/duration/revision/profile，并在失败后保持第一个根因和完整失败摘要。CI 使用固定版本 PowerShell runner；本地 Windows PowerShell/pwsh 的差异必须有 contract test，不能靠 shell 默认行为决定是否通过。

Task focused test 的命名必须稳定，让计划和 CI 可直接寻址：

| 范围 | 预定 filter/target |
|---|---|
| Rust architecture | `--test architecture_scale_boundaries` |
| IPC serialization/registry | `ipc_contract` / `command_registry` |
| composition/facade | `application_composition` / `command_facades` |
| lifecycle | `task_supervisor` / `operation_registry` / `blocking_executor` |
| outbound | `async_outbound` |
| provider | `provider_conformance` + provider kind filter |
| frontend bridge | Vitest `bridge` / `runtimeContract` |
| query lifecycle | Vitest matching feature + `PageVisibility` |
| Tauri smoke | repository-owned `tauri_smoke` target |

若真实测试框架要求不同命名，Task 0/2 只能统一修改本文和 runner，不能留下文档命令、package script、CI command 三套漂移。release-only live/soak/signing 不得塞进普通 unit target 导致 PR 不稳定，但 PR 的 compile、unit、architecture 和 fixture integration 不能省略。

## 17. 每个 Task 的标准 Checkpoint

每个原子 shard 完成报告必须包含；没有独立 Checkpoint 的 shard 视为未完成：

1. Task/shard ID、base HEAD、result HEAD、允许文件和实际 staged 文件。
2. 观察到的 RED 失败及其为何准确指向缺失能力。
3. focused、affected、stage gate 的命令、结果、耗时和 profile。
4. production cutover 点、已删除旧路径、尚存 temporary adapter 及删除 Task。
5. architecture inventory/allowlist 数量变化，禁止只说“代码更干净”。
6. 可靠性证据：失败、取消、超时、partial、shutdown 或幂等行为。
7. 可维护性证据：owner、dependency edge、facade fan-out、generated drift。
8. 可拓展性证据：新增 command/provider/task/query 所需修改面是否符合目标。
9. `git diff --cached --check`、`git diff --cached --name-only` 和 staged snapshot verification。
10. 未验证项和真实原因；timeout、未启动进程或 debug-only run 不得写成通过。

## 18. 回滚、提交与合并策略

- 每个原子 shard 一个或少量语义单一提交；先 contract/gate，再实现，再 cutover/delete，避免混合提交无法回滚。
- 回滚单位是 command group、feature、task/operation kind 或 provider capability，不是整个项目。
- feature flag 只允许位于 composition root，必须有删除 Task；禁止散落在业务分支。
- 新旧实现不得双写；shadow read 仅可在 fixture/benchmark 中运行，不用于 production decision。
- 发生 regression 时回滚到上一个已通过 stage gate 的单路径版本；不能用 fallback 把错误藏起来。
- 每个 Stage 合并前重新 intake 主线已提交 drift；未提交主线状态不能声称已吸收。
- 合并冲突先保护用户可见行为和 owner 语义，不在冲突解决中顺手改 schema、重做 UI 或扩大 facade。
- push 默认不执行；需要发布时另按用户授权完成远端动作。

## 19. 停止条件

出现以下任一情况必须停止当前 Task：

- 需要编辑 Persistence V2 内核/schema 才能继续。
- baseline 或前一 Stage gate 未通过且原因未分类。
- generator 无法稳定覆盖普通 command/serde 合同，且 ADR 中无可信替代。
- composition 无法原子注册窄 state，出现半初始化 runtime 风险。
- 新旧路径开始双写、互相 fallback 或 owner 不唯一。
- cancellation 测试证明不可逆副作用边界不清，却仍准备暴露“已取消”。
- provider 迁移需要把业务语义塞进 outbound/supervisor/registry。
- parser gate 只能靠源码 regex 才能声称通过。
- live provider 结果与 fixture 不同且尚未解释 provenance/endpoint/auth 差异。
- 变更超出 Task 文件范围或与主 checkout dirty hunks重叠。

停止后应输出证据和需要修订的 spec/ADR/plan，不继续打补丁掩盖架构冲突。

## 20. 架构正式审阅

### 20.1 可靠性审阅

| 风险 | 计划中的控制 | 可执行证据 |
|---|---|---|
| Desktop 故障被 mock 掩盖 | 显式 backend mode、handshake、无 fallback | Task 8-9 failure matrix/Tauri smoke |
| IPC 两端漂移 | Rust authority、generated binding、hash/registry/ACL gate | Task 3-6 zero-diff/serialization tests |
| mutation 重放或结果未知 | 幂等分类、stable key、result unknown | Task 5 mutation matrix |
| server state 竞态 | Query 单一 owner、authoritative cache transition | Task 11-13 race tests |
| 隐藏页面继续工作 | 唯一 visibility、hidden-query gate | Task 10/26 lifecycle metrics |
| daemon/operation 无法停止 | token/join/terminal/deadline/shutdown report | Task 14-18/27 fault matrix |
| blocking pool 饱和 | 有界 queue、timeout、late-result discard | Task 14/18 saturation tests |
| provider partial 伪成功 | typed failure/evidence/completeness | Task 19-22 conformance suite |
| 网络 retry 超预算 | shared remaining budget/cancellation | Task 14/19-22 outbound fixtures |
| 诊断泄密或高基数耗尽内存 | 统一 redaction、bounded low-cardinality metrics | Task 18 canary/capacity tests |
| 发布时才发现问题 | PR/release shared fail-closed entrypoint | Task 2/28 workflow contract |

结论：可靠性不是靠“多加 try/catch”，而是靠 fail closed 的边界、单一权威、明确幂等、真实取消、有界并发、唯一终态和可复核发布证据。计划为每一项安排了先失败测试、production cutover 和最终故障矩阵；没有把 timeout 或 debug success 当成通过。

### 20.2 可维护性审阅

| 风险 | 计划中的控制 | 可执行证据 |
|---|---|---|
| AppServices/BackendClient 变 locator | command/feature 只收窄 facade/client | Task 7-9 AST fan-out gate |
| facade 变新 god object | 只允许真实跨 service use case，不镜像全部方法 | dependency ledger/public API review |
| 状态同步模板扩散 | Query/visibility/operation 各有唯一 owner | Task 10-15 owner tests |
| 跨语言 DTO 重复 | Rust generated binding | Task 3-6 drift gate |
| 只搬文件不降复杂度 | 先稳定 owner/edge，Stage 6 才物理拆分 | Task 23-25 precondition |
| source regex 给出假安全 | TypeScript Compiler API/`syn`/graph gate | Task 2 fixture proofs |
| provider 逻辑散落 | orchestrator/driver/parser/mapping 分责 | Task 19-22 dependency gate |
| 大 Task 无法审查和回滚 | mandatory shard/one-owner cutover protocol | §3.1 + 每 shard Checkpoint |
| 产物污染索引和 watcher | 统一 output policy 和 ignore parity | Task 2/25/28 artifact gate |

结论：可维护性通过“谁拥有状态、谁做决策、依赖朝哪里走”来保证，不用行数或目录美化冒充。每个抽象都有禁止职责，临时 adapter 有删除 Task，Stage 6 前置条件阻止把旧业务堆积原样搬家。

### 20.3 可拓展性审阅

| 扩展场景 | 目标修改面 | 不应修改 |
|---|---|---|
| 新普通 command | Rust DTO/registry、窄 facade、generated client、tests | 手写 TS command string/DTO、无关 feature |
| 新 provider collector | ProviderKind/entry、CollectorDriver、fixtures | supervisor、query、persistence workflow、remote-key service |
| provider 新 remote-key 能力 | RemoteKeyDriver implementation/registration | collector orchestrator、authorization flow |
| 新 periodic task | TaskSpec、窄 task body、composition registration | supervisor state machine |
| 新 foreground operation | OperationSpec/controller/application owner | resource Query Cache、daemon registry |
| 新列表聚合字段 | bounded application read model/DTO/query selector | 每行新 IPC、proxy runtime snapshot |
| 新 demo 能力 | 对应 domain demo client 和 deterministic dataset | DesktopBackend、真实 credential/network |

结论：扩展点保持编译期封闭且按能力拆分。新增功能必须局部修改 owner/registration/fixture，不通过万能 trait、动态插件、事件总线或 service locator 获得“表面灵活”。这符合当前本地桌面模块化单体的规模。

### 20.4 行业成熟度与安全边界审阅

本计划的“先进”不以引入更多框架为目标，而以采用已经被主流 Rust/React/Tauri 工程验证的边界和运行模型为标准。实施时必须满足下表约束；只复刻名词但自行重写底层机制，不属于按本计划实施。

| 领域 | 成熟实现基线 | 本计划的实施约束 | 明确不采用 |
|---|---|---|---|
| 部署与模块边界 | 桌面应用中的模块化单体、显式 composition root | 保持单进程部署，按 application/domain/infra/command 划分 owner，由窄 facade 和依赖门禁控制修改半径 | 微服务、sidecar、通用 DI 容器、service locator |
| 跨语言契约 | Rust 权威类型、build-time TS bindings、版本握手 | generator 仅在 build/CI 运行；生成结果、compiled registry、ACL 和 contract hash 同 revision 校验 | runtime reflection、手写双份 DTO、错误时回退 mock/demo |
| 前端 server state | TanStack Query 的 query/mutation/cache lifecycle | Query 只拥有 server state；form/draft/operation 分离，aggregate read model 消除页面级 N+1 | Redux/Zustand 再复制 server state、组件自行拼 IPC 同步 |
| 异步生命周期 | Tokio/Tokio-util 的 structured cancellation、join、semaphore 和 bounded channel | `CancellationToken` 加 `TaskTracker` 或 bounded `JoinSet` 作为主要 join owner；TaskSupervisor 只增加 policy/status | 自定义 executor、线程池、actor runtime、workflow DSL、无 owner spawn |
| HTTP 与凭据 | `reqwest` client pool、显式 timeout/budget、redirect policy、secret boundary | AsyncOutboundClient 统一 transport policy，provider 只构建 typed request/解析 response；凭据通过短生命周期 accessor 获取 | 自写 HTTP/TLS、per-request client、同步网络塞进 blocking pool、provider 自建 retry 栈 |
| Provider 扩展 | 编译期封闭 registry 与 capability-specific traits | registry 只组合 descriptor/capability；conformance suite 验证 provider，新增能力不改 supervisor/query/persistence workflow | 动态 ABI/插件系统、万能 Provider trait、按字符串或错误文本分发 |
| 可观测性 | `tracing` spans/events、低基数有界 metrics、端到端 correlation | 日志和指标只做诊断，不成为状态 owner；统一 redaction 与容量/TTL | 业务通过日志反推状态、raw payload/secret 记录、无界 label |
| WebView 安全 | Tauri 2 CSP、least-privilege capabilities、remote window isolation | production CSP 非空；main/capture/preview 分权；remote invoke 再校验 window/station/revision/exact origin | `csp: null`、remote window 继承 main commands、main WebView 任意导航 |
| 退出与发布 | Tauri `ExitRequested` 可阻止退出阶段、bounded async drain、可复现 locked build | 所有退出入口汇入幂等 ExitCoordinator；同一 staged revision 完成 smoke/soak/live/release 资格 | 在 `RunEvent::Exit` 才 `block_on` 主要关机、仅凭 debug build 或 HTTP 200 放行 |
| 架构门禁 | 编译器/类型系统/标准 lint 优先，AST fitness function 补缺口 | ESLint/compiled registry 能表达的规则不自造；custom parser 必须有 bypass fixtures、真实 cfg 和 stale allowlist 检查 | regex source test、把 CodeGraph 或单一自定义 parser 当 CI correctness 唯一来源 |
| 依赖生命周期 | 官方支持窗口、安全公告、锁定依赖和独立 major-upgrade qualification | Stage 0 台账核对 React 18/Vite 6/Rust 2021 edition 等实际基线；unsupported/EOL/高危状态先走 prerequisite shard | 永久冻结旧 major、无证据追 latest、把 major upgrade 混进 owner cutover |

成熟度结论：这些选择都建立在当前生态的稳定主路径上，同时保留桌面应用需要的低部署复杂度。技术前沿性来自强类型契约、结构化并发、capability isolation、可执行架构规则和可复现资格证据；稳定性来自复用 Tokio、reqwest、TanStack Query、tracing 和 Tauri 原生生命周期，而不是自行搭建平台层。任何替代技术必须先通过 ADR，证明维护状态、Windows/Tauri 兼容性、失败语义、迁移成本和可回滚性均不弱于此基线。

### 20.5 审阅中发现并已修正的计划风险

| 初始风险 | 调整 |
|---|---|
| 只写 Stage 目标，执行者仍需临场猜 cutover | 每个 Task 明确文件、RED、cutover、删除和退出证据 |
| generator 与 ACL/registry 可能继续三份名单 | Task 3 要求同一 registry 驱动/校验三者 |
| “有界”没有数值仍不可验收 | Task 1 增加 capacity budget ledger |
| Query O(1) 可能变成无界大 payload | Task 11/26 同时验证 pagination、payload bytes 和 duration |
| Supervisor 可能吞并前台 operation 和业务依赖 | daemon/operation/blocking 三 owner 分离并设 dependency gate |
| ProviderRegistry 可能变万能 provider service | registry 只查 capability，网络/retry/persistence 明确禁止 |
| 先拆 adapter 会固化同步 HTTP | Stage 4 async outbound 必须先于 Stage 5 driver |
| 最后统一清理可能留下长期双轨 | 每个 Task 都有单一 cutover，Stage 6 只做物理收口和删除审计 |
| release workflow 可能复制一套命令再次漂移 | Task 2/28 强制 repository-owned shared entrypoint |
| lifecycle、blocking、outbound 原本同一 Task 过大 | Task 14 拆成四个独立 shard，分别测试、注册和回滚 |
| monitor/capture/shutdown 原本混合 cutover | Task 17 拆成 runner、blocking ports、shutdown 三个 shard |
| structured tracing/metrics 只有零散要求 | Task 18 增加 correlation、redaction、bounded metrics 和本地诊断完整合同 |
| operation terminal 可能无限保留或误删 running handle | Task 1/15 冻结 TTL/容量/GC，只淘汰 terminal，GC 后 typed Expired |
| “matching files” 可能造成执行越界 | §3.1 + Task 0 `execution_shards` 强制展开精确路径后才可开工 |
| O(1) IPC 可能隐藏 backend N+1 | Task 11 同时门禁 backend query count、snapshot、cursor 和 query plan |
| PR 依赖解析可能漂移 | Task 2 强制 frozen pnpm、locked Cargo 和有期限 advisory exceptions |
| `syn`/Compiler API gate 仍可能被 cfg、glob、barrel 或同名 symbol 绕过 | Task 0/2 增加 boundary manifest、真实 target cfg、macro compiled registry、stale allowlist、descendant fan-out 和完整 parser fixtures |
| runtime handshake 早于 backend mode 会破坏 browser preview | Task 4 只建立纯 contract/validator，Task 8 再以显式 desktop/demo 启动状态机接入 UI |
| Stage 1 feature 直接使用 generated binding 会造成二次迁移 | Task 5 限定 generated binding 先藏在 legacy transport wrapper 后，Task 8/9 才迁 domain client |
| operation progress 可挤掉 terminal 或 detached 页面丢结果 | Task 15 增加原子 admission、有界 progress、独立 terminal 查询、lag recovery、TTL/Expired 合同 |
| redirect/retry/debug 可能复制凭据 | Task 14/17/19 增加 secret wrapper、zeroize、跨 origin header stripping、HTTPS downgrade 拒绝和 refresh single-flight |
| remote-key create 响应丢失后可能重复创建 | Task 15/20/21 增加 idempotency、ResultUnknown、reconciliation 和 commit barrier |
| release 只验证构建未验证安装/升级/旧资产 | Task 27/28 增加 crash restart、fresh install、supported upgrade、contract mismatch、single-instance 和 update relaunch matrix |
| TaskSupervisor 名义上统一生命周期，实际演变为自造 runtime | Task 0/14 先在 `TaskTracker` 与 bounded `JoinSet` 中选定唯一主要 join owner；禁止自定义 executor、线程池、mailbox、actor address 和 workflow DSL |
| production 保留 `csp: null`，typed IPC 之外仍存在 WebView 注入面 | Task 0 threat model 建账，Task 8 独立 production entry/config，Task 28 对最终 bundle 解析并以非空 CSP 为硬门禁 |
| capture remote URL shell 过宽或继承 main capability | Task 2/17/28 分离 main/capture/preview capability，并对每次 invoke 做 window/station/revision/exact-origin 二次校验 |
| tray/window/updater 各自直接退出，主要 drain 到 `RunEvent::Exit` 才 `block_on` | Task 17.C 建立幂等 ExitCoordinator，在 `ExitRequested` 阶段 prevent + bounded async drain，最终 exit 只执行一次 |
| production 与 demo 共享启动图，运行时开关可能把故障伪装成 demo | Task 8 使用独立 HTML/entry/Vite config；production module graph 不可达 DemoBackend，握手失败只进入 fail-closed recovery |
| generator/custom parser 被选中后失维护或规则可绕过 | Task 0 做维护/兼容性 spike，Task 2 以标准 lint/compiled gate 优先并用 bypass fixtures 验证补充 parser；工具只存在于 build/CI |
| “不升级大版本”演变为长期冻结失支持依赖 | Task 0 建 lifecycle ledger，Task 2/26/28 校验支持状态和复查日期；major upgrade 独立分片，但 unsupported/EOL/不可接受高危风险阻塞 release |

### 20.6 最终审阅结论

本计划符合可靠性、可维护性、可拓展性、安全性和行业成熟度要求，前提是按顺序执行并把 Gate 当作硬条件，而不是建议。成熟结论依赖于复用 Tokio/Tokio-util、reqwest、TanStack Query、tracing 与 Tauri 原生安全/退出生命周期；若改为自行实现底层 runtime、transport 或动态扩展平台，本结论自动失效。其核心保证不是新增更多抽象，而是：

- 一个状态/生命周期/契约只有一个 owner；
- 边界失败显式且 fail closed；
- 并发、队列、payload、timeout 和 shutdown 全部有界；
- command、feature、task、operation、outbound、registry、driver 的职责和禁止依赖可由 parser gate 执行；
- 每次迁移都有唯一 production cutover 和旧路径删除点；
- 新 command/provider/task/query 的修改面局部、可预测、由 conformance/architecture tests 保护；
- production CSP、main/capture capability isolation、独立 demo build graph 和 ExitCoordinator lifecycle 属于 release 硬门禁；
- 依赖支持窗口和安全状态可追溯；架构 cutover 不混入大版本迁移，但失支持/高危依赖也不能以范围控制为由带入 release；
- 最终资格基于同一 staged revision 的真实证据，而不是文件变小、测试脚本读源码文本或 debug 环境偶然成功。

任何执行者若跳过 Stage 0/1、保留长期 fallback/双写、把完整 AppServices/BackendClient 继续当 locator、把网络塞回 blocking pool，或先拆文件再稳定 owner，都视为未按本计划实施，即使构建和现有测试暂时通过也不能验收。
