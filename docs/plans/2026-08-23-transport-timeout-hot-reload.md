# 本地路由传输超时热加载实施计划

状态：核心链路已实施，控制面与状态展示仍按后续任务收口。本文定义将路由策略中的五项传输超时从“重启本地路由后生效”升级为“保存后对后续新请求生效”的实施路径；它不替代当前代码、自动化契约或目标规格。

日期：2026-08-23

关联入口：[`../README.md`](../README.md)、[`../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md`](../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md)、[`2026-08-20-intelligent-routing-retry-failover-configuration.md`](2026-08-20-intelligent-routing-retry-failover-configuration.md)

适用范围：`RoutingPolicyConfigV2.timeoutPolicy`、本地代理 ingress/execution/upstream client、路由策略保存与受管 JSON 导入、运行状态 read model、路由设置页。

不在本计划范围：监听地址或端口热切换、并发/内存预算热调整、自动中断在途请求、主动合成 probe、发布/安装包验证、真实 Provider smoke、性能压测和 `pnpm.cmd verify:release`。

> 所有任务均在现有工作区改动之上执行，不回退或覆盖无关改动。每项行为先增加最小 RED 证据，再变更唯一 owner；没有通过相应验证不得修改任务状态。

## 1. 背景与目标

当前 `TimeoutPolicyV2` 已随 routing policy 保存，但代理启动时会把它转换为 `ProxyServerLimits`，再编译为固定的 `TransportExecutionPolicyV1` 和 `UpstreamClientPool`。因此保存新的 timeout revision 不会改变已启动代理中的执行对象，设置页只能提示重启。

本计划完成后，五项字段的生效契约固定为：

| 策略字段 | 对后续新请求 | 对在途请求 | 对已构造 HTTP client |
| --- | --- | --- | --- |
| `connectSeconds` | 使用新 snapshot | 保持旧 snapshot | 新请求使用匹配 connect 配置的 client |
| `firstByteSeconds` | 使用新 snapshot | 保持旧 snapshot | 不适用 |
| `precommitSeconds` | 使用新 snapshot | 保持旧 snapshot | 不适用 |
| `bufferedExecutionSeconds` | 使用新 snapshot | 保持旧 snapshot | 不适用 |
| `streamIdleSeconds` | 使用新 snapshot | 保持旧 snapshot | 不影响连接池 idle 回收 |

这里的“新请求”是 ingress 完成本地鉴权、尚未读取 request body 时创建的请求。一个请求的 body read、规划、credential 选择、attempt、retry、fallback、响应 bootstrap 与 stream idle deadline 必须绑定同一个不可变 snapshot；保存不得修改其行为。

## 2. 目标架构与不可变决定

```text
Routing policy document / settings draft
  -> RoutingPolicyMutationCoordinator
  -> validate + compile TransportPolicySnapshot(revision)
  -> SQLite CAS commit
  -> TransportPolicyStore.publish_if_newer(snapshot)
  -> new ingress request loads one Arc snapshot
  -> execution / retry / response stream use that exact snapshot
```

### 2.1 配置、资源与状态的边界

| 类型 | Owner | 内容 | 激活方式 |
| --- | --- | --- | --- |
| `RoutingPolicyConfigV2` | routing policy aggregate | 用户可编辑的 `timeoutPolicy` 和其他路由意图 | SQLite revision/CAS |
| `TransportPolicySnapshot` | proxy transport policy | source policy revision、五项 `Duration`、已验证的派生 deadline | 新请求热加载 |
| `ProxyStartupResourceLimits` | proxy server composition | 连接数、并发、body/header/buffer budget、生命周期 writer 容量、shutdown | 启动时固定 |
| `CircuitBreakerPolicy` / `CircuitBreakerState` | health protection owner | 保护规则 / 跨请求状态 | 分离；timeout 保存不得重置 state |

`ProxyServerLimits` 的混合职责必须消除：完成 Task 1 后它不得再包含用户可编辑的五项上游 timeout。不得保留 `type ProxyServerLimits = ...` 兼容别名；Rust 编译错误应帮助找全遗留消费者。

### 2.2 请求一致性

- `TransportPolicySnapshot` 必须包含 `source_routing_policy_revision`，并由 `TimeoutPolicyV2` 的纯编译函数构造。它只说明 timeout 来自哪一版 aggregate，不得冒充请求的完整 routing/planning revision。
- 每个请求只允许在 ingress 读取一次当前 snapshot。execution、retry 或 stream wrapper 不得再次读取 `TransportPolicyStore`。
- retry 不得重置 precommit deadline；每个 attempt 使用同一剩余 request budget。
- in-flight request 持有 `Arc<TransportPolicySnapshot>`，保存新策略不取消、修改或重新计时该请求。
- 策略 revision 递增，晚到的发布绝不能覆盖较新的运行时 revision。

### 2.3 HTTP client 生命周期

`reqwest::Client` 的 connect timeout 和 connection pool idle timeout 在构造后不可安全修改。连接池 idle 回收是 transport infrastructure 参数，不是用户的 stream idle timeout；本计划把它归入 `ProxyStartupResourceLimits` 或独立的不可编辑 `UpstreamConnectionPoolLimits`，并删除现有用 `streamIdleSeconds` 配置 `pool_idle_timeout` 的耦合。

`UpstreamClientPool` 只按实际会改变 client 构造结果的配置隔离版本：

```text
{ outbound_proxy_route, upstream_client_config_fingerprint }
```

其中 `upstream_client_config_fingerprint` 当前仅由 connect timeout 和将来明确影响 reqwest builder 的字段计算；first-byte、precommit、buffered execution、stream idle 或单纯候选排序的保存不得无意义轮换连接池。新 client 配置惰性创建；旧配置不再承接对应新请求，已被 in-flight attempt 持有的 client 自然完成。cache 必须有明确上界与回收策略，不能因为连续保存策略无限保留 client。不得通过清空 pool、重启 server 或中断 stream 来实现切换。

### 2.4 代理生命周期与发布线性化

`TransportPolicyStore` 是 `ProxyRuntimeState` 的长期成员，不能在每次 `start` 时创建私有 store。启动、停止与策略发布必须通过仅覆盖配置切换的短时 `policy_activation_gate` 排序；请求读取 snapshot 永不获取该 gate。

- proxy start 读取 SQLite 后先执行 `publish_if_newer`，再用同一个长期 store 构造 ingress/executor；不得把启动时的 snapshot 复制为 executor 私有字段。
- 保存与外部文件导入在 CAS commit 后通过同一 gate 发布；若 start 与发布交错，revision 较大的 snapshot 必须留下来。
- stop 只停止 server 和 worker，不清空 store。停机期间的保存可以更新 desired snapshot，但 status 仍是 `persisted_only`，不能称为运行实例已生效。
- 启动前、启动中、运行中和停止中均不得存在第二个 transport policy source；测试必须覆盖 start/publish 交错。

### 2.5 提交与发布的失败语义

SQLite 是唯一事实来源。策略在 CAS 提交前必须完成 schema、领域和 transport snapshot 编译校验。提交后仅进行内存不可变 snapshot 发布，发布路径不允许 I/O。

- 代理运行：提交成功后以同一 revision 发布；API 成功仅在 runtime 已接收 snapshot 时返回。
- 代理停止：提交成功，返回 `persisted_only`；下次启动从 SQLite 构建 snapshot。
- CAS 冲突、验证失败：不写 SQLite，不发布 runtime。
- 受管 JSON 镜像失败：保持当前已提交策略与已发布 runtime，沿用现有 `pending_write` / `unavailable` 语义。
- 不得声称数据库事务与内存发布是一个原子事务；以“先完成纯编译、后 commit、立即单调 publish”保证运行态不会落到非法或过时版本。

## 3. 模块职责

| 路径 | 最终职责 | 本计划改动 |
| --- | --- | --- |
| `src-tauri/src/models/routing_policy.rs` | 持久化策略、默认值、字段验证与 V1/V2 upgrade | 保持 `TimeoutPolicyV2` 的唯一字段/范围 owner；不增加 proxy 状态。 |
| `src-tauri/src/services/proxy/limits.rs` | 启动资源限制 | 重命名并移除五项用户 timeout；保留 ingress 资源、预算和 shutdown。 |
| `src-tauri/src/services/proxy/transport_policy.rs` | 编译后的 request-local transport policy | 定义 immutable snapshot、revision、编译/校验和单调发布 store。 |
| `src-tauri/src/services/proxy/ingress.rs` | 请求接入与早期 deadline | 创建并附加 snapshot；不读取数据库。 |
| `src-tauri/src/services/proxy/runtime.rs` | proxy composition 与 runtime status | 组装 store、启动时加载持久化 revision、注入 ingress/executor，公开 active revision。 |
| `src-tauri/src/services/proxy/execution.rs`、`attempt.rs`、`response_body.rs` | attempt、retry 与流式执行 | 显式传递 snapshot，删除固定 transport policy/stream idle 字段。 |
| `src-tauri/src/services/proxy/upstream.rs` | client pool 与 outbound send | 使用 client 构造参数 fingerprint 的 pool key，执行有界回收。 |
| `src-tauri/src/application/routing.rs` | policy persistence/CAS | 保持 aggregate 校验与持久化，不依赖 proxy runtime。 |
| command facade 与 policy document runner | policy write orchestration | 统一调用 mutation coordinator，在 commit 后调用 runtime publisher。 |
| `src-tauri/src/ipc/dto/`、`src/lib/bridge/generated.ts` | 公共 read/write 契约 | 暴露 persisted/active revision 和 activation state；按生成流程更新。 |
| `src/features/routing/` | 草稿、设置、状态 UI | 复用既有 CAS 草稿；改写激活文案与运行 revision 展示。 |

禁止 `RoutingService` 直接依赖 reqwest、Axum 或 `ProxyRuntimeState` 的内部细节。运行时发布通过窄的 `TransportPolicyPublisher` port 完成；它只接收已经编译的 immutable snapshot，不接收原始 JSON 或整个 routing aggregate。`RoutingPolicyMutationCoordinator` 是 application composition owner，负责同时编排 `RoutingService`、该 port 和 document mirror；它不是领域模型或 proxy execution 的新依赖。

## 4. 实施任务

### Task 0：建立基线和删除台账

**目标：** 在不改变行为前冻结当前重启语义与相关资源边界。

**文件**

- Update: `src-tauri/src/services/proxy/transport_policy.rs`
- Update: `src-tauri/src/services/proxy/runtime.rs`
- Update: `src-tauri/src/services/proxy/upstream.rs`
- Update: 相关 proxy loopback/fault tests

**步骤**

1. 写出当前 `TimeoutPolicyV2 -> ProxyServerLimits -> TransportExecutionPolicyV1 -> UpstreamClientPool` 调用图。
2. 加入 RED test：运行中的代理保存 timeout policy 后，下一请求仍采用旧超时；该测试在 Task 4 后必须反转为 GREEN。
3. 加入资源回归：保存 timeout policy 不得改变 request semaphore、body budget、header/body admission timeout 或 server listener。
4. 列出待删除的固定字段：`ProxyExecutor.stream_idle_timeout`、固定 `ExecutionEngine` transport policy、`TransportExecutionPolicyV1::from_limits`、`ProxyServerLimits::from_timeout_policy`。

**完成条件：** 可用 focused test 重现“保存不生效”，且有明确删除清单；本任务不改变产品行为。

### Task 1：拆分启动资源与传输策略

**目标：** 消除 `ProxyServerLimits` 的混合语义。

**文件**

- Modify: `src-tauri/src/services/proxy/limits.rs`
- Modify: `src-tauri/src/services/proxy/server.rs`
- Modify: `src-tauri/src/services/proxy/ingress.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: 所有受影响 proxy tests

**步骤**

1. 将 `ProxyServerLimits` 重命名为 `ProxyStartupResourceLimits`。
2. 将五项 transport timeout 移出该 struct；`header_timeout`、`body_timeout` 仍属 ingress resource/protocol owner，不在本次设置页开放。
3. 删除 `ProxyStartupResourceLimits::from_timeout_policy`，使所有旧转换调用在编译期失败。
4. 调整 `IngressState`、server spawn、lifecycle writer 和 test fixture，使其只接收启动资源。
5. 证明启动资源的默认值和现有 resource safety tests 行为不变。

**完成条件：** 启动资源类型中不存在 `connect`、`first_byte`、`precommit`、`buffered_execution` 或 `stream_idle` 字段，且没有兼容 alias。

### Task 2：实现版本化不可变 transport snapshot

**目标：** 让运行时只有一个可安全替换的当前策略值。

**文件**

- Modify: `src-tauri/src/services/proxy/transport_policy.rs`
- Modify: `src-tauri/Cargo.toml`（仅当现有依赖不能提供等价原子 `Arc` 替换时）
- Create/update: transport policy focused tests

**步骤**

1. 将 `TransportExecutionPolicyV1` 演进为 `TransportPolicySnapshot`，包括 `source_routing_policy_revision`、五项 duration、`request_deadline` 与 validation 方法；另提供只含 client builder 输入的 `upstream_client_config_fingerprint`。
2. 添加 `compile_timeout_policy(policy: &TimeoutPolicyV2, revision: u64)` 纯函数；保持 `precommit <= buffered_execution` 的领域校验，拒绝零值、未知版本和无效 revision。
3. 添加 `TransportPolicyStore`，持有当前 `Arc<TransportPolicySnapshot>`。读取方法只返回 clone 的 `Arc`；不得返回可写引用。
4. 实现 `publish_if_newer(snapshot)`；保存端串行化并拒绝 `snapshot.revision <= active.revision` 的覆盖。
5. 优先使用 lock-free `ArcSwap`。若引入新 crate，先确认许可证与 Cargo.lock 变更合理；不得用全局 async mutex 包住请求读取。

**完成条件：** unit tests 覆盖编译、非法值、单调 revision、多个读者持有旧 snapshot 以及发布后新读者得到新 snapshot。

### Task 3：在 ingress 固定请求快照

**目标：** 将超时语义固定到完整请求生命周期，而不是固定到 proxy generation。

**文件**

- Modify: `src-tauri/src/services/proxy/ingress.rs`
- Modify: `src-tauri/src/services/proxy/request.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Update: ingress focused tests

**步骤**

1. `IngressState` 接收 `Arc<TransportPolicyStore>`。
2. 本地鉴权成功、body read 之前调用 `store.load()`；将返回的 `Arc<TransportPolicySnapshot>` 写入 request timing/context。
3. 用 snapshot 的 precommit deadline 包住 body read、admission 和后续 execution 的总预算；不得在 body read 后才选择 snapshot。
4. 缺失 snapshot 视为内部 lifecycle/configuration failure，fail closed，不允许回落到隐式 default timeout。
5. 使 test fixture 可以注入固定 store，并断言请求实际携带的 revision。

**完成条件：** 同一 request 在 store 发布新 revision 后仍持有开始时的 snapshot；未开始的新 request 才获取新 revision。

### Task 4：让 execution、retry 和 stream 使用 snapshot

**目标：** 删除代理 generation 固定的 transport policy。

**文件**

- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/attempt.rs`
- Modify: `src-tauri/src/services/proxy/response_body.rs`
- Update: execution/stream fault tests

**步骤**

1. 删除 `ProxyExecutor` 固定的 `stream_idle_timeout` 与固定 `ExecutionEngine` policy 构造参数。
2. 将 request snapshot 显式传入 execution entry、attempt executor 和 stream wrapper。
3. 所有 retry/fallback 使用同一 snapshot 的 `remaining_request_deadline`；不得为第二个 attempt 重建 timeout 或加载最新 policy。
4. buffered execution 和 first-byte timeout 必须与同一 precommit budget 取最小值；stream idle 在 response body wrapper 使用 request snapshot。
5. 保持 ReplayGate、FailureClassifier、RetryActionPlanner 和 health protection 的既有 owner；本任务不改变哪些错误可重试、何时熔断或候选排序。

**完成条件：** 代码中 execution path 不再持有启动时 `TransportExecutionPolicy`；每个 transport deadline 可追溯到 request snapshot。

### Task 5：将上游 client pool 按 revision 轮换

**目标：** 正确处理 reqwest 的构造期 timeout，不影响在途请求。

**文件**

- Modify: `src-tauri/src/services/proxy/upstream.rs`
- Modify: `src-tauri/src/services/proxy/attempt.rs`
- Update: upstream client pool tests

**步骤**

1. 将 pool cache key 扩展为 outbound proxy route 与 `upstream_client_config_fingerprint`，而不是 aggregate revision。
2. 以 request snapshot 构造/获取 client；`build_client` 只读取传入 snapshot，不读取全局 store。
3. 对历史 client fingerprint 的 cache 设置有界回收：只保留当前配置和有限数量的最近闲置配置；in-flight client 不得被强制销毁。
4. 为构造 client 增加 test seam，验证 connect timeout 变化会使用新 client，first-byte、precommit、buffered execution、stream idle 或无关 routing 字段变化不会无谓替换 client；连接池 idle 参数不再来自 stream idle。
5. 记录低基数的 cache rotation/reclaim 指标或诊断计数，不记录 URL、凭据或原始 header。

**完成条件：** 保存策略后无需重启代理，新请求得到新 client；旧 request 完成后旧 client 可回收，cache 增长有上界。

### Task 6：统一提交后的运行时发布

**目标：** 确保 UI、受管 JSON、历史恢复和未来导入都不会漏掉热发布。

**文件**

- Modify: `src-tauri/src/application/routing.rs`
- Modify: `src-tauri/src/application/command_facades/routing.rs`
- Modify: `src-tauri/src/application/app_services.rs`（仅用于 composition/port 注入）
- Modify: managed document reconciliation 的调用入口
- Update: routing policy CAS/document tests

**步骤**

1. 定义窄 `TransportPolicyPublisher` port，只接收已编译 snapshot 并返回 `running_applied` 或 `persisted_only`；`ProxyRuntimeState` 用长期 store 和 `policy_activation_gate` 实现该 port。
2. 创建或收口 `RoutingPolicyMutationCoordinator`；UI apply、文件 watch/import 与历史恢复全部调用它，禁止绕过 coordinator 直接写 active policy。现有 `background_tasks/policy_document_runner.rs` 必须改为注入该 coordinator/port，不能继续只持有 `PersistenceHandle` 后直接调用 `RoutingService`。
3. coordinator 在 CAS 提交前编译 snapshot，提交后立即执行单调 publish；document mirror 保持既有提交后的 best-effort 同步。
4. 代理启动时从当前 SQLite policy revision 编译初始 snapshot，并通过长期 store 的 `publish_if_newer` 安装；代理停止时保存返回 `persisted_only`，不制造假的 active runtime revision。
5. 测试并发保存：revision 42 的延迟发布不得覆盖已发布的 43；CAS conflict 不得发布任何新 snapshot；另测试 start/publish 与 stop/publish 交错仍得到单一单调 store。

**完成条件：** 所有 policy 写入入口共享同一 post-commit activation path，且运行时永不倒退到较小 revision。

### Task 7：状态、IPC 与设置页

**目标：** 让用户看到真实激活状态，而非“已保存但不知道是否生效”。

**文件**

- Modify: `src-tauri/src/application/command_facades/routing.rs`
- Modify: `src-tauri/src/ipc/dto/routing_mutations.rs` 与相关 read DTO
- Regenerate: registry、TypeScript binding、ACL manifest（使用仓库既有生成命令）
- Modify: `src/lib/types/routing.ts`、`src/lib/bridge/generated.ts`、`src/lib/api/routing.ts`
- Modify: `src/features/routing/LocalRoutingSettingsEditor.tsx`
- Modify: `src/features/routing/LocalRoutingStatusTab.tsx`
- Update: relevant Vitest files

**步骤**

1. 在 routing status/apply response 中返回 `persistedRoutingPolicyRevision`、`activeTransportPolicySourceRevision`、`proxyRunning` 与 activation enum；trace 中的完整 routing revision 与 transport source revision 必须允许不同，且用不同字段/文案表达。
2. activation enum 只允许 `new_requests`、`persisted_only` 和 `restart_required_resource`；前端不得根据文案自行推断状态。
3. 超时区说明改为“保存后仅影响后续请求；进行中的请求保持原设置”，删除“需要重启本地路由”。
4. 代理停止时显示“已保存，将在下次启动本地路由时使用”，不得显示不存在的运行中 revision。
5. 将当前 request 的完整 routing/planning revision 与 transport source revision 分别保留在 trace/summary 中；UI 不得把其中任一字段说成“所有路由设置都来自同一 revision”，除非未来引入整个 request policy snapshot。
6. 覆盖 loading、保存中、CAS conflict、字段错误、代理停止、运行 revision 暂时滞后和窄窗口布局。

**完成条件：** 页面、status API 和真实 runtime 使用同一 activation 语义；用户不再需要重启代理来使 timeout 对新请求生效。

### Task 8：清理、文档和架构门禁

**目标：** 防止双 owner 和过期重启语义回流。

**文件**

- Modify: `docs/specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md`
- Modify: `docs/README.md`
- Update: architecture/single-owner scripts only when已有门禁覆盖该边界
- Remove: 已无调用方的旧转换、固定 policy constructor 和过期测试名称

**步骤**

1. 将当前规格的 timeout activation 从 `restart_required` 改为 `new_requests`，并写明在途请求固定旧 snapshot。
2. 更新本文件状态与 README 导航；不要把计划本身写成代码事实。
3. 添加静态/架构断言：execution 不得读取 SQLite、`ProxyStartupResourceLimits` 不得含五项 timeout、只有 coordinator 能调用 runtime publisher。
4. 删除旧文案、无消费者 helper、兼容 alias 和“保存后重启”测试。

**完成条件：** 搜索不到旧的 `from_timeout_policy -> limits -> transport policy` 生产链，所有当前文档对超时激活语义一致。

## 5. 验收矩阵

| 场景 | 预期结果 |
| --- | --- |
| 代理运行，保存更短 first-byte timeout | 后续慢请求按新值 timeout，无需重启。 |
| 代理运行，保存更长 first-byte timeout | 后续慢请求按新值成功。 |
| 慢请求进行中时保存 | 在途请求使用旧 revision/旧 deadline；不被取消或延长。 |
| 同一请求发生 retry/fallback 时保存 | 全部 attempt 使用请求开始时 snapshot。 |
| 保存 connect timeout | 后续请求使用匹配新 connect 配置的 client；旧连接的在途请求可完成。 |
| 保存 first-byte/precommit/buffered/stream idle | 后续请求使用新 request deadline；client 不因这些字段无谓轮换。 |
| 频繁保存多次 | source revision 单调递增；client cache 按 fingerprint 有界。 |
| CAS conflict / 字段验证失败 | SQLite 与 runtime 都保持旧 revision。 |
| 代理停止时保存 | SQLite 更新，状态为 `persisted_only`；下次启动加载新 revision。 |
| 保存 timeout policy | body budget、并发、端口、listener、熔断状态与重试类别不变。 |
| 受管 JSON 导入或历史恢复 | 经相同 coordinator 发布，不能只更新数据库。 |

## 6. 验证策略

每个 Task 先运行直接相关测试；本计划不要求发布级验证。最终跨层切换完成后最低运行：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib transport_policy -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
pnpm.cmd test -- src/features/routing/LocalRoutingSettingsEditor.test.tsx
pnpm.cmd test -- src/features/routing/LocalRoutingStatusTab.test.tsx
pnpm.cmd build
pnpm.cmd verify:fast
```

若实际测试目标名称因实现拆分而变化，应运行覆盖同一验收矩阵的最窄测试集合，并在实施记录中注明。不得把未运行的真实 Provider、安装包、性能压测或 release gate 说成已通过。

## 7. 完成定义

只有同时满足以下条件，才能将本计划标记为已实施：

1. 五项 timeout 保存后对后续请求生效，不需要重启本地路由。
2. 在途请求、retry/fallback 链和 stream 均固定使用一个不可变 revision snapshot。
3. 启动资源、传输策略和熔断状态具有独立类型与唯一 owner；不存在 `ProxyServerLimits` 混合对象或长期 compatibility alias。
4. 上游 client pool 仅按 client 构造 fingerprint 安全轮换并有界回收，不中断在途请求；stream idle 不再兼任连接池回收时间。
5. UI、IPC、trace、受管 JSON 和历史恢复对激活状态使用同一契约，start/stop 与发布交错时没有第二个策略 source。
6. 验收矩阵和第 6 节要求的相关检查通过；未验证范围被明确记录。
