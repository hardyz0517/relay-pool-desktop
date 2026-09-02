# 路由策略“保存即切换”升级计划

状态：Implemented，已完成首期 policy-only fast activation；timeout/出站代理仍按 Phase 3 保持完整 generation lane

日期：2026-09-01

适用范围：V3 路由策略保存、generation cutover、代理运行时快照、Tauri IPC、路由设置页及相关测试。

关联入口：

- [`../README.md`](../README.md)
- [`../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md`](../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md)
- [`../plans/2026-08-23-transport-timeout-hot-reload.md`](2026-08-23-transport-timeout-hot-reload.md)
- [`../plans/2026-08-23-routing-ownership-lifecycle-cleanup.md`](2026-08-23-routing-ownership-lifecycle-cleanup.md)

本文保留实施范围、决策和验收记录；当前运行行为以代码和自动化契约为准。

## 实施结果

- 已完成 typed `RoutingPolicyImpact` 字段影响分类，质量、熔断、策略-only 和传输变化有明确 owner；增加 exhaustive 分类和组合回归测试。
- 已完成 policy-only 快路径：复用现有质量/熔断 generation，在同一 generation coordinator 中执行严格的 generation、revision、watermark、fingerprint、checkpoint、qualification、CAS 和 fence 校验；不满足条件自动回到完整 generation lane。
- 已完成运行态发布资格判断。只有 `Running` 且 server/routing runtime 同时存在时才尝试实时发布；停止态保存返回 `persisted_only`，启动时从 SQLite 恢复。
- 已完成 IPC/前端状态契约：`activationPath`、`runtimeStatus`、`activeRevision`、封闭低基数 `fallbackReason`，以及 `waiting_latest_input`；轮询仅作为恢复兜底，快路径成功不依赖轮询。
- 已完成前端回归覆盖：停止态不轮询、同 revision 发布元数据刷新不覆盖 dirty 草稿、后续轮询保持 generation fence。

首期快路径仅覆盖策略分组、评分权重、倍率、回退、亲和和最大重试次数。`timeoutPolicy.*` 与出站代理变化仍然走完整 generation lane，待传输 snapshot/client-pool 的独立验收完成后再启用。

## 1. 执行摘要

当前所有 V3 路由策略保存都先写入 `staged`，再由后台任务创建、资格校验并原子切换 runtime generation。`component_rebuild_plan` 已经能够复用未受影响的质量/熔断组件，但“复用组件”仍然要等待统一 generation 流程，因此“全部分组”、评分权重、倍率等参数也不能保存后立即生效。

本计划不做整体重构，也不修改评分、重试或熔断语义。目标是增加一条受严格条件约束的 **policy-only fast activation**：

```text
完整策略草稿
  -> 严格校验 + 纯编译 + CAS 提交
  -> 校验质量/熔断组件指纹与输入水位未变化
  -> 复用现有不可变组件，创建新的 policy generation
  -> 现有 generation coordinator 原子激活
  -> 新请求读取新快照，在途请求继续使用旧快照
```

无法证明安全快切时，自动回到现有完整 generation lane。正确性优先于低延迟；快路径失败不能覆盖旧 active 策略，也不能把“被最新输入替代”显示成永久重建失败。

## 2. 当前基线与问题证据

| 事实 | 当前位置 | 影响 |
| --- | --- | --- |
| UI/文件入口先经过统一 mutation coordinator，提交后不直接发布运行时 | [`routing_policy_control_plane.rs`](../../src-tauri/src/application/routing_policy_control_plane.rs) | 保存返回 `staged`，需等待后台 runner |
| 质量/熔断组件可按字段复用 | [`routing_generation_cutover_runner.rs`](../../src-tauri/src/background_tasks/routing_generation_cutover_runner.rs) 的 `component_rebuild_plan` | 只能减少 replay，不能减少 generation/qualification/cutover 等待 |
| generation coordinator 负责 ready、qualification、fence 和 active registry | [`routing_generation_cutover_runner.rs`](../../src-tauri/src/background_tasks/routing_generation_cutover_runner.rs) | 不能绕过它直接改 active 行或内存配置 |
| 请求使用不可变运行时快照 | proxy runtime/transport policy 相关模块 | 可保证切换时在途请求不被改写，是快路径的基础 |
| 传输 timeout 已有运行时热加载能力，但保存入口仍受 generation 流程约束 | [`2026-08-23-transport-timeout-hot-reload.md`](2026-08-23-transport-timeout-hot-reload.md) | timeout 无需重启，但当前不是保存即切换 |
| UI 将 `staged`/`ready` 显示为等待状态 | [`LocalRoutingSettingsEditor.tsx`](../../src/features/routing/LocalRoutingSettingsEditor.tsx) | 用户无法区分“必要重建”和“可快速发布但尚未实现” |

## 3. 目标、不变量与非目标

### 3.1 目标

- 对不改变质量摘要、熔断状态机和输入水位的策略变更，在运行中的代理上完成一次短事务内的原子切换。
- 新请求使用新 policy/transport snapshot；已经取得旧快照的请求保持旧行为，不能被中断或半途改写。
- 质量/熔断相关参数继续走完整重建和资格校验，不以“即时”名义跳过 replay 或保护状态恢复。
- UI、受管 JSON、历史恢复等所有写入入口共享同一判定和激活路径。
- 并发保存、主动监控输入、熔断事件、代理停止/启动和崩溃恢复都保持单调 revision 与 SQLite 权威性。
- 用可测试的字段影响模型和组件指纹，避免以后新增字段时偷偷落入错误快路径。

### 3.2 非目标

- 不删除不可变 generation、CAS、fence、qualification 或 durable circuit 状态。
- 不直接更新 `routing_policy_v3_staged` 的 active 行来伪造即时生效。
- 不自动取消在途请求，不重置质量统计、熔断状态或亲和状态。
- 不重写评分算法、重试分类、容量准入、凭据存储或代理监听器生命周期。
- 不引入第二套配置来源、全局可变 policy、脚本化规则或云同步。

## 4. 参数影响模型

新增后端唯一 owner `RoutingPolicyImpact`（名称可按现有命名调整），在 canonicalize 和 domain validation 之后，对“当前 active policy -> 目标 policy”做 typed 比较。不得使用散落的字符串字段列表；优先使用按领域分组的 typed projection，并增加“所有公开字段均已分类”的测试。

### 4.1 影响分类

| 影响级别 | 字段 | 处理路径 |
| --- | --- | --- |
| `QualityRebuild` | `reliabilitySourceWeights.realTrafficPercent`、`monitoringPercent`；`reliabilitySampling.historicalMinimumSamples`、`recentMinimumSamples`、`optimisticReliabilityPercent`、`optimisticLatencyMs` | 完整质量 generation replay、验证和切换 |
| `CircuitRebuild` | `retry.consecutiveFailureThreshold`；`circuitBreaker.recoverySuccessThreshold`、`recoveryWaitSeconds` | 完整熔断 generation replay、状态恢复和切换 |
| `PolicyOnlyFastCandidate` | 四项评分权重、`allowDepletedFallback`、`affinityEnabled`、`affinityTtlSeconds`、`maxRateMultiplier`、`routingGroupFilter`、`retry.maxRetryCount` | 复用质量/熔断组件，满足安全条件时走快路径 |
| `PolicyOnlyFastCandidate + TransportPublish` | `timeoutPolicy.*`、`outboundProxyMode`、`outboundProxyUrl` | 同上；另外纯编译并发布新的 transport snapshot/client-pool fingerprint。首期可在快路径稳定后启用 |
| 非运行时元数据 | policy/document format version、`baseRevision`、算法/系统版本 | 只参与版本和 CAS 校验，不作为用户运行参数；变化时拒绝或进入明确升级流程 |

如果一次保存同时修改多类字段，取最高影响级别：`QualityRebuild` > `CircuitRebuild` > `PolicyOnlyFastCandidate`。质量和熔断只在各自输入或输入尾部确实变化时重建，但最终切换仍由同一个 generation coordinator 完成。

### 4.2 影响模型不变量

- 组件指纹必须包含所有会改变该组件结果的字段；无关字段变化不能导致无意义 replay。
- 指纹、checkpoint、输入 observation/circuit watermark 必须一起校验；只比较 policy JSON 不足以证明可以复用组件。
- 新增 V3 字段时，若未显式加入影响模型，Rust 测试必须失败，不能默认落入快路径。
- `source_profile_changed`、observation tail、circuit event tail 即使 policy 内容未变，也必须阻止对应快路径并回到完整 lane。

## 5. 快路径设计

### 5.1 提交前

1. `RoutingPolicyMutationCoordinator` 保持现有进程内 mutation gate。
2. 读取当前 active policy/generation，执行严格 decode、领域校验、timeout/client 配置纯编译。
3. 计算 `RoutingPolicyImpact`、quality/circuit/planner/transport fingerprints；无变化的完整文档按现有 no-op 规则处理。
4. 在 SQLite 短事务中执行 re-read、CAS、revision/history 写入及 staged 记录。提交前不修改 active runtime。

### 5.2 `try_fast_activate` 条件

提交后由同一个 coordinator 立即尝试快激活；以下任一条件不满足就返回“不可快切”，交给现有后台 generation runner：

- 当前存在 active runtime generation，且状态、quality/circuit checkpoint 均完整并已 qualified。
- 目标只属于 `PolicyOnlyFastCandidate`（或已明确开启的 transport 子集），没有 quality/circuit policy change。
- active generation 的质量和熔断输入水位与当前数据库尾部一致；没有 observation/circuit event tail、source profile 变化或正在进行的 fence。
- 目标 policy generation 仍是最新 revision，未被并发保存或文件 watcher 替代。
- 目标策略已通过纯编译；复用组件的 fingerprint、generation id、checkpoint ref 与 active registry 逐项相符。
- 代理运行态可接收新 snapshot；代理已停止时只返回 `persisted_only`，不得声称运行实例已生效。

这些检查必须是有界的数据库读和一次短写事务，不能等待质量 replay、主动 probe 完成或在途请求结束。

### 5.3 原子激活

新增窄 API（建议放在 `RoutingGenerationCoordinator`，而不是让 `RoutingService` 了解 proxy 细节）：

```text
activate_reused_components(target_policy_generation, active_generation_baseline)
```

该 API 在一个受现有 coordinator 保护的事务中：

1. 再次确认 active registry、目标 revision 和输入水位没有变化。
2. 注册一个新的 runtime generation，引用旧的 quality/circuit generation 与 checkpoint，但使用新的 policy generation/fingerprint。
3. 使用现有 active registry/CAS/fence 机制将新 generation 设为 active，并按既有生命周期规则处理旧 generation。
4. 事务提交后，通过长期 `TransportPolicyStore` 单调发布 transport snapshot；不得把原始 JSON 直接塞进 proxy execution。

快路径不创建第二个运行时 owner，也不修改旧 generation 的内容。若 CAS 竞争、checkpoint 不一致、数据库错误或 transport snapshot 编译失败，旧 active 保持不变，目标保持 staged/queued，随后由完整 lane 处理。

### 5.4 请求一致性

- ingress 在请求进入执行边界前只读取一次当前 `Arc` 快照。
- 快路径切换不获取请求级锁，不清空 client pool，不杀掉连接或 stream。
- 旧请求继续使用旧 policy、timeout、retry/circuit 视图；未开始的新请求才看到新 revision。
- 失败路径不得出现“数据库显示 active、内存回退到更旧 revision”的倒退；发布接口继续使用单调 revision 检查。

## 6. 完整 generation lane 的收口

快路径不是新的长期分叉。完整 lane 仍是唯一兜底路径，并做以下收口：

- 质量/熔断输入尾部在一次构建期间合并到最新 watermark，避免每个 probe 都制造一个无效中间 generation。
- 被新输入替代的 generation 使用 `waiting_latest_input`/`superseded_by_input_tail` 语义，不标记为业务重建失败。
- 同一 policy revision 只允许一个有效 build；旧 ready/build generation 按既有 stale/superseded 生命周期清理。
- 未变化的组件继续复用，但资格校验必须明确记录“复用的 checkpoint/fingerprint”，不能静默跳过验证。
- 后台 runner 继续负责重试、取消和崩溃恢复；快路径失败只入队，不复制一套 runner。

## 7. IPC、状态与 UI

### 7.1 后端契约

在不泄露内部 SQL/secret 的前提下，扩展发布状态以区分：

- `activationPath`: `fast`、`generation`、`persisted_only`；
- `runtimeStatus`: `active`、`staged`、`ready`、`waiting_latest_input`、`failed`、`expired`；
- `activeRevision` 与目标 revision 的单调关系；
- 可选的低基数 `fallbackReason`，例如 `quality_tail`、`circuit_tail`、`concurrent_revision`、`runtime_unavailable`。

优先复用现有 generation/status 表和 failure code；只有现有 schema 无法保留必要审计事实时才新增 migration。不得为快路径另建一套 policy 表或 outbox。

### 7.2 前端行为

- 快路径成功：保存命令在同一调用中返回 `active`，显示“已生效（后续新请求使用）”。
- 快路径条件不满足：显示“已保存，正在等待最新输入/重建”，不把可预期回退误报成失败。
- 完整重建：继续显示 `等待重建`、`等待切换`、`重建失败`，但 `superseded_by_input_tail` 显示为“等待最新输入收敛”。
- 轮询仍保留作为崩溃/跨进程恢复兜底；快路径成功后立即失效相关 query，不能依赖定时轮询才刷新。
- 在途请求边界、运行 revision、超时事实的展示沿用现有 read model，不在前端自行推导 active 状态。

## 8. 分阶段实施

### Phase 0：冻结基线与契约

- 补齐当前保存耗时、各状态停留时间、generation supersede 原因的低基数诊断。
- 为 `routingGroupFilter`、评分权重、timeout、质量采样和熔断字段各增加一条端到端基线测试。
- 明确 active/desired/runtime revision 的命名和 read model，冻结本计划中的字段分类。

出口条件：现有行为可稳定复现；没有测试依赖真实 secret、原始 URL 或本机数据库。

### Phase 1：typed impact 与 fingerprint

- 在 Rust domain/application 层实现影响分类、组件投影和稳定 fingerprint。
- 增加 exhaustive classification、canonical JSON、no-op、字段组合和新增字段保护测试。
- 先只记录 `fast_candidate` 诊断，不改变激活行为，验证分类与实际 replay 计划一致。

出口条件：所有公开 V3 字段有明确 owner；质量/熔断指纹变化与 `component_rebuild_plan` 完全一致。

### Phase 2：后端 policy-only fast activation

- 在 generation coordinator 增加复用组件的原子激活 API。
- 将 mutation coordinator 的提交后动作改为：尝试快路径，失败则交给现有 runner。
- 完成 active registry CAS、watermark/fingerprint 校验、并发 revision 和代理停止态处理。
- 保持旧 generation 与在途请求可读，验证旧组件不被修改或删除。

出口条件：仅修改分组/评分/倍率/回退/亲和/最大重试次数时，运行代理可在一次短事务后返回 active；任何不安全条件都会回到完整 lane。

### Phase 3：传输 timeout 与出站代理接入

- 复用 `TransportPolicyStore` 的 immutable snapshot 和单调 publish；将 timeout 编译放在提交前。
- 仅对新请求热加载；connect 变化按已有 client fingerprint 规则惰性创建新 client，旧 client 由在途请求自然持有。
- 明确代理停止、启动交错和 runtime publisher 失败的 `persisted_only` 语义。

出口条件：五项 timeout 不需要重启；timeout/出站代理变化不重建质量或熔断摘要，也不会无意义清空连接池。

### Phase 4：前端状态与文案

- 更新 DTO/bindings、publication polling 和设置页反馈，区分 `fast` 与 `generation`。
- 保留窄窗口、loading、error、conflict、unavailable、timed-out 状态。
- 为“保存即切换”的后续新请求边界增加可见 revision，而不是承诺中断在途请求。

出口条件：用户能够知道“已保存”“已生效”“仍在等待重建”三者差异；状态不可用时不作成功宣称。

### Phase 5：并发、故障与性能验收

- 覆盖 UI、受管 JSON、历史恢复、startup reconcile 四个入口共用同一快路径。
- 覆盖主动观测/熔断事件 race、双保存 42/43、CAS 冲突、崩溃重启、代理 stop/start、数据库 busy、publisher 失败和新输入替代。
- 测量 save-to-active 延迟、fast-path 命中率、fallback 原因、generation replay 次数和 client-pool 回收；只记录低基数指标。
- 目标是在常规本地数据库条件下将 policy-only 激活控制在一次短事务的 p95（建议目标 ≤300 ms）；达不到目标时不得牺牲一致性，应记录并回退完整 lane。

出口条件：focused tests、Rust/Cargo 检查、前端测试/build、`pnpm verify:fast` 均通过；性能结果和未覆盖边界写入审计记录。

### Phase 6：启用与回滚

- 首次发布先启用 planner-only 字段；timeout/出站代理在 Phase 3 验收后再启用。
- 保留一个内部 kill switch：关闭快路径后所有保存自动使用现有 generation lane，数据库格式和历史不变。
- 发现任何 active registry、在途请求快照或 revision 单调性问题，立即关闭快路径并保留诊断，不执行数据回退或破坏性清理。
- 连续稳定后再删除仅用于 shadow/diagnostic 的临时代码；不保留两个长期 policy runtime owner。

## 9. 验收矩阵

| 场景 | 预期结果 |
| --- | --- |
| 修改 `routingGroupFilter=all_groups`/指定分组 | 无质量/熔断 replay；满足条件时同一保存调用返回 active |
| 修改评分权重、倍率、亲和或最大重试次数 | 同上；新请求使用新 policy revision |
| 修改任一质量采样/来源权重 | 必须完整重建质量 generation，旧 active 保持到成功切换 |
| 修改连续失败阈值或熔断恢复参数 | 必须完整重建熔断 generation，不重置旧状态直到新 generation 合格 |
| 保存同时有 observation/circuit tail | 快路径拒绝，合并最新水位后走完整 lane |
| 两次并发保存 | 较新 revision 胜出；旧发布不能覆盖新发布 |
| 在途请求跨越保存 | 在途请求保持旧快照；后续请求读取新快照 |
| 代理已停止 | 保存可成功持久化，但返回 `persisted_only`；启动时从 SQLite 构建最新快照 |
| CAS 冲突、校验失败或编译失败 | 不写入 active、不发布 runtime，返回 typed error |
| 快路径内部失败 | 旧 active 不变，目标转交完整 lane；不得显示永久“重建失败” |
| 应用崩溃后重启 | registry、generation、policy revision 可恢复；无半激活状态 |

## 10. 风险与控制

| 风险 | 控制措施 |
| --- | --- |
| 快路径错误地复用过期质量/熔断组件 | typed fingerprint + checkpoint + watermark 三重校验；不满足即回退 |
| 新字段漏进快路径 | exhaustive classification 测试；禁止未知字段默认安全等级 |
| DB active 与内存 snapshot 不一致 | 提交前纯编译、提交后单调发布、代理停止态明确为 `persisted_only` |
| 快路径形成第二套生命周期 | 只在现有 generation coordinator 增加窄 API，复用同一 registry/CAS/fence |
| 高频 probe 使保存反复失效 | 合并最新输入尾部，区分 `waiting_latest_input` 与真正失败 |
| timeout 变化无意轮换大量 client | pool key 只使用真正影响 client 构造的 fingerprint，并设置有界回收 |
| 用户误以为保存会取消旧请求 | UI 和文档明确“后续新请求生效，在途请求保持旧快照” |

## 11. 交付门禁

- 不提交 API key、cookie、token、真实 URL、用户数据库或诊断原始 payload。
- 不修改或删除当前用户数据库，不使用破坏性 Git 命令，不自动 stage/commit/push。
- Rust 改动至少运行 `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml` 和相关 Cargo 测试。
- 前端改动至少运行相关 Vitest 与 `pnpm build`。
- 跨层契约和 generation 改动至少运行 `pnpm verify:fast`；若受运行中 exe 锁定等环境问题影响，交付时明确记录未完成检查及风险。
- 每个阶段完成后更新对应审计记录，记录实际命中快路径比例、fallback 原因和未覆盖故障场景。

## 实际验证记录（2026-09-01）

- `cargo check --locked --manifest-path src-tauri/Cargo.toml`：通过。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_policy_impact`：通过。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib fast_activation`：通过；包含快路径、组件回退、损坏元数据传播和持久化不可用场景。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib publication_availability`：通过。
- `pnpm.cmd exec vitest run src/features/routing/useRoutingPolicyDraft.test.ts src/features/routing/LocalRoutingSettingsEditor.test.tsx`：33/33 通过。
- `pnpm.cmd test`：140 个测试文件、655 个测试通过。
- `pnpm.cmd exec tsc --noEmit`：通过。
- `pnpm.cmd build`：通过。
- `pnpm.cmd generate:bindings --check`：通过，两次生成确定性校验通过；当前 IPC hash 为 `5b9823053a6dd7dc5d57d8d6684db1fb0ab771ec22950a18514d11d36fedaa8d`。
- `pnpm.cmd verify:fast`：dead-code、architecture bypass、TypeScript、Rust test topology、bindings、runtime event catalog、command registry、Tauri security、build entries、artifact policy、dependency lifecycle、ESLint 和 TypeScript check 均通过；最后 Rust architecture fixture 因运行中的 `relay-pool-desktop.exe` 文件锁退出（Windows `os error 5`），未将其误记为代码失败。
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`：仅被两处既有、与本任务无关的 alerting 格式差异阻塞：`src-tauri/src/application/alerting/ingress.rs:514`、`src-tauri/src/models/alerting/incident.rs:400`；本次未修改。
- 未执行完整 Cargo 测试矩阵：Windows 文件锁会阻止覆盖 `src-tauri/target/debug/relay-pool-desktop.exe`；已执行与本次改动直接相关的 Cargo 测试。
