# Rust Dead Code 可靠性升级实施计划

状态：Implemented；非生产/test-target warning 噪音和真实签名 release 环境验证作为后续/发布前置事项单独跟踪

日期：2026-08-03

适用范围：`src-tauri` 主库中的 Rust `dead_code`、相关测试辅助代码、生产组合根、路由与请求终结、凭据生命周期、数据维护与便携迁移、监控协议选择，以及掩盖未使用代码的 lint 豁免。

参考规范：

- `docs/superpowers/specs/2026-07-30-routing-operational-unification-upgrade-spec.md`
- `docs/superpowers/plans/2026-07-30-routing-operational-unification-upgrade.md`
- `docs/superpowers/plans/2026-07-31-schema15-upgrade-debt-cleanup.md`

> 本计划的目标不是机械消除编译器警告，而是让每段保留代码都有明确所有者、真实入口和可验证行为。零警告只是结果之一，不能替代可靠性测试、故障测试和架构门禁。

## 1. 执行摘要

本计划已完成生产 dead-code 门禁落地：基线普通 `cargo check --lib` 有 64 个 `dead_code` warning group，当前 production policy 为 0 个 production dead_code diagnostics、0 个 blanket `allow(dead_code)`、0 个 local `allow(dead_code)`，保留的 54 个 `expect(dead_code)` 都是登记过的外部/序列化合同。剩余 warning 主要来自 test-target/source-included fixture、ESLint、clippy、cargo-deny、Node deprecation、Vite chunk-size 和 Windows linker stdout，应按各自策略继续清理。

原始基线代码大致分成四类：

| 类别 | 基线可见诊断 | 主要含义 | 默认处置 |
|---|---:|---|---|
| 数据迁移、维护、恢复与安全 | 39 | 生产主流程已经存在；剩余项混有无 operation ID wrapper、测试分配型解析器、未消费证据字段、兼容 variant 和尚未接入的安全校验 | 保留现有主流程，逐项删除 wrapper、隔离测试支持、接入必要校验或登记兼容合同 |
| 路由与代理语义 | 16 | affinity、语义错误、失败归一化和终结投影存在，但生产热路径仍有断点 | 接入唯一执行链路，删除重复 bridge |
| 凭据与 session 生命周期 | 6 | 后台 session revision guard 和精确失效链未被真实 caller 使用；普通加密凭据写入已经具备事务 | 接通 endpoint-revision-aware refresh；精确失效无产品 caller 时整条删除；不改公开 DTO |
| 监控、schema 与测试辅助 | 3 | 测试构造器、常量或协议分支只在测试使用，或生产决策未完成 | 移入 test support、引用单一常量，或明确删除未采用分支 |
| 合计 | 64 | 仅为普通主库编译当前能看到的下限 | 不能用作完整债务总数 |

64 不是完整数量；原始代码里还有 blanket/local `allow(dead_code)` 隐藏的债务。当前结果已经把这些生产豁免归零，并通过 `scripts/dead-code-inventory.mjs --mode ci --scope production` 固化为门禁。完成后，每个符号只能处于以下三种状态之一：

1. **生产能力**：从 composition root、Tauri command、后台 runner 或代理执行链路可达，并有行为测试。
2. **测试支持**：位于 `#[cfg(test)]` 模块或显式 `test-support` feature 下，发布构建不可达。
3. **已删除能力**：实现、测试夹具、兼容 wrapper 和文档声明一起删除，不留下第二入口。

## 2. 目标与收益

| 质量目标 | 实施要求 | 接入后的收益 |
|---|---|---|
| 可靠性 | 真实入口、typed error、原子事务、失败回滚、崩溃恢复和并发测试同时成立 | 已写但未运行的保护逻辑真正生效；减少凭据半更新、迁移中断和路由错误误判 |
| 可维护性 | 一个业务语义只有一个 owner 和一条调用路径；删除重复 bridge 和 blanket allow | 修改错误分类、路由策略或迁移规则时，不必同步多份半成品实现 |
| 可拓展性 | composition root 依赖窄 port；策略使用 sealed enum/registry；迁移使用 planner/executor | 新增协议、路由策略或迁移版本时扩展既有边界，不再新增旁路和临时豁免 |
| 可观测性 | 每个关键状态有 typed outcome、稳定原因码和脱敏日志 | 能区分输入错误、provider 故障、容量不足、恢复等待等不同问题 |
| 发布质量 | CI 同时检查可见 dead code、blanket allow、测试隔离和完整回归 | GitHub Release Action 不再被大量无上下文警告淹没，新增债务会在合并前暴露 |

预期数据流收口为：

```text
Tauri command / local proxy / background runner
  -> application facade or use case
  -> typed domain decision
  -> persistence / transport port
  -> atomic side effect
  -> typed terminal outcome
  -> projection, invalidation and redacted observation
```

代码不能仅因为“将来可能有用”而留在生产构建中。未来能力应保留规范、测试用例草案或 ADR；只有进入当前组合根后才保留生产实现。

## 3. 执行规则

1. 按依赖图执行，不要求所有领域严格串行：Task 0 -> Task 1；Task 2/3/4/5-6 在各自依赖满足后可独立交付；Task 7 等待所有领域收口；Task 8 最后执行。每个 Task 开始前运行 `git status --short --branch`，只修改该 Task 列出的路径，不覆盖当前 dirty hunk。
2. 每项采用 RED-GREEN-REFACTOR：先证明当前能力不可达、行为缺失或重复，再做最小接入/删除，最后执行退出门禁。
3. 删除前必须在债务台账中记录符号、调用者、生产可达性、规范依据、决定和替代路径；不能只依据 IDE 的“0 references”。
4. 不用 `#[allow(dead_code)]`、crate 级 lint 降级、假调用、无意义 `pub`、虚构测试引用或 `_unused` 改名来消警告。
5. 仅对真实外部/序列化协议保留 `#[expect(dead_code, reason = "...")]`；reason 必须包含合同、owner 和删除条件。每个 expect 都进入台账和 CI 白名单。
6. 测试 helper 必须放入 `#[cfg(test)]` 或显式 `test-support` feature。不能为了集成测试方便把构造器留在普通生产编译中。
7. 生产接入必须经过已有 application facade、composition root 和 port；不能让 command 直接依赖 SQLx，也不能建立第二套 proxy/routing executor。
8. 凭据、迁移包、日志、fixture 和错误信息不得包含完整 API key、Cookie、Authorization、token、prompt、响应正文或可还原账号身份的数据。
9. 任一 `Run` 命令没有真实退出码 0，该 Task 保持未完成。警告数减少、单个 happy-path 测试或人工点选都不能代替退出门禁。
10. 不使用 `git add .` 或 `git add -A`。若后续单独要求提交，只 stage 当前 Task 的明确路径并运行 `git diff --cached --check`。

## 4. 非目标

本计划明确不做：

- 不为了消除 dead code 重写路由算法、数据层或监控 V2。
- 不引入通用工作流引擎、事件总线、插件系统或动态迁移 DAG。
- 不把所有内部函数改成 `pub`，也不通过反射、动态注册或伪入口规避静态分析。
- 不承诺接入所有已经写好的实验能力；没有当前产品需求和可靠合同的能力应删除。
- 不把测试专用 fixture、故障注入器或便捷构造器发布进生产二进制。
- 不用“GitHub Action 不显示 warning”作为完成定义；必须保留行为、架构、安全和发布回归。

## 5. 决策方法

每个候选符号按下面顺序判断：

```text
是否存在当前规范要求？
  no  -> 是否仅服务测试？ -> yes: 隔离到 test support
                              no: 删除实现、wrapper、fixture 和旧文档引用
  yes -> 是否已有唯一生产 owner 和入口？
           yes -> 接通入口并补行为/故障测试
           no  -> 先确定 owner；无法确定则不得接入，升级为 ADR 决策
```

删除判定必须同时满足：

- `rg`、编译器调用图和测试夹具均找不到生产调用者；
- 当前规范、冻结 DTO、数据库格式和外部兼容合同不要求该符号；
- 删除不会改变序列化字段、migration ledger、Tauri command 或本地 OpenAI-compatible 协议；
- 聚焦测试和完整回归通过；
- 台账记录了删除理由和恢复方式（通常为 Git 历史，而不是保留注释代码）。

接入判定必须同时满足：

- 能指出 composition root 和 terminal outcome；
- 成功、预期失败、取消/超时和重复调用行为明确；
- 敏感数据边界、事务边界和幂等边界明确；
- 不复制已有 owner，不新增跨层反向依赖。

本计划的依赖图：

```text
Task 0 baseline/ledger
  -> Task 1 low-risk helpers/constants
       -> Task 2 routing/failure/affinity
       -> Task 3 credential session refresh
       -> Task 4 monitoring Auto decision
       -> Task 5 maintenance wrappers/admission
            -> Task 6 portable migration residue
Task 2 + Task 3 + Task 4 + Task 6
  -> Task 7 remove blanket allows
       -> Task 8 CI/release qualification
```

## 6. Task 0：冻结完整基线和删除台账

**依赖：** 无。

**Files：**

- Create: `docs/superpowers/audits/2026-08-03-dead-code-inventory.md`
- Create: `scripts/dead-code-inventory.mjs`
- Modify: `package.json`（仅新增审计命令）
- Read only: `src-tauri/src/**/*.rs`

**RED：**

- 运行普通 `cargo check --lib`，冻结当前 64 个 warning group 及四类分布；台账另行展开每个 group 包含的 symbol/field/variant，不能把 warning group 数误称为符号数。
- 使用 `--force-warn dead_code` 重新检查，证明 blanket allow 后还有隐藏项。
- 分别记录 default `--lib`、`--all-targets` 和 release `--lib` 三个编译矩阵；区分 production、unit-test、integration-test 和 release-only cfg。
- 静态扫描生产源中的 `allow(dead_code)`、`expect(dead_code)`、`cfg(test)` 和可选 test-support 边界。
- 台账逐项记录：symbol、文件、类别、当前调用者、规范依据、敏感性、建议动作、owner、验证测试和状态。

**GREEN：**

- `scripts/dead-code-inventory.mjs` 直接启动 Cargo JSON diagnostics，区分普通可见项、force-warn 暴露项、blanket allow 和已审批 expect。
- 脚本以 JSON diagnostic 的 lint code、primary span 和 target kind 作为身份，不匹配 rustc 的英文展示文本；同一 symbol 在不同编译矩阵中去重但保留来源。
- 脚本只保存路径、行号、lint code 和符号，不保存环境变量、响应内容或 secret。
- 台账初始状态只能是 `wire`、`test-only`、`delete` 或 `needs-decision`；禁止使用含糊的 `keep for future`。
- 把当前 dirty 工作区和基线 commit 记录到审计文档，不把无关改动计入本计划。

**Run：**

```powershell
git status --short --branch
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo check --locked --manifest-path src-tauri/Cargo.toml --release --lib
node scripts/dead-code-inventory.mjs --mode baseline
node scripts/dead-code-inventory.mjs --mode force-warn
git diff --check -- docs/superpowers/audits/2026-08-03-dead-code-inventory.md scripts/dead-code-inventory.mjs package.json
```

**Exit gate：** 普通诊断、隐藏诊断、blanket allow 和 expect 都有可复现清单；每个候选有 owner 和明确处置，不再只依赖一次终端输出。

## 7. Task 1：隔离测试 helper 并统一 schema 常量

**依赖：** Task 0。

**Files：**

- Modify: `src-tauri/src/application/monitoring/*`
- Modify: `src-tauri/src/services/monitoring/*`
- Modify: `src-tauri/src/services/data_store/test_support.rs`
- Modify: `src-tauri/src/services/secrets/baseline_conversion.rs`
- Modify: `src-tauri/src/models/pricing_group_monitoring.rs`
- Modify: `src-tauri/src/application/queries/pricing_group_monitor_status.rs`
- Modify: `src-tauri/src/ipc/dto/pricing_reads.rs`
- Modify: `src-tauri/Cargo.toml`（仅在确实需要跨 crate test-support feature 时）
- Modify: 对应 `src-tauri/tests/*.rs`

**RED：**

- 测试证明默认生产编译不包含 `ProbeTransportResult::available`、故障生成器和只用于 fixture 的 resolver 构造器。
- 测试仍能从测试侧 helper 构造 transport result、device resolver 和 generation fixture。
- contract 测试只扫描 pricing group monitoring contract 的 owner/query/DTO，发现其中任何硬编码 schema version；不能把其他独立 `schemaVersion: 1` 合同误报为本领域问题。
- 对 `RetryPolicy::max_attempts` 先确认语义：若生产 orchestrator 已用字段直接计算，则删除重复 helper；若它是唯一规则 owner，则让生产代码改用该方法。

**GREEN：**

- `ProbeTransportResult::available` 优先移到 `monitoring_orchestrator` 集成测试自己的 helper；不为单个便捷构造器新增 Cargo feature。
- 只有多个 integration test crate 确实共享复杂 fixture builder 时，才新增默认关闭的 `test-support` feature；此时必须修改 `Cargo.toml`、用 `#[cfg(any(test, feature = "test-support"))]` 包围最小模块，并增加 release/default-feature 负向编译门禁，防止 feature unification 把 helper 带入发布构建。
- `resolver_from_parts` 的生产调用者若本身只是未使用 generation wrapper，必须按整条可达链判断，不能因为 dead production wrapper 调用了它就保留；最终要么由真实 startup/migration entry 可达，要么移入 test support/删除。
- query 的输入校验和输出 `schema_version`、DTO validation/fixture 全部引用 `PRICING_GROUP_MONITORING_SCHEMA_VERSION` 的单一 owner；其他 IPC registry/schema 合同继续拥有自己的版本常量。
- 删除只包装常量、重复字段计算或无额外不变量的 helper，并同步删除其孤立测试。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib monitoring -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_orchestrator -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test pricing_group_monitor_status -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
node scripts/dead-code-inventory.mjs --mode verify --scope test-support
```

**Exit gate：** 测试便利 API 不进入默认生产构建；generation/resolver 按完整生产可达链保留或隔离，不能由 dead wrapper 制造伪入口；pricing monitoring schema version 无第二来源；本类别不再产生普通 dead code。

## 8. Task 2：接通路由语义错误和 affinity

**依赖：** Task 1；必须遵守路由一体化规范中的 success-only affinity、硬资格和容量约束。

**Files：**

- Modify: `src-tauri/src/services/proxy/adapters/{openai,responses,capability}.rs`
- Modify: `src-tauri/src/services/proxy/{error,execution,request,ingress,finalization}.rs`
- Modify: `src-tauri/src/application/routing_engine/{affinity,request,routing_failure,controller}.rs`
- Modify: `src-tauri/src/application/routing.rs`
- Modify: `src-tauri/src/application/request_finalization/*`
- Modify: composition-owned runtime route state and its construction/shutdown path
- Modify: `src-tauri/src/commands/error.rs`
- Modify: 对应 routing/proxy tests 和 architecture scripts

**RED：**

- table-driven tests 冻结 adapter raw error 到 semantic signal、canonical failure、proxy response 的唯一映射。
- 测试覆盖 client input、authentication、rate limit、model unsupported、endpoint unavailable、timeout、cancel 和 committed stream failure；不能都折叠成 station unhealthy。
- `session_hash` 命中时只提供 affinity preference，仍必须通过 group/tag scope、endpoint/credential revision、model class、multiplier ceiling、health、eligibility 和真实 capacity lease。
- 入站 `previous_response_id` 只用于 lookup；成功 Responses 请求产生的**新 upstream response ID**才在 durable RequestOutcome success 后 bind。不能把入站旧 ID 重新绑定为本次结果，也不能从错误正文猜 response ID。
- session affinity 使用入站 `session_hash` lookup，并在 durable RequestOutcome success 后把同一个 session key 绑定到最终 selected station key；失败 attempt、取消、超时和未 commit 请求不得 bind/rebind。
- TTL、容量上限、revision 变化和并发更新必须产生 typed hit/miss，不依赖错误字符串。
- 并发测试证明 registry lock 不跨越 snapshot 构造、capacity wait、upstream I/O 或 durable writer `await`；慢请求不能阻塞其他 lookup/bind。

**GREEN：**

- adapter 只负责协议事实，routing failure 模块拥有规范化失败语义，proxy error 只负责本地 HTTP/OpenAI-compatible 表达。
- ingress 中已有 `session_hash` / `previous_response_id` 进入 immutable routing request facts；不得由 executor 再解析原始 JSON。Responses adapter 通过 typed protocol outcome 提供成功响应的新 response ID。
- `RuntimeRouteState` 是 `AffinityRegistry` 的唯一进程内 owner，由 composition root 构造一次并纳入 shutdown；planner 只消费 immutable、bounded lookup result，不自行持有第二个 registry。
- affinity 只能在当前合格 stratum 内提升候选：`PriorityFirst` 不跨 availability/priority 与 hard ceiling；`CostFirst` 不跨 exact/multiplier 5% band；之后仍需真实 capacity acquire。不得使用含糊的“在 eligibility 前后均可”。
- affinity bind 由 finalization 的 durable success consumer 执行，先等待 RequestOutcome ack，再短暂取得 registry 写访问；使用既有 bounded registry、TTL 和 revision 校验。
- `PublicError -> ProxyFailure` 等重复 bridge 只能保留一个方向和一个 owner；无调用的兼容实现连同测试一起删除。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_planner_controller -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_runtime_state -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_dual_terminal_lifecycle -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_stream_finalization_faults -- --nocapture
node scripts/routing-error-contract.test.mjs
node scripts/routing-operational-architecture.test.mjs
```

**Exit gate：** 两种 affinity 都能从真实入口走到 success-only bind；错误语义只有一个 owner；任何 affinity 都不能绕过硬约束；重复错误 bridge 已删除。

## 9. Task 3：收口 station session 刷新与凭据失效

**依赖：** Task 2 的 failure/finalization 边界稳定。

**Files：**

- Modify: `src-tauri/src/application/credentials.rs`
- Keep unchanged by default: `src-tauri/src/application/command_facades/credentials.rs`
- Modify: `src-tauri/src/application/provider_drafts.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs` 及真实 session refresh caller
- Modify: credential store 和 routing session invalidation port
- Keep unchanged by default: 公开 credential command / DTO
- Modify: 对应 credential、routing security 和 persistence tests

**RED：**

- 后台 collector/login refresh 读取 endpoint revision N 后，只有 station 仍为 revision N 才能持久化 session；endpoint 已更新到 N+1 时返回 typed stale revision，旧结果不得覆盖新配置。
- 两个并发后台刷新只有与当前 endpoint revision 匹配的结果能提交；这不是公开用户凭据编辑的通用 CAS，也不新增含义不明的 credential revision。
- 现有用户凭据写入继续在一个 `PersistenceRuntime::write` 事务内完成 encrypted secret upsert、metadata 更新和旧 secret 清理；故障测试证明 rollback 不留下半更新，不为消 dead code 重写已原子的路径。
- 更新或失效 station A 的 session 只使其关联的 route snapshot、session affinity 和 credential cache 失效，station B 不受影响。
- 删除或禁用凭据后，新请求不可获得 secret；已开始请求遵守已冻结 lease/commit 合同。
- 所有日志和错误断言验证 secret、header 和原始 credential 不泄漏。

**GREEN：**

- collector/provider session source port 调用 `CredentialService::{update,persist}_station_session_if_revision` 的一个 canonical 方法；store 在同一 write transaction 内检查 `stations.endpoint_revision` 并更新 session。
- 公开 `UpdateStationCredentialsInput` / `UpdateStationSessionInput` 默认不增加 `expected_revision`。若未来需要用户编辑 CAS，必须单独定义 credential revision、DTO compatibility 和迁移方案，不复用 endpoint revision。
- 对 `invalidate_station_session_credential` 先按真实错误恢复合同决定：若 auth refresh 能按 AccessToken/RefreshToken/Cookie 精确失效则接入；若当前 collector 只支持整体 session 重建，则删除 enum、trait、service、store 的整条未使用链。
- cache/session invalidation 使用 station/key/revision 身份，不使用显示名或全局清空作为默认实现。
- 未进入这条生产链的旧 update helper、兼容 session wrapper 和重复 DTO conversion 删除。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib credentials -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib provider_drafts -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_collectors -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_security_boundaries -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_sessions -- --nocapture
node scripts/local-routing-redaction.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
```

**Exit gate：** endpoint-revision-aware 后台 session refresh 形成生产闭环；公开 credential DTO 没有被无依据扩展；精确失效链要么真实可达要么整条删除；事务回滚、stale result、定向失效和无泄漏测试通过。

## 10. Task 4：决定监控 `ProtocolSelection::Auto`

**依赖：** Task 1。

**Files：**

- Modify: `src-tauri/src/application/monitoring/{planner,definition_bridge,orchestrator}.rs`
- Modify: `src-tauri/src/services/monitoring/adapters/protocol_auto.rs`
- Modify: monitoring DTO/definition（仅在产品合同需要时）
- Modify: `src-tauri/tests/monitoring_*.rs`

**RED：**

- 冻结当前证据：持久化 definition/command 只接受具体 `ProtocolKind`，`definition_bridge` 只构造 `ProtocolSelection::Explicit`，当前 UI/DTO 没有 Auto 合同；`protocol_auto` helper 的存在本身不构成产品合同。
- 若保留 Auto，测试覆盖 capability evidence 优先级、unknown、探测失败、fallback 上限和明确协议不被覆盖。
- 若删除 Auto，contract 测试证明所有 persisted/current definitions 都是 Explicit，旧输入得到稳定 validation error 或一次性兼容映射。

**GREEN：**

- 当前高置信度默认执行**删除路线**：删除 `ProtocolSelection::Auto`、planner branch 和只验证该不可达分支的测试；保留 `protocol_auto` 仅当 monitoring transport 或其他真实入口仍直接使用它。
- 只有实施时发现已发布持久化值、冻结 DTO fixture 或当前 UI 明确承诺 Auto，才暂停删除并形成 ADR；ADR 必须定义 definition 表达、唯一 resolver owner 和兼容迁移，再走保留路线。
- 不把“将来可能自动探测协议”作为保留生产 variant 的理由；未来需求可以基于现有 capability facts 重新接入。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_orchestrator -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_persistence -- --nocapture
node scripts/monitoring-architecture.test.mjs
```

**Exit gate：** Auto 要么拥有从 definition 到 adapter 的完整生产路线，要么从当前合同和实现中完整删除；不能继续处于“测试能构造、产品不能配置”的状态。

## 11. Task 5：收口现有数据维护 coordinator

**依赖：** Task 1；可与 Task 2/3/4 独立交付。

**Files：**

- Modify: `src-tauri/src/application/data_maintenance.rs`
- Modify: `src-tauri/src/application/data_migration/{export_service,import_service,import_prepare}.rs`
- Keep architecture: `src-tauri/src/lib.rs` 中已有的唯一 coordinator composition
- Modify: 中央 command admission boundary（仅当现有 persistence freeze 之外仍有明确产品合同）
- Modify: persistence / migration concurrency and fault tests

**RED：**

- 冻结当前生产事实：composition root 已构造一个 `DataMaintenanceCoordinator`；export/inspect/prepare 已调用 `begin`；激活已调用 `freeze_dependencies_for_activation_except` 和 `commit_activation_lease`。
- 对当前未使用项逐一建立删除/接入证据：`Recovering`、`DataCommandAdmission`、`MutationRejected`、`state`、`admit_command`、`enter/finish_recovery`、两个非 `except` freeze wrapper 和 lease `activity` getter。
- 证明持久 startup recovery 在 ready runtime 构造之前完成；若成立，进程内 coordinator 不需要 `Recovering` 状态，删除该 variant 和 enter/finish wrapper。
- 证明 `PersistenceRuntime::freeze_for_activation` 已是写 admission 的权威；如果中央 command boundary 没有额外、用户可见的提前拒绝合同，则删除未使用的 `DataCommandAdmission`，不把检查散布到所有 facade。
- 生产使用的 `_except` freeze 路径继续覆盖：排除当前 operation、停止新 operation、drain collector/proxy、冻结 persistence、写 journal 和 activation pending。
- panic、cancel、磁盘满和重启测试证明：freeze 前 RAII lease 可回到 Normal；freeze/journal 后保持 activation pending 直到进程退出/恢复，不能错误地重新开放旧 runtime。

**GREEN：**

- 保留现有 `Normal/Exporting/InspectingImport/PreparingImport/ActivationPending` 状态和 RAII lease，不重命名、不另建 `Idle/Frozen/Activating` 第二状态机。
- `DataMaintenanceCoordinator` 继续作为进程内维护 lease 的唯一 owner，由 composition root 构造并注入 migration facade。
- 生产保留一个参数最完整的 `freeze_dependencies_for_activation_except`；仅包装默认参数、无额外不变量的 freeze 方法删除，测试改为调用 canonical path 或测试侧 helper。
- `state` / `activity` 若只为断言服务则放入 `#[cfg(test)]`；`MutationRejected` 若无稳定 DTO 映射则删除。
- 若确有 command-level 提前拒绝需求，只在一个中央 dispatch/admission port 接入，并让 persistence freeze 保持最终权威；禁止每个 command facade 复制 match。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib data_maintenance -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_faults -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_fault_matrix -- --nocapture
node scripts/portable-migration-startup-boundary.test.mjs
```

**Exit gate：** 已有 production migration 路径和状态名保持不变；只有一个 canonical freeze 路径；startup recovery 与 runtime maintenance 职责不混合；未使用的 admission/recovery/wrapper 要么有中央真实入口，要么删除/测试隔离。

## 12. Task 6：收口便携迁移残留代码与安全校验

**依赖：** Task 5。

**Files：**

- Modify/Delete: `src-tauri/src/application/data_migration/{export_service,import_service,import_prepare}.rs` 中无 operation ID wrapper 和过宽 artifact
- Modify/Delete: Task 0 台账命中的 `src-tauri/src/services/portable_migration/*`
- Modify/Delete: `src-tauri/src/services/data_store/generation_upgrade.rs` 中被新 startup planner/executor 取代的 wrapper
- Modify: migration DTO/contract fixture（仅处理真实兼容 variant，不新增 command）
- Modify: `src-tauri/tests/portable_migration_{e2e,faults,malicious}.rs`

**RED：**

- 先冻结当前真实入口：command facade 使用 `export_portable_package_with_export_id`、`inspect_portable_package_with_inspection_id` 和 `prepare_portable_import_for_activation_with_import_id`；这些路径不能被未使用的 convenience wrapper 替代或重写。
- 对无 operation ID 的 `export_portable_package`、`inspect_portable_package`、`prepare_portable_import*` wrapper 建立调用图；若只被模块测试使用，测试改用 canonical 带 ID 入口或测试 helper 后删除。
- 对 `read_framed_payload`、`decrypt_framed_payload`、`ParsedPortablePayload`、`PortableActivationFault::Injected` 等确认 production 已使用 bounded streaming `*_to_writer`/显式 fault port；分配型或注入型 API 只能测试隔离或删除。
- 对 `app_secret_binding_policy`、`validate_setting_key`、`validate_secret_selector`、direct SQLite copy rejection、schema fingerprint/occupancy helper 做差分测试：若当前 production validator 已等价覆盖则删除重复实现；否则接入唯一 inspect/transform 边界。
- 对 recovery/DTO variant 和未读字段逐项确认是否属于已发布序列化/on-disk 合同；serde 能构造但 Rust 代码不显式构造的合法 variant 使用 contract fixture + 具名 expect，不能为了消警告删除。
- e2e/fault/malicious fixture 继续覆盖 export、加密 envelope、streaming inspect/prepare、postcondition、atomic activation、重启、路径穿越、资源上限、篡改、wrong key 和无 secret 泄漏。

**GREEN：**

- 保持 application facade 对 workflow 的现有 ownership，portable migration service 继续只提供 format、validate、transform、staging、writer 和 recovery primitive。
- 删除无 operation ID convenience wrapper；生产和集成测试统一经过带 ID、cancellation、commit barrier 的 canonical entry。
- 缩窄 `PortablePackageExportArtifact`、prepare/recovery result：生产 terminal/UI 真正需要的字段保留；只服务内部 postcondition 的证据在 workflow 内消费；仅测试断言字段改从脱敏 audit/fixture 读取，避免为测试扩大生产返回类型。
- production 保留 bounded streaming parser/decryptor；整包分配型 parser、fault injector 和 fixture constructor 移到 `#[cfg(test)]` 或测试文件。
- 安全 selector/setting/catalog 校验只有一个 owner，并在任何 staging write 前执行；若已有 registry 提供同等 fail-closed 行为则删除旧 validator。
- `prepare_generation_two*` 若只服务历史测试，测试迁移到当前 startup planner/executor fixture 后删除；不得让已废弃 wrapper 继续维持 `resolver_from_parts` 的伪生产可达性。
- activation journal/recovery 继续由现有 startup path 消费，不增加第二次 startup probe 或新的恢复状态机。

**Run：**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_faults -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_malicious -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib generation_upgrade -- --nocapture
node scripts/portable-migration-boundary.test.mjs
node scripts/portable-migration-redaction.test.mjs
node scripts/portable-migration-fixture-matrix.test.mjs
```

**Exit gate：** 带 operation ID 的现有生产主流程保持通过；无 ID wrapper、分配型测试解析器和 superseded generation helper 已删除/测试隔离；必要安全校验在唯一边界可达；外部兼容 variant 有 fixture/expect；没有为了 dead code 重写迁移协议。

## 13. Task 7：删除 superseded bridge 并解除 blanket allow

**依赖：** Task 2 至 Task 6 全部完成。

**Files：**

- Modify: Task 0 台账列出的所有生产 `#![allow(dead_code)]` 模块
- Modify/Delete: superseded error、routing、finalization、operational fact、pricing 和 persistence bridge
- Modify: 相应 integration tests 中的 blanket allow
- Modify: `scripts/dead-code-inventory.mjs`

**RED：**

- 按模块移除一个 blanket allow，使用 `--force-warn dead_code` 得到该模块真实清单；一次只处理一个 ownership boundary。
- 对每个新暴露符号重新执行“生产接入 / test-only / 删除”三选一，不做批量 `pub` 或批量 expect。
- integration test crate 中的 blanket allow 也要解除；共享 fixture 应移动到 `tests/support` 并只导入实际使用项。

**GREEN：**

- 推荐顺序：monitoring/pricing leaf modules -> operational facts -> routing kernel -> request finalization -> routing decision persistence。
- 已被新 use case 替代的 facade、conversion、repository method 和 compatibility DTO 整条删除，包括孤立 fixture 和只验证实现细节的测试。
- 对序列化/外部协议真正需要但 Rust 内部不读取的字段，使用具名 `#[expect(dead_code, reason = "contract=...; owner=...; remove_when=...")]`。
- 不删除 migration SQL、已发布 DTO 字段或 on-disk format；这类兼容字段若必须保留，应由 contract test 和 expect 共同说明。

**Run：**

```powershell
node scripts/dead-code-inventory.mjs --mode verify --scope production
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets
node scripts/routing-operational-architecture.test.mjs
node scripts/request-lifecycle-architecture.test.mjs
pnpm.cmd test:contracts
```

**Exit gate：** 生产源不存在 blanket `allow(dead_code)`；integration tests 不再整 crate 屏蔽 dead code；所有 expect 都有合同、owner、删除条件和 CI 白名单；台账中无 `needs-decision`。

## 14. Task 8：CI 门禁、完整回归和发布资格

**依赖：** Task 7。

**Files：**

- Modify: `scripts/dead-code-inventory.mjs`
- Modify: `scripts/verify.ps1`
- Modify: `.github/workflows/*`（只接入已有 verify profile，不复制检查逻辑）
- Create: `docs/superpowers/audits/2026-08-03-dead-code-closeout.md`
- Modify: `package.json`

**RED：**

- fixture 验证新增普通 dead function 会失败。
- fixture 验证新增模块级/crate 级 `allow(dead_code)` 会失败。
- fixture 验证无 reason 的 expect、未知 expect 和 test helper 泄漏到默认 feature 会失败。
- 正常 source fixture 通过，证明门禁不是依赖当前 warning 文本的脆弱字符串匹配。

**GREEN：**

- `verify:fast` 执行 source policy：禁止 blanket allow、校验 expect ledger、检查默认 feature 的普通 dead code。
- `verify:full` 增加 `cargo check --all-targets` 和关键 routing/migration/credential fault suites。
- GitHub Release workflow 只调用共享 release verifier 的 phase scripts（`verify:release:prebundle` / `verify:release:postbundle`）；不在 YAML 中维护第二份 lint 白名单，也不通过 `pnpm --` 向 PowerShell 转发 phase 参数。
- closeout 记录清理前后数量、接入能力、删除能力、保留 expect、所有命令退出码和未完成的 live/soak 条件。

**Run：**

```powershell
node scripts/dead-code-inventory.mjs --mode ci
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo check --locked --manifest-path src-tauri/Cargo.toml --release --lib
cargo test --locked --manifest-path src-tauri/Cargo.toml
pnpm.cmd test
pnpm.cmd test:contracts
pnpm.cmd build
pnpm.cmd verify:fast
pnpm.cmd verify:full
```

发布前另行执行：

```powershell
pnpm.cmd verify:release
```

**Exit gate：** 本地 full gate 和 CI 使用同一检查入口；正常生产编译没有 dead code 警告；新增 blanket allow 或未登记 expect 会使 CI 失败；完整 Rust、前端、contract 和 build 回归通过。

## 15. 最终质量门禁

只有同时满足下列条件，计划才能标记为 Implemented：

- 普通 `cargo check --lib` 的 dead code warning group 从 64 降为 0；default/all-targets/release 三个矩阵均通过。
- 生产源中的模块级/crate 级 `allow(dead_code)` 为 0，局部 allow 也为 0。
- 所有保留 expect 都对应外部/序列化合同，并有 owner、删除条件和测试。
- 台账所有项都关闭为 `wired`、`test-only` 或 `deleted`，无 `needs-decision` 和 `keep for future`。
- 路由语义错误、session/previous-response affinity 和 endpoint-revision-aware session refresh 从真实入口可达；精确凭据失效、维护 admission 等可选链路若没有当前 caller 已整条删除。
- 便携迁移继续使用既有带 operation ID 的生产路径；未使用 wrapper/分配型 parser 已删除或测试隔离，必要安全校验在唯一边界可达。
- success、typed failure、取消/超时、并发冲突、故障恢复和重启测试通过。
- 默认 production feature 不包含 fixture builder、fault injector、fake transport 或明文 secret 构造器。
- `cargo fmt`、`cargo check --all-targets`、完整 Cargo tests、前端 tests/build、contract tests 和 `verify:full` 真实退出码为 0。
- Release Action 使用同一 release verifier 执行生产 dead-code policy；新增生产 dead code、blanket allow 或未登记 expect 会失败。prebundle/postbundle 通过 phase-specific npm scripts 调用，避免 PowerShell `--` 参数转发问题。test-target/source-included fixture 及工具链 warning 噪音另有台账，不再作为生产 dead-code 完成定义。

## 16. 回滚与分批交付

每个 Task 独立交付，禁止把“接入生产流程”和“大批量删除”放在同一个不可审查提交中。推荐每个领域使用三步提交：

1. 添加 RED 行为/故障测试和台账决策。
2. 接入唯一生产链路或隔离 test support。
3. 删除 superseded 代码、解除该模块 allow，并执行该领域退出门禁。

如果某个接入导致回归，回滚该领域 use case 和删除提交即可；不要恢复 blanket allow。若删除后发现外部兼容合同遗漏，应从 Git 历史恢复最小合同字段/适配器，并补 contract test 和具名 expect，不能恢复整套旧旁路。

## 17. 后续扩展规则

完成本计划后，新增 Rust 能力遵守以下规则：

- 新类型先有 use case 和 composition-root owner，再提交生产实现。
- 实验实现使用独立 feature 或草案分支，不进入默认发布构建。
- 新协议扩展既有 adapter contract；新路由语义扩展 canonical failure；新迁移版本扩展 registry/postcondition/fixture。
- 新测试辅助代码默认放在 `#[cfg(test)]`；确需跨 crate 时使用显式 `test-support` feature，并由 CI 验证默认关闭。
- 每个临时 expect 必须带删除条件；条件满足时由 CI/台账任务删除，不能永久沉积。

这样处理后，dead code 检查不再只是清理提示，而会成为架构反馈：它能尽早指出“能力写了但没有接入”“同一语义出现第二 owner”或“测试 API 泄漏到生产构建”，从而持续保护软件的可靠性、可维护性和可拓展性。
