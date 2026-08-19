# 本地路由模型映射策略实施计划

状态：Phase 1、Phase 2、bounded Phase 3 runtime，以及 routing-policy 完整 document apply、typed trusted source context 和共享 document coordinator 已实施并可手测；legacy mutation notice 覆盖、watcher restart/overflow 的 release qualification、legacy alias 退役和 release/live-provider qualification 仍是明确的后续工作。本计划不把这些剩余项误写成已完成的发布资格。

本轮执行记录（2026-08-18）：Phase 1 的 Chat、Responses、Responses-to-Chat、Embeddings、`/models`/`/usage` bypass、CAS/history、bounded trace、IPC 和 Routing 页面已通过相关测试；Phase 2 已接入 Profile/Binding、fallback chain、`CandidateModelVariant` planner/admission/retry identity、native model capability identity、fallback trigger 和迁移 review UI；Phase 3 已启用有界 glob、编译期 overlap/shadowing diagnostics。共享 `PolicyDocumentCoordinator` 已在 composition root 启动 native `notify` watcher，使用 750 ms coalescing、watcher error/overflow immediate reconciliation/rebuild 与 30 秒 digest reconciliation 覆盖 `routing_policy` / `model_mapping` 两种 document kind。routing-policy 完整 apply 使用 document `baseRevision` CAS，内部适配器使用 typed trusted source；当前最高 migration 为 `0046_model_mapping_rejection_metadata.sql`。

收尾验证记录：`application::model_mapping` 34 passed（含 stale external base-revision rejection 和 startup crash-left materialized mapping recovery），`services::policy_documents` 6 passed，错误信封专项 12 passed，mapping DTO 5 passed，migration 专项 1 passed，mapping/routing Vitest 8 passed，routing loopback E2E 8 passed。`pnpm.cmd generate:bindings --check`、`pnpm.cmd test:contracts`、`pnpm.cmd exec tsc --noEmit`、`pnpm.cmd build`、`cargo fmt --check`、`cargo check --locked` 与 `git diff --check` 均通过。schema fingerprint 与 precommit budget 的短时序测试已隔离重跑通过。剩余控制面差距记录在 `docs/audits/model-mapping-control-plane-gap.md`，不阻塞用户手动验证已实现的 fixed、Profile/Binding、fallback 与 bounded-glob mapping。

日期：2026-08-17

批准设计：`docs/proposals/MODEL_MAPPING_STRATEGY_SPEC.md`（Phase 1、Phase 2 与 bounded Phase 3 已进入当前实现；控制面收口与发布资格仍按 gap 文档管理）

前置设计：`docs/specs/ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md`（拟实施规格）

适用范围：本地 OpenAI-compatible Proxy、路由策略配置控制面、SQLite migration、Planning Snapshot / Candidate Plan / attempt commitment、模型能力事实、请求与路由诊断、Tauri IPC、生成 bindings 与 Routing 页面。

历史关系：本计划在获批后取代现有 `model_aliases(client_model, upstream_model)` 的正常运行时解析、其前端增删改 API，以及 proxy / Planning Snapshot 的双重别名解析。旧表只作为一次性迁移输入和只读审计来源保留到规定的回滚窗口结束；不允许长期 dual-read、双 resolver 或旧表 fail-open fallback。

> 本计划每个 Task 都以 RED-GREEN-REFACTOR 执行。RED 必须能证明目标行为尚未成立或旧路径仍可达；GREEN 后运行该 Task 的门禁。任何必跑命令未实际退出 0，Task 即未完成。

---

## 1. 执行前决策与完成定义

在 Task 0 结束前冻结下列产品决策，写入 decision record 和测试 fixture，不在实现期间临时改变：

1. `map_fallback_chain` 最大长度为 3。
2. `unmatched_model_behavior` 默认 `preserve`，保留 `reject`。
3. 无 Binding 的 Profile 可使用显式 `default_upstream_model`，UI 和 trace 标记为 default / 未验证。
4. legacy 同一 client model 的多个启用 alias 不自动转为回退链，只保留当前创建时间最早的首项并创建 review record。
5. `retry_exhausted_before_output` 是 Phase 2 的高级选项，默认关闭；Phase 1 不实现。
6. `/v1/models`、`/usage` 和无 `model` 的入口绕过 mapping resolver；虚拟模型目录另行设计，不在本计划偷渡实现。

完成后的生产数据流固定为：

```text
Ingress immutable request facts
  -> CompiledModelMappingConfiguration snapshot
  -> ResolvedModelPlan
  -> PlanningSnapshot candidate projection
  -> CandidateModelVariant by target rank
  -> existing eligibility / tier / score / capacity lease
  -> frozen TargetExecutionCommitment
  -> endpoint adapter rewrites candidate upstream_model
  -> outcome / capability fact / decision trace
```

只有同时满足下列条件，整个计划才完成：

- 所有带模型的推理请求只经一个大小写敏感、trim 后的 Mapping resolver；proxy、Planning Snapshot、能力 subject、target resolver 与 endpoint adapter 不再各自扫描 alias 或猜测模型名。
- `requested_model`、`route_model`、`upstream_model`、mapping revision、resolution fence、rule ID 和 target rank 可在 request trace 中关联。
- 配置、Profile、Binding 通过一个完整文档与单一 `baseRevision` CAS 保存；SQLite 是 active truth，文件只作为受管镜像和输入入口。
- Phase 1 能稳定交付 `codex-5.4 -> 任意用户填写的上游模型`，覆盖 Chat、Responses、Responses-to-Chat 与 Embeddings。
- Phase 2 能让同一 logical Profile 在不同 Station / Key 使用不同 `upstream_model`，并且 Key 共享容量、variant 隔离模型错误、模型回退不越级。
- 新旧 alias 正常运行时路径、别名 UI command、行级直接 mutation、`model_alias_revision` 作为模型能力主身份的写入全部删除。
- 生产、模拟、前端预览和请求解释只消费后端 compiler / resolver / read model；日志、IPC、fixture 和导出不含 secret、认证头或原始请求正文。

## 2. 前置条件、执行纪律与不可混入事项

### 2.1 前置条件

1. `MODEL_MAPPING_STRATEGY_SPEC.md` 由设计评审批准，且第 13 节决策已冻结。
2. `ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md` 至少完成其后端 document codec、CAS、history、document sync、revision notice 与恢复契约。若尚未完成，Task 1 必须先将其实施为通用 `document_kind` 基础设施；不能让模型映射另起 watcher / outbox。
3. 执行者在每个 Task 开始前记录 `git status --short --branch`、`git log -5 --oneline`、迁移最大编号及 `pnpm --version` / `cargo --version`。不覆盖既有用户改动。
4. 每个新 migration 的编号从 `src-tauri/src/persistence/migrations/` 实际枚举，本文统一写为 `00NN`；不得修改已发布 migration。

### 2.2 不变量

- 不引入 JavaScript、Lua、SQL、shell、WASM、webhook 或任意用户代码执行。
- Mapping rule 只读取 immutable `ModelRequestFacts`；不得读取 API key、header、prompt、余额、时间、健康、容量或实时上游状态。
- 配置 mutation 只能走完整 document apply；前端局部编辑只是 draft reducer，store 不得暴露绕过 CAS 的 rule/profile/binding CRUD。
- Mapping document 的 `baseRevision` 是 apply 的唯一 CAS 前置条件。`apply_model_mapping_document` 不再接收第二个 `expectedRevision`。
- 无效 draft 不能进入 proxy。代理启动或 revision reload 无法获得已验证 snapshot 时，对带模型推理请求 fail closed 为 `model_mapping_configuration_unavailable`。
- `unknown` 模型能力可在没有已知不兼容事实时作为未验证候选尝试；`unsupported`、Key 级 endpoint / feature hard gate 和其他既有资格规则必须严格排除。
- 容量继续按 Key / Station account / provider account 计数，不能按 model variant 计数。
- 任何在请求中持有的 compiled mapping snapshot 必须一直存活到最后一个 attempt 完成；后续配置更新只影响新 ingress。

### 2.3 原子 cutover 规则

下列组合不得作为可交付状态：

- 新 Mapping resolver + `model_aliases` fallback read；
- proxy 使用新 resolver、Planning Snapshot 仍自行 `eq_ignore_ascii_case` 解析；
- Profile / Binding 已启用、candidate / retry identity 仍只有 `station_key_id`；
- `model_not_found` 使用 mapping revision 作为 capability 主键；
- 前端编辑完整 document、后端仍暴露 alias 行级写入；
- mapping 自建文件 watcher、sync table 或 retry runner；
- 新 IPC 已注册但 TypeScript binding 手写或未生成；
- 新生产 UI 与 alias UI 同时修改 active mapping truth。

Phase 1 的 Tasks 4-7 是一个 cutover unit：可分提交准备，但只在 backend、proxy、IPC、frontend、删除清单和相关测试都到位时标记 Phase 1 完成。Phase 2 同样将候选 variant、能力事实、回退与 Profile UI 作为一个 cutover unit。

## 3. 依赖图

```text
0 Baseline / decisions / red fixtures
  -> 1 Shared document-kind control plane
  -> 2 Mapping domain, strict document codec, compiler and resolver
  -> 3 Schema, stores, legacy migration and capability identity bridge
  -> 4 Mapping application service and snapshot publisher
  -> 5 Phase-1 proxy / planning / lifecycle cutover
  -> 6 Phase-1 IPC, read models and generated bindings
  -> 7 Phase-1 routing UI and alias UI deletion
  -> 8 Phase-1 qualification and operational migration review
  -> 9 Phase-2 Profile / Binding and CandidateModelVariant execution model
  -> 10 Phase-2 capability facts, fallback semantics, diagnostics and UI
  -> 11 Phase-2 cutover qualification and legacy table retirement decision
  -> 12 Phase-3 glob compiler and overlap diagnostics
  -> 13 final qualification, documentation and closeout
```

Tasks 1-3 可以并行准备设计和 RED fixture，但只能按图中的顺序合入 production path。Task 7 不得在 Task 6 前调用未生成的 binding。Task 9 与 10 不能拆成“先启用 Profile，再补 retry identity”的发布。

## 4. 目标文件地图

具体文件名允许在 Task 0 根据已实现的 routing-policy configuration control plane 微调；下列 owner 边界不可改变。

| 路径 | 最终职责 |
| --- | --- |
| `src-tauri/src/models/model_mapping.rs` | 完整 document、Rule/Profile/Binding 值对象、受限 tagged union、字段边界与 DTO-neutral validation |
| `src-tauri/src/application/model_mapping/{mod,compiler,resolver,service,query}.rs` | 完整 document admission、纯编译、请求解析、immutable snapshot 发布、CAS orchestration、workspace / trace read model |
| `src-tauri/src/persistence/stores/model_mapping_store.rs` | mapping aggregate、history、normalized Rule/Profile/Binding、legacy review SQL；不处理文件或 proxy |
| `src-tauri/src/persistence/stores/*document*_sync_store.rs` | 以 `document_kind` 分区的 durable coalescing materialization state；不含 mapping 业务校验 |
| `src-tauri/src/services/*policy*_document/` | 共享 strict JSON codec、atomic materializer、watcher、reconciliation；新增 mapping kind，不新增第二套服务 |
| `src-tauri/src/application/operational_facts/{assembler,reader,planning_snapshot,target_resolver}.rs` | 把 frozen plan / variant 引入事实、资格、commitment 与 revalidation；删除 ModelAliasFact 生产消费 |
| `src-tauri/src/application/routing_engine/{request,planning_snapshot,candidate_plan,coordinator}.rs` | `ResolvedModelPlan`、`CandidateModelVariant`、rank-aware planning / retry identity；不读 SQLite |
| `src-tauri/src/services/proxy/{execution,routing_repository,endpoint_adapter,attempt,lifecycle/*}.rs` | ingress 安装 plan、传递 commitment、按 frozen `upstream_model` 改写 body、记录 outcome；不解析规则 |
| `src-tauri/src/persistence/stores/routing_health_verdict_store.rs` | 既有 scoped health 与 native-model capability facts 的清晰边界；不再把 mapping revision 作为 capability identity |
| `src-tauri/src/application/request_finalization/*` 与 `src-tauri/src/persistence/stores/{request_*,routing_decisions/*}.rs` | 持久化三种模型身份、mapping revision、fence、variant 和可展示 decision evidence |
| `src-tauri/src/ipc/dto/model_mapping.rs` 与 command facade / registry | consumer DTO、typed errors、command registration；TypeScript 由 generator 产生 |
| `src/lib/{api,queries,types}/modelMapping*.ts` | mapping API、query keys、server-state types；不实现规则解释 |
| `src/features/routing/model-mapping/` | 规则列表、draft editor、preview、Profile / Binding、migration review；复用 Routing 页面容器和 query invalidation |
| `src/lib/bridge/{BackendClient,DesktopBackend,DemoBackend}.ts` | 新 command pass-through 和 demo unsupported behavior；不手写 generated binding |
| `src-tauri/src/persistence/migrations/00NN_*.sql` | append-only structural/data migration 和 compatibility metadata |

现有 `src-tauri/src/application/routing_engine/model_alias.rs`、`model_aliases` 的正常读取 API、`src/lib/api/routing.ts` 中 alias CRUD、`src/features/routing` 的 alias UI 和相关 generated command 必须列入删除台账，不作为兼容 facade 长期保留。

---

## Workstream A：基线、控制面与领域内核

### Task 0：冻结基线、决策和删除台账

**Files**

- Create: `docs/audits/2026-08-17-model-mapping-baseline.md`
- Create: `docs/audits/model-mapping-acceptance-matrix.md`
- Create: `docs/audits/model-mapping-deletion-ledger.md`
- Modify: 本计划和获批后的模型映射规格状态
- Read only: proxy、operational facts、routing health、request finalization、routing UI、generated bindings、schema / portable migration modules

**Steps**

- [ ] 记录 dirty paths、HEAD、migration 最大编号、当前 alias 行数及重复 `client_model` 分组，只记录计数和 fake fixture，不记录真实模型或 secret。
- [ ] 从 ingress 到 endpoint adapter 画调用图，精确登记 `model_alias::mapped_model`、`RoutingRepository::load_model_alias_pairs`、`planning_snapshot::resolved_model`、`model_alias_revision`、`resolved_upstream_model` 的每个生产读写者。
- [ ] 将规格第 10、11、12 节逐条映射到 Task、owner、RED test、GREEN test 与最终验证命令；未映射项阻止 Task 1。
- [ ] 冻结第 1 节列出的产品决策；每一项建立 golden fixture 和用户可见文案 key。
- [ ] deletion ledger 至少登记旧 alias command、store methods、`ModelAliasFact`、每请求 `load_model_alias_pairs`、Planning Snapshot re-resolve、alias UI、generated DTO、旧 schema / portable catalog references。
- [ ] 写 RED 回归：当前 alias 重复顺序、proxy exact 与 snapshot case-insensitive 不一致、改 alias 使 capability identity 失效。RED 必须可复现，不能仅 source grep。

**Run**

```powershell
git status --short --branch
git log -5 --oneline
Get-ChildItem src-tauri/src/persistence/migrations -File | Sort-Object Name | Select-Object -Last 5 -ExpandProperty Name
rg -n "model_alias|ModelAlias|model_alias_revision|resolved_upstream_model" src-tauri/src src
pnpm.cmd test -- src/lib/api/routing.test.ts src/lib/queries/routingQueries.test.ts
cargo test --locked --manifest-path src-tauri/Cargo.toml model_alias -- --nocapture
```

**Exit gate**: 每个生产 alias consumer 和每个 `model_alias_revision` 读写者有删除或迁移 Task；所有规格验收项有可执行证据；决策已冻结。

### Task 1：将受管配置控制面泛化为 `document_kind`

**Files**

- Modify/Create: routing-policy configuration control plane 的 application service、sync store、document service、IPC DTO 与 revision notice owner
- Create: mapping document kind fixture、canonical JSON fixture、file watcher / reconciliation fixture
- Modify: document sync migration / postcondition（编号以实际最大 migration 为准）

**Steps**

- [ ] 先完成或验证 routing-policy configuration spec 的单一 `PolicyDocumentCoordinator`、严格 JSON decoder、canonical formatter、CAS、history、after-commit revision notice、atomic materializer 与 30 秒 digest reconciliation。
- [ ] 将 durable sync state 的 key 改为 `document_kind`，至少支持 `routing_policy` 和 `model_mapping`；状态只保存最新 desired revision / digest / typed error，不能为 mapping 建 FIFO outbox 或第二个 watcher。
- [ ] `document_kind` 决定文件名、document codec、history loader 和 authorized service；proxy 永远不读取文件。
- [ ] mapping 文件固定为受控配置子目录内的 `model-mapping.json`。数据目录迁移、portable import/export、恢复、startup / resume 与 watcher overflow 都复用同一协调器。
- [ ] 确认 duplicate JSON key、JSONC、未知字段、未知 document format、过长文件、symlink / reparse point、临时半文件均 fail closed；日志和 IPC 不返回原始无效文件内容。
- [ ] 文档 apply 统一只以 document 中的 `baseRevision` CAS；history restore 可保留独立的 typed `expectedRevision` 契约，不能把两者混入普通 apply。

**Exit gate**: `model_mapping` 能通过共享控制面获得 document status、materialization、watcher reconciliation 与 revision notice；没有映射专用 watcher / retry table / direct filesystem write。

### Task 2：建立 Phase-1 领域类型、严格 document codec 与纯 compiler

**Files**

- Create: `src-tauri/src/models/model_mapping.rs`
- Create: `src-tauri/src/application/model_mapping/{mod,compiler,resolver}.rs`
- Create: compiler、resolver、document codec golden tests
- Modify: `src-tauri/src/models/mod.rs` 和 application module registry

**Steps**

- [ ] 定义 `ModelMappingDocumentV1`、`ModelMappingPolicy`、`ModelMappingRule`、`RuleConditions`、`Matcher`、`Action`、`TargetRef`、稳定 error code 和明确长度 / 数量上限。
- [ ] Phase 1 decoder 仅接受 `exact` / `default` 与 `map_fixed(literal)` / `preserve` / `reject`；拒绝 glob、Profile target、fallback chain 和任何未知 tagged variant，防止写入无 consumer 的配置。
- [ ] 统一客户端模型规范化为 Unicode whitespace trim，之后所有 identity 比较保持大小写敏感和字节精确；将这一规则放在单一 helper，不在 SQL / React / adapter 各自实现。
- [ ] compiler 完成 rule precedence、同优先级 overlap、全遮蔽、无条件 default、空 / 重复 target、无效条件、边界和 canonical diagnostic 检查。Phase 1 无 glob 时必须显式拒绝 glob，不能静默按字符串处理。
- [ ] resolver 输入仅为 immutable `ModelRequestFacts`，输出 `ResolvedModelPlan` 的稳定决策投影。它不读 DB、文件、secret、健康、容量或 wall clock；`resolved_at_ms` 由调用层写入，测试比较不含时间与 snapshot 句柄。
- [ ] 明确 `/models`、`/usage`、无 model 请求 bypass resolver；请求 model 缺失 / 类型错误 / 超长继续由 ingress 返回客户端输入错误。
- [ ] 为 Phase 2 类型预留 non-public internal slots 仅限此模块，不向 document decoder、IPC 或 UI 暴露 Profile / Binding / fallback 值。

**Run**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml model_mapping -- --nocapture
```

**Exit gate**: compiler / resolver 在纯函数测试中证明 deterministic；所有 Phase 2/3 syntax fail closed；同一输入生成相同稳定决策投影。

### Task 3：迁移、存储和 legacy alias 迁移边界

**Files**

- Create: `src-tauri/src/persistence/migrations/00NN_model_mapping_foundation.sql`
- Create: `src-tauri/src/persistence/stores/model_mapping_store.rs`
- Modify: schema registry、postconditions、schema15 fixture、portable migration catalog / target writer / reader、domain revision owner
- Modify: `routing_health_verdict_store` 和相关 schemas，为 native model capability identity 建桥

**Steps**

- [ ] 在一个 append-only migration 中建立 `model_mapping_policies`、rules、rule targets、profiles、bindings、legacy migration reviews 和完整 document history 所需结构；所有模型 identity column / unique index 使用 `BINARY` collation。
- [ ] Binding 使用 `station_key_id` 与 `station_id` 两个 nullable concrete FK，加 XOR `CHECK`，对每种 scope 建 unique index，并使用 `ON DELETE RESTRICT`。启用 SQLite FK enforcement 并在 Station / Key 删除 service 返回 typed reference error。
- [ ] 通过完整 document apply 写 normalized rows、aggregate revision、object revisions、history、generic document sync desired revision 和 domain revision；store 只做 SQL，不能 decode 文件、执行 compiler 或发布事件。
- [ ] 执行一次性 legacy migration：每个 client model 的第一条启用行按 `created_at ASC, id ASC` 转为 exact fixed literal；剩余行进入 review。禁用行不启用。空白或损坏值产生 typed migration review，不得猜测。
- [ ] 新 capability record identity 以 `station_key_id + upstream_model + endpoint/protocol + credential_revision + endpoint_revision` 为准。将旧 `model_alias_revision` 保留为历史 provenance，禁止新写入将它作为 model-on-key / unsupported-model identity。
- [ ] 为 request / attempt / decision trace 增加 `requested_model`、route target、actual upstream model、`model_mapping_revision`、`model_resolution_fence` 和 target rank 的可恢复字段；历史记录不回写伪造新值。
- [ ] 为 migration 添加 schema postcondition、schema15 到最新的升级 fixture、新库、现库、空 alias、重复 alias、禁用 alias、interrupted recovery 和 portable import/export coverage。

**Run**

```powershell
pnpm.cmd verify:fast
cargo test --locked --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml startup_upgrade -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml model_mapping_store -- --nocapture
```

**Exit gate**: 从 schema 15 到最新、现有数据库和 portable import 均能恢复；新 active data 不读旧 alias 表；所有 FK / BINARY / CHECK / RESTRICT 与 capability identity 测试通过。

### Task 4：Mapping application service 与 immutable snapshot publisher

**Files**

- Create: `src-tauri/src/application/model_mapping/{service,query}.rs`
- Modify: routing composition root、domain revision notice、shared document coordinator registration
- Create: service CAS、history restore、file import、snapshot reload 与 failure-mode tests

**Steps**

- [ ] 实现 `get`、`validate`、`apply`、`restore`、file-watch import 与 startup reconcile 的唯一 application owner。所有入口先严格 decode、验证、编译完整 document，再在一笔 write transaction 内 CAS、history、normalized rows、sync desired revision 和 revision advance。
- [ ] 使用 `Arc<CompiledModelMappingConfiguration>` 发布已经验证的完整规则 + Profile + Binding snapshot。DB commit 后只做原子替换，不能在发布路径重新编译或 I/O。
- [ ] 新请求必须取得与当前 committed revision 一致的 snapshot；旧请求通过 `ResolvedModelPlan.mapping_snapshot` 持有其版本，直至最后 attempt。revision gap、load error 或无 snapshot 时 fail closed。
- [ ] 让 simulation 的 optional draft 用同一 compiler / resolver，但不取得容量、不解密 credential、不写 health / affinity / request log、不访问网络。read model 只输出安全 DTO。
- [ ] 实现 typed conflict，返回 current revision 与安全 summary，不返回原始文件或内部 error string。相同 canonical document 幂等，不制造无意义 revision。

**Exit gate**: UI、file watch、manual import、history restore 走同一 service；并发 Binding / rule 更新不能让一个请求的 plan 与展开结果跨 revision。

---

## Workstream B：Phase-1 精确映射生产切换

### Task 5：以单一 ResolvedModelPlan 取代 proxy / snapshot 双 resolver

**Files**

- Modify/Delete: `src-tauri/src/application/routing_engine/model_alias.rs`
- Modify: `src-tauri/src/services/proxy/{execution,routing_repository,endpoint_adapter}.rs`
- Modify: `src-tauri/src/application/operational_facts/{assembler,planning_snapshot,target_resolver}.rs`
- Modify: `src-tauri/src/application/routing_engine/{request,planning_snapshot,candidate_plan,coordinator}.rs`
- Modify: request finalization、attempt writer、routing decision persistence 与 loopback tests

**Steps**

- [ ] ingress 在解析 endpoint、stream、tools、vision、reasoning 后创建 immutable `ModelRequestFacts`，调用 Mapping service 一次并在 request lifecycle context 保存 `ResolvedModelPlan`。保留 immutable 原始 request body bytes，绝不原地重写共享 body。
- [ ] 删除每请求 `load_model_alias_pairs` 和 `model_alias::mapped_model`。RoutingRepository 不再暴露 alias pair loading；Planning Snapshot / capability subject / target resolver 不得扫描 `ModelAliasFact` 或按上游名反向匹配。
- [ ] Phase 1 对每个 eligible Key 生成一个实际模型固定的 `CandidateModelVariant`。candidate / attempt / commitment / trace 附带 requested、route、upstream、mapping revision 与 resolution fence；容量 identity 仍复用实际 Key / account domain。
- [ ] 将 Station Key 的 model allowlist / blocklist 与 native-model capability 查询都改为比较 variant 的最终 `upstream_model`，而不是 `requested_model`；endpoint、stream、tools、vision、reasoning 仍按既有 hard gate 执行。
- [ ] endpoint adapter 只接受 candidate commitment 的 frozen `upstream_model`。在 Responses-to-Chat 结构适配完成后写 JSON 根 `model`；Chat、Responses、Embeddings 统一覆盖。Models / usage 不应用映射。
- [ ] capability 与 outcome 使用最终 `upstream_model`，不是 requested name。`model_not_found` 仅影响 Key + native model，endpoint / credential / account failure 继续按既有更大 failure scope 分类。
- [ ] 代理、Planning Snapshot、target resolver 或 adapter 发现 plan 缺失 / fence 不一致时返回 typed mapping configuration error，不得 preserve 或回退 alias 表。
- [ ] 删除旧运行时 ModelAlias consumer 和 feature tests；legacy table 仅由 migration review query 读取。

**Run**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml model_mapping -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml proxy -- --nocapture
```

**Exit gate**: Chat、Responses、Responses-to-Chat、Embeddings 的 loopback test 都证明请求起始到最终 body 使用同一 frozen plan；没有生产 alias resolver 或反向大小写匹配。

### Task 6：Phase-1 read model、IPC 与生成契约

**Files**

- Create: `src-tauri/src/ipc/dto/model_mapping.rs`
- Modify: command facade、IPC registry、DTO module registry、generated binding source annotations
- Modify/Create: mapping workspace / mapping trace query 与 request log DTO
- Modify: `src/lib/bridge/{BackendClient,DesktopBackend,DemoBackend}.ts`
- Generate: `src/lib/bridge/generated.ts` 和受管 `.typescript.txt` artifacts

**Steps**

- [ ] 暴露 `get_model_mapping_workspace`、`get_model_mapping_document`、`validate_model_mapping_document`、`apply_model_mapping_document({ document, source })`、`restore_model_mapping_revision`、`simulate_model_mapping`、`resolve_request_mapping_trace`。
- [ ] Phase 1 workspace 只公开规则、document status、known literal suggestions、legacy reviews、typed diagnostics 和安全的 candidate count；不暴露 Profile / fallback editor。
- [ ] DTO 限制文本、数组、priority、enum 和 draft size，使用 `deny_unknown_fields`。错误使用稳定 code / safe model name，不允许前端解析 Rust 错误文本。
- [ ] request trace DTO 说明命中 rule、未匹配行为、解析/资格/执行失败阶段、requested/route/upstream 模型和 revision fence；不含 API URL、secret、header、body。
- [ ] 运行 binding generator，更新 registry / generated tests。Demo backend 对新命令显式返回 unsupported，不能伪造 production mapping。

**Exit gate**: 新 command 契约生成且可调用；未生成 binding、unknown field、超长输入、stale base revision 与配置不可用均有 typed contract test。

### Task 7：Phase-1 Routing UI 与旧 alias UI 删除

**Files**

- Create: `src/features/routing/model-mapping/{ModelMappingPanel,RuleList,RuleEditor,MappingPreview}.tsx` 及测试
- Modify: `src/features/routing/RoutingPage.tsx`、routing query synchronization / invalidation
- Create: `src/lib/{api,queries,types}/modelMapping*.ts` 及测试
- Modify/Delete: `src/lib/api/routing.ts`、routing query / bridge 里的 alias CRUD 和旧 alias UI components

**Steps**

- [ ] 在既有 Routing 页面容器内新增一级“模型映射”分区，使用服务端 workspace query 与完整 document draft reducer；不创建独立页面状态机或前端规则解释器。
- [ ] Phase 1 规则列表展示 priority、exact/default matcher、conditions、fixed/preserve/reject、enabled、诊断、目标候选摘要、更新时间和 icon 操作。覆盖 loading、empty、error、disabled、保存中、冲突和窄窗口横向阅读。
- [ ] 编辑器只展示 Phase 1 capability：exact/default、有限 conditions、fixed literal / preserve / reject。客户端模型允许手输；literal 可提示未验证但不能伪造模型发现。
- [ ] preview 调用后端 compiler / resolver；显示 higher-priority miss、matched rule、requested/route target、保存或 draft source、冲突/遮蔽。preview 不写 active configuration。
- [ ] 保存以完整 document 的 `baseRevision` 提交。收到 revision notice 时，无 dirty draft 自动 reload；有 dirty draft 进入 typed conflict / merge / overwrite 流程。
- [ ] 从页面、API、query keys、bridge 和测试删除 alias CRUD UI。legacy migration review 保留但在 Phase 1 只读显示，不提供 Phase-2 特有的“加入回退链”。

**Run**

```powershell
pnpm.cmd generate:bindings
pnpm.cmd test:contracts
pnpm.cmd test -- src/features/routing src/lib/api src/lib/queries src/lib/bridge
pnpm.cmd build
```

**Exit gate**: 用户可创建、预览、保存、停用、删除 `codex-5.4 -> deepseek-v4-flash` 类规则，并能从请求详情看见实际执行模型；前端没有 alias row mutation 生产入口。

### Task 8：Phase-1 原子 qualification 与迁移审计

**Steps**

- [ ] 对新库、single legacy alias、duplicate legacy alias、disabled legacy alias、外部 `model-mapping.json` 修改、文件写失败、watcher overflow、stale UI draft、进程启动 snapshot load failure 运行端到端 fixture。
- [ ] 用 loopback fake station 验证 fixed mapping 的 Chat、Responses、Embeddings 上游 JSON 根 `model`，并验证 `/models` 和 `/usage` 不被 reject policy 阻断。
- [ ] 审查 route decision、request log、error response、support export 和 fixture，确认不存在 raw body、Authorization、cookie、API key 或完整 URL query。
- [ ] 在删除台账逐项确认旧 resolver、旧 normal runtime alias read、alias UI 和手写 mapping fallback 已不可达；未删除的 legacy schema / review query 标为仅迁移审计。

**Run**

```powershell
pnpm.cmd verify:fast
pnpm.cmd build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
pnpm.cmd test:contracts
```

**Exit gate**: Phase 1 是单一 production path，能稳定执行 fixed literal mapping，且所有被删旧路径在 architecture / dead-code inventory 中不可达。

---

## Workstream C：Phase-2 Profile、Binding、模型能力与回退

### Task 9：Profile / Binding 与 CandidateModelVariant 执行模型（已完成）

**Files**

- Modify: model mapping domain/compiler/service/store/document codec / workspace query
- Modify: Planning Snapshot、candidate plan、coordinator、target resolver、routing repository、proxy attempt types
- Modify: Station / Key deletion commands 和 tests

**当前状态（已完成）**：Profile/Binding 查找、fallback rank 展开和
`CandidateModelVariant` 已接入 production planner、admission、attempt/retry
identity、capacity sharing 与 endpoint model rewriting。IPC、持久化、启动加载
和 proxy 多目标边界已按同一版本契约启用；相关回归证据见 acceptance matrix
第 11-14 项。

**Steps**

- [x] 解锁 `model_profile` target、Profile、Station Binding、Station Key Binding 与显式 default upstream model。Profile / Binding 仍只能通过完整 mapping document 写入。
- [x] candidate projector 从 plan 持有的 compiled mapping snapshot 解析 binding：Key binding > Station binding > Profile default。不得在 projection 重新读取当前 mapping table，避免 resolver 与 binding 版本穿插。
- [x] 将 planner candidate identity、attempt-progress visited set、retry exclusion、commitment 和 trace 从单一 Key 升级为 `CandidateModelVariant`。至少包含 Key、upstream model、target rank、endpoint、credential / endpoint revision 与 resolution fence。
- [x] 将 capacity lease / accounting identity 保持 Key / account / provider domain；模型 variant 只能影响目标选择、模型错误隔离与 trace，不能虚增共享容量。
- [x] 加入 exact execution identity 去重：同一 Key + upstream model + endpoint + revision/fence 已尝试时，后续 Profile/rank 不重复发送相同 request；保留首次 rank 的解释。
- [x] Station / Key 删除前查 Binding reference，返回 typed rejection。禁止 `ON DELETE CASCADE` 静默删除另一个配置 aggregate；用户必须编辑完整 document。
- [x] 把 `source=discovered` 限定为用户接受发现建议时写入的 provenance。collector / discovery 只能更新建议与 capability evidence，不能自动创建、删除或改写 Binding。

**Exit gate**: 一个 Profile 可在不同 Key 解析为不同 native model；修改 Binding 后新请求使用新 snapshot，正在执行的请求仍使用旧 snapshot；相同 native variant 不重复 attempt。

### Task 10：模型能力事实、模型级回退、诊断与 UI（已完成）

**Files**

- Modify: capability store / health verdict store / outcome classifier / projection tests
- Modify: routing engine coordinator、attempt lifecycle、failure-domain and trace projection
- Modify: IPC DTO、workspace / trace query、frontend mapping components
- Create: `ModelProfilePanel`、`BindingEditor`、`LegacyAliasMigrationReview`、fallback chain editor and tests

**当前状态（已完成）**：native model capability identity、显式 fallback trigger、
rank-aware candidate/admission/retry、Profile/Binding/fallback UI 与 legacy
migration review 已落地；前端和 Rust 证据见 acceptance matrix 第 13-15 项。

**Steps**

- [x] 将 `model_not_found`、confirmed unsupported endpoint / feature 写入 Key + actual upstream model 的 capability fact；其他模型和 logical name 不受影响。unknown 模型证据在无 hard incompatibility 时可尝试，成功或明确错误后更新 evidence。
- [x] 从 scoped health / capability data 删除新的 `model_alias_revision` identity writes。保留旧列和历史读取直到迁移窗口结束，但新 schema、new records 和 planner subjects 使用 native model identity + credential / endpoint revision。
- [x] 解锁 `map_fallback_chain`，最多 3 个不同 `TargetRef`。实现 `no_eligible_target` 为默认：上层 target 通过所有资格并完成正常容量等待前，不能降到下一模型。`retry_exhausted_before_output` 仅在上游可重试 attempts 耗尽且无输出提交时允许降级。
- [x] rank 是模型级前置层；每个 rank 内再使用既有 Primary / Backup / Emergency、评分和 dispatch。低 rank 不能因更低成本或一次 lease miss 越级。
- [x] frontend 新增逻辑模型目录和 Binding 编辑：只显示已知 Station / Key、实际模型名、来源、能力 evidence、受影响候选数；不展示 API key。回退链用稳定上下移动 / DnD 编辑，保存时后端正规化 `position`。
- [x] 完成 legacy migration review 操作：删除历史目标、创建独立规则或显式加入回退链。后两个动作先显示语义变化和候选预览，再写完整 document。
- [x] 扩展预览 / trace，按 target rank 显示 candidate counts、排除原因、未验证 evidence、实际 variant、失败 scope 和是否在输出前发生模型切换。

**Run**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path src-tauri/Cargo.toml model_mapping -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml routing_health -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml proxy -- --nocapture
pnpm.cmd test -- src/features/routing src/lib/api src/lib/queries src/lib/bridge
pnpm.cmd build
```

**Exit gate**: Profile/Binding、native capability facts、variant-aware retry、fallback semantics、UI/trace 和 migration review 全部同一版本可运行；没有 Key-only retry set 或 mapping-revision capability identity 残留。

### Task 11：Phase-2 qualification、迁移窗口与 legacy 表退役决定（运行时完成，发布资格待完成）

**Steps**

- [ ] 编写 property / integration tests：Key binding 覆盖 Station binding、Station fallback 到 Profile default、unknown / unsupported evidence、同 Key 多 native model、相同 native dedupe、model-not-found scope、endpoint/credential/account scope、两类 fallback trigger、stream / non-stream output commitment。
- [ ] 压测 candidate expansion 的 rule/profile/binding/key 上限，确认 planner batch、trace target list、JSON document、IPC output 和 UI table 都有硬上限。
- [ ] 运行并发测试：UI save、file watch、Binding update、in-flight retry、config reload、station/key deletion attempt、watcher overflow / restart，验证无 partial document 和无 fence escape。
- [ ] 收集 legacy alias migration review 的完成率。仅在所有 review record 已处置、回滚窗口结束、export / portable migration 不再声明旧 alias 输入后，单独批准删除 `model_aliases` 表的后续 migration；不可在 Phase 2 自动删除。

**当前状态**：Phase 2 runtime、migration review UI 与本地回归已完成；并发/压测、
真实 provider、release-machine qualification 与 legacy 表退役决定仍保留为发布前
工作，不阻塞当前手动测试。

**Run**

```powershell
pnpm.cmd verify:fast
pnpm.cmd verify:full
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
pnpm.cmd test:contracts
pnpm.cmd build
```

**Exit gate**: 多模型映射与显式模型回退满足全部 Phase-2 specs；legacy 表是否退役有审计证据与明确后续 migration decision。

---

## Workstream D：Phase-3 glob 与最终收口

### Task 12：受限 glob、自动机交集与高级规则诊断（已完成）

**Files**

- Modify: model mapping compiler / resolver / DTO schema / document upgrader
- Modify: mapping UI editor, list filters and diagnostics
- Create: bounded glob automaton, intersection, resource-bound and benchmark tests

**当前状态（已完成）**：有界 glob matcher、编译期交集/overlap/shadowing
diagnostics 与 bounded resource checks 已启用，运行时只消费编译结果；对应
证据见 acceptance matrix 第 16 项。

**Steps**

- [x] 仅支持整串 `*`、`?` 和受限转义；不引入 regex、路径语义、backreference、lookaround 或不可证明线性复杂度的 matcher。
- [x] 固定 pattern byte length、rule count、automaton state count、intersection work budget。超限是 typed compile error，不得在 proxy 热路径退化为回溯匹配。
- [x] compiler 对 same-priority potential overlap 执行自动机交集判断；交集非空或分析超资源即阻止 enable。不同 priority 的完全 / 部分 shadowing 生成 deterministic diagnostics。
- [x] resolver 只使用编译结果，不在每请求重建 glob / 比较 pattern。增加 worst-case model name / pattern benchmark 与 cancellation-safe validation。
- [x] UI 根据 document version 显示 glob，呈现 server diagnostics；不提供“手工试样例即可启用”的绕过按钮。

**Exit gate**: glob 不引入不可解释重叠、热路径无不受限分配或回溯、同规则集在 compile / simulate / proxy 结果一致。

### Task 13：最终资格、文档状态和运行准备（部分完成）

**Steps**

- [x] 更新获批规格的状态、计划进度、迁移 / recovery / portable export 文档、IPC contract inventory、architecture / dead-code manifests 和 user-facing routing diagnostics 文案。
- [ ] 执行安全 review：所有日志、trace、document status、validation error、conflict DTO、fixture、support export、screenshot 和 test failure 不暴露 secret、header、cookie、body 或完整上游 response。
- [ ] 执行 desktop loopback：规则 CRUD、外部文档编辑、conflict、file permission failure、restart reconciliation、fixed mapping、Profile mapping、fallback、streaming、terminal failures、narrow-window keyboard flow。
- [x] 更新 acceptance matrix 和 deletion ledger，写明实际命令、退出码、未运行项和外部依赖。只有所有必需项真实通过才将计划 / 规格标为已实施。

**当前状态**：实现状态、acceptance matrix、baseline 与 control-plane gap 已同步；
统一 routing-policy document apply、typed source context 和共享 coordinator 已完成。
legacy compatibility mutation 的 notice 覆盖、watcher restart/overflow 的发布资格、
routing-policy history provenance 决策、release/live-provider qualification 和
legacy schema retirement 仍未宣称完成。

**Run**

```powershell
pnpm.cmd verify:full
pnpm.cmd build
pnpm.cmd test:contracts
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace
```

**Exit gate**: 规格第 12 节的验收标准逐项有通过证据；没有遗留 production alias path、duplicated config owner、未受限 matcher 或未经验证的 schema migration。

---

## 5. 测试矩阵与失败处置

| 层 | 最小必测内容 | 失败时的处置 |
| --- | --- | --- |
| Domain compiler | trim / exact / default、conditions、priority、conflict、shadow、reject、strict document | 不保存 document；不发布 snapshot |
| Mapping service | CAS、idempotency、history、file import、snapshot reload、typed conflict | 回滚 write transaction，保留上一个 active snapshot |
| Persistence / migration | schema15、legacy alias order、FK / RESTRICT、capability fence、interruption | 进入现有 typed upgrade recovery，不增加启动 repair |
| Proxy data plane | Chat / Responses / Embeddings、adapter ordering、bypass endpoints、frozen plan | 本请求 typed failure；不得读旧 alias 或猜测 preserve |
| Planner / retry | variant identity、capacity sharing、dedupe、model-not-found scope、fallback trigger | 停在当前 rank 或 canonical retry path；不跨模型语义 |
| IPC / frontend | generated contract、draft preview、conflict、loading/error/narrow window/a11y | 显示 backend typed state；不本地重算规则 |
| File sync | atomic write、permission failure、watcher loss、restart / resume reconciliation | active SQLite policy 不回滚；sync state 最终收敛 |
| Security | logs, exports, fixtures, errors, support bundle | 删除泄露字段并加 regression fixture；不以 redaction 失败继续发布 |

## 6. 交付记录要求

每个完成 Task 的记录必须包含：变更文件、实际运行命令与退出码、RED/GREEN 证据、migration number、是否触发 generated bindings、已知风险和删除台账状态。没有运行的命令必须说明原因与受影响范围，不能写成“已通过”。

实施过程中发现以下情形时停止并回到设计评审，而不是临时扩展本计划：需要任意脚本规则、需要自动语义等价判断、需要响应模型伪装、需要按 Authorization/IP/prompt 匹配、需要将未知模型一律变为支持、或需要为 mapping 新建独立文件同步机制。
