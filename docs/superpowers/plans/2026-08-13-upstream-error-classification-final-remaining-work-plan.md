# 上游错误分类与重试：剩余实质工作收口计划

状态：Ready for implementation

日期：2026-08-13

适用范围：本计划根据任务 `019ff60e-2d4d-70b0-8b8b-708a17fef8bf` 与
`019ff0a9-246c-7972-9048-d69d81e998c9` 已落地的代码和定向验证，列出仍未
关闭的实质工作。它补充而不替代
[`2026-08-12-upstream-error-classification-retry-upgrade.md`](2026-08-12-upstream-error-classification-retry-upgrade.md)
和已有的 2026-08-13 工作记录；实际实现仍以当前代码、自动化契约和
`docs/README.md` 指向的规范为准。

## 已完成基线

以下内容已有实现，不应重新拆分开发；只能在后续集成验证中发现问题时修复：

1. 上游错误的 canonical 分类、受限错误 envelope、运行时 `DecisionTrace`、
   有界诊断内存和 scoped verdict 写入链已进入当前工作区。
2. `0037_request_routing_outcome_summaries.sql` 已提供版本化、脱敏的 durable
   terminal summary，finalization 已向同一终态写入路径传递 typed summary。
3. `0038_trusted_capacity_domains.sql` 已提供显式
   `provider_family`、可选 deployment/region 和 revision；domain commitment
   仅由这些可信事实构造。缺失身份时 fail closed，不允许跨域回退。
4. Coordinator 已在同目标 capacity 重试耗尽后排除同域 sibling，并最多允许
   一次不同可信域的 outbound fallback；其 terminal 后关闭该请求的 fallback
   链。定向测试已覆盖不同域一次回退和同域不轮换 key。

## 不变量

- 绝不从 URL、station/key/type、HTTP status、body poll 或下游 commit 推断
  capacity domain 或 socket send phase。
- 上游是否可能接受请求不明确时，非幂等请求必须 fail closed；已提交响应不得
  retry，也不得产生第二个 terminal。
- durable DTO、日志、fixture 和 metrics 只保存稳定闭集值、版本和经审计的
  digest，不保存 secret、认证信息、完整 URL/query、原始 message 或真实请求标识。
- 每批都保留当前脏工作区的无关变更；不以增加 retry 预算、放宽 replay gate、
  suppress 检查或双 owner 作为临时通过手段。

## 实施顺序

### 1. 关闭可信容量身份的运营配置闭环

目标：让操作者能通过受控的本地配置路径维护 `station_capacity_domains`，使已实现
的跨域 fallback 在真实配置中可用，同时维持身份缺失时的 fail-closed 行为。

1. 确认身份事实的唯一 owner、授权边界和 station 生命周期语义：创建、更新、
   删除 station 时的级联/保留策略，及 revision 对正在执行请求的 revalidation
   影响。
2. 增加 typed store、command facade、IPC DTO、ACL/生成 bindings 和本地设置 UI
   的写入路径。字段只接受明确 provider family 和受限 deployment/region 值；
   不暴露 commitment 的输入或替代推断规则。
3. 为缺失、非法、并发更新、station 删除、revision 漂移和导入/导出后的行为补齐
   persistence 与 IPC 回归。确认 portable migration 能保留该表和 trigger。

验收：可配置的可信身份出现在下一次 snapshot；修改 identity/revision 后旧 attempt
不能写入新事实；空身份不会产生跨域 outbound。

### 2. 完成 durable outcome 与 decision-trace 的跨层验收

目标：验证 `0037` 的 summary 在真实 proxy terminal、重启和 IPC 查询中保持同一
canonical 语义，而不是只通过 store 单测。

1. 跑通并修复 `intelligent_routing_persistence`、request lifecycle、trace query
   及 portable migration 的组合回归，覆盖成功、canonical failure、duplicate
   terminal、payload collision 和 writer failure。
2. 证明 `get_request_decision_trace` 在 runtime ring 缺失或进程重启后仍优先返回
   durable terminal summary；ring 只补充当前进程的有界事件。
3. 审核 schema catalog、reader、fixture fingerprint、DTO serialization 和 redaction，
   确保新 summary 与 capacity-domain digest 不会泄露敏感或高基数数据。

验收：重启后 IPC 仍能读到同一版本化终态摘要；重复终态仅接受完全一致重放；
schema/read-write/IPC/redaction/collision 测试全部通过。

### 3. 决定并实现可靠 transport send phase

目标：将 retry/replay 判断从当前保守近似升级为 transport owner 报告的单调事实。

1. 先完成 Windows transport spike，验证候选实现对 direct/system/HTTP/SOCKS
   proxy、TLS、HTTP/2、timeout 和 streaming 的兼容性及许可证；无法可靠证明的
   路径必须维持 `Unknown`。
2. 定义并由同一 production/test reporter 生成
   `NotConnected`、`ConnectedNoHeaders`、`HeadersSent`、`BodyPartiallySent`、
   `BodyFullySent`、`ResponseStarted` 和 `Unknown`。
3. 使用本地 TCP/HTTP harness 覆盖 connect、TLS、headers、partial/full body、
   response-started、mid-stream failure、timeout 和 cancellation，证明 body 被
   poll 不等于 socket 已接收。
4. 将最后可靠 phase 传给 canonical outcome；按幂等性、body replayability 和
   provider capability 建立 replay matrix。

验收：生产与测试没有双 reporter；不可靠路径明确 fail closed；非幂等且可能已被
接受的请求没有透明 retry。

### 4. 补齐容量状态机的生产组合与并发证据

目标：将当前定向单测扩大为 coordinator/target resolver/operational facts 的组合
证据，审查四次总尝试上限不会意外放宽其他失败类型。

1. 检查 `DEFAULT_MAX_ATTEMPTS = 4` 对所有失败分类的实际影响，明确它仅支持三次
   同 target capacity 尝试加一次可信跨域终局，或按现有预算契约收紧实现。
2. 增加 production-composition trace 测试：同域 sibling 无 outbound、不同可信域
   至多一次 outbound、identity/revision 漂移后 revalidation、无身份时完全禁止
   跨域。
3. 用 fake clock/loopback 覆盖 lease 释放和重新获取、FIFO/waiter 上限、queue full、
   cancel/shutdown、half-open race、sleep/deadline 和高并发资源回收。

验收：Execution 不自行拼装 domain 或第二套预算；所有路径资源计数回到基线；
跨域 attempt 的任一 terminal 都停止 retry 链。

### 5. 关闭 scoped health/capability 的生产 E2E

目标：证明终态 effect 只影响正确 scope，且随权威 revision 恢复。

1. 为 group/subscription failure 完成“写入 -> 下一 snapshot 排除对应 group ->
   无关 subject 可选 -> group revision 恢复”的 production-composition E2E。
2. 为 `model_not_found` 完成 key/model/deployment capability 的同等 E2E，并覆盖
   model alias/profile/subject revision。
3. 增加维度正交测试：credential、account、group、balance、quota、rate-limit 和
   capability verdict 可共存，恢复一个维度不得清除其他 verdict。

验收：planner 只消费 typed scoped verdict；没有 candidate N+1；crash/restart、
shadow rebuild、duplicate/late terminal 仍保持 revision fence。

### 6. 删除重复 owner、完成门禁与文档收口

前置条件：第 1-5 项均有 production 调用与回归证据。

1. 删除旧 classifier、fallback、compatibility writeback、parser owner 和仅服务旧
   行为的测试 API；先用 architecture/contract 搜索证明不存在生产 consumer。
2. 依次运行 `git diff --check`、专项 contract/architecture tests、`cargo fmt
   --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked
   --manifest-path src-tauri/Cargo.toml`、相关与全量 Cargo tests、`pnpm.cmd test`、
   `pnpm.cmd build`、`pnpm.cmd verify:fast`、`pnpm.cmd verify:full`。
3. 将专项计划、acceptance matrix、qualification、deletion ledger、boundary manifest、
   `docs/README.md` 和必要 release note 更新为与同一代码 revision 一致的
   `done with evidence`、`partial`、`pending` 或 `externally blocked` 状态。

验收：全部本地工程门禁退出码为 0；未能运行的检查记录确切原因和影响范围；
真实 provider smoke 未获授权时仅标为外部证据待补。

### 7. 真实 provider smoke（外部授权）

仅在上述工程收口完成且用户明确提供隔离测试账号、凭据与范围后执行。使用假业务
输入，验证 capacity、model-not-found、retry/commit 边界和 trace redaction；产物经
脱敏审计后再写入 qualification。此项不是本地工程完成的前置条件。

## 依赖关系

```text
可信身份配置（1） ----> 容量组合/并发证据（4） ----+
可靠 send phase（3） ------------------------------+--> 删除、全量验证、文档（6）
durable outcome/trace 验收（2） -------------------+
scoped verdict E2E（5） ---------------------------+
                                                       -> 真实 smoke（7，外部授权）
```

第 1、2、3、5 项可并行准备；第 4 的跨域 replay 断言依赖第 3 的安全事实，第 6
只能在其他本地工程项关闭后执行。
