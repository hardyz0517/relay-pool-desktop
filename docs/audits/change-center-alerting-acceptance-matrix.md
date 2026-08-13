# Change Center Alerting Acceptance Matrix

Date: 2026-08-08

| Area | Acceptance condition | Evidence | Result |
|---|---|---|---|
| Lifecycle | Abnormal, healthy, recovering, resolved, and reopen transitions are deterministic | Rust lib tests; alerting projector tests | Pass |
| Freshness | Stale or out-of-order observations cannot mutate an incident | Projector/ingress tests | Pass |
| Idempotency | Repeated source observation keys do not duplicate occurrences or deliveries | Alerting persistence tests | Pass |
| Policy | Trigger count/duration, recovery count/duration, repeat and cooldown values are persisted and validated | Policy/settings tests and DTO contract | Pass |
| Attention | Seen, acknowledge, and snooze are episode-scoped and do not alter incident truth | Attention service/store tests | Pass |
| Delivery | Claim lease, retry, suppression, recovery notification, and desktop unavailable paths are durable | Delivery worker/persistence tests | Pass locally; Windows smoke pending |
| Read model | Current, incident detail, occurrence history, and delivery history are cursor/page bounded | IPC DTO and frontend alerting tests | Pass |
| Migration | Schema 29 foundation, durable backfill, current-facts rebuild, and schema 30 destructive postcondition | Migration, alerting persistence, generation upgrade tests | Pass on fixtures |
| Recovery | Schema 15 to latest, wrong key, journal fault, and import rollback converge deterministically | Generation upgrade matrix | Pass |
| Legacy boundary | No production legacy writer/reader/IPC/binding/query path remains | `pnpm.cmd test:contracts`, `pnpm.cmd architecture:alerting` | Pass; upgrade-only reader remains allowlisted |
| Security | DTOs and notification payloads are bounded and redacted | Contract/security gates | Pass locally |
| Scale baseline | 10/100/500 current query samples run through production query factories | `pnpm.cmd architecture:scale-baseline` | Pass |
| Observation | One full release cycle with zero legacy adapter calls | Runtime evidence | Pending |
| Backup/restore | Verified backup restores and restarts before destructive cleanup | Recovery drill | Pending |
| Release policy | Advisory/license/source scan completes | `pnpm.cmd verify:full` | Blocked by unavailable RustSec network |

## Exit rule

No final release or deletion-ledger closure is allowed while any row marked
Pending or Blocked remains unresolved.
