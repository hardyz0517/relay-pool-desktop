# ADR 0001: Hierarchical Routing Kernel

Status: Superseded by `docs/proposals/INTELLIGENT_ROUTING_ENGINE_SPEC.md` on 2026-08-05; retained as historical ADR
Date: 2026-07-30

## Context

Current routing mixes legacy policy scoring, scheduler weights, cheap-first estimated cost, sticky affinity, load, health, and capacity explanation. This made the route explanation look smart while leaving important production semantics unproven: `cheap_first` can compare arbitrary `input + output` prices, slot availability can appear after a candidate is already in the executable order, and `PriorityFirst`/`CostFirst` would be easy to implement as two diverging selectors.

The product direction explicitly rejects LLM routing, bandits, online learning, and heavyweight distributed algorithms. We need deterministic, explainable routing that composes facts cleanly.

## Decision

Implement one pure hierarchical kernel with sealed ordering profiles:

- `PriorityFirst`
- `CostFirst`

Both profiles consume the same request facts, candidate projections, runtime overlay snapshot, and hard eligibility output. They differ only in the lexicographic strata order.

Common stages:

1. Hard eligibility: enabled, endpoint kind, protocol, model/capability, group scope, multiplier ceiling, balance depletion policy, hard health block, route budget.
2. Pool ejection guard: runtime/durable health suppression cannot eject the last viable scoped pool member unless the rejection is a true hard block.
3. Availability strata: healthy/probing/degraded/unknown/fallback; cooldown/hard-block are not executable.
4. Profile-specific strata.
5. Least utilization and bounded LRU fairness.
6. Deterministic shuffle only inside a fully equal stratum.

`PriorityFirst` orders priority before soft cost. `CostFirst` orders exact comparable cost strata first, then multiplier proxy, then unpriced fallback, and only compares priority inside the selected cost stratum.

Cost basis rules:

- Exact scalar cost is allowed only when `PricingProjector` produces a request-applicable, same-currency, same-unit, same-basis scalar.
- Multiplier proxy is allowed only when trusted effective multiplier is fresh and comparable; trace must state it is a multiplier proxy, not exact model price.
- Unpriced fallback remains eligible after priced/proxy candidates but cannot be made cheaper by default values.
- `input_price + output_price` is forbidden unless a future ADR defines a public reference usage formula.

Affinity rules:

- Affinity is a soft stratum move, never a hard eligibility bypass.
- Affinity can bind only after selected attempt and RequestOutcome are durably successful.
- `CostFirst` affinity cannot cross the approved cost band or jump ahead of exact priced strata.

## Consequences

- Existing `RoutingPolicy::CheapFirst` migrates to `CostFirst` semantics only through readiness UI and explicit config migration.
- Legacy score/weight code remains only until production cutover and must be physically deleted from default-v2.
- Tests must prove the two profiles share the same kernel and trace all strata decisions.
