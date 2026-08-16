# Runtime Logging Canary Matrix

状态：Implementation baseline；仅使用明显的假值，禁止真实账号、日志或本地数据库。

| Canary | Inject into | Expected result |
| --- | --- | --- |
| `sk-secret` | error mapper, collector, proxy, support bundle | no original value; stable code plus `redacted` or export rejection |
| `Authorization: Bearer fake-token` | outbound error, tracing field, fixture | no header or token in event/DTO/stderr |
| `cookie=fake-cookie` | monitoring/collector failure | no cookie value in any output |
| `password=fake-password` | IPC input/error and crash path | no password or error text persisted |
| `https://user:pass@example.test/v1/x?token=fake#frag` | URL/error/subject | no userinfo/query/fragment/path leak |
| `C:\\Users\\fixture\\relay-pool.db` | persistence and recovery errors | no absolute path or username |
| `prompt fixture` / `response fixture` | proxy/transport/stream failure | no prompt/response content |
| malformed/unknown JSONL line | reader and support bundle | fixed corruption count; raw line never reaches UI/export |

For every producer, the test must assert both absence of the original canary and preservation of the expected stable error/event code. A passing marker scan without a producer-level assertion is insufficient.
