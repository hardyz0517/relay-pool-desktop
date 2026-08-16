# Runtime Logging Deletion Ledger

状态：Implementation baseline；每一项必须在旧路径删除后附验证命令和证据。

| Legacy path/symbol | Replacement | Required evidence | Status |
| --- | --- | --- | --- |
| `src-tauri/src/lib.rs` direct stdout/stderr | bootstrap/runtime event adapter | architecture scan + bootstrap integration test | closed |
| `background_tasks/exit.rs` direct stderr | shutdown event/fixed fallback | architecture scan | closed |
| `background_tasks/routing_projection_runner.rs` tracing warning | task failure event | producer test + scan | closed |
| `application/routing.rs` snapshot stderr | application boundary event | routing failure test + scan | closed |
| `services/data_store/installation_lease.rs::log_installation_lease_event` | generic lock primitive + runtime adapter | lease test + no direct print | closed |
| `services/proxy/lifecycle/writer.rs` dynamic tracing | typed persistence/proxy event | lifecycle failure test + canary scan | closed |
| `services/proxy/startup_auto_start.rs` stderr | proxy startup event | startup test + scan | closed |
| `services/station_collectors.rs` warnings | collector typed event | collector failure test | closed |
| `services/monitoring/runner.rs` stderr | monitoring runner event | monitoring integration test | closed |
| `services/monitoring/maintenance.rs` stderr | monitoring maintenance event | maintenance fault test | closed |
| `observability/events.rs` dead-code model | deleted; `runtime::subject::{StableEventCode,RedactedResourceId}` is the sole typed identity boundary | `rg observability::events` empty; `observability_contract` and `observability::runtime::contract_tests` | closed |
| `observability/metrics.rs` generic stage inventory | reduced to the bounded proxy routing classification ring; runtime diagnostics owns durable events | `rg MetricKind/MetricLabel` producer inventory + routing metric tests | closed |
| `observability/diagnostics.rs` parallel snapshot | deleted; `application::runtime_diagnostics` reads `RuntimeLogService` | `rg observability::diagnostics` empty; diagnostics command/support-bundle tests | closed |
| `observability/redaction.rs` duplicate preview policy | deleted; task status uses `services::secrets::mask::redact_text_preview`, runtime events never accept free text | `rg observability::redaction` empty; security canary gate | closed |
| raw frontend ErrorBoundary error/stack | fixed frontend event | Vitest + security scan | closed |

No row may be closed with `temporary`, `later`, `compat`, or a directory-level allowlist. A close entry must include the exact source search and focused test that proves absence or typed replacement.
