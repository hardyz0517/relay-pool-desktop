# 状态监控协议来源与参考边界

状态：Task 0 protocol source baseline
验证日期：2026-07-29
原则：官方协议文档是 adapter wire shape 的依据；Relay Pulse 只作为监控理念、调度和 UI 信息架构参考。

## 官方协议来源

| Provider | Scope | Official source | 本次支持范围 |
|---|---|---|---|
| OpenAI Chat Completions | non-stream and stream chat completion request/response, usage, model, finish reason | https://platform.openai.com/docs/api-reference/chat/create | `POST /v1/chat/completions` compatible shape; stream requires complete terminal event such as `[DONE]` and parser-visible output |
| OpenAI Responses | non-stream and streaming Responses API events | https://platform.openai.com/docs/api-reference/responses/create and https://platform.openai.com/docs/guides/streaming-responses | `POST /v1/responses`; handle `response.completed`, `response.failed`, `response.incomplete`, output text delta and usage/model extraction |
| Anthropic Messages | Messages create and streaming events | https://docs.anthropic.com/en/api/messages and https://docs.anthropic.com/en/api/messages-streaming | `POST /v1/messages`; content blocks, message stop, stream error, usage and API version header |
| Gemini Native | generateContent and streamGenerateContent | https://ai.google.dev/api/generate-content and https://ai.google.dev/gemini-api/docs/text-generation | Native Gemini `generateContent` and stream API; candidates/parts, finish reason, safety/block, usage metadata |
| xAI/Grok | Chat completions and streaming | https://docs.x.ai/docs/api-reference#chat-completions | xAI chat completion semantics via explicit `xai_grok` adapter, not inferred from OpenAI-like URL/body alone |

## Adapter Rules Derived From Sources

- HTTP 2xx is necessary but not sufficient. Availability requires a protocol-valid envelope, terminal semantics, extracted output and challenge validator success.
- Streaming adapters must distinguish transport EOF from protocol terminal completion.
- Provider-native APIs stay separate even if wire shapes overlap. `generic_openai` is an explicit OpenAI-compatible floor, not an auto-detected catch-all.
- `protocol_kind=auto` may select from persisted capability facts only. It must not try multiple providers over the network.
- Unknown or conflicting protocol facts return `needs_configuration` with zero outbound probe attempts.

## CLI Compatibility Source Boundary

CLI compatibility profiles are allowed because the user wants optional request-image compatibility to reduce upstream risk. They must remain:

- optional per monitor/profile;
- versioned and fixture-backed;
- unable to override auth secret source, target host, TLS, proxy, body/response limits or redaction policy;
- unable to persist Authorization/API key/Cookie values;
- separate from provider protocol adapters.

`grok_cli_compat` remains present only as a disabled capability ID until authorized verification and versioned fixtures exist. Standard xAI/Grok adapter remains required.

### Task 5 profile registry notes

- `standard_api` v1 is the canonical low-risk request image. It delegates provider-specific method/path/body semantics to the selected adapter and adds only generic `accept`/`content-type` defaults.
- `codex_cli_compat` v1 is limited to OpenAI Chat, OpenAI Responses and explicit Generic OpenAI-compatible probes. It contains only versioned, fixture-backed low-risk public headers and does not copy large prompts, tool schemas, OAuth/device identity or account metadata.
- `claude_code_compat` v1 is limited to Anthropic Messages and uses the official `anthropic-version` boundary without persisting auth headers or API keys.
- `gemini_cli_compat` v1 is limited to Gemini Native and uses public client image headers only. Gemini OpenAI-compatible mode remains outside this native profile and must be selected explicitly via Generic/OpenAI-compatible handling.
- `grok_cli_compat` is registered as `enabled=false` with no supported protocol until a separate authorized fixture/evidence pass exists.
- Profile hash snapshots include method, path, supported protocols, public header names/values and body default keys, but the golden summaries expose only safe names/default fields plus the hash.
- Profile validation rejects `Authorization`, `Proxy-Authorization`, API-key headers, cookies, host forwarding headers and related auth/transport overrides.

## Relay Pulse Reference Mapping

Reference repository: https://github.com/prehisle/relay-pulse/tree/c62537085f4202f6f1f28716f45c107303f2836f
Observed license: MIT

Concepts worth borrowing:

- nearest-due scheduler based on a min heap;
- bounded concurrency with cancellation and drain;
- provider/proxy keyed HTTP client pool and explicit transport policy;
- template/challenge based active probes;
- sub-status taxonomy for auth, rate-limit, server/client error, timeout, network and content mismatch;
- compact horizontal status table with filter toolbar, stable columns and fixed trend blocks;
- trend aggregation that preserves severity and counts.

Concepts not copied:

- public SaaS/status-site backend, SEO, sponsor/community features and notification integrations;
- PostgreSQL/server deployment model;
- dark visual theme and branding;
- request bodies, prompts, CLI identities or code implementation details;
- any unofficial protocol assumption that conflicts with official provider docs.
