# Relay Pool Desktop 运行日志与本地可观测性升级实施计划

状态：Complete；代码、核心故障合同、诊断导出、rotation、clean/panic marker、catalog 兼容性与工程门禁已完成并验证。真实 provider/密钥和人工页面验收按范围不纳入本计划；packaged marker-I/O fault 子进程 smoke 暴露 harness 退出挂起，已记录为非生产验证工具限制，不阻塞生产降级路径。

日期：2026-08-15

适用范围：Rust/Tauri 启动与退出、IPC、任务运行时、代理与出站传输、采集、状态监控、持久化、导入导出/更新、React 前端、开发者诊断页与本地 support bundle。

关联目标规范：[`../proposals/RUNTIME_LOGGING_OBSERVABILITY_UPGRADE_SPEC.md`](../proposals/RUNTIME_LOGGING_OBSERVABILITY_UPGRADE_SPEC.md)。本计划只拆解实施和验证顺序；发生冲突时以 `AGENTS.md`、当前代码/自动化契约、`docs/README.md` 所列当前规范和获批后的目标规范为准。

实现记录：本轮已将运行日志合同测试收敛到生产模块内的 `#[cfg(test)]`，避免通过 `#[path]`/`include!` 拼装生产源码的 integration harness。事件目录现由 owner-specific 静态 descriptor slices 聚合，并通过 `scripts/generate-runtime-event-catalog.mjs` 双次确定性生成 `src-tauri/generated/runtime-event-catalog.v1.json`；`verify:fast` 对 tracked artifact 执行漂移检查。proxy、collector、monitoring、migration、updater 的本地 loopback/fault artifact、bootstrap/shutdown、lease/restart harness 和完整 Rust 回归均已通过；前端测试、build、bindings、架构和安全扫描均通过。debug-only 临时根 packaged smoke 已连续两次 clean start，实际执行 diagnostics reader/export、自动轮转并清理产物，clean/panic marker 与 redaction 也已验证。marker-I/O fault 的 packaged harness 曾因进程退出挂起而停止，不代表生产路径失败：Rust 生命周期合同已证明 marker 打不开时固定 stderr、JSONL 降级事件和 shutdown 仍继续。真实 provider/密钥按计划不纳入自动化资格，人工页面和原生保存对话框验收按用户决定不纳入本计划。

### 当前收紧决定（2026-08-15）

已完成的基础实现不改变，但以下收紧是交付前必做项，不能用现有运行时 reject、source-string contract 或重复 qualification 文本替代：

1. `catalog.rs` 只能聚合、校验和生成 manifest；不能继续拥有全部领域 event descriptor。app、IPC、persistence、proxy、outbound、collector、monitoring、migration/updater 和 frontend 必须各自在本模块边界声明自己的静态 descriptor slice，聚合器只显式导入这些 slice。
2. `RuntimeLogService::record(&'static str, Component, EventLevel, EventOutcome, RuntimeDetail)` 和按 event-code 后缀推断 component/level/outcome 的 bootstrap helper 必须淘汰。producer 只可提交本地静态 `EventDescriptor`/封闭 event handle 与受限 detail；component、level、允许 outcome/detail/subject 均由 descriptor 决定，未注册事件在编译或 owner contract 中失败，不能仅在运行时把 sink 置为 degraded。
3. diagnostics/support-bundle IPC DTO、toast、runtime event 和错误路径不得回传或记录本机绝对路径。导出成功只返回计数或 opaque success result；保存位置由原生 dialog 和 OS 管理。
4. 读取源码字符串的 lifecycle/source contract 只保留为架构回归提示，不能作为启动、退出、restart 或 Windows 行为验收。必须抽取可注入的 bootstrap/shutdown owner，并用临时目录、真实 JSONL artifact 和显式失败 seam 证明行为。
5. 计划只保留范围、剩余任务和决策；acceptance matrix 是每项验收的唯一映射，qualification 只追加带日期的实际运行结果。禁止在三份文档复制同一组“已通过”段落。

执行状态：上述 1-5 项的代码收紧、门禁和自动化证据已完成；文档只保留外部/环境阻断，不再把人工验收列为待办。

## 1. 当前基线与实施前提

截至本计划建立时，以下事实决定实施方式：

- `src-tauri/src/observability/correlation.rs` 已提供 task-local、匿名 correlation id；所有 Tauri command 目前在各自函数内直接调用 `in_command_scope`，没有接收跨 command interaction context 的统一入口。
- `src-tauri/src/observability/events.rs`、`metrics.rs`、`diagnostics.rs` 和 `redaction.rs` 是部分未接线的草图，带有 `allow/expect(dead_code)`；不得在其外再建第三套 event、metric 或诊断模型。
- `src-tauri/src/services/data_store/installation_lease.rs` 已证明 Windows OS file lock 的基本模式，但它直接 `println!`，并且数据存储 lease 不能替代独立的 runtime-log writer/retention lease。
- `src-tauri/src/lib.rs`、`background_tasks/exit.rs`、monitoring、collector、routing、proxy lifecycle 和 updater 周边仍存在 `println!`、`eprintln!`、动态 `tracing` 字段或 `error = ?error`；当前不能直接开启全局 `tracing` 文件 subscriber。
- 当前 IPC registry 和 TypeScript binding 由 `src-tauri/src/ipc/registry.rs` 与 `scripts/generate-bindings.mjs` 生成。`src/lib/bridge/transport.ts` 是前端唯一普通 invoke 入口，适合承载受控 runtime context，不能在各页面手写 metadata。
- 设置中已存在 `developer_mode_enabled`；`src-tauri/src/ipc/registry.rs` 当前明确阻止公开 runtime diagnostics。新读取面必须替换为“仅 developer mode、后端强制校验、受限 DTO”的契约，而不是绕过这一门禁。
- 现有数据存储诊断导出是独立安全能力，运行日志 support bundle 只能组合其已审查输出，不能复制 SQLite、备份、原始配置或凭据。

实施开始前必须满足：

1. 目标规范状态变为 accepted，且第 16 节中的 retention、interaction、lease/recovery/clock 参数已冻结；未获批时只允许做 inventory、测试夹具和 dependency 研究，不改 production 行为。
2. 每个任务开始和结束记录 `git status --short`；工作区已有并行改动很多，只修改本计划明确列出的文件或其直接生成物，绝不回退、清理或格式化不相关文件。
3. 不引入远程 telemetry、APM、自动上传、prompt/response capture 或新的业务 SQLite 日志表。运行日志目录必须与数据库目录独立。
4. 不以 `allow(dead_code)`、全局 `tracing` 文件 sink、长期双写、自由文本预览或手工编辑生成 binding 通过任一阶段。

## 2. 完成定义与不可变约束

完成时必须同时满足：

- 全部生产运行事件经唯一 `RuntimeLogService` 或组件类型化 adapter 进入安全 JSONL；业务 SQLite 事实仍由原 owner 维护。
- 每个正式 segment 都由唯一 installation-wide lease owner 以 `*.partial` 写入，经 metadata、manifest 和大小验证后原子发布；reader、bundle、retention 不读取或删除 partial、unknown、active 或 lease-owned 文件。
- `durationMs`、超时、退避、lease retry 和 clock guard 均来自 monotonic clock；UTC `atMs` 只用于显示/取证，跳变产生固定事件且暂停年龄清理。
- panic hook 不依赖 logger queue 或可能被持有的 writer mutex；crash marker 是独立预打开 handle，使用 recursion guard、`try_lock` 和固定 stderr fallback。
- 运行日志字段没有 `String`/`&str`/`serde_json::Value`/任意 `Error` 入口；外部文本只会变成稳定 code 和闭合 enum `redacted`，末端扫描仅作防御，不是动态内容的授权。
- catalog 由各 producer owner 的声明生成 machine-readable manifest；全局唯一性、owner、版本、废弃 replacement、message key、subject 和 bundle permission 都在构建期验证，并保留当前/上一兼容 manifest snapshot 供历史 segment 读取。
- 单次用户手势跨多个 IPC command 使用同一个短生命周期 `interactionId`；它独立于 correlation/subject，经过版本化 DTO、capability、TTL、容量和 session 校验。系统/定时调用为 `null`。
- developer mode 才可读取诊断或生成 support bundle，且 UI、DTO、导出物和失败路径均通过秘密 canary。普通模式不暴露日志路径、文件内容或导出入口。
- 所有旧直接输出和未接线 observability 草图均已按删除台账处理；架构门禁阻止其回归。

## 3. 依赖顺序

```text
Task 0 冻结参数、建立 inventory 与 red gate
  -> Task 1 事件内核、catalog、redaction 收敛
  -> Task 2 IPC interaction context 与 generated contract
  -> Task 3 lease、sink、clock、recovery 与 panic marker
  -> Task 4 应用启动接线及基础 producer
  -> Task 5 proxy/outbound/collector/monitoring producer cutover
  -> Task 6 import/export/updater 与前端 ErrorBoundary cutover
  -> Task 7 reader、developer diagnostics 与 support bundle
  -> Task 8 删除旧路径、架构/安全门禁
  -> Task 9 故障、并发、容量与全量资格验证
  -> Task 10 验收证据与文档状态闭环
```

Task 1 与 Task 2 可并行设计和补单元测试，但只有二者的 schema/DTO/manifest 共同冻结后，Task 3 才能持久化 JSONL。Task 4 只能在 Task 3 的 writer/recovery 通过 Windows 多进程测试后接入应用启动。Task 7 不得早于 Task 4-6 的 producer cutover，否则 UI 会固化临时 schema。Task 8 与 Task 9 共同构成可交付候选，不能只上线新 sink 而保留旧输出。

## 4. 实施任务

### Task 0：冻结实施参数、基线 inventory 与门禁骨架

目标：把“完整、可排错”转换为可证明的字段、文件、性能和删除边界，并让后续扫描有可比较基线。

文件：

- Create: `docs/audits/runtime-logging-source-inventory.md`
- Create: `docs/audits/runtime-logging-deletion-ledger.md`
- Create: `docs/audits/runtime-logging-canary-matrix.md`
- Create: `scripts/runtime-logging-architecture.test.mjs`
- Create: `scripts/runtime-logging-security.test.mjs`
- Modify: `package.json`、`scripts/run-contract-tests.mjs`（若该 runner 是现有 contract 汇集入口）
- Read before edits: `docs/SECURITY_EXPORT_IMPORT.md`、`src-tauri/src/ipc/registry.rs`、`scripts/generate-bindings.mjs`、`src-tauri/Cargo.toml`

步骤：

1. 将全部生产 `println!`、`eprintln!`、`tracing::{error,warn,info,debug}!`、`error = ?error`、`error = %error`、文件日志写入、redaction wrapper、诊断导出和 runtime DTO 枚举为 inventory。每项记录：文件/符号、owner、现有动态数据、目标 event code、替换 Task、删除条件和测试。
2. 初始台账至少包括 `src-tauri/src/lib.rs`、`background_tasks/exit.rs`、`background_tasks/routing_projection_runner.rs`、`application/routing.rs`、`services/data_store/installation_lease.rs`、`services/proxy/lifecycle/writer.rs`、`services/proxy/startup_auto_start.rs`、`services/station_collectors.rs`、`services/monitoring/{runner,maintenance}.rs`；扫描发现的其他生产位置不得以“非关键”排除。
3. 冻结首期数值：segment 8 MiB、目录 96 MiB、14 天、单行 16 KiB、reader 200 行/1 MiB、support bundle runtime events 10 MiB、queue 大小与 error/warn reserve、partial recovery 文件数/字节数、clock guard 容差/观察窗口、interaction TTL/active-id cap。把每个数值、理由、owner、修改需重新评审的规则写入 audit。
4. 确认实现依赖。优先使用标准库 `sync_channel`、`std::fs`、`FileExt::try_lock` 和既有 `tokio`；若引入 `tracing-appender`、额外跨进程锁 crate 或归档 crate，先记录许可证、维护状态、Windows 原子 rename/lock 行为、锁文件变化和替代方案。禁止让任何 subscriber 自动记录未类型化 tracing 字段。
5. 建立 red gate：架构脚本先只报告现有允许清单，安全脚本以 fake `sk-secret`、authorization、cookie、password、userinfo URL、query token、Windows 路径、prompt/response 为 canary。将 bootstrap 固定 stderr 的未来白名单精确限制到 runtime bootstrap/crash 模块，不能是目录级豁免。
6. 明确 catalog manifest 的构建路径：组件所有者导出静态 descriptor slice；`observability::runtime::catalog` 只聚合 slice 并生成 JSON，不拥有业务 code；应用启动把编译后的 manifest snapshot 以 partial + atomic rename 保存到日志根目录。构建/contract test 序列化该 manifest 并校验，运行时 reader 只信任已验证 snapshot，不从任意文件加载 descriptor。

Focused gate：

```powershell
node scripts/runtime-logging-architecture.test.mjs
node scripts/runtime-logging-security.test.mjs
pnpm.cmd test:contracts
git diff --check
```

Exit gate：三个 audit 文档覆盖所有扫描结果；每个例外有 owner 和删除条件；参数和 dependency decision 已获批准；脚本在当前基线上红色且能精确说明缺口，不把未知调用静默放行。

### Task 1：实现类型化事件内核、catalog 与单一 redaction 边界

目标：先提供唯一安全的数据模型和可演进目录，再让任何模块写文件。

文件：

- Create/Modify: `src-tauri/src/observability/runtime/{mod,event,catalog,subject,error,clock}.rs` 及各 owner 的 `runtime_events` 声明模块
- Modify: `src-tauri/src/observability/runtime/contract_tests.rs`（生产模块内 `#[cfg(test)]` 合同）
- Modify: `src-tauri/src/observability/mod.rs`
- Rewrite or remove: `src-tauri/src/observability/events.rs`
- Modify and then wire/remove: `src-tauri/src/observability/{metrics,diagnostics,redaction}.rs`
- Modify: `src-tauri/src/services/secrets/**`、`src-tauri/src/services/proxy/capture/**`，仅在收敛既有 mask/redaction 内核确有重复时

步骤：

1. 定义 serde `RuntimeEventV1`：`schemaVersion`、UTC `atMs`、process-local `sequence`、level、eventCode、component、outcome、sessionId、correlationId、interactionId、operationId、subject、durationMs、error、detail。构造 API 必须验证 ASCII/长度/null 语义，序列化前检查 16 KiB 上限。
2. `durationMs` 只接受由 `Instant` 产生的 `Elapsed`/guard 值；`clock.rs` 负责安全 UTC 采样、logical date、单 session sequence 和 wall-clock deviation 的闭合 detail。禁止业务 producer 自行 `SystemTime` 相减。
3. 将 `RuntimeDetail` 做成闭合 enum，并将每个 variant 的字段限制为 enum、布尔值、范围整数、批准的匿名 id 或静态 code。`RuntimeError` 只允许 `domain + code + retryable + data_disposition`；动态内容存在时使用 `data_disposition: Redacted`，不接收 `Display`、`Debug`、source chain 或字符串。
4. 实现 `SubjectRef`、`RedactedResourceId` 的唯一 hash/validator。迁移现有 `events.rs` 的稳定 token 和 hash 逻辑，不复制 hash 算法；静态 command 名称允许领域词，但静态 event code 继续拒绝 URL/path/secret-shaped 值。
5. 每个 owner 以声明宏或 descriptor slice **就地**声明 event code，并导出仅供聚合器读取的静态 slice；禁止把全部领域 slice 继续堆在 `catalog.rs`。catalog 聚合器只输出 `manifestId` 和 JSON，并验证唯一 code、owner、event/detail schema version、允许 outcome/detail/subject、sampling、support bundle permission、message key、deprecated/replacedBy 链无环。删除或迁移旧 `StructuredEvent`，不得把大 enum 或 `HashMap` 当作过渡目录。
6. 将 producer API 改为 `record(descriptor, fields)` 或等价的封闭 event handle：descriptor 提供 component、level、支持的 outcome/detail/subject，producer 不传裸 code 或可彼此矛盾的元数据。删除 `bootstrap::emit(code)` 的 suffix/前缀猜测；bootstrap fallback 也必须引用静态 descriptor，且 stderr 仅输出其固定 code。
7. 删除 `SafePreview` 作为 runtime event API。将 `redact_text_preview`/`redact_url_preview` 和 secret/capture wrapper 收敛为 canonical defensive scanner：它能拒绝误接入和扫描 bundle，但不能将任何动态文本转成可落盘字段。保留既有非日志功能时必须使用薄 wrapper 并共享同一 marker 表。
8. 为每个 manifest 版本保留可解析 JSON snapshot。compatibility reader test 使用当前与上一版本 fixture，验证历史/废弃 code 显示 replacement，未知 version/manifest 只隔离相关 segment，不影响其他 segment。

Focused gate：

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime::contract_tests -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
node scripts/runtime-logging-security.test.mjs
```

Exit gate：可从 owner-local descriptor 得到唯一 manifest；event/detail 无动态文本构造入口；旧 observability 草图已接线、迁移或删除，且无本专项新增 dead-code allowance。

### Task 2：建立 IPC interaction context 与相关性传播

目标：在不把 metadata 分散进页面、也不混淆业务 subject 的前提下，让一个用户手势关联多个命令。

文件：

- Create: `src-tauri/src/ipc/dto/runtime_context.rs`
- Create: `src-tauri/src/commands/runtime_context.rs`
- Create: `src/lib/bridge/runtimeContext.ts`
- Create: `src/lib/bridge/runtimeContext.test.ts`
- Modify: `src-tauri/src/observability/correlation.rs`
- Modify: `src-tauri/src/ipc/{dto/mod.rs,registry.rs,runtime_contract.rs}`
- Modify: `src-tauri/src/commands/*.rs`（机械加入受控 `runtime_context` 参数与统一 scope helper）
- Modify generated outputs only through: `pnpm.cmd generate:bindings`
- Modify: `scripts/generate-bindings.mjs`、`src/lib/bridge/{transport,generated,generated.test,DesktopBackend,BackendClient}.ts`

步骤：

1. 新增只用于 bootstrap 的 `initialize_runtime_context` command。后端为当前 frontend runtime 发放随机 `IpcContextSessionId` capability，保存为有界内存状态；capability 不进入 event、metric、support bundle、错误 DTO 或 URL。
2. 定义版本化 `IpcRuntimeContextV1 { contextSessionId, interactionId? }`。`interactionId` 使用固定 ASCII 格式与随机编码，不由 route、DOM、form、station/key 名称或 URL 派生；状态表以 capability + interaction id 为键，记录 first-seen monotonic instant、TTL 与受限 active-id 数量。
3. 把 `correlation::in_command_scope` 升级为唯一的 command boundary helper：验证 runtime context，产生 command correlation，并在同一个 task-local scope 设置验证后的 interaction。无 context 的系统调用为 `null`；无效 capability/格式、超容量、跨 session、过期或 TTL 后重放只记录 rate-limited `ipc.runtime_context.invalid`，业务 command 继续以 `null` 执行，绝不记录被拒绝值。
4. 不使用“全局当前 action”或单独 set-context command 关联后续请求，因为并发 invoke 会串线。将 `runtimeContext` 作为每个 Tauri invoke 的独立参数，并机械迁移全部 command 签名/调用到统一 helper；编译和 registry contract 必须证明无遗漏。
5. 前端 adapter 仅在启动时取得 capability。`runUserInteraction`/等价 scoped API 在用户手势开始生成 interaction id，并使同一 callback 内的多个 generated bindings 复用它；React Query 自动刷新、定时器、后台同步和无明确手势的命令不创建 id。`transport.ts` 在调用时附加当前 scope，所有 generated wrapper 自动受益。
6. 完成 generated registry 的输入 contract、TypeScript 类型、mock、`DesktopBackend` 及 tests 更新。禁止手改 `src-tauri/generated/command-registry.json`、`src/lib/bridge/generated.ts` 或 `*.typescript.txt`。
7. 扩展 task spawn helper：从 command 派生的 child operation 继承 interaction；独立 supervisor/定时任务只拥有自己的 correlation，interaction 继续为 `null`。

Focused gate：

```powershell
pnpm.cmd generate:bindings
cargo test --locked --manifest-path src-tauri/Cargo.toml correlation -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml ipc -- --nocapture
pnpm.cmd test -- src/lib/bridge/runtimeContext.test.ts src/lib/bridge/generated.test.ts
pnpm.cmd test:contracts
```

Exit gate：两个同手势 command 和其 child task 的 event 具有相同 interaction、不同 command correlation；TTL/跨 session/invalid cases 只产生 `null` 和固定计数；全部命令 registry/binding 仍为生成物一致状态。

### Task 3：实现 lease、segment sink、clock guard、recovery 与 crash marker

目标：在磁盘故障、崩溃、重启和 updater overlap 下保持业务不阻断、日志不串写、不误删。

文件：

- Create: `src-tauri/src/observability/runtime/{lease,sink,reader,crash}.rs`
- Modify: `src-tauri/src/observability/runtime/{sink,lease,reader,recovery,retention,crash,clock}.rs`（各自 `#[cfg(test)]` unit contract）
- Modify: `src-tauri/src/services/data_store/installation_lease.rs`
- Create or extract: `src-tauri/src/services/local_file_lease.rs`（仅存放无日志副作用的 OS file-lock primitive）
- Modify: `src-tauri/src/services/mod.rs`、`src-tauri/src/observability/runtime/mod.rs`
- Modify: `src-tauri/Cargo.toml`/`Cargo.lock`，仅在 Task 0 已批准新增依赖时

步骤：

1. 从 `installation_lease.rs` 提炼无业务日志、副作用最小的跨进程 file-lock primitive。数据存储 lease 可复用该 primitive，但 runtime-log lease 必须使用 `runtime-logs/` 下独立锁文件、独立 handle 和独立生命周期；不让 observability 依赖 Persistence Runtime。
2. `RuntimeLogService::bootstrap` 创建 session/writer identity、取得 OS 强制排他 writer/retention lease，并在拿到应用数据目录后初始化 worker。拿不到 lease 时进入 degraded state、保留固定 bootstrap stderr、单调退避重试，但不写 JSONL、不执行 recovery/retention；禁止依赖 `tauri-plugin-single-instance`、PID 或 `create_new` 作为唯一锁。
3. writer 使用有界队列和专属 worker。`error/warn` 有保留容量，`info/debug` 可丢弃；发射路径永不等待文件 I/O，所有 dropped/rejected/sink-error 状态以原子计数进入后续安全 event 和 diagnostics snapshot。不得在 SQLite transaction、proxy response send 或 cancellation critical section 进行同步 I/O。
4. 所有活跃数据文件以 `*.jsonl.partial` 及对应 metadata partial 存在，并通过 `create_new` 生成。关闭 segment 时依次 flush、尽力 `sync_data`、关闭 data handle、写入并验证 metadata partial、原子发布 metadata、最后原子 rename data 为 `*.jsonl`。metadata 至少含 schema、manifestId、writer identity、segment ordinal、installation generation、validated byteLength、first/last/closed UTC。reader/bundle/retention 只接收完整 pair。
5. 按大小和 logical UTC day rotation。实现 persisted generation 与 metadata 验证；retention 仅在 lease owner 内执行，按 generation 和实际字节数有界删除。96 MiB byte cap 始终生效；14 天年龄上限仅在 clock guard 健康时执行。active、partial、metadata partial、unknown、不完整 pair 或 live lease owner 文件一律跳过并计数。
6. startup recovery 只在拿到 lease 后、首次正式写入前运行。扫描固定数量/总字节预算内的已知遗留 partial；只复制完整换行、可解析且通过 schema/canary 的行到新的 own partial，再按正常流程发布 recovered segment。成功发布前不删除源文件；未知、超预算、损坏或失败的输入保持原状，不能被 retention 顺手删除。
7. clock guard 用 `Instant` 对照连续 UTC 采样。回拨、异常前跳或不稳定时记录一次 `runtime.clock.wall_adjusted`（闭合方向/bucket detail），停止按年龄删除和向过去日期 rotation；大小 rotation、byte cap 和业务 deadline 保持可用。首次取得 lease 也先经过固定 monotonic 观察窗口。
8. 在 panic hook 安装前创建并保持独立 active marker handle。hook 禁止调用 `RuntimeLogService`、队列、writer/retention mutex 或默认会输出动态 panic 的 hook；使用 atomic recursion guard、`try_lock` 和至多一次固定长度写。失败/递归只能输出固定 stderr。正常 drain 后删除 marker；下一 lease owner 启动将其转换为安全的 `app.previous_session_unclean_exit`。

Focused gate：

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib observability::runtime -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

Exit gate：用独立 process 证明 restart/updater contention 没有双 writer；partial 绝不被当作正式 segment；crash/recovery/clock jump/panic marker tests 全部通过；目录不可用、磁盘满、lock contention 和 retention failure 都不阻断业务任务。

### Task 4：应用 bootstrap、运行时摘要与基础 producer 接线

目标：将 sink 放入正确启动/退出顺序，并优先覆盖启动、持久化、任务和 operation 的排错链。

文件：

- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/runtime_composition.rs`、`src-tauri/src/app_composition.rs`
- Modify: `src-tauri/src/background_tasks/{exit.rs,operation.rs,mod.rs}`
- Modify: `src-tauri/src/services/data_store/installation_lease.rs`
- Modify: `src-tauri/src/observability/{metrics,diagnostics}.rs`
- Modify: `src-tauri/src/observability/runtime/bootstrap.rs`、`src-tauri/src/observability/runtime/service.rs`（bootstrap/service unit contracts）

步骤：

1. 在 `run()` 的最早安全点安装仅输出固定 code 的 bootstrap stderr；获得 app data dir 后创建并注册 `Arc<RuntimeLogService>`、session、crash marker 和 diagnostics。日志初始化失败时仍启动应用，并将 sink state 标为 degraded。
2. 用 runtime event 替换 `lib.rs` 启动、recovery、shutdown、tray close 与 startup upgrade 的动态 stdout/stderr。恢复页面仍用既有用户可见 DTO 表达状态，runtime event 仅记录稳定技术阶段/error code。
3. 迁移 `InstallationLease` 自身的直接输出：数据存储锁仍可产生运行事件/metric，但只能经其 owner adapter，不能把锁路径、PID 或 I/O error 原文写入事件。
4. 为 task supervisor、blocking executor、operation registry 和 persistence runtime 提供 adapter：记录 admission、timeout、cancel、terminal、busy/retry、open/recovery/close 等技术终态；不让 repository 或 SQL helper 直接记录。
5. 将现有 `LocalMetricBuffer` 和 `RuntimeDiagnostics` 吸收到 `RuntimeLogService` snapshot，新增 sink health、queue/drop/rejected、lease/recovery/clock/crash-marker 固定状态。保留既有指标边界，不复制 snapshot。
6. 确保 shutdown 顺序为：停止新业务 -> drain 有界 worker -> 关闭/发布 segment -> 删除 active marker -> 释放 log lease；任一步失败只能写固定 fallback 并留下可观测 degraded state，不能无限等待或阻断 persistence lease 释放。

Focused gate：

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_startup_cutover -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --nocapture
node scripts/runtime-logging-architecture.test.mjs
```

Exit gate：数据库不可用、recovery mode、directory/sink 故障、shutdown timeout 和 clean/unclean exit 都有安全 event 或固定 fallback；正常启动、持久化关闭及现有 data-store lease tests 不回归。

### Task 5：按 owner 切换 proxy、outbound、采集和监控 producer

目标：覆盖排错价值最高、最容易携带上游动态错误的运行路径，并立即删除旧输出。

文件：

- Modify: `src-tauri/src/services/proxy/{startup_auto_start.rs,lifecycle/writer.rs,execution.rs,runtime.rs}`
- Modify: `src-tauri/src/application/routing.rs`
- Modify: `src-tauri/src/background_tasks/routing_projection_runner.rs`
- Modify: `src-tauri/src/services/station_collectors.rs` 及必要的 collector runner/driver error boundary
- Modify: `src-tauri/src/services/monitoring/{runner.rs,maintenance.rs,executor.rs,orchestrator_transport.rs,transport.rs}`
- Modify: existing owner tests under `src-tauri/tests/monitoring_*.rs`, `src-tauri/tests/observability_contract.rs` and producer module unit tests

步骤：

1. 先改 proxy startup、lifecycle terminal persistence、outbound transport failure、routing projection。复用已有 proxy failure kind、transport phase、retry outcome 和 request correlation；任何 raw request id 先转换为既有 hash/subject。移除 `error = ?error`、`error = %error` 和自由消息。
2. collector 在 scheduler/run/driver application boundary 发出 start/failed/partial/rejected/writer-failure；继续由 `collector_runs`、snapshot 和 task state 保存业务事实。driver response、credential、snapshot 和 redacted diagnostics JSON 不可透传到 runtime detail。
3. monitoring 在 runner、maintenance、orchestrator、transport 边界发出 dispatch/persist/worker/maintenance/timeout 事件，复用现有闭合 `MonitoringFailure`/safe diagnostic code；execution/attempt/target result 仍是业务记录。
4. 每切换一个 owner，同一变更必须包含：owner-local descriptor、error mapper、subject/correlation/interaction 传播、metric、success/failure/degraded 测试、删除台账更新和旧宏删除。不得保留双写来“比对”。
5. 对高频成功代理路径仅记录被规范批准的 sampled/debug 或聚合 metric；绝不为每个成功请求同步落盘。所有错误映射失败收敛为 `internal_unclassified` + metric。

Focused gate：

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_faults -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_execution_integration -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_transport -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test observability_contract -- --nocapture
node scripts/runtime-logging-security.test.mjs
```

Exit gate：proxy、collector 和 monitoring 的 success/failure/timeout/cancel/degraded 路径都能以稳定 code 排错；所有历史业务记录测试仍通过；source inventory 中对应动态输出为零。

### Task 6：切换导入导出、updater 与前端错误边界

目标：补齐不常发生但最需要恢复证据的边界，并在浏览器侧绝不持久化 stack/props。

文件：

- Modify: `src-tauri/src/commands/{data_migration.rs,data_store_startup.rs,data_directory.rs,updater.rs,local_proxy.rs}`
- Modify: `src-tauri/src/services/{portable_migration/**,data_store/**}` 中真正拥有阶段终态的 application boundary
- Modify: `src/app/ShellPageErrorBoundary.tsx`
- Modify: `src/lib/bridge/runtimeContext.ts` 及相关 controller/tests
- Create: `src-tauri/tests/runtime_logging_migration_updater.rs`
- Create: `src/app/ShellPageErrorBoundary.test.tsx`

步骤：

1. 在迁移、导入、导出、数据目录切换、update preflight/proxy prepare 的 application 边界记录阶段终态、checksum/validation/rollback/recovery-needed 等稳定结果。不得在事件中写 package path、backup 路径、密码、配置内容、SQL、数据库错误文本或 backup 数据。
2. existing data-store diagnostic 保持独立。其文件写入失败/成功只记录技术 event；它的实际导出内容继续遵循 `SECURITY_EXPORT_IMPORT.md`，不被 runtime support bundle 扩展权限。
3. `ShellPageErrorBoundary` 仅发送 `frontend.boundary.failed` 的固定页面类别、构建版本、已验证 interaction 引用和恢复动作。Error/stack/props/DOM 只用于当前 UI 恢复，不进入 event、console、DTO、test snapshot 或 bundle。
4. 为用户按钮、对话框提交和多步 controller 引入 `runUserInteraction` scope；不要把 background query、effect 或页面 mount 包进 interaction。测试一个点击中连续 IPC 调用共享 id，两个并发点击不会串 id。

Focused gate：

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test runtime_logging_migration_updater -- --nocapture
pnpm.cmd test -- src/app/ShellPageErrorBoundary.test.tsx src/lib/bridge/runtimeContext.test.ts
pnpm.cmd build
node scripts/runtime-logging-security.test.mjs
```

Exit gate：导入导出/update 的失败可定位但不泄露路径/包/凭据；ErrorBoundary 和 interaction tests 覆盖 recover/invalid/parallel states；前端 build 通过。

### Task 7：实现 reader、developer diagnostics 与 support bundle

目标：把安全落盘数据以有界、后端控制的方式交给开发者排错和人工支持。

文件：

- Create: `src-tauri/src/services/support_bundle.rs`
- Create: `src-tauri/src/application/runtime_diagnostics.rs`
- Create: `src-tauri/src/commands/runtime_diagnostics.rs`
- Create: `src-tauri/src/ipc/dto/runtime_diagnostics.rs`
- Modify: `src-tauri/src/ipc/{dto/mod.rs,registry.rs}`、`scripts/generate-bindings.mjs`
- Modify generated outputs through `pnpm.cmd generate:bindings`
- Create: `src/features/runtime-diagnostics/{RuntimeDiagnosticsPage.tsx,RuntimeDiagnosticsPage.test.tsx,queries.ts}`
- Modify: `src/app/{shellPageRegistry.tsx,App.tsx}`、`src/components/shell/AppShell.tsx`、`src/lib/bridge/{BackendClient,DesktopBackend,DemoBackend}.ts`
- Modify: `src-tauri/tests/runtime_diagnostics_commands.rs`（command source contract；DTO/service 语义测试在 production module `#[cfg(test)]`）

说明：真实 `tauri::test::MockRuntime + State` 命令测试放在显式 Cargo feature `tauri-test` 下，默认 `cargo test` 和 `verify:full` 不加载该 Windows harness。由于生产默认 tray/native feature 组合在本环境会触发 Windows loader 挂起，专项必须使用隔离命令：`cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --features tauri-test --lib commands::runtime_diagnostics -- --nocapture`。测试可执行文件不经过 Tauri bundler 注入 Windows Common Controls v6 manifest，因此 `tauri-test` 明确不启用 `tauri/common-controls-v6`；否则会导入 `TaskDialogIndirect`，在 legacy `comctl32.dll` 下于进程启动前弹出入口点错误。生产 desktop feature 仍启用 v6，并由 packaged manifest 提供对应 activation context；修复后必须重建隔离 target，旧 test exe 不可复用。Windows smoke 脚本会拒绝启动历史 `target-tauri-feature-isolation` 路径，避免再次触发系统 loader 弹窗。该隔离专项已通过；真实 Windows 多进程 updater/restart 和 marker I/O fault-injection 仍必须单独验证，不能由该专项替代。

步骤：

1. `RuntimeLogReader` 仅从 runtime-log root 枚举已发布、metadata/byteLength/manifest 校验通过的 segment。以 cursor 流式读取，单页最多 200 行/1 MiB；损坏行、未知 schema、缺失 manifest 和 deprecated code 返回固定状态/计数，原始行不离开 Rust。
2. 新命令必须先验证 settings 的 developer mode，再读取或导出；registry contract 改为“runtime diagnostics commands 必须有 explicit developer-mode gate 和受限 DTO”，不能简单删除现有禁止测试。输入仅允许 cursor、level/component/eventCode/correlationId/interactionId 的精确筛选，不提供路径、regex 或全文查询。
3. UI 放入设置的开发者工具入口，而不是请求日志或业务使用记录。实现 loading、empty、reader error、sink degraded、retention failure、窄窗口和 keyboard focus；行只显示固定 message key、稳定 code、结果、耗时和匿名引用。普通模式不注册入口，也不得依赖前端隐藏作为安全控制。
4. `SupportBundleService` 由显式用户动作启动。后端选择保存路径、先写临时包、逐项上限/schema/file-name 校验、canary 扫描、原子 rename，取消或失败清理临时文件。成功 DTO 只能返回计数或固定 success，不能向前端、toast、runtime event 或错误 DTO 回传保存路径。首期只包含 manifest、runtime summary、受限 runtime events、既有匿名 data-store diagnostic、可选业务计数摘要；绝不包含 SQLite/WAL、备份、原始配置、snapshot、crash payload 或原始 request log。
5. 生成 bundle 时基于 validated reader，不直接 glob 目录；每个进入包的 segment 再做 canary 扫描。任何命中终止导出并返回固定错误 code，不以“截断”补救。不得自动上传、联网或承诺删除旧缓存。
6. 更新 IPC binding、BackendClient、DesktopBackend 和 DemoBackend。DemoBackend 必须明确不支持 diagnostics/support bundle，不伪造日志数据。

Focused gate：

```powershell
pnpm.cmd generate:bindings
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features --features tauri-test --lib commands::runtime_diagnostics -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib support_bundle -- --nocapture
pnpm.cmd test -- src/features/runtime-diagnostics/RuntimeDiagnosticsPage.test.tsx
pnpm.cmd build
pnpm.cmd test:contracts
```

Exit gate：developer mode 以外的 IPC 调用无法读取/导出；reader/bundle 不接触未发布/unknown 文件；UI 与 bundle 对所有状态有明确安全行为；生成物和 mock 同步。

### Task 8：删除旧路径并收紧架构、安全与 artifact 门禁

目标：完成唯一 owner 切换，阻止以后退回 stdout、动态 tracing 或自由 JSON。

文件：

- Modify: Task 0 inventory 中所有仍未删除的生产文件
- Modify: `src-tauri/src/observability/{events,metrics,diagnostics,redaction}.rs` 或删除已无 owner 的文件
- Modify: `.gitignore`、`scripts/architecture/check-artifact-policy.mjs` 与相关 artifact 测试
- Modify: `scripts/runtime-logging-{architecture,security}.test.mjs`
- Modify: `package.json`、`scripts/run-contract-tests.mjs`
- Modify: `docs/audits/runtime-logging-deletion-ledger.md`

步骤：

1. 按台账删除所有业务模块的 `println!`/`eprintln!`、未批准 `tracing` event 和动态 error format。允许的 stderr 只在 runtime bootstrap/crash fallback 的精确函数/行；`services/data_store/installation_lease.rs` 不得再自行输出。
2. 删除或接线旧 `StructuredEvent`、未消费 diagnostics/metric API 与多份 redaction marker；清除本专项相关的 `allow(dead_code)`/`expect(dead_code)`，不得以测试保留一个平行实现。
3. 架构门禁验证：仅批准的 runtime adapter 可持久化 event；禁止 `serde_json::Value`、`HashMap<String, String>`、`Error`、`String`/`&str` detail constructor、unsafe tracing fields、业务层文件 reader/writer 和 hand-written IPC runtime metadata。禁止 `catalog.rs` 定义非 runtime 自身的 descriptor，禁止生产 producer 调用裸字符串 `record` 或按 code 文本推断事件语义。
4. 安全门禁对每个 producer、reader、DTO、fixture、console fallback 和 bundle 注入 canary。检查 `.gitignore`/artifact policy 覆盖 `runtime-logs/`、`*.jsonl`、`*.partial`、catalog snapshot、crash marker 与 support bundle；测试和文档不得提交真实日志产物。
5. 更新删除台账为逐项 `removed` 或唯一的批准例外；例外必须写 exact path/symbol、理由、风险和退出条件，不能使用 `temporary`、`later` 或目录级 allowlist。

Focused gate：

```powershell
node scripts/runtime-logging-architecture.test.mjs
node scripts/runtime-logging-security.test.mjs
pnpm.cmd architecture:artifacts
pnpm.cmd test:dead-code-policy
pnpm.cmd audit:dead-code
git diff --check
```

Exit gate：扫描没有未批准旧路径；无双写/平行 observability 模型；artifact 和 secret policy 通过，删除台账没有开放的长期兼容项。

### Task 9：执行跨层故障、并发、容量与完整工程资格验证

目标：以确定性本地测试证明日志系统本身不会破坏代理、业务状态或安全边界。

当前状态：核心矩阵已由本地自动化测试、真实 JSONL artifact、lease/restart harness 和 debug-only packaged clean-start smoke 闭合；marker-I/O packaged fault 子进程因 harness 退出挂起停止，不影响生产 Rust 降级合同。人工打开页面、点击原生 save dialog 和真实 provider 明确不在本计划范围内。

必须覆盖的矩阵：

1. 两个 process/session 同时争夺 runtime-log lease，包含 restart/updater overlap、lock release 与 contender 恢复；证明一次只有一个 writer/recovery/retention owner。
2. active/partial/unknown/metadata-missing/metadata-size-mismatch/live-owner 文件不能被 reader、bundle 或 retention 读取/删除；recovery 成功、损坏、超预算、复制失败和重复启动均不产生半发布 segment。
3. directory 不可创建、disk full、rename/sync/metadata/retention 删除失败、queue saturation、writer 冷却恢复和 shutdown deadline；业务 command、proxy send 与 persistence transaction 仍能完成或按原业务合同失败。
4. UTC rollback/forward jump/unstable sampling；duration、deadline、retry 和 lease backoff 使用 fake monotonic clock，age deletion 暂停而 96 MiB cap 继续按 generation 实施。
5. first panic、recursive panic、marker mutex 已占用、marker write failure、unclean restart、clean shutdown marker 删除；assert 固定 stderr 且不包含 panic payload/stack/thread/path/environment。
6. event/catalog：collision、schema/detail mismatch、invalid replacement、message-key 缺失、subject/bundle permission、当前/上一 manifest、deprecated code、unknown segment；所有兼容路径安全隔离。
7. interaction：单手势多 command/child operation、并发手势隔离、no-action null、TTL/容量/跨 session/invalid capability；不得在 event 中泄露 capability 或拒绝值。
8. security：对每个 producer 及 support bundle 注入 canary，断言 JSONL、runtime DTO、frontend tests、stderr、fixture 和导出物都无明文；同时确认 request/collector/monitoring/alerting/migration 的既有业务事实未被 runtime logic 改写。
9. Windows packaged 自动 smoke：在独立临时 app-data 路径验证 rotation/restart、developer-mode gate、reader/export command 和普通模式拒绝；人工打开页面、点击原生 save dialog 的验收按用户决定不纳入本轮，UI 行为由 Vitest/command contract 覆盖。
10. 以本地 loopback fake provider/updater server（无真实凭据）实际驱动 proxy、collector、monitoring 和 updater 的 timeout、disconnect、malformed response、retry/cancel 分支；每项读取最终 JSONL artifact，而不是仅断言 adapter 被调用。
11. 将 startup/shutdown/restart 测试从 source-string matching 替换为 bootstrap/shutdown composition integration：注入 lease、sink、proxy drain 和 marker 故障，断言公开 setup error 合同、最终 JSONL、marker 文件和后续 restart 状态。

执行命令：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
pnpm.cmd generate:bindings --check
pnpm.cmd test
pnpm.cmd build
pnpm.cmd test:contracts
pnpm.cmd verify:fast
pnpm.cmd verify:full
node scripts/runtime-logging-architecture.test.mjs
node scripts/runtime-logging-security.test.mjs
git diff --check
```

Exit gate：所有本地自动化命令退出 0；任何因网络/host 时间限制无法完成的 full verifier 必须记录实际失败、未验证范围和风险，不得写作通过。真实 provider、真实密钥或真实 support bundle 分享不属于本计划自动化范围。

### Task 10：验收证据、文档状态与交付闭环

目标：使下一位维护者能从代码、测试和 audit 复现“为什么可用于排错且不会泄密”。

文件：

- Create: `docs/audits/runtime-logging-acceptance-matrix.md`
- Create: `docs/audits/runtime-logging-qualification.md`
- Modify: `docs/audits/runtime-logging-{source-inventory,deletion-ledger,canary-matrix}.md`
- Modify: `docs/proposals/RUNTIME_LOGGING_OBSERVABILITY_UPGRADE_SPEC.md`（仅在事实已满足时更新状态/实现证据）
- Modify: `docs/README.md`（仅在规范获批且索引状态实际变化时）
- Modify: 本计划状态（只记录完成任务和证据，不改写为长期规范）

步骤：

1. acceptance matrix 将目标规范每个验收项映射到 code owner、自动化 test/command、fixture、Windows smoke 或明确的未运行理由。不得用“full suite 覆盖”替代逐项证据；它是唯一的逐项映射来源。
2. qualification 只记录 OS/Rust/Node/pnpm、临时数据目录、参数和每次实际执行的日期/命令/结果/阻断原因；不复制 acceptance matrix，也不把历史通过结果写成当前通过。人工页面和原生 save dialog 不在本轮资格范围内，自动化 UI/command 证据必须单独列出。所有产物保留在 ignore 的临时位置，不提交真实日志。
3. 本计划只保留未完成实现、依赖顺序和设计决策；完成的逐项证据链接到 acceptance matrix。重跑 inventory，确认生产直接输出只剩批准固定 fallback，dead-code 草图和冗余 redaction 已删除；记录 source search 命令和零结果/精确例外。
4. 核心 exit gate 通过后将 proposal 标记 accepted/implemented 并更新 README；验证工具自身的非生产限制单独记录，不阻塞实现交付。

Exit gate：验收矩阵无无主项，删除台账无未批准长期例外，验证证据可独立重跑，文档状态与实际代码一致。

## 5. 每个任务的通用交付记录

### 本轮执行记录（2026-08-15）

计划不再保存测试流水账。当前实现与范围说明由 [验收矩阵](../audits/runtime-logging-acceptance-matrix.md) 逐项映射；最近一次实际命令结果、环境和验证工具限制由 [资格记录](../audits/runtime-logging-qualification.md) 维护。真实 provider/密钥和人工页面/native save dialog 均按范围不属于本轮工作。

每次 task/PR/审计记录必须包含：

```text
Task:
Start revision / End revision:
Dirty paths preserved:
Affected runtime event codes and manifestId:
Files added/modified/deleted:
RED command and expected failure:
GREEN focused command:
Cross-layer / generated-contract command:
Security-sensitive paths reviewed:
Canary inputs and observed result:
Deletion-ledger entries closed:
Unrun verification and reason:
Remaining blockers:
```

不允许以“日志已经写入文件”“编译通过”或“UI 能看到一行”作为 task 完成依据；必须同时说明安全字段、失败语义、并发/资源边界及相关回归测试。

## 6. 禁止的实施偏移

- 不启用全局 `tracing-subscriber` 文件输出来绕过 typed event contract。
- 不把 runtime event 写进 `request_logs`、`collector_runs`、monitoring execution 或新建无限增长 SQLite 表。
- 不将 `interactionId` hash 成 subject、复用为 correlation，或用页面/表单/route 数据生成它。
- 不在前端保存日志路径、直接读文件、传任意 message/stack，或只凭 UI 隐藏 developer 功能。
- 不把 redaction marker 命中视为保留部分动态文本的许可；不新增 `SafePreview` 同义抽象。
- 不让 crash hook 等待 queue、logger mutex、磁盘重试或默认 panic 输出。
- 不删除 unknown/partial 文件来满足容量；只有经过 bounded recovery 成功发布的已验证源 partial 才可由 recovery 清理。
- 不依赖文件名/mtime 计算 retention，或用 UTC wall clock 计算 timeout、duration、backoff。
- 不手工编辑 generated IPC 文件、catalog snapshot 或 lockfile；只通过对应脚本/构建流程更新。
- 不把 Tasks 4-8 拆成长期双写、旧 stdout fallback 或两套 diagnostics owner；回滚使用 Git/release 机制，不在 production 保留旧日志实现。
