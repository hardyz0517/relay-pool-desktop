# ADR 0005: Routing Policy Legacy Mapping

Status: Accepted for Task 10 migration boundary

The runtime compiler consumes only a complete `RoutingPolicyConfigV1`. The six
legacy strategy names are classified once at migration/import time and are not
looked up from generic settings during route planning.

| Legacy value | V1 preset | Preserved fields | Intentionally lost semantics | Configuration required |
|---|---|---|---|---|
| `automatic_balanced` | default weights | weights, candidate limit, exploration, fallback, affinity | none; automatic balancing is represented by bounded factors | no |
| `priority_fallback` | reliability 5000, responsiveness 2500, cost 1000, preference 1500 | weights, fallback | legacy priority tie-break | no |
| `stable_first` | reliability 5000, responsiveness 2500, cost 1000, preference 1500, affinity enabled | weights, affinity | stable queue order | no |
| `backup_only` | reliability 3500, responsiveness 2000, cost 1500, preference 3000 | weights | backup role/tier cannot be inferred from a policy name | yes |
| `cheap_first` | reliability 2000, responsiveness 1000, cost 6000, preference 1000 | weights, candidate limit | unbounded price comparator | no |
| `cost_stable_first` | reliability 2500, responsiveness 1500, cost 4500, preference 1500, affinity enabled | weights, affinity | legacy cost/stability tie-break order | no |

The mapping is intentionally one-to-one. Even when two old selectors happened
to share an implementation branch, they retain distinct presets and reasons;
future migration code must not silently merge them. `backup_only` is admitted as
`routing_configuration_required` until explicit candidate role metadata is
saved. The destructive removal of the old settings rows remains owned by Task
18.
