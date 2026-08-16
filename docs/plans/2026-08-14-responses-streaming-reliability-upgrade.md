# Relay Pool Desktop Responses 流式可靠性修复升级计划

状态：已实施；仓库级完整校验的联网 advisory 更新受网络阻断，已完成离线等价安全检查

日期：2026-08-14

适用范围：修复密钥连通性测试与状态监控对 OpenAI Responses SSE 的整包缓冲、错误限流和诊断不足问题，并建立可复用的增量流式解析基础。本计划不扩大 Relay Pool Desktop 的产品边界，不改变本地代理的对外协议，也不把某个中转站的非标准行为直接固化为全局协议规则。

关联规范与事实入口：

- `AGENTS.md`
- `docs/README.md`
- `docs/specs/STATUS_MONITORING_REFACTOR_SPEC.md`
- OpenAI Responses streaming typed events：`response.output_text.delta`、`response.completed`、`response.failed` 等

## 1. 问题陈述与当前证据

### 1.1 已确认的问题

密钥连通性测试当前虽然调用 `execute_stream`，但回调只把每个 chunk 追加到 `response_body`；网络流结束后才把完整响应一次性交给 `StationKeyConnectivitySseDecoder`。decoder 在解析和释放已完成事件之前先检查 pending 是否超过 64 KiB。

这会导致：一条由很多合法小事件组成、总大小超过 64 KiB 的 Responses SSE，被误判为 `SSE pending buffer too large`。该限制原本应保护“尚未形成完整事件的 pending 数据”，实际却限制了“整条流”。

状态监控存在同类结构：transport 把流式 body 全量缓存，adapter 在响应结束后整体解析，并使用 256 KiB 总响应、512 个 SSE 事件等统一限制。Responses 的所有 framing、事件、终止或上限问题最终大多被压缩为 `protocol_mismatch`，持久化记录不足以区分具体失败门槛。

本机已有执行证据表明，同一密钥和 `deepseek-v4-flash` 模型：

- `open_ai_responses + standard_api` 连续得到 `protocol_mismatch`；
- 改为 `open_ai_chat + standard_api` 后连续 available；
- 密钥测试的 Responses 流式分支超过 pending 限制，但非流式 Responses 回退可以返回正文。

因此，当前证据支持“流式消费与严格解析实现存在缺陷”，不支持“密钥无效”或“模型不支持 Responses”的结论。

### 1.2 尚未确认的上游差异

监控历史目前没有持久化安全的响应字节数、事件数、最后事件类型和 terminal 状态，无法仅凭 `protocol_mismatch` 判断具体属于：

- 总响应超过限制；
- 事件数量超过限制；
- 单事件或 framing 异常；
- Content-Type 不符合；
- 缺少 `response.completed`；
- 中转站使用了兼容但非标准的 Responses 方言。

实施时必须先补安全诊断和本地可控复现，不能仅靠调大限制猜测原因。

## 2. 目标与非目标

### 2.1 目标

1. 密钥测试和状态监控都按网络 chunk 增量消费 SSE，不再先缓存完整成功响应。
2. 多个合法小事件组成的较大流可以正常完成；异常大的单个未完成事件仍会被有界拒绝。
3. Chat 与 Responses 共用 SSE framing 核心，但保留各自独立的协议状态机和终止语义。
4. `standard_api` 与 `codex_cli_compat` 保持显式区分，协议能力与请求画像不混淆。
5. 流式失败、同协议非流式回退、协议失败和内容验证失败具有不同的稳定诊断结果。
6. 日志、IPC、持久化和 fixture 不包含密钥、Authorization、完整原始响应或内部推理正文。
7. 新增解析器可供后续 OpenAI-compatible dialect 复用，而不依赖 URL 字符串猜测供应商。

### 2.2 非目标

- 不把 Responses 失败静默切换为 Chat 并标记 Responses 可用。
- 不通过无限增大 body/event 限制解决问题。
- 不在本计划中重做整个 outbound transport、监控调度器或本地代理架构。
- 不为单一 DeepSeek 中转写 URL、模型名或站点名特判。
- 不默认将所有 Responses 监控切换为 `codex_cli_compat`。
- 不保存真实 provider 响应作为测试 fixture；fixture 必须最小化、脱敏并人工审查。

## 3. 必须保持的不变量

- 每个流的 pending event、总输入字节、事件数、提取输出和诊断字段都有明确且唯一的上限 owner。
- parser 在任意 chunk 边界下结果一致，包括 JSON、CRLF 和 UTF-8 多字节字符中间切分。
- 只有看到协议规定的成功终止事件，流式 Responses 才能标记 available。
- `response.failed`、`response.incomplete`、顶层或流内 error 不能被 `[DONE]` 或 EOF 覆盖成成功。
- 非 2xx HTTP 状态优先按 HTTP/错误 envelope 分类，不得被 SSE parser 的次生错误掩盖。
- 同协议的 stream -> non-stream fallback 必须可见；Responses -> Chat 不属于同协议降级。
- 诊断只保存闭合错误码和有界元数据，不保存原始认证信息、完整事件或模型推理文本。
- cancellation、timeout、body limit 和 parser failure 后必须立即释放 buffer、网络流和监控 permit。

## 4. 目标架构

```text
AsyncOutboundClient chunk stream
  -> 共享增量 SSE framing decoder
       - LF / CRLF
       - comment / empty line
       - multiline data
       - partial UTF-8 / arbitrary chunk split
       - pending-event limit
  -> 协议事件 reducer
       - OpenAI Responses typed events
       - OpenAI Chat delta / finish / [DONE]
  -> 调用场景
       - 密钥连通性测试
       - 状态监控 adapter
  -> 有界安全诊断
       - total bytes / event count / last event type
       - terminal seen / response mode / stable failure code
```

共享层只负责 framing 和协议事件归约，不读取数据库、不决定监控健康写回、不选择请求 Profile，也不决定跨协议 fallback。

建议的新 owner 位于中立的 service 模块，例如：

```text
src-tauri/src/services/protocol_streaming/
  mod.rs
  sse.rs
  openai_responses.rs
  openai_chat.rs
```

最终目录必须在 Task 1 通过现有 architecture tests 验证后冻结；若仓库已有更合适的共享 owner，应复用现有边界，不新增第二套抽象。

## 5. 数据与错误合同

### 5.1 分离四类上限

不得再用一个 `body.len()` 同时表达所有风险。至少区分：

| 上限 | 保护对象 | 超限结果 |
| --- | --- | --- |
| `max_pending_event_bytes` | 尚未遇到事件边界的单个 SSE event | `sse_event_too_large` |
| `max_total_stream_bytes` | 单次探测读取的累计字节 | `response_body_limit` |
| `max_sse_events` | 单次探测事件数量 | `sse_event_limit` |
| `max_output_bytes` | 实际提取的模型输出 | `output_limit` |

默认值必须由一个 policy owner 提供，并通过测试锁定。若不同协议确实需要不同值，应使用显式 protocol policy，不根据模型名或 URL 猜测。

### 5.2 稳定失败原因

在现有 `FailureKind` 的对外粗分类下增加内部稳定 reason，至少覆盖：

- `response_body_limit`
- `sse_event_limit`
- `sse_event_too_large`
- `invalid_sse_utf8`
- `invalid_sse_framing`
- `invalid_event_json`
- `unexpected_content_type`
- `missing_terminal_event`
- `upstream_failed_event`
- `upstream_incomplete_event`
- `content_validation_failed`
- `stream_transport_failed`

对 UI 和持久化可以继续映射为有限的 `FailureKind`，但不得丢失安全的内部 reason。优先复用现有 `error_summary`、terminal reason 和 read model；只有现有字段无法稳定表达时才设计 additive migration。

### 5.3 安全诊断摘要

允许记录：

- HTTP status；
- Content-Type 的规范化类别；
- total bytes、event count、max pending bytes、output bytes；
- last event type 的闭合或截断值；
- terminal seen；
- stream / non-stream fallback；
- protocol、profile id/version 和 request profile hash。

禁止记录：

- Authorization、API key、cookie；
- 完整 URL/query；
- 完整 SSE event、原始响应 body；
- 模型推理正文、prompt、动态 challenge token；
- 未限制长度或高基数的上游错误文本。

## 6. 分阶段实施任务

### Task 0：冻结复现、基线和诊断缺口

目标：在改行为前证明当前失败路径，并建立不会泄密的回归输入。

实施：

1. 保存 `git status --short`，确认并保留用户已有改动。
2. 为当前 decoder 增加最小回归测试：总流大于 64 KiB、每个事件远小于 pending limit、包含合法 `response.completed`。
3. 为监控 Responses adapter 增加当前 `protocol_mismatch` 分支矩阵测试，明确 body/event/terminal/framing 的现状。
4. 从现有现象人工构造最小脱敏 Responses fixture，不复制真实响应正文或 provider 标识。
5. 冻结期望：较大合法流成功；超大单事件失败；缺终止事件失败。

Exit gate：测试能够稳定复现“总流误当 pending”问题，并且 fixture 通过 secret/隐私审查。

### Task 1：建立共享增量 SSE framing decoder

目标：形成与调用场景无关、可独立测试的有界 framing 核心。

实施：

1. decoder 的 `push(chunk)` 在追加数据后循环解析所有完整事件，及时 drain 已消费字节。
2. `max_pending_event_bytes` 只在解析完当前可用完整事件后检查剩余 pending。
3. 支持 `\n\n`、`\r\n\r\n`、comment、空事件、多个 `data:` 行和跨 chunk 边界。
4. 不要求 chunk 是完整 UTF-8；只在形成完整事件后验证该事件 UTF-8。
5. 独立累计 total bytes、event count 和 observed max pending，不把这些统计混成一个限制。
6. decoder 输出结构化 `SseEvent`，不在 framing 层解析 Responses 或 Chat JSON。
7. 删除或迁移旧的重复 boundary/parser helper，禁止长期双轨。

Focused tests：

- 每个字节一个 chunk；
- JSON token、换行、CRLF 和 UTF-8 字符中间切分；
- 单 chunk 包含多个事件；
- 总流大于 64 KiB但单事件正常；
- 单事件刚好等于和超过限制；
- EOF 时仍有 incomplete event；
- comment-only 和 multiline data。

Exit gate：共享 decoder 在所有 chunk 分割方式下产生相同事件序列，资源限制和错误 reason 稳定。

### Task 2：实现 Responses 与 Chat 独立事件 reducer

目标：共享 framing，但不混用协议语义。

Responses reducer 至少处理：

- `response.created`
- `response.output_text.delta`
- `response.output_text.done`
- `response.completed`
- `response.failed`
- `response.incomplete`
- `error`
- reasoning、usage 和其他合法非输出事件：有界忽略或提取安全元数据，不拼入最终回答。

Chat reducer 至少处理：

- `choices[].delta.content`
- 非空 `finish_reason`
- `[DONE]`
- 顶层和流内 error。

实施要求：

1. reducer 是显式状态机，成功、失败和 terminal 状态不可逆。
2. EOF 不能代替 Responses 的成功终止事件。
3. output limit 只统计实际输出文本，不统计 reasoning 或完整事件 JSON。
4. unknown event 默认安全忽略并计数，不因协议新增无关 typed event 直接失败；影响终止或错误语义的未知事件仍保守处理。
5. 非流式 Responses JSON 提取与流式 reducer 共用最终 output/usage 校验合同。

Exit gate：官方事件序列、失败序列、未知非关键事件、reasoning 事件和 terminal race 均有确定性测试。

### Task 3：接入密钥连通性测试

目标：消除当前 64 KiB 总流误判，并让回退结果语义清晰。

实施：

1. 在 `execute_stream` chunk callback 中直接调用共享 decoder/reducer，不再累计完整成功 body。
2. 仅为非 2xx 错误保留有界错误摘要；HTTP status 未返回前产生的 parser 状态不得覆盖最终 HTTP 分类。
3. 流式 Responses 成功时返回 `response_mode=stream`。
4. 流式失败、非流式 Responses 成功时返回整体成功和 `response_mode=non_stream_fallback`，同时保留稳定 fallback reason。
5. Responses 失败后是否尝试 Chat 继续遵守现有 capability/upstream format 策略；若尝试，结果必须明确最终验证的是 Chat，而不是 Responses。
6. UI 文案区分“密钥失败”“流式降级成功”和“协议 fallback 成功”。

Exit gate：当前 DeepSeek 类大流 fixture 不再触发 pending 误判；非流式回退与跨协议 fallback 在 UI/DTO 中不会被混淆。

### Task 4：接入状态监控

目标：监控与密钥测试使用同一解析事实，保留监控领域的严格完成和 challenge 校验。

实施：

1. 将 `MonitoringTransport::execute_streaming` 改为向有界 consumer 交付 chunk，并只返回响应 metadata/统计，不再返回完整成功 body。
2. `OpenAiResponsesAdapter` 和 `OpenAiChatAdapter` 消费共享 decoder/reducer 的事件或结果。
3. 监控继续要求协议 terminal 和 challenge 精确匹配；parser 成功不等于语义挑战成功。
4. 将稳定内部 reason 映射到现有 `FailureKind`，并通过 attempt/target read model 暴露安全诊断。
5. stream -> non-stream 是否用于监控必须作为显式策略决定：
   - 若启用，只允许同协议 Responses 回退，并记录 `traffic_equivalence=degraded` 或等价可见状态；
   - 不允许静默切换为 Chat 后写回 Responses available。
6. 检查监控共享 outbound client 的 body limit owner，确保 transport limit 与 parser policy 不重复冲突。

Exit gate：状态监控对较大合法 Responses 流可用；每类失败原因可从安全 read model 区分；健康写回只消费最终明确的监控结论。

### Task 5：澄清 Profile 与能力测试

目标：避免把“协议支持”与“Codex 请求画像支持”混为一谈。

实施：

1. 保持 `standard_api` 与 `codex_cli_compat` 显式选择，不根据模型名、站点名或 URL 自动切换。
2. 监控表单在选择 Responses 时继续展示兼容 Profile，但说明二者验证的请求画像不同。
3. capability test 结果至少能表达：
   - Responses + standard profile；
   - Responses + Codex profile；
   - Chat + standard profile。
4. 新建监控默认值仍由显式产品决策决定；不得因单次 provider 现象全局改为 Codex Profile。
5. request profile hash 和 version 必须进入执行快照，运行中的 execution 不受编辑影响。

Exit gate：Codex 成功不再被 UI 或健康逻辑解释为 standard Responses 一定成功，反之亦然。

### Task 6：删除旧路径并完成工程验证

目标：只保留一个 SSE framing owner，并完成跨层资格验证。

实施：

1. 删除旧 `find_sse_event_boundary`、整包 `sse_data_events` 或其他被替代的重复 owner。
2. 更新 architecture/dead-code/生成 binding 契约；生成文件只能通过仓库脚本更新。
3. 检查所有 error/fixture/log/IPC DTO 的 secret 与正文泄漏风险。
4. 检查 cancellation、timeout、oversize、malformed event 和 parser early failure 的资源释放。
5. 更新本计划状态和实际验证证据；未运行的真实 provider smoke 明确保持未验证。

Exit gate：不存在生产双轨 parser、未使用兼容 helper 或绕过安全上限的路径；完整验证通过。

## 7. 预计文件范围

最终以代码和 architecture tests 为准，预期涉及：

- `src-tauri/src/application/connectivity_probe/mod.rs`
- `src-tauri/src/commands/station_key_connectivity.rs`
- `src-tauri/src/services/monitoring/transport.rs`
- `src-tauri/src/services/monitoring/executor.rs`
- `src-tauri/src/services/monitoring/adapters/sse.rs`
- `src-tauri/src/services/monitoring/adapters/openai_responses.rs`
- `src-tauri/src/services/monitoring/adapters/openai_chat.rs`
- `src-tauri/src/services/monitoring/adapters/contract.rs`
- 新的共享 protocol streaming 模块及其测试
- 监控 read model/store/IPC DTO，仅在安全诊断需要时修改
- `src/features/key-pool/KeyConnectivityTestDialog.tsx`
- `src/features/channels/components/MonitorExecutionDrawer.tsx`
- 相关 Vitest、Cargo tests、architecture tests 与生成 binding

如果实施需要新增数据库 migration，必须先证明现有 attempt/target `error_summary` 和 reason 字段无法表达稳定诊断；不得为了方便调试扩大持久化敏感数据。

## 8. 验证矩阵

### 8.1 Rust 单元与集成测试

至少覆盖：

1. 合法 Responses 总流超过 64 KiB，单事件均在限制内，成功完成；
2. 合法流跨 256 KiB 时按明确 total policy 处理，不误报 pending；
3. 单事件超限返回 `sse_event_too_large`；
4. 事件数超限返回 `sse_event_limit`；
5. LF、CRLF、comment、multiline data；
6. 任意 chunk split 与 partial UTF-8；
7. `response.completed`、`response.failed`、`response.incomplete`、error 和 EOF；
8. reasoning events 不进入最终 challenge answer；
9. Chat `[DONE]` 与 `finish_reason`；
10. 非 2xx 错误不被 parser failure 覆盖；
11. cancellation、timeout、body limit 后资源释放；
12. standard/Codex Profile 请求体和 header 契约不漂移。

### 8.2 前端测试

至少覆盖：

- 流式成功；
- 同协议非流式降级成功；
- Responses 失败后 Chat fallback 成功；
- 密钥整体失败；
- 监控 drawer 展示稳定 reason，不展示原始正文；
- 窄窗口、loading、disabled 和错误状态保持可读。

### 8.3 必跑命令

按实际改动范围至少运行：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml <相关专项测试>
pnpm.cmd test -- <相关 Vitest>
pnpm.cmd build
pnpm.cmd verify:fast
```

由于该改动涉及共享 transport/monitoring 基础设施，完成阶段应运行：

```powershell
pnpm.cmd verify:full
```

若任何检查因环境或外部依赖未完成，交付必须记录真实退出状态和未验证范围。

### 8.4 可选真实 provider smoke

真实 DeepSeek/Codex-compatible smoke 需要用户明确授权、隔离测试密钥和可接受的小额调用预算。它用于验证中转方言，不替代本地确定性 fixture，也不作为无授权情况下的工程完成条件。

Smoke 矩阵建议：

| 协议 | Profile | 传输 | 期望 |
| --- | --- | --- | --- |
| Responses | standard API | stream | 明确成功或稳定方言错误 |
| Responses | standard API | non-stream | 明确成功或稳定方言错误 |
| Responses | Codex CLI compat | stream | 明确成功或稳定 Profile 错误 |
| Chat | standard API | stream | 基线对照 |

## 9. 完成定义

必须同时满足：

- 密钥测试不再把大于 64 KiB 的合法总流误当作 oversized pending event；
- 监控不再先缓存完整成功 SSE 才解析；
- 密钥测试和监控共用唯一 SSE framing owner；
- Responses 和 Chat 使用独立、明确的终止状态机；
- `protocol_mismatch` 能通过安全 reason 进一步定位；
- stream、同协议 non-stream fallback 和跨协议 fallback 在结果中可区分；
- standard/Codex Profile 能力不会互相冒充；
- 恶意超大单事件、总流和事件洪泛仍被有界拒绝；
- 所有相关专项测试、build、`verify:fast` 和 `verify:full` 按要求通过或如实记录未完成原因；
- 工作区中没有真实 secret、原始认证数据、真实响应正文或诊断产物被提交。

## 10. 风险与回滚

### 10.1 主要风险

- parser 迁移时短期存在双轨行为，导致连接测试与监控结论不一致；
- callback 中提前返回错误可能覆盖真正的非 2xx HTTP 错误；
- 对 unknown Responses event 过严会降低前向兼容，过松会漏掉失败语义；
- 提高总流能力后，如果 total/event/output admission 不独立，可能扩大内存风险；
- UI 把“非流式降级成功”误显示为完全等价的流式能力成功；
- 为诊断新增字段时泄露高基数或敏感正文。

### 10.2 控制措施

- 先锁定共享 decoder/reducer 测试，再迁移调用方；
- 每个调用方迁移完成后立即删除旧 owner，不长期 feature-flag 双写；
- 非 2xx 分类优先级写入契约测试；
- unknown event 策略区分“非关键可忽略”和“影响终止/错误必须保守”；
- 所有限制集中在单一 policy，并有边界测试；
- 诊断 DTO 使用闭合 enum、数值计数和有界字符串；
- 若监控接入出现不可接受回归，可回退调用方接线，但不得回退共享 decoder 的安全测试或通过无限调大限制绕过问题。

## 11. 交付物

1. 共享增量 SSE framing decoder；
2. OpenAI Responses 与 Chat 独立事件 reducer；
3. 密钥连通性测试增量接入与清晰回退语义；
4. 状态监控增量接入与稳定安全诊断；
5. Profile/协议能力展示与测试更新；
6. 脱敏 fixture、单元测试、loopback 集成测试和前端回归测试；
7. 实际验证记录及未验证真实 provider 范围说明。

## 12. 实施记录（2026-08-14）

已完成：

- 新增 `services/protocol_streaming` 作为唯一的 OpenAI SSE framing/reducer owner：单个 pending event 为 64 KiB、总流为 2 MiB、事件数为 4096；分别限制输出文本，不累积 reasoning。
- 密钥连通性测试与 OpenAI Chat/Responses 监控均改为按 chunk 归约成功流；非 2xx 的有界错误摘要和 HTTP 分类优先级保持不变。
- 对 Responses/Chat 的 terminal、错误、`done.text` 去重、UTF-8/CRLF/EOF 边界和大 reasoning 流增加了回归测试。EOF 后有完整末行会被消费；真正截断的末行以及缺少协议成功 terminal 仍会失败。
- 监控将闭合的安全 `error_summary` 写入 attempt；不持久化原始 SSE、推理或认证信息。
- 连接测试新增 standard API/Codex CLI 兼容档案选择，结果返回最终验证协议和实际请求档案，明确区分流式成功、同协议非流式回退、跨协议 Chat 回退。
- 已通过仓库脚本更新 IPC bindings。
- 首次真实界面复测暴露了第二个独立问题：标准 Responses 探测的 `max_output_tokens=32` 会截断带有前置推理/短前缀的上游回复，触发真实的 `response.incomplete`。现已提高到受控的 128，并保留 `response.incomplete` 的严格失败语义。

实际验证：

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml`；
- 共享流式核心 16 项、`monitoring_adapter_contracts` 35 项、`monitoring_execution_integration` 35 项、`monitoring_transport` 3 项、`monitoring_orchestrator` 11 项、`monitoring_write_path` 10 项和 `station_key_connectivity` 23 项专项测试；
- 相关 Vitest 6 项、`pnpm.cmd build`、`pnpm.cmd generate:bindings --check`、`pnpm.cmd verify:fast`；
- `pnpm.cmd verify:full` 已运行到 advisory 更新步骤；其余此前阶段通过，但从 GitHub 拉取 RustSec advisory 数据库因网络连接失败而退出。随后以 `RELAY_POOL_CARGO_DENY_OFFLINE=1` 重跑 advisory/license/source gate，离线数据库检查通过。

未执行：真实 DeepSeek/Codex-compatible provider smoke。它需要隔离测试密钥和调用预算的明确授权，不能以本地工程验证替代。
