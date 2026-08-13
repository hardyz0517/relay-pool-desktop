# 跨站点采集并发与统一协调器升级实施计划

> **实施方式：** 按 Task 顺序执行并用 checkbox（`- [ ]`）跟踪；每个行为变更先建立可解释的 RED，再完成最小实现与 GREEN。可使用任务执行工具辅助，但实施不依赖某个可选插件或 skill 才能继续。

状态：已实施并完成可靠性 / 可维护性 / 可扩展性复审（2026-08-13）

适用范围：后台定时采集、现有已保存站点的单站手动采集与登录探测、采集并发设置和启动组合。本文不增加批量采集 UI，也不引入单站内部并发。

**Goal:** 允许不同 `station_id` 的采集在统一的全局上限内并发执行，同时保证同一站点的手动、定时和登录探测互斥、单站子任务继续串行，并让失败、取消、配置变化和应用关闭都能可靠释放执行额度。

**Architecture:** 新增进程内、显式注入的 `StationCollectionCoordinator`，集中拥有全局并发额度和按站点互斥状态；后台 runner 使用有界异步流执行不同站点，手动 facade 使用同一个 coordinator 做即时准入。Coordinator 只管理执行生命周期，不持有凭据、Provider、数据库或采集结果；现有 collector driver、V2 apply、SQLite 单写者和单站任务顺序保持不变。

**Tech Stack:** Rust 2021、Tokio、`tokio_util::sync::CancellationToken`、`futures-util`、Tauri 2、现有 V2 collector/application/persistence 边界、Node.js architecture contract tests。

## Global Constraints

- 并发单位只能是已保存站点；不同 `station_id` 可并发，同一 `station_id` 在后台定时采集、手动采集和已保存站点登录探测之间最多一个在途操作。
- `balance -> groups -> remote key refresh` 等单站内部顺序保持串行；本次禁止在 provider driver 内增加 `join!`、`spawn` 或无界 fan-out。
- 全局上限使用现有 `collector_max_concurrency`，合法范围保持 `1..=8`，默认值保持 `3`，不增加新的设置或 migration。
- 手动与后台共享同一个全局额度，不能各自拥有一套 semaphore/HashSet，也不能依赖前端 `stationAction` 作为后端互斥保证。
- 后台遇到同站点已运行时跳过该站点并留待下一轮；手动入口遇到同站点已运行时返回稳定 `conflict`；手动入口遇到全局额度已满时返回可重试 `overloaded`。
- 后台可以等待全局额度，但等待必须响应 runner 的 `CancellationToken`。取消是协作式边界：已观察到取消的等待者不得取得 lease；刚取得 lease 的 future 必须在第一个 task 前再次检查 token；已经进入 driver 的请求继续依赖现有 request-budget / cancellation contract 收口，不能承诺消除所有纳秒级竞态。
- 降低并发设置不取消已开始的采集，只阻止新 lease，直到活动数低于新上限；提高设置唤醒等待者。后台流的 fan-out 上限最迟下一轮调度使用新值。
- Lease 必须通过 RAII 在成功、错误、取消、future drop 和 panic unwind 时释放站点占用与全局额度。
- Coordinator 不保证 FIFO、公平排队或手动优先；它只保证容量和同站互斥。首版手动入口不等待，因此不会在 coordinator 内形成手动 waiter；若将来需要优先级、进度或持久化恢复，必须设计独立作业队列。
- HTTP 请求并发、SQLite 写入继续通过现有单写者串行提交；不得为本升级增加数据库连接或绕过 `PersistenceRuntime::write`。
- 不记录 endpoint、cookie、API key、密码或原始认证响应；涉及 runner 错误日志时使用既有脱敏函数或稳定错误分类。
- 不新增依赖。Windows 命令全部使用 PowerShell / `pwsh` 兼容语法。
- 当前工作区已有用户改动；实施时只修改本计划列出的文件，不覆盖无关 diff。
- 未经用户在执行阶段明确授权，不 stage、commit、push、建分支或创建 PR；各 Task 末尾只建立 review checkpoint。

---

## 1. 现状、问题与非目标

当前代码已经具备以下基础：

- `collector_max_concurrency` 已进入 Rust/TypeScript 设置模型和设置 UI，但后台 runner 尚未消费它。
- `AsyncOutboundClient` 是可克隆异步客户端，已有连接池、请求预算和取消边界。
- V2 apply 使用 `endpoint_revision` fence 和 SQLite 单写者，允许不同站点先并发完成远端 I/O，再短暂排队提交。
- `station_collectors.rs` 有静态 `ACTIVE_STATION_RUNS`，但它只保护后台 runner，命令 facade 没有经过同一互斥边界。
- 后台到期站点当前使用 `for collection in collections { ... await }`，因此所有站点串行。

本次非目标：

- 不增加“采集全部”“采集所选”或逐站进度 UI。
- 不改变 `collect_station_info`、`collect_station_task` 等 IPC 输入/成功输出。
- 不实现跨进程或持久化作业队列；Relay Pool Desktop 当前只有单实例进程，这一互斥边界是进程内运行时服务。
- 不并发执行同一站点的 balance/groups，也不改变 full parent/child apply 语义。
- 不重构 Provider Registry、collector driver、remote-key 独立命令或 persistence schema。采集流程内部在持有 station lease 时触发的 remote-key refresh 视为同一 operation，不允许二次准入。
- Provider Draft preview/remote-key scan、`test_station_login_input`（尚无持久化 `station_id`）以及独立 remote-key 创建/扫描/删除命令不纳入本次 coordinator；它们分别由 draft 生命周期、输入级 probe 和现有 remote-key conflict/revision 边界负责。若后续要把独立 remote-key mutation 纳入同站 operation family，需单独审计其错误合同和嵌套调用，不能直接在内部 refresh 上重复 acquire。
- 不把通用 `OperationRegistry` 强行改造成站点协调器；两者生命周期、冲突语义和可配置上限不同。

## 2. 文件结构与职责

### 新建

- `src-tauri/src/services/station_collection_coordinator.rs`
  - 唯一职责：全局采集额度、按 `station_id` 互斥、可取消等待、动态上限和 RAII lease。
  - 不依赖 application service、Tauri state、数据库或 provider driver。

### 修改

- `src-tauri/src/services/mod.rs`
  - 暴露 crate 内 coordinator 模块。
- `src-tauri/src/services/station_collectors.rs`
  - 保留 due 查询、runner loop 和单站任务顺序。
  - 删除静态 `ACTIVE_STATION_RUNS`/`StationCollectorRunGuard`。
  - 使用 coordinator 和 `for_each_concurrent` 实现跨站有界并发。
- `src-tauri/src/application/command_facades/station_collection.rs`
  - 为手动采集和登录探测申请即时 lease，并在完整操作结束后释放。
  - 用一个小型 `run_with_station_collection_lease` wrapper 统一 lease 作用域，避免两个入口复制准入/释放逻辑。
  - 增加 typed admission error，不把冲突折叠成内部错误。
- `src-tauri/src/application/command_facades/settings_stations.rs`
  - 设置成功持久化后同步 coordinator 的运行时上限。
- `src-tauri/src/commands/station_collection.rs`
  - 将同站冲突、容量已满和无效内部 station ID 映射成稳定公共错误。
- `src-tauri/src/app_composition.rs`
  - 将同一个 coordinator clone 注入 settings facade 和 station collection facade。
- `src-tauri/src/lib.rs`
  - 启动时从已加载 settings 构造唯一 coordinator，并把 clone 传给命令 facade 与后台 runner。
- `scripts/station-auto-collector.test.mjs`
  - 将当前未接入门禁且引用旧 collector 路径的历史脚本收敛为当前并发架构合同。
- `scripts/run-contract-tests.mjs`
  - 将更新后的 station auto collector architecture contract 纳入 `test:contracts`。

### 不修改

- `src-tauri/src/services/collectors/**`：单站 driver 和 full child 顺序不变。
- `src-tauri/src/persistence/**`：现有单写者、revision fence 和 schema 不变。
- `src/features/stations/**`：现有 UI 仍一次发起一个手动动作；后端为未来批量入口提供正确边界，但本次不扩 UI。
- `src-tauri/src/application/command_facades/provider_drafts.rs`、`src-tauri/src/application/command_facades/remote_keys.rs`：不扩大本次 operation family；仅验证采集内部 refresh 没有新增 coordinator acquire。
- generated bindings/permissions：没有新增或删除 IPC command/DTO，不应产生生成物 diff。

## 3. 固定接口与运行语义

Coordinator 采用以下 crate-private 接口；实施中保持命名和错误语义一致：

```rust
#[derive(Clone)]
pub(crate) struct StationCollectionCoordinator {
    inner: Arc<StationCollectionCoordinatorInner>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StationCollectionAdmissionError {
    AlreadyRunning,
    AtCapacity,
    Cancelled,
    InvalidStationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StationCollectionCoordinatorSnapshot {
    pub max_concurrency: usize,
    pub active: usize,
}

impl StationCollectionCoordinator {
    pub(crate) fn new(max_concurrency: NonZeroUsize) -> Self;
    pub(crate) fn set_max_concurrency(&self, max_concurrency: NonZeroUsize);
    pub(crate) fn max_concurrency(&self) -> NonZeroUsize;
    pub(crate) fn snapshot(&self) -> StationCollectionCoordinatorSnapshot;

    // 手动入口：不排队。冲突和容量满立即返回。
    pub(crate) fn try_acquire(
        &self,
        station_id: &str,
    ) -> Result<StationCollectionLease, StationCollectionAdmissionError>;

    // 后台入口：同站冲突立即返回；仅在全局容量满时等待。
    pub(crate) async fn acquire(
        &self,
        station_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<StationCollectionLease, StationCollectionAdmissionError>;
}
```

内部状态只包含：

```rust
struct StationCollectionCoordinatorState {
    max_concurrency: NonZeroUsize,
    active_station_ids: HashSet<String>,
}
```

禁止维护独立的“已用 permit 数”；它必须始终等于 `active_station_ids.len()`，避免两个计数漂移。`StationCollectionLease::drop` 从集合删除 station 并调用 `Notify::notify_waiters()`。异步等待必须先创建并 `enable()` 一个 pinned `Notified`、再锁状态检查；只创建但不 poll/enable 的 `Notified` 不能可靠接收 `notify_waiters()`，会在 release 发生于检查与 await 之间时丢失唤醒。`Notify` 只负责避免忙等，不提供 FIFO 公平性：

```rust
loop {
    if cancellation.is_cancelled() {
        return Err(StationCollectionAdmissionError::Cancelled);
    }
    let notified = inner.notify.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();
    match try_insert_station(&inner.state, station_id)? {
        TryInsert::Acquired => {
            let lease = StationCollectionLease::new(
                Arc::clone(&inner),
                station_id.to_owned(),
            );
            if cancellation.is_cancelled() {
                drop(lease);
                return Err(StationCollectionAdmissionError::Cancelled);
            }
            return Ok(lease);
        }
        TryInsert::AlreadyRunning => {
            return Err(StationCollectionAdmissionError::AlreadyRunning)
        }
        TryInsert::AtCapacity => {}
    }
    tokio::select! {
        _ = cancellation.cancelled() => {
            return Err(StationCollectionAdmissionError::Cancelled);
        }
        _ = &mut notified => {}
    }
}
```

`acquire` 在插入 active set 后自身再检查一次 cancellation，若已取消就立即 drop lease 并返回 `Cancelled`；runner 在拿到 lease 后、调用第一个 provider task 前仍再检查一次。两层检查分别守 coordinator 的通用合同与调用侧副作用边界，不能互相替代。

`try_insert_station` 的判断顺序固定为：先拒绝 `station_id.trim().is_empty()`，再以原始 canonical `station_id` 执行 `active_station_ids.contains`，然后判断 `active_station_ids.len() >= max_concurrency`，最后 insert。不要 trim 后换 key，也不要大小写归一化；持久化 ID 是不透明标识。这样同站运行且全局也恰好满载时稳定返回 `AlreadyRunning`，不会随其他站点数量漂移成 `AtCapacity`。

`StationCollectionLease` 不实现 `Clone`，内部保存 canonical station ID；`Drop` 删除该 ID。debug 输出若实现只能展示 bounded 计数，不能输出 active ID。

标准库 Mutex poison 是 advisory 状态，不应让已经成功持久化的设置更新变成半更新。所有锁获取统一使用一个私有 `lock_state()` helper，并通过 `PoisonError::into_inner` 恢复原状态；关键区内禁止回调、panic、`.await`、日志、数据库或网络操作。`set_max_concurrency` 只更新上限并在值实际变化时唤醒等待者；活动 lease 数可以暂时高于新上限。`max_concurrency()` 和 `snapshot()` 只读取同一 state，不引入第二份 atomic/cache。

---

### Task 1: 建立可测试的共享协调器与动态额度合同

**Files:**

- Create: `src-tauri/src/services/station_collection_coordinator.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: `src-tauri/src/services/station_collection_coordinator.rs` (`#[cfg(test)]`)

**Interfaces:**

- Consumes: `std::num::NonZeroUsize`、`std::sync::{Arc, Mutex}`、`tokio::sync::Notify`、`tokio_util::sync::CancellationToken`。
- Produces: 第 3 节固定的 `StationCollectionCoordinator`、`StationCollectionLease`、`StationCollectionAdmissionError` 和 snapshot 接口。

- [x] **Step 1: 先写同站互斥和不同站并发的失败测试**

```rust
#[test]
fn clones_share_station_exclusion_but_allow_different_stations() {
    let coordinator = StationCollectionCoordinator::new(NonZeroUsize::new(2).unwrap());
    let clone = coordinator.clone();
    let first = coordinator.try_acquire("station-a").expect("first station starts");
    let second = clone.try_acquire("station-b").expect("different station starts");

    assert!(matches!(
        clone.try_acquire("station-a"),
        Err(StationCollectionAdmissionError::AlreadyRunning),
    ));
    assert_eq!(coordinator.snapshot().active, 2);
    drop((first, second));
    assert_eq!(coordinator.snapshot().active, 0);
}
```

- [x] **Step 2: 写容量、动态降级和 RAII 释放的失败测试**

覆盖以下精确断言：

```rust
let coordinator = StationCollectionCoordinator::new(NonZeroUsize::new(2).unwrap());
let a = coordinator.try_acquire("a").unwrap();
let b = coordinator.try_acquire("b").unwrap();
assert!(matches!(
    coordinator.try_acquire("c"),
    Err(StationCollectionAdmissionError::AtCapacity),
));

coordinator.set_max_concurrency(NonZeroUsize::new(1).unwrap());
drop(a);
assert!(matches!(
    coordinator.try_acquire("c"),
    Err(StationCollectionAdmissionError::AtCapacity),
));
drop(b);
assert!(coordinator.try_acquire("c").is_ok());
```

- [x] **Step 3: 写可取消等待和上限提高唤醒等待者的异步失败测试**

使用 `tokio::sync::oneshot`、`Barrier` 或带状态的 `watch` 通道，测试控制信号必须有记忆；不要用裸 `Notify::notify_waiters()` 作为“稍后释放全部 task”的唯一状态，也不要使用真实长时间 sleep。仅用短 `timeout` 作为失败上界，不能依赖 wall-clock 排序。测试必须证明：

1. `acquire("b")` 在 `a` 占满容量时不会提前完成；
2. drop `a` 后 `b` 获得 lease；
3. cancellation 发生时等待返回 `Cancelled`，snapshot 仍为 `active == 1`；
4. 从 1 提高到 2 会唤醒等待者，而不是等已有 lease drop。
5. 同站已运行且全局也满时稳定返回 `AlreadyRunning`，而不是 `AtCapacity`。
6. `station_id = "   "` 返回 `InvalidStationId`；非空 ID 保持原字符串作为 opaque key。

- [x] **Step 4: 写 future drop 与 panic unwind 的 lease 释放失败测试**

在一个 helper future 中 acquire lease 后等待永不完成的 `pending()`；spawn 后 abort task，join 返回 cancelled，再断言同站可重新 acquire。另用 `std::panic::catch_unwind(AssertUnwindSafe(...))` 在持有 lease 时触发测试 panic，catch 后断言 snapshot active 回到 0 且同站可重新 acquire。测试不得跨 `.await` 持有标准库 Mutex guard。

- [x] **Step 5: 运行测试并确认 RED**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml station_collection_coordinator -- --nocapture
```

Expected: FAIL，因为 coordinator 类型和模块尚不存在。

- [x] **Step 6: 实现最小 coordinator、lease 和 typed errors**

实现第 3 节接口；`try_acquire` 只用 `trim()` 判断是否为空，实际 key 保留原始 canonical ID；空值返回 `InvalidStationId`，不写入 active set。Mutex poison 恢复已有 state 后继续执行，不 panic、不清空集合，也不重建一份并发状态。

把重复的 lock recovery 与 insert 判定收进私有 helper，`try_acquire` 和 `acquire` 共用同一条判断路径；lease 不可 clone，避免同一个 active entry 被多个 owner 重复 drop。

- [x] **Step 7: 运行 coordinator 测试并确认 GREEN**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml station_collection_coordinator -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Expected: coordinator 单元测试全部 PASS；format check PASS。

- [x] **Step 8: Review checkpoint（不 stage/commit）**

检查 diff 只新增纯运行时协调器，不引用 Tauri、settings、collector driver 或 persistence；确认没有 `static Mutex<HashSet<_>>` 和持锁 `.await`。

---

### Task 2: 后台 runner 按站点有界并发且保持单站顺序

**Files:**

- Modify: `src-tauri/src/services/station_collectors.rs`
- Test: `src-tauri/src/services/station_collectors.rs` (`#[cfg(test)]`)

**Interfaces:**

- Consumes: `StationCollectionCoordinator::{max_concurrency, acquire}` 和现有 `StationCollectorRunnerPort`。
- Produces: 后台跨站并发；同站冲突跳过；观察到取消后不再进入新 task；现有单站 task 顺序不变。

- [x] **Step 1: 写跨站并发上限的失败测试**

新增受控 fake port，使用 `AtomicUsize` 记录 `active`/`peak`，以 `Barrier` 确认前两个 station 已进入，再用 `watch<bool>` 或每站 `oneshot` 释放；不要用可能丢失先行通知的裸 `Notify`。测试启动三个不同站点、coordinator limit=2，并断言：

```rust
assert_eq!(port.peak_active(), 2);
assert_eq!(port.started_station_ids().len(), 2);
release_tx.send(true).expect("release controlled tasks");
runner.await.expect("batch joins");
assert_eq!(port.completed_station_ids().len(), 3);
```

测试不得只检查源码含 `for_each_concurrent`；必须用可控 future 证明真实重叠和 `peak <= limit`。

- [x] **Step 2: 写失败隔离、同站跳过与取消的失败测试**

精确覆盖：

- station A 返回失败时 station B/C 仍运行完成；
- 预先持有 station A lease 后执行 due batch，A 不调用 port，B 正常运行；
- coordinator limit=1，先由测试预持有一个非 due 站点 X 的 lease 占满额度，再启动包含 B 的 due batch；确认 B 的 future 已进入 `acquire` 等待后取消 runner token，B 的 `collect_task` 调用次数仍为 0；不能用“A 占用 stream 唯一 fan-out slot”来声称 B 正在等待，因为此时 B 尚未被 poll；
- cancellation 与 X lease release 同时发生时，若 B 先取得 lease，也必须在调用 `collect_task` 前再次检查 token 并退出；用 Barrier/oneshot 控制交错并重复有限次，不要求消除取得 lease 前后的纳秒级竞态；
- `StationCollectorRunnerState::stop_and_join` 在受控 task 正在运行或等待额度时触发取消；fake port 响应传入 token 后 join 成功，coordinator snapshot 最终为 `active == 0`，且取消不记录成 provider failure；
- 现有 `guarded_collection_runs_balance_then_groups_for_due_station` 继续断言单站调用顺序为 balance 后 groups。

- [x] **Step 3: 运行 focused tests 并确认 RED**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml station_collectors -- --nocapture
```

Expected: 新增并发测试至少有一项 FAIL；当前 loop 的 peak 仍为 1，且同站保护仍来自旧静态 guard。取消等待用例可能因待实现 helper/签名而先编译失败，这同样是有效 RED，不要求一次 RED 同时观察到所有运行时断言。

- [x] **Step 4: 将 runner 改为有界异步流**

引入：

```rust
use futures_util::stream::{self, StreamExt};
```

将 `run_due_station_collections_once_v2` 改为接收 `&StationCollectionCoordinator`，在每一轮开始时快照 `max_concurrency().get()`，再执行。`for_each_concurrent` 限制本轮活跃 future 数，coordinator 则统一限制后台与手动入口；双层限制用途不同，不得删除 coordinator 后只保留 stream 上限：

```rust
stream::iter(collections)
    .for_each_concurrent(Some(max_concurrency), |collection| async {
        match run_station_collection_guarded_v2(
            port,
            coordinator,
            &collection,
            context,
        )
        .await
        {
            Ok(ScheduledStationCollectionOutcome::Completed) => {}
            Ok(ScheduledStationCollectionOutcome::SkippedAlreadyRunning) => {}
            Ok(ScheduledStationCollectionOutcome::Cancelled) => {}
            Err(error) => tracing::warn!(
                error = %crate::services::secrets::mask::redact_text_preview(&error, 512),
                "scheduled station collection failed"
            ),
        }
    })
    .await;
```

定义 crate-private `ScheduledStationCollectionOutcome::{Completed, SkippedAlreadyRunning, Cancelled}`，避免通过比较英文错误字符串控制流程。

due query 失败分支也改用相同的结构化、脱敏日志，不再 `eprintln!` 原始 error；日志只包含稳定事件名和脱敏后的错误，不输出 station ID、endpoint 或 credential。

- [x] **Step 5: 用 coordinator 替换旧静态 guard**

在任何 task 开始前调用 `coordinator.acquire(station_id, cancellation)`，取得 lease 后立即再次检查 `context.cancellation_token.is_cancelled()`；若已取消则 drop lease 并返回 `Cancelled`。将 lease 保持到该站所有 tasks 和其触发的 remote-key refresh 结束。映射规则：

- `AlreadyRunning -> Ok(SkippedAlreadyRunning)`；
- `Cancelled -> Ok(Cancelled)`；
- `InvalidStationId -> Err("station collection invariant violation")`；
- `AtCapacity` 不应由等待式 `acquire` 返回；若出现则作为 coordinator invariant violation fail closed。

删除 `ACTIVE_STATION_RUNS`、`StationCollectorRunGuard` 及其全局状态测试；把原 reentry 测试改成预先持有 coordinator lease 的显式隔离测试。

单站 `for task` 的每次迭代开始和 remote-key refresh 前再次检查 token。`collect_task`/refresh 返回 error 后若 token 已取消，立即返回 `Cancelled`，不把取消字符串加入 provider failure 聚合，也不继续下一个 child task；仅在 token 未取消时才记录真实 provider failure。不要通过匹配 `"cancelled"` 英文字符串判断控制流。

- [x] **Step 6: 保持任务失败聚合但不串行化其他站点**

单站内部继续按现有 `for task in &collection.tasks` 聚合失败；禁止把共享 `Vec`/Mutex 放到跨站外层。每个 station future 自己记录和返回结果，A 的失败不得 cancel batch token。

后台等待者不承诺 FIFO；due collections 保持 `merge_due_station_collections` 的稳定输入顺序作为 best-effort 启动顺序，但验收只检查容量、完成性和无饥饿的有限批次结果，不断言严格先后。

- [x] **Step 7: 运行 runner 测试并确认 GREEN**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml station_collectors -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Expected: 跨站 peak 与配置一致、失败隔离、取消和原单站顺序测试全部 PASS。

- [x] **Step 8: Review checkpoint（不 stage/commit）**

确认没有单站 child 并发，没有 `tokio::spawn` 后失去 join/cancellation 所有权，也没有继续保留静态全局 active set。

---

### Task 3: 手动采集和登录探测进入同一站点互斥边界

**Files:**

- Modify: `src-tauri/src/application/command_facades/station_collection.rs`
- Modify: `src-tauri/src/commands/station_collection.rs`
- Test: `src-tauri/src/application/command_facades/station_collection.rs`
- Test: `src-tauri/src/commands/station_collection.rs`

**Interfaces:**

- Consumes: `StationCollectionCoordinator::try_acquire`。
- Produces: `run_with_station_collection_lease` 这一单一准入 wrapper、`StationCollectionCommandError::Admission(StationCollectionAdmissionError)` 和稳定公共冲突/过载映射。

- [x] **Step 1: 先写独立准入 wrapper 的失败测试**

不要为了这次准入逻辑给整个 facade 新增 collector/outbound/apply port；现有 facade 依赖均为具体 service，这样做会扩大重构。先在同一文件新增待实现 helper 的测试：

```rust
async fn run_with_station_collection_lease<T, F, Fut>(
    coordinator: &StationCollectionCoordinator,
    station_id: &str,
    operation: F,
) -> Result<T, StationCollectionCommandError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, StationCollectionCommandError>>,
```

构造共享 coordinator，预先持有 `station-1` lease，把会设置 `AtomicBool` 的 closure 传给 helper。测试必须证明 closure 未被调用，并得到：

```rust
assert!(matches!(
    error,
    StationCollectionCommandError::Admission(
        StationCollectionAdmissionError::AlreadyRunning
    )
));
```

再以 limit=1、预先持有其他站点 lease，证明 station-2 返回 `AtCapacity`，closure 未被调用且不排队。该 helper 测试直接证明所有包裹它的 prepare/outbound/apply 操作都不会在准入失败时被 poll，不需要伪造整条 facade 依赖图。

- [x] **Step 2: 写 wrapper 的完整 lease 生命周期测试**

在 helper closure 内通过 `oneshot` 通知“operation 已开始”，再等待另一个 `oneshot` 释放。operation 等待期间，用 coordinator clone 对同一站点 `try_acquire`，必须返回 `AlreadyRunning`；释放后 helper 完成，再次 acquire 必须成功。closure 返回 error 和 helper future 被 abort 两种路径也必须释放 lease。

生产 wiring 通过代码结构保证：`run_station_collection` 与已保存站点的 `test_station_login` 都只在最外层调用同一个 helper。NewAPI 登录探测可能更新 session，因此必须与采集认证恢复共享站点 lease。无持久化 station ID 的 `test_station_login_input` 明确不调用 helper。

- [x] **Step 3: 写公共错误合同失败测试**

在 `commands/station_collection.rs` 的 test module 精确断言：

```rust
let conflict = public_station_collection_error(
    StationCollectionCommandError::Admission(
        StationCollectionAdmissionError::AlreadyRunning,
    ),
);
assert_eq!(conflict.code, CommandErrorCode::Conflict);
assert!(conflict.retryable);

let overloaded = public_station_collection_error(
    StationCollectionCommandError::Admission(
        StationCollectionAdmissionError::AtCapacity,
    ),
);
assert_eq!(overloaded.code, CommandErrorCode::Overloaded);
assert!(overloaded.retryable);
```

Conflict message 固定为 `"A collection for this station is already running."`；不得包含 station name、ID 或上游错误。Conflict 使用 `retryable = true`，因为当前 operation 结束后可重试；AtCapacity 复用 `CommandError::from_work(WorkFailure::Overloaded)`。`InvalidStationId` 和不可能从 `try_acquire` 返回的 `Cancelled` 映射为 bounded internal error。补测试通过 `CommandError::try_new` 的敏感文本校验。

Conflict 通过 `CommandError::try_new(CommandErrorCode::Conflict, ..., true, None, ...)` 创建，不附带会泄露资源标识的 `PublicErrorDetails::Conflict`；若 envelope invariant 意外拒绝固定常量，fail closed 到 `CommandError::internal`，不能 panic。AtCapacity 沿用现有 Overloaded 固定文案，避免增加第二套公共过载合同。

- [x] **Step 4: 运行 focused tests 并确认 RED**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml station_collection -- --nocapture
```

Expected: 新增测试因 coordinator 尚未注入 facade、error enum 尚无 Admission variant 或 helper 尚不存在而 FAIL；不要求所有新测试在同一次 RED 中都进入运行期。

- [x] **Step 5: 实现单一 wrapper 并在 facade 最外层使用**

给 `StationCollectionCommandFacade` 增加 coordinator 字段和 constructor 参数，实现 Step 1 的 `run_with_station_collection_lease`。把现有两个方法主体分别移入 `run_station_collection_inner` 和 `test_station_login_inner`，公开入口只负责调用 wrapper；这样 lease 在任何 blocking prepare、网络、session persistence、apply 和采集触发的 remote-key refresh 之前取得，并保持到 inner future 返回。

禁止只包住 `finish_*` 网络阶段；否则 prepare/apply/remote-key refresh 仍可能与后台同站任务交错。禁止给 `RemoteKeysCommandFacade::scan_remote_station_keys_with_context` 增加第二次 acquire，因为它是当前 operation 的内部步骤，会造成自冲突。

- [x] **Step 6: 增加 typed error 和公共映射**

扩展：

```rust
pub(crate) enum StationCollectionCommandError {
    Admission(StationCollectionAdmissionError),
    Prepare(ApplicationError),
    Apply(ApplicationError),
    Blocking(BlockingExecutorError),
}
```

公共映射使用现有 `CommandErrorCode::{Conflict, Overloaded}`；不修改全局 `CommandError` DTO 或 generated TypeScript。

- [x] **Step 7: 运行 facade/command tests 并确认 GREEN**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml station_collection -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Expected: wrapper 的同站冲突、全局满载、error/abort 释放和完整生命周期测试，以及现有采集结果测试全部 PASS。

- [x] **Step 8: Review checkpoint（不 stage/commit）**

确认 `detect_sub2api_station`、`collect_sub2api_station`、`detect_station_info`、`collect_station_info`、`collect_station_task` 最终都调用 `run_station_collection`，`test_station_login` 调用已保存站点 wrapper，而 `test_station_login_input` 按非目标保持独立；确认错误输出不暴露 station 标识或 secret。

---

### Task 4: 启动组合使用唯一实例并让设置变化实时生效

**Files:**

- Modify: `src-tauri/src/application/command_facades/settings_stations.rs`
- Modify: `src-tauri/src/app_composition.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/application/command_facades/settings_stations.rs`
- Test: `src-tauri/src/app_composition.rs`

**Interfaces:**

- Consumes: 已验证的 settings `collector_max_concurrency: u16`。
- Produces: 一个启动期 coordinator 实例，其 clone 同时进入 settings facade、station collection facade 和 runner。

- [x] **Step 1: 写 settings 持久化到运行时同步的行为测试**

不要为一次 persistence failure 给 `SettingsStationsCommandFacade` 的全部具体 service 增加测试 trait。提取一个只覆盖关键顺序、仍依赖真实 `SettingsService` 的私有 async helper，例如：

```rust
async fn persist_and_apply_collection_runtime_settings(
    settings_service: &SettingsService,
    coordinator: &StationCollectionCoordinator,
    input: UpdateSettingsInput,
) -> Result<AppSettings, ApplicationError> {
    let settings = settings_service.update(input).await?;
    coordinator.set_max_concurrency(validated_collection_limit(&settings));
    Ok(settings)
}
```

按 `application/settings.rs` 现有测试方式用 `PersistenceRuntime::initialize_new` 和临时目录构造真实 `SettingsService`。测试 input 通过本文件内的合法 fixture builder 生成，只覆写本用例关心的 `collector_max_concurrency`，避免复制一排魔法字段。先以合法 input 把并发从 3 改为 1，断言返回值、重新 load 的持久化值和 coordinator 都为 1；再提交 `collector_max_concurrency = 0` 的非法 domain input 触发 store/service 失败，断言 coordinator 和重新 load 的值仍为 1。测试结束关闭 runtime。这样直接证明失败不会产生“数据库未保存但运行时已改变”，同时不伪造整条 facade 依赖图。

该顺序不是数据库和内存的原子事务，也不需要做成分布式事务：内存更新不可失败；若进程恰在持久化成功后、内存更新前退出，重启会从持久化 settings 重建 coordinator。把这条崩溃恢复语义写入测试名称/注释，避免以后错误交换顺序。

- [x] **Step 2: 写启动/组合显式注入测试**

coordinator 自身的 clone 共享行为已由 Task 1 行为测试证明；不要给 facade 暴露内部字段或测试专用 getter 来比较指针地址。调整 `app_composition` 的现有 constructor 调用和编译覆盖，确保 settings facade 与 collection facade 都新增显式 coordinator 参数；无需为了“测试 constructor”组装整套数据库、vault 和 outbound runtime。runner 的同实例 wiring 由 `lib.rs` 组合代码和 Task 5 的窄架构门禁保护。

- [x] **Step 3: 运行 focused tests 并确认 RED**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml settings_stations -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml app_composition -- --nocapture
```

Expected: 新增 helper/constructor 覆盖因 settings facade 与 composition 尚未注入 coordinator 而 FAIL；已有无关 `app_composition` 测试应保持通过。

- [x] **Step 4: 调整启动顺序但不扩大重构**

在 `lib.rs` 中，`app_services` 创建完成后立即加载一次 settings；使用经过 store 校验的值构造：

```rust
let station_collection_coordinator = StationCollectionCoordinator::new(
    NonZeroUsize::new(usize::from(settings.collector_max_concurrency))
        .expect("validated collector concurrency is non-zero"),
);
```

复用这次 load 的同一 `settings` 设置 tray behavior，删除该 setup 流程后面的重复 load；不要承诺或机械约束整个进程只能调用一次 `settings.load()`。不要新增默认值回退；若 settings 无法加载，沿用当前 startup fail-closed。

- [x] **Step 5: 显式注入三个 owner**

把 `station_collection_coordinator.clone()` 传给：

1. `compose_settings_stations_command_facade`；
2. `compose_station_collection_command_facade`；
3. `StationCollectorRunnerState::start_v2`。

不要把 coordinator 注册成额外 Tauri command state，也不要在 `v2_runner_port` 内偷偷构造新实例。

- [x] **Step 6: 设置持久化成功后更新 coordinator**

在 `SettingsStationsCommandFacade::update_settings` 中调用 Step 1 的 `persist_and_apply_collection_runtime_settings(...).await?`，最后更新 tray behavior。转换使用 `NonZeroUsize::new(usize::from(...)).expect(...)`，依据 IPC/domain/store 已有 `1..=8` 校验。

保持运行时副作用的固定顺序：持久化 settings → 更新不可失败的 coordinator → 更新不可失败的 tray state → 返回成功。禁止在持久化前更新 coordinator，也不要为了形式上的回滚引入补偿事务。

- [x] **Step 7: 验证动态降低与提高语义**

复用 Task 1 coordinator tests：两个 active lease 存在时从 2 降为 1，active snapshot 仍为 2；释放到 active 小于新 limit 前不发放新 lease。提高为 3 后等待者被唤醒。settings helper 级只测试持久化成功/失败后的值同步，不重复 coordinator 的并发状态机测试。

明确 runner 的动态语义：coordinator 立即采用新 limit；当前 `for_each_concurrent` 的 stream fan-out 使用本轮启动时快照，所以提高 fan-out 最迟下一轮生效，降低时即使已有更多等待 future，也会在 coordinator 准入处受新 limit 约束；已经持有 lease 的任务不取消。

- [x] **Step 8: 运行组合测试并确认 GREEN**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml settings_stations -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml app_composition -- --nocapture
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

Expected: tests PASS；cargo check PASS；启动组合不存在 constructor 漏传或第二实例。

- [x] **Step 9: Review checkpoint（不 stage/commit）**

确认本 setup 流程复用同一份已加载 settings，运行时更新发生在持久化成功之后；确认调低上限不 cancel active lease，且没有为单一同步点引入新的 service port/trait。

---

### Task 5: 修复自动采集架构门禁并完成回归验证

**Files:**

- Modify: `scripts/station-auto-collector.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`
- Verify only: `src-tauri/src/services/collectors/**`
- Verify only: `src-tauri/src/persistence/runtime.rs`

**Interfaces:**

- Consumes: Tasks 1-4 的生产调用链。
- Produces: 防止并发设置再次成为无消费者、手动入口绕过协调器或静态全局 guard 回归的 architecture contract。

- [x] **Step 1: 先运行历史脚本记录当前 RED 基线**

```powershell
node .\scripts\station-auto-collector.test.mjs
```

Expected: 当前脚本因引用已移除的旧 collector 路径或旧启动字符串而失败；记录失败只作为该死门禁需要修复的证据，不改生产代码迁就旧路径。

- [x] **Step 2: 将脚本收敛为当前架构合同**

先逐条分类现有 assertion：

1. 保留仍成立且有稳定来源的产品合同，例如按站点 interval 查询 due、定时任务仍包含 balance/groups、Stations 页面通过共享 query 刷新采集结果；
2. 对已移动但语义仍成立的合同，更新到当前 `collectors/drivers/**` 或当前模块，而不是让生产代码迁就旧路径；
3. 删除已经由 Rust 行为测试更可靠覆盖的私有函数名/调用顺序字符串，或将其迁到对应 Rust test；
4. 删除确实已废弃的合同，并在 diff review 中写明替代保护或删除理由，不能因为脚本当前是死门禁就无差别清空有效检查。

然后删除对已移除的 `services/collectors/sub2api.rs`、`collectors/adapters/**` 和旧 `StationCollectorRunnerState::start` 的读取，并新增少量、稳定的 owner/wiring 不变量，例如：

```javascript
assert.ok(coordinatorSource.includes("pub(crate) struct StationCollectionCoordinator"));
assert.ok(!stationCollectorSource.includes("ACTIVE_STATION_RUNS"));
assert.ok(!stationCollectorSource.includes("StationCollectorRunGuard"));
assert.ok(servicesModSource.includes("mod station_collection_coordinator;"));
assert.ok(libSource.includes("let station_collection_coordinator = StationCollectionCoordinator::new"));
assert.ok(!appCompositionSource.includes("StationCollectionCoordinator::new"));
assert.ok(!stationCollectorSource.includes("StationCollectionCoordinator::new"));
```

再对 `compose_settings_stations_command_facade`、`compose_station_collection_command_facade` 和 `StationCollectorRunnerState::start_v2` 的调用点做窄的参数存在性检查，但不要锁定完整函数体、变量出现次数、`for_each_concurrent` 这种实现细节或错误文案。脚本新增部分只保护“生产环境唯一构造点、禁止静态旧 guard、三个显式 consumer”这些 owner 边界；真实并发、取消、设置顺序和错误映射继续由 Rust 行为测试负责，禁止用源码字符串替代 Tasks 1-4 的 Rust tests。

如果稳定的调用点无法在不匹配大段源码的情况下检查，宁可省略该正向断言并依赖 constructor 类型检查，也不要建立每次格式化/改名都会误报的门禁。

- [x] **Step 3: 把修复后的脚本接入 contract suite**

在 `scripts/run-contract-tests.mjs` 的 contracts 数组增加：

```javascript
["node", ["scripts/station-auto-collector.test.mjs"]],
```

- [x] **Step 4: 运行 architecture contract**

```powershell
node .\scripts\station-auto-collector.test.mjs
pnpm.cmd test:contracts
```

Expected: PASS，且不再读取不存在的历史路径。

额外做一次门禁故障注入验证：临时在工作区副本/内存字符串中移除一个新 owner 标记，确认脚本确实失败后立即恢复；不得靠永久修改生产文件制造失败。若脚本结构不便做内存替换，至少分别运行旧基线 RED 与修复后 GREEN，并在 review 中人工核对每条 assertion 的目标文件真实存在。

- [x] **Step 5: 运行完整的相关 Rust 验证**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml station_collection_coordinator -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml station_collectors -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml station_collection -- --nocapture
```

Expected: 全部 PASS。

- [x] **Step 6: 运行仓库级快速门禁**

```powershell
pnpm.cmd verify:fast
```

Expected: PASS。若失败，必须区分本任务回归与当前工作区已有改动造成的失败，并在交付记录中列出具体命令、首个失败和未验证范围。

- [x] **Step 7: 检查无意扩张和生成物**

```powershell
git status --short
git diff -- src-tauri/src/services/station_collection_coordinator.rs src-tauri/src/services/station_collectors.rs src-tauri/src/application/command_facades/station_collection.rs src-tauri/src/application/command_facades/settings_stations.rs src-tauri/src/commands/station_collection.rs src-tauri/src/app_composition.rs src-tauri/src/lib.rs scripts/station-auto-collector.test.mjs scripts/run-contract-tests.mjs
git diff -- src-tauri/generated src-tauri/gen src/lib/bridge/generated.ts
```

Expected: 第一条 diff 只包含本计划边界；第二条没有 generated/permission 变化。

- [x] **Step 8: Review checkpoint（不 stage/commit）**

交付时报告实际修改、实际验证结果、当前工作区无关失败和未验证项。只有用户另行授权后，才按 Task 边界选择明确路径 stage/commit，禁止 `git add .` 或 `git add -A`。

---

## 4. 可靠性验收矩阵

| 场景 | 必须结果 |
|---|---|
| limit=3，A/B/C 到期 | 三站可重叠执行，peak=3 |
| limit=2，A/B/C 到期 | peak 不超过 2，第三站在额度释放后开始 |
| A balance 失败，B/C 正常 | B/C 不取消；A 继续其同站后续 task 并按现有规则聚合失败 |
| A 后台运行时手动采集 A | 手动立即返回可重试 `conflict`，不发第二组网络请求 |
| A 后台运行时手动采集 B，仍有额度 | B 可并发 |
| 全局额度已满时手动采集其他站 | 立即返回可重试 `overloaded`，不无界等待 |
| 手动 A 运行时后台轮到 A | 后台跳过 A，不记为 provider failure，下一轮仍可 due |
| runner 取消时有 station 等额度 | 已观察到取消的等待者返回 Cancelled；即使取消与发放 lease 竞态，准入后的二次检查也阻止 provider 请求 |
| 在途数=3，将 limit 从 3 降到 1 | 三个在途任务不被杀；全部完成前不发放新 lease |
| limit 从 1 升到 3 | coordinator 唤醒等待者；后台 stream 最迟下一轮采用更大 fan-out |
| prepare/apply/remote-key refresh 失败 | lease 由 Drop 释放，后续同站任务可重新进入 |
| panic unwind / future 被 drop | lease 释放；active set 不残留 station ID |
| station endpoint revision 在采集中变化 | 继续由现有 revision fence 拒绝陈旧 apply；并发升级不绕过该门禁 |
| SQLite writer 忙 | 网络任务可并发完成，写入按现有单写者排队，不新增写连接 |

## 5. 可扩展性与维护边界

- 未来“采集全部/采集所选”应由 Rust 应用层用 bounded stream 调度每个站点，并在每个 job 内复用 `StationCollectionCoordinator::acquire`；coordinator 只是全局准入原语，不是作业队列。前端只发起批任务并消费进度，不用 `Promise.all` 自建并发状态。
- 新增 Provider 不需要修改 coordinator；Provider 只实现现有单站 collection contract。
- 若未来需要手动任务优先级、FIFO、公平性、队列进度、取消单个 job 或持久化恢复，应新设计应用层作业队列，不能向 coordinator 的 Mutex state 塞入调度策略、UI 或数据库职责。
- 若未来确需单站 child 并发，必须另立设计并证明认证刷新、请求预算、集合完整性和 apply 顺序安全；不能把本计划的跨站额度误用为 child 并发数。
- `snapshot()` 只为测试、诊断和未来只读可观测性提供 bounded 数值；不得暴露 active station ID 列表。
- Provider Draft 预览、`test_station_login_input` 和独立 remote-key 扫描若未来需要限流，应按各自可用的稳定 identity 和资源风险单独设计；不得伪造 saved station ID 塞入本 coordinator。
- 本次仅拆出已有 active-run 状态，因为它正是跨入口共享边界；不顺带拆分 `station_collectors.rs` 的 due query、task adapter 或 provider route。

## 6. 完成定义

同时满足以下条件才可声称升级完成：

1. Rust 行为测试证明不同站点真实并发且不超过 `collector_max_concurrency`，不是只靠源码扫描。
2. 同一 coordinator clone 同时保护后台、手动采集和登录探测。
3. 旧静态 `ACTIVE_STATION_RUNS`/`StationCollectorRunGuard` 已删除。
4. 单站 balance/groups/remote-key refresh 顺序未改变。
5. 失败隔离、取消、动态调低/调高、RAII 释放测试通过。
6. 手动冲突/过载使用稳定公共错误，且不泄露 station/secret。
7. 更新后的 station auto collector architecture contract 已纳入 `pnpm.cmd test:contracts`。
8. `cargo fmt --check`、`cargo check --locked`、相关 Cargo tests 与 `pnpm.cmd verify:fast` 实际通过；若受当前工作区无关改动阻塞，交付必须准确披露，不能声称通过。
9. 没有 schema、generated bindings、Tauri permissions、前端页面或 collector driver 的无关变化。
