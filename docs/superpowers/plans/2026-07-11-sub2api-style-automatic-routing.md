# Sub2API-Style Automatic Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the legacy multi-policy local proxy router with one Sub2API-style automatic scheduler that honors an optional Key group filter and a hard multiplier ceiling.

**Architecture:** Add a focused scheduler subsystem under `src-tauri/src/services/proxy/scheduler/` and route simulation plus real proxy attempts through it. Keep request parsing and upstream forwarding in `runtime.rs`, keep persistence in `database.rs`, and keep UI state in the existing routing/settings pages. Multiplier ceiling and multiplier scoring are Relay Pool extensions; group-scoped candidate pools, sticky affinity, TopK ordering, capacity waiting, and fresh-load retry mirror the reviewed Sub2API behavior.

**Tech Stack:** Rust/Tauri 2, rusqlite, React, TypeScript, Vite, Node contract scripts, existing `pnpm.cmd` and Cargo commands.

---

## Scope And Guardrails

- Reference spec: `docs/superpowers/specs/2026-07-11-sub2api-style-automatic-routing-design.md`.
- Reference Sub2API commit: `e316ebf52838a89d57fc790981cce7520f819ac8`.
- Do not copy Sub2API Go code. Reimplement behavior in Rust.
- Do not use `git add .`, `git add -A`, or `git commit -a`. Stage exact paths only.
- Current workspace may contain unrelated Rust implementation changes. Before each task, run `git status --short` and only touch files listed in that task.
- The group filter is opt-in. `AllGroups` is the migrated default. When a specific group filter is active, it is a hard pool boundary and never falls back to another group.
- The multiplier ceiling is always hard. Unknown, stale, invalid, or over-ceiling multiplier facts reject.

## File Structure

Create:

- `src-tauri/src/services/proxy/scheduler/mod.rs` - module exports and public scheduler API.
- `src-tauri/src/services/proxy/scheduler/types.rs` - request, settings, group filter, factors, decisions, errors.
- `src-tauri/src/services/proxy/scheduler/multiplier.rs` - effective multiplier fact resolution.
- `src-tauri/src/services/proxy/scheduler/eligibility.rs` - hard gates and candidate rejection facts.
- `src-tauri/src/services/proxy/scheduler/scoring.rs` - normalized factors and base score.
- `src-tauri/src/services/proxy/scheduler/selection.rs` - TopK, tie-breaks, weighted order, fresh-order rebuild.
- `src-tauri/src/services/proxy/scheduler/metrics.rs` - in-memory EWMA runtime metrics.
- `src-tauri/src/services/proxy/scheduler/capacity.rs` - per-key slot and wait registry.
- `src-tauri/src/services/proxy/scheduler/affinity.rs` - group-scoped session and response affinity.
- `src-tauri/src/services/proxy/scheduler/explanation.rs` - bounded secret-safe decision snapshots.
- `scripts/local-routing-automatic-settings.test.mjs` - frontend settings/routing text and type contract.
- `scripts/local-routing-scheduler-log-contract.test.mjs` - request-log and explanation field contract.

Modify:

- `src-tauri/src/services/proxy/mod.rs` - export scheduler module.
- `src-tauri/src/models/routing.rs` - replace public routing policy surface with automatic scheduler types while preserving readable legacy log labels.
- `src-tauri/src/models/settings.rs` - add max multiplier, group filter, advanced scheduler settings.
- `src-tauri/src/models/station_keys.rs` - add concurrency/load/schedulable/manual multiplier fields.
- `src-tauri/src/models/group_facts.rs` - expose group type/scope inputs needed by `RoutingGroupFilter`.
- `src-tauri/src/services/database.rs` - schema migration, settings persistence, candidate fact loading.
- `src-tauri/src/services/proxy/router.rs` - route simulation through scheduler and retire old score branches.
- `src-tauri/src/services/proxy/runtime.rs` - use scheduler for real attempts and forward-time rechecks.
- `src-tauri/src/services/proxy/routing_snapshot.rs` - expose automatic settings, group filter, and scheduler explanation fields.
- `src-tauri/src/services/proxy/routing_affinity.rs` - either delete after migration or shrink to compatibility wrapper around scheduler affinity.
- `src-tauri/src/services/proxy/routing_policy.rs` - delete after replacement, or keep only a temporary compatibility shim during tasks.
- `src-tauri/src/services/proxy/routing_health.rs` and `routing_failure.rs` - reuse failure classification where semantics match.
- `src/lib/types/routing.ts` - TypeScript scheduler contracts and simulation input/output.
- `src/lib/types/localRouting.ts` - Local Routing workspace fields.
- `src/lib/types/settings.ts` - settings API types.
- `src/features/routing/LocalRoutingEditTab.tsx` - group filter and max multiplier controls.
- `src/features/routing/LocalRoutingStatusTab.tsx` - scheduler summary and blocking reasons.
- `src/features/routing/LocalRoutingCandidateRow.tsx` - group/multiplier/load/error/TTFT factors.
- `src/features/routing/RoutingPage.tsx` - remove old five-strategy surface.
- `src/features/settings/SettingsPage.tsx` - remove legacy strategy editing or route to Local Routing.
- `docs/PROJECT_PLAN.md` - attribution note for Sub2API-inspired independent scheduler.

---

### Task 1: Contracts, Settings, And Schema

**Files:**
- Modify: `src-tauri/src/models/routing.rs`
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/models/station_keys.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src/lib/types/routing.ts`
- Modify: `src/lib/types/settings.ts`
- Test: Rust unit tests in `src-tauri/src/models/routing.rs`

- [ ] **Step 1: Write the failing routing type tests**

Add this test module to `src-tauri/src/models/routing.rs`:

```rust
#[cfg(test)]
mod automatic_scheduler_contract_tests {
    use super::*;

    #[test]
    fn routing_group_filter_round_trips_all_groups_and_group_type() {
        let all = serde_json::to_string(&RoutingGroupFilter::AllGroups).expect("serialize all");
        assert_eq!(all, "\"all_groups\"");

        let typed = serde_json::to_string(&RoutingGroupFilter::GroupType(PricingGroupType::Gpt))
            .expect("serialize group type");
        assert_eq!(typed, "{\"group_type\":\"gpt\"}");

        let decoded: RoutingGroupFilter =
            serde_json::from_str("{\"group_type\":\"image_generation\"}").expect("decode group");
        assert_eq!(decoded, RoutingGroupFilter::GroupType(PricingGroupType::ImageGeneration));
    }

    #[test]
    fn automatic_scheduler_settings_reject_missing_multiplier_but_not_all_groups() {
        let settings = AutomaticSchedulerSettings {
            max_rate_multiplier: None,
            default_routing_group_filter: RoutingGroupFilter::AllGroups,
            advanced: SchedulerAdvancedSettings::default(),
        };

        assert_eq!(
            settings.validate_for_routing().unwrap_err(),
            SchedulerConfigError::MultiplierLimitNotConfigured
        );
    }
}
```

- [ ] **Step 2: Run the RED test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml automatic_scheduler_contract_tests --lib
```

Expected: FAIL because `RoutingGroupFilter`, `PricingGroupType`, `AutomaticSchedulerSettings`, and `SchedulerConfigError` do not exist.

- [ ] **Step 3: Add the public Rust contracts**

Add these types to `src-tauri/src/models/routing.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PricingGroupType {
    Gpt,
    Claude,
    Gemini,
    Grok,
    ImageGeneration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingGroupFilter {
    AllGroups,
    UngroupedOnly,
    GroupBindingId(String),
    GroupIdHash(String),
    GroupType(PricingGroupType),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutomaticSchedulerSettings {
    pub max_rate_multiplier: Option<f64>,
    pub default_routing_group_filter: RoutingGroupFilter,
    pub advanced: SchedulerAdvancedSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerAdvancedSettings {
    pub scheduler_top_k: usize,
    pub scheduler_weight_multiplier: f64,
    pub scheduler_weight_priority: f64,
    pub scheduler_weight_load: f64,
    pub scheduler_weight_queue: f64,
    pub scheduler_weight_error_rate: f64,
    pub scheduler_weight_ttft: f64,
    pub scheduler_weight_quota_headroom: f64,
    pub scheduler_weight_previous_response: f64,
    pub scheduler_weight_session_sticky: f64,
    pub multiplier_min_confidence: f64,
    pub sticky_weighted_enabled: bool,
    pub sticky_escape_enabled: bool,
    pub sticky_escape_ttft_ms: i64,
    pub sticky_escape_error_rate: f64,
    pub sticky_session_ttl_seconds: i64,
    pub sticky_response_ttl_seconds: i64,
    pub sticky_max_waiting: usize,
    pub sticky_wait_timeout_seconds: i64,
    pub fallback_max_waiting: usize,
    pub fallback_wait_timeout_seconds: i64,
}

impl Default for SchedulerAdvancedSettings {
    fn default() -> Self {
        Self {
            scheduler_top_k: 7,
            scheduler_weight_multiplier: 1.0,
            scheduler_weight_priority: 1.0,
            scheduler_weight_load: 1.0,
            scheduler_weight_queue: 0.7,
            scheduler_weight_error_rate: 0.8,
            scheduler_weight_ttft: 0.5,
            scheduler_weight_quota_headroom: 0.0,
            scheduler_weight_previous_response: 5.0,
            scheduler_weight_session_sticky: 3.0,
            multiplier_min_confidence: 0.8,
            sticky_weighted_enabled: false,
            sticky_escape_enabled: true,
            sticky_escape_ttft_ms: 15_000,
            sticky_escape_error_rate: 0.5,
            sticky_session_ttl_seconds: 3_600,
            sticky_response_ttl_seconds: 3_600,
            sticky_max_waiting: 3,
            sticky_wait_timeout_seconds: 120,
            fallback_max_waiting: 100,
            fallback_wait_timeout_seconds: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerConfigError {
    MultiplierLimitNotConfigured,
    InvalidMultiplierLimit,
    InvalidAdvancedSetting(&'static str),
}
```

Add `validate_for_routing()` on `AutomaticSchedulerSettings` and `validate()` on `SchedulerAdvancedSettings`. Reject non-finite/negative multiplier limits, non-positive TopK, negative weights, base weights all zero, confidence outside `[0, 1]`, error threshold outside `[0, 1]`, and non-positive TTL/timeout values.

- [ ] **Step 4: Add persistent settings and station key fields**

Add settings fields in `src-tauri/src/models/settings.rs`:

```rust
pub max_rate_multiplier: Option<f64>,
pub default_routing_group_filter: RoutingGroupFilter,
pub scheduler_advanced_settings: SchedulerAdvancedSettings,
```

Add station key fields in `src-tauri/src/models/station_keys.rs`:

```rust
pub max_concurrency: i64,
pub load_factor: Option<i64>,
pub schedulable: bool,
pub manual_rate_multiplier: Option<f64>,
pub manual_rate_updated_at: Option<String>,
```

Update create/update input structs so clients can set `max_concurrency`, `load_factor`, `schedulable`, and manual multiplier override. Keep `manual_rate_updated_at` backend-owned.

- [ ] **Step 5: Add database migration and defaults**

In `src-tauri/src/services/database.rs`, add idempotent migrations:

```sql
ALTER TABLE app_settings ADD COLUMN max_rate_multiplier REAL;
ALTER TABLE app_settings ADD COLUMN default_routing_group_filter TEXT NOT NULL DEFAULT 'all_groups';
ALTER TABLE app_settings ADD COLUMN scheduler_advanced_settings_json TEXT NOT NULL DEFAULT '';
ALTER TABLE station_keys ADD COLUMN max_concurrency INTEGER NOT NULL DEFAULT 3;
ALTER TABLE station_keys ADD COLUMN load_factor INTEGER;
ALTER TABLE station_keys ADD COLUMN schedulable INTEGER NOT NULL DEFAULT 1;
ALTER TABLE station_keys ADD COLUMN manual_rate_multiplier REAL;
ALTER TABLE station_keys ADD COLUMN manual_rate_updated_at TEXT;
```

When `scheduler_advanced_settings_json` is empty, hydrate `SchedulerAdvancedSettings::default()` and persist the JSON on the next settings save.

- [ ] **Step 6: Add TypeScript contracts**

Update `src/lib/types/routing.ts`:

```ts
export type PricingGroupType = "gpt" | "claude" | "gemini" | "grok" | "image_generation";

export type RoutingGroupFilter =
  | { kind: "all_groups" }
  | { kind: "ungrouped_only" }
  | { kind: "group_binding_id"; value: string }
  | { kind: "group_id_hash"; value: string }
  | { kind: "group_type"; value: PricingGroupType };

export type SchedulerAdvancedSettings = {
  schedulerTopK: number;
  schedulerWeightMultiplier: number;
  schedulerWeightPriority: number;
  schedulerWeightLoad: number;
  schedulerWeightQueue: number;
  schedulerWeightErrorRate: number;
  schedulerWeightTtft: number;
  schedulerWeightQuotaHeadroom: number;
  schedulerWeightPreviousResponse: number;
  schedulerWeightSessionSticky: number;
  multiplierMinConfidence: number;
  stickyWeightedEnabled: boolean;
  stickyEscapeEnabled: boolean;
  stickyEscapeTtftMs: number;
  stickyEscapeErrorRate: number;
  stickySessionTtlSeconds: number;
  stickyResponseTtlSeconds: number;
  stickyMaxWaiting: number;
  stickyWaitTimeoutSeconds: number;
  fallbackMaxWaiting: number;
  fallbackWaitTimeoutSeconds: number;
};
```

- [ ] **Step 7: Run GREEN checks**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml automatic_scheduler_contract_tests --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm.cmd exec tsc --noEmit
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add -- src-tauri/src/models/routing.rs src-tauri/src/models/settings.rs src-tauri/src/models/station_keys.rs src-tauri/src/services/database.rs src/lib/types/routing.ts src/lib/types/settings.ts
git commit -m "feat: add automatic scheduler contracts"
```

---

### Task 2: Effective Multiplier Resolver

**Files:**
- Create: `src-tauri/src/services/proxy/scheduler/mod.rs`
- Create: `src-tauri/src/services/proxy/scheduler/types.rs`
- Create: `src-tauri/src/services/proxy/scheduler/multiplier.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write failing multiplier tests**

Create `src-tauri/src/services/proxy/scheduler/multiplier.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn source(overrides: impl FnOnce(&mut MultiplierSourceFacts)) -> MultiplierSourceFacts {
        let mut facts = MultiplierSourceFacts {
            station_key_id: "key".to_string(),
            manual_rate_multiplier: None,
            manual_rate_updated_at: None,
            group_binding_id: Some("binding-gpt".to_string()),
            group_id_hash: Some("hash-gpt".to_string()),
            group_name: Some("gpt".to_string()),
            collected_rate_multiplier: Some(1.25),
            collected_rate_source: Some("group_rate".to_string()),
            collected_rate_confidence: Some(0.95),
            collected_rate_collected_at_ms: Some(1_000),
            collected_rate_valid_until_ms: Some(10_000),
        };
        overrides(&mut facts);
        facts
    }

    #[test]
    fn manual_override_wins_with_confidence_one() {
        let fact = resolve_effective_multiplier(&source(|facts| {
            facts.manual_rate_multiplier = Some(0.8);
            facts.manual_rate_updated_at = Some("2026-07-11T00:00:00Z".to_string());
        }), 2_000, 0.8, 20 * 60 * 1_000)
        .expect("manual fact");

        assert_eq!(fact.value, 0.8);
        assert_eq!(fact.confidence, 1.0);
        assert_eq!(fact.source, "manual");
    }

    #[test]
    fn collected_fact_below_confidence_rejects() {
        let err = resolve_effective_multiplier(&source(|facts| {
            facts.collected_rate_confidence = Some(0.5);
        }), 2_000, 0.8, 20 * 60 * 1_000)
        .unwrap_err();

        assert_eq!(err, MultiplierRejectReason::LowConfidence);
    }

    #[test]
    fn expired_collected_fact_rejects() {
        let err = resolve_effective_multiplier(&source(|facts| {
            facts.collected_rate_valid_until_ms = Some(1_500);
        }), 2_000, 0.8, 20 * 60 * 1_000)
        .unwrap_err();

        assert_eq!(err, MultiplierRejectReason::Expired);
    }
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml multiplier::tests --lib
```

Expected: FAIL because scheduler module and resolver types are missing.

- [ ] **Step 3: Implement scheduler module shell**

Add `src-tauri/src/services/proxy/scheduler/mod.rs`:

```rust
pub mod multiplier;
pub mod types;
```

Update `src-tauri/src/services/proxy/mod.rs`:

```rust
pub mod scheduler;
```

- [ ] **Step 4: Implement multiplier types**

Add to `src-tauri/src/services/proxy/scheduler/types.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveMultiplierFact {
    pub station_key_id: String,
    pub value: f64,
    pub source: String,
    pub collected_at_ms: Option<i64>,
    pub valid_until_ms: Option<i64>,
    pub confidence: f64,
    pub group_binding_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiplierSourceFacts {
    pub station_key_id: String,
    pub manual_rate_multiplier: Option<f64>,
    pub manual_rate_updated_at: Option<String>,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
    pub collected_rate_multiplier: Option<f64>,
    pub collected_rate_source: Option<String>,
    pub collected_rate_confidence: Option<f64>,
    pub collected_rate_collected_at_ms: Option<i64>,
    pub collected_rate_valid_until_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiplierRejectReason {
    Missing,
    Invalid,
    Negative,
    Expired,
    UnboundGroup,
    LowConfidence,
}
```

- [ ] **Step 5: Implement resolver**

Add to `src-tauri/src/services/proxy/scheduler/multiplier.rs`:

```rust
use super::types::{EffectiveMultiplierFact, MultiplierRejectReason, MultiplierSourceFacts};

pub fn resolve_effective_multiplier(
    facts: &MultiplierSourceFacts,
    now_ms: i64,
    min_confidence: f64,
    group_rate_interval_ms: i64,
) -> Result<EffectiveMultiplierFact, MultiplierRejectReason> {
    if let Some(value) = facts.manual_rate_multiplier {
        validate_multiplier_value(value)?;
        return Ok(EffectiveMultiplierFact {
            station_key_id: facts.station_key_id.clone(),
            value,
            source: "manual".to_string(),
            collected_at_ms: None,
            valid_until_ms: None,
            confidence: 1.0,
            group_binding_id: facts.group_binding_id.clone(),
        });
    }

    let binding = facts
        .group_binding_id
        .clone()
        .ok_or(MultiplierRejectReason::UnboundGroup)?;
    let value = facts
        .collected_rate_multiplier
        .ok_or(MultiplierRejectReason::Missing)?;
    validate_multiplier_value(value)?;

    let confidence = facts.collected_rate_confidence.unwrap_or(0.0);
    if confidence < min_confidence {
        return Err(MultiplierRejectReason::LowConfidence);
    }

    let valid_until_ms = facts.collected_rate_valid_until_ms.or_else(|| {
        facts.collected_rate_collected_at_ms.map(|collected| {
            let freshness = (group_rate_interval_ms * 3).max(60 * 60 * 1_000);
            collected + freshness
        })
    });
    if valid_until_ms.is_some_and(|valid_until| now_ms > valid_until) {
        return Err(MultiplierRejectReason::Expired);
    }

    Ok(EffectiveMultiplierFact {
        station_key_id: facts.station_key_id.clone(),
        value,
        source: facts
            .collected_rate_source
            .clone()
            .unwrap_or_else(|| "group_rate".to_string()),
        collected_at_ms: facts.collected_rate_collected_at_ms,
        valid_until_ms,
        confidence,
        group_binding_id: Some(binding),
    })
}

fn validate_multiplier_value(value: f64) -> Result<(), MultiplierRejectReason> {
    if !value.is_finite() {
        return Err(MultiplierRejectReason::Invalid);
    }
    if value < 0.0 {
        return Err(MultiplierRejectReason::Negative);
    }
    Ok(())
}
```

- [ ] **Step 6: Connect database fact loading**

Add a database helper in `src-tauri/src/services/database.rs`:

```rust
pub fn load_multiplier_source_facts(&self, station_key_id: &str) -> Result<MultiplierSourceFacts, String> {
    let connection = self.connection()?;
    load_multiplier_source_facts_in_connection(&connection, station_key_id)
}
```

Map existing `station_keys.group_binding_id`, `group_id_hash`, `group_name`, `rate_multiplier`, `rate_source`, `rate_collected_at`, and new manual fields into `MultiplierSourceFacts`.

- [ ] **Step 7: Run GREEN**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml multiplier::tests --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add -- src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/scheduler/mod.rs src-tauri/src/services/proxy/scheduler/types.rs src-tauri/src/services/proxy/scheduler/multiplier.rs src-tauri/src/services/database.rs
git commit -m "feat: resolve scheduler multiplier facts"
```

---

### Task 3: Eligibility And Group-Scoped Candidate Pools

**Files:**
- Create: `src-tauri/src/services/proxy/scheduler/eligibility.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/mod.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/types.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`

- [ ] **Step 1: Write failing eligibility tests**

Add tests to `eligibility.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::routing::{PricingGroupType, RoutingGroupFilter};

    fn candidate(id: &str, group_type: Option<PricingGroupType>, multiplier: f64) -> SchedulerCandidate {
        SchedulerCandidate {
            station_key_id: id.to_string(),
            station_id: format!("station-{id}"),
            priority: 0,
            group_binding_id: group_type.as_ref().map(|kind| format!("binding-{kind:?}")),
            group_id_hash: group_type.as_ref().map(|kind| format!("hash-{kind:?}")),
            group_type,
            enabled: true,
            station_enabled: true,
            schedulable: true,
            secret_available: true,
            supports_endpoint: true,
            supports_stream: true,
            supports_tools: true,
            supports_vision: true,
            supports_reasoning: true,
            supports_model: true,
            health_blocked: false,
            balance_depleted: false,
            effective_multiplier: Some(EffectiveMultiplierFact {
                station_key_id: id.to_string(),
                value: multiplier,
                source: "group_rate".to_string(),
                collected_at_ms: Some(1_000),
                valid_until_ms: Some(10_000),
                confidence: 0.9,
                group_binding_id: Some("binding".to_string()),
            }),
        }
    }

    #[test]
    fn group_type_filter_is_hard_gate_before_budget() {
        let request = ScheduleRequest {
            endpoint: RouteEndpointKind::ChatCompletions,
            requested_model: Some("gpt-5.4".to_string()),
            mapped_model: None,
            routing_group_filter: RoutingGroupFilter::GroupType(PricingGroupType::Gpt),
            stream: false,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            max_rate_multiplier: 2.0,
            session_hash: None,
            previous_response_id: None,
            excluded_key_ids: Default::default(),
            now_ms: 2_000,
        };

        let claude = candidate("claude", Some(PricingGroupType::Claude), 0.2);
        let rejection = evaluate_candidate(&request, &claude).unwrap_err();

        assert_eq!(rejection.primary_code, "routing_group_mismatch");
    }

    #[test]
    fn over_ceiling_key_rejects_even_when_group_matches() {
        let request = ScheduleRequest {
            endpoint: RouteEndpointKind::ChatCompletions,
            requested_model: Some("gpt-5.4".to_string()),
            mapped_model: None,
            routing_group_filter: RoutingGroupFilter::GroupType(PricingGroupType::Gpt),
            stream: false,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            max_rate_multiplier: 1.0,
            session_hash: None,
            previous_response_id: None,
            excluded_key_ids: Default::default(),
            now_ms: 2_000,
        };

        let gpt = candidate("gpt", Some(PricingGroupType::Gpt), 1.2);
        let rejection = evaluate_candidate(&request, &gpt).unwrap_err();

        assert_eq!(rejection.primary_code, "routing_no_candidate_within_multiplier_limit");
    }
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml eligibility::tests --lib
```

Expected: FAIL because `SchedulerCandidate`, `ScheduleRequest`, and `evaluate_candidate` do not exist.

- [ ] **Step 3: Implement scheduler candidate and request types**

Add to `scheduler/types.rs`:

```rust
use std::collections::HashSet;
use crate::models::routing::{PricingGroupType, RouteEndpointKind, RoutingGroupFilter};

#[derive(Debug, Clone)]
pub struct ScheduleRequest {
    pub endpoint: RouteEndpointKind,
    pub requested_model: Option<String>,
    pub mapped_model: Option<String>,
    pub routing_group_filter: RoutingGroupFilter,
    pub stream: bool,
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub max_rate_multiplier: f64,
    pub session_hash: Option<String>,
    pub previous_response_id: Option<String>,
    pub excluded_key_ids: HashSet<String>,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub struct SchedulerCandidate {
    pub station_key_id: String,
    pub station_id: String,
    pub priority: i64,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_type: Option<PricingGroupType>,
    pub enabled: bool,
    pub station_enabled: bool,
    pub schedulable: bool,
    pub secret_available: bool,
    pub supports_endpoint: bool,
    pub supports_stream: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub supports_model: bool,
    pub health_blocked: bool,
    pub balance_depleted: bool,
    pub effective_multiplier: Option<EffectiveMultiplierFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateRejection {
    pub primary_code: &'static str,
    pub reasons: Vec<&'static str>,
}
```

- [ ] **Step 4: Implement eligibility gates**

Add to `scheduler/eligibility.rs`:

```rust
use crate::models::routing::RoutingGroupFilter;
use super::types::{CandidateRejection, ScheduleRequest, SchedulerCandidate};

pub fn evaluate_candidate(
    request: &ScheduleRequest,
    candidate: &SchedulerCandidate,
) -> Result<(), CandidateRejection> {
    let mut reasons = Vec::new();

    if !candidate.station_enabled || !candidate.enabled || !candidate.schedulable || !candidate.secret_available {
        reasons.push("asset_unavailable");
    }
    if !group_matches(&request.routing_group_filter, candidate) {
        reasons.push("routing_group_mismatch");
    }
    if !candidate.supports_endpoint
        || (request.stream && !candidate.supports_stream)
        || (request.uses_tools && !candidate.supports_tools)
        || (request.uses_vision && !candidate.supports_vision)
        || (request.uses_reasoning && !candidate.supports_reasoning)
    {
        reasons.push("capability_mismatch");
    }
    if !candidate.supports_model {
        reasons.push("model_mismatch");
    }
    if candidate.health_blocked {
        reasons.push("health_blocked");
    }
    if candidate.balance_depleted {
        reasons.push("balance_depleted");
    }
    let multiplier = match &candidate.effective_multiplier {
        Some(fact) => fact.value,
        None => {
            reasons.push("routing_no_multiplier_evidence");
            0.0
        }
    };
    if candidate.effective_multiplier.is_some() && multiplier > request.max_rate_multiplier {
        reasons.push("routing_no_candidate_within_multiplier_limit");
    }

    if reasons.is_empty() {
        Ok(())
    } else {
        Err(CandidateRejection {
            primary_code: reasons[0],
            reasons,
        })
    }
}

pub fn group_matches(filter: &RoutingGroupFilter, candidate: &SchedulerCandidate) -> bool {
    match filter {
        RoutingGroupFilter::AllGroups => true,
        RoutingGroupFilter::UngroupedOnly => candidate.group_binding_id.is_none() && candidate.group_id_hash.is_none(),
        RoutingGroupFilter::GroupBindingId(id) => candidate.group_binding_id.as_deref() == Some(id.as_str()),
        RoutingGroupFilter::GroupIdHash(hash) => candidate.group_id_hash.as_deref() == Some(hash.as_str()),
        RoutingGroupFilter::GroupType(kind) => candidate.group_type.as_ref() == Some(kind),
    }
}
```

- [ ] **Step 5: Add group-scoped candidate loading**

In `database.rs`, add:

```rust
pub fn load_scheduler_candidates(
    &self,
    filter: &RoutingGroupFilter,
    now_ms: i64,
) -> Result<Vec<SchedulerCandidate>, String> {
    let connection = self.connection()?;
    load_scheduler_candidates_in_connection(&connection, filter, now_ms)
}
```

For `GroupBindingId`, `GroupIdHash`, and `GroupType`, add SQL predicates before returning rows. For `AllGroups`, do not add a group predicate. For `UngroupedOnly`, require both `group_binding_id IS NULL` and `group_id_hash IS NULL`.

- [ ] **Step 6: Run GREEN**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml eligibility::tests --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add -- src-tauri/src/services/proxy/scheduler/mod.rs src-tauri/src/services/proxy/scheduler/types.rs src-tauri/src/services/proxy/scheduler/eligibility.rs src-tauri/src/services/database.rs
git commit -m "feat: add scheduler eligibility gates"
```

---

### Task 4: Scoring, TopK, And Weighted Ordering

**Files:**
- Create: `src-tauri/src/services/proxy/scheduler/scoring.rs`
- Create: `src-tauri/src/services/proxy/scheduler/selection.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/mod.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/types.rs`

- [ ] **Step 1: Write failing scoring and selection tests**

Add tests in `scoring.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplier_factor_prefers_lower_value_inside_ceiling() {
        let factors = normalize_multiplier_factors(&[0.5, 1.0, 1.5]);
        assert_eq!(factors, vec![1.0, 0.5, 0.0]);
    }

    #[test]
    fn missing_ttft_is_neutral_half() {
        let factors = normalize_ttft_factors(&[None, Some(100.0), Some(300.0)]);
        assert_eq!(factors, vec![0.5, 1.0, 0.0]);
    }
}
```

Add tests in `selection.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_k_ties_match_sub2api_order() {
        let candidates = vec![
            scored("b", 10.0, 1, 0.2, 0),
            scored("a", 10.0, 1, 0.2, 0),
            scored("c", 10.0, 2, 0.0, 0),
        ];

        let top = select_top_k(candidates, 3);
        let ids = top.iter().map(|item| item.station_key_id.as_str()).collect::<Vec<_>>();

        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    fn scored(id: &str, score: f64, priority: i64, load_rate: f64, waiting: usize) -> ScoredCandidate {
        ScoredCandidate {
            station_key_id: id.to_string(),
            priority,
            score,
            load_rate,
            waiting,
            sticky_kind: None,
        }
    }
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml scoring::tests selection::tests --lib
```

Expected: FAIL because scoring and selection helpers do not exist.

- [ ] **Step 3: Implement scoring helpers**

Add `NormalizedFactors` and helpers in `scoring.rs`:

```rust
pub fn normalize_multiplier_factors(values: &[f64]) -> Vec<f64> {
    normalize_lower_is_better(values, 1.0)
}

pub fn normalize_priority_factors(values: &[f64]) -> Vec<f64> {
    normalize_lower_is_better(values, 1.0)
}

pub fn normalize_ttft_factors(values: &[Option<f64>]) -> Vec<f64> {
    let present = values.iter().filter_map(|value| *value).collect::<Vec<_>>();
    if present.is_empty() {
        return vec![0.5; values.len()];
    }
    let min = present.iter().copied().fold(f64::INFINITY, f64::min);
    let max = present.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    values
        .iter()
        .map(|value| match value {
            None => 0.5,
            Some(_) if (max - min).abs() < f64::EPSILON => 0.5,
            Some(value) => 1.0 - ((*value - min) / (max - min)).clamp(0.0, 1.0),
        })
        .collect()
}

fn normalize_lower_is_better(values: &[f64], equal_value: f64) -> Vec<f64> {
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if values.is_empty() {
        return Vec::new();
    }
    if (max - min).abs() < f64::EPSILON {
        return vec![equal_value; values.len()];
    }
    values
        .iter()
        .map(|value| 1.0 - ((*value - min) / (max - min)).clamp(0.0, 1.0))
        .collect()
}
```

- [ ] **Step 4: Implement TopK and weighted order types**

Add to `selection.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredCandidate {
    pub station_key_id: String,
    pub priority: i64,
    pub score: f64,
    pub load_rate: f64,
    pub waiting: usize,
    pub sticky_kind: Option<StickyKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StickyKind {
    PreviousResponse,
    Session,
}

pub fn select_top_k(mut candidates: Vec<ScoredCandidate>, top_k: usize) -> Vec<ScoredCandidate> {
    candidates.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| a.load_rate.total_cmp(&b.load_rate))
            .then_with(|| a.waiting.cmp(&b.waiting))
            .then_with(|| a.station_key_id.cmp(&b.station_key_id))
    });
    candidates.truncate(top_k.min(candidates.len()));
    candidates
}

pub fn positive_weights(top_k: &[ScoredCandidate]) -> Vec<f64> {
    let min_score = top_k.iter().map(|item| item.score).fold(f64::INFINITY, f64::min);
    top_k.iter().map(|item| item.score - min_score + 1.0).collect()
}
```

- [ ] **Step 5: Add sticky front-move test**

Add this test to `selection.rs`:

```rust
#[test]
fn weighted_sticky_candidate_inside_top_k_moves_to_front() {
    let ordered = move_sticky_to_front(vec![
        scored("normal", 10.0, 0, 0.0, 0),
        ScoredCandidate {
            station_key_id: "sticky".to_string(),
            priority: 0,
            score: 9.0,
            load_rate: 0.0,
            waiting: 0,
            sticky_kind: Some(StickyKind::Session),
        },
    ]);

    assert_eq!(ordered[0].station_key_id, "sticky");
}
```

Implement `move_sticky_to_front()` so `PreviousResponse` wins over `Session` when both are present.

- [ ] **Step 6: Run GREEN**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml scoring::tests selection::tests --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add -- src-tauri/src/services/proxy/scheduler/mod.rs src-tauri/src/services/proxy/scheduler/types.rs src-tauri/src/services/proxy/scheduler/scoring.rs src-tauri/src/services/proxy/scheduler/selection.rs
git commit -m "feat: add scheduler scoring and topk"
```

---

### Task 5: Runtime Metrics, Capacity, And Waiting

**Files:**
- Create: `src-tauri/src/services/proxy/scheduler/metrics.rs`
- Create: `src-tauri/src/services/proxy/scheduler/capacity.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/mod.rs`

- [ ] **Step 1: Write failing EWMA tests**

Add to `metrics.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ewma_uses_sub2api_alpha() {
        let metrics = RuntimeMetricsRegistry::default();
        metrics.report_result("key", false, Some(1_000));
        metrics.report_result("key", true, Some(2_000));

        let snapshot = metrics.snapshot("key");

        assert!((snapshot.error_rate_ewma - 0.8).abs() < 0.0001);
        assert!((snapshot.ttft_ewma_ms.unwrap() - 1_200.0).abs() < 0.0001);
    }
}
```

- [ ] **Step 2: Write failing capacity tests**

Add to `capacity.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_max_concurrency_allows_unlimited_slots_but_load_capacity_is_one() {
        let registry = CapacityRegistry::default();
        let first = registry.try_acquire("key", 0).expect("first");
        let second = registry.try_acquire("key", 0).expect("second");

        assert!(first.acquired);
        assert!(second.acquired);
        assert_eq!(effective_load_capacity(0, None), 1);
    }

    #[test]
    fn positive_max_concurrency_blocks_when_full() {
        let registry = CapacityRegistry::default();
        let first = registry.try_acquire("key", 1).expect("first");
        let second = registry.try_acquire("key", 1).expect("second");

        assert!(first.acquired);
        assert!(!second.acquired);
    }
}
```

- [ ] **Step 3: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml metrics::tests capacity::tests --lib
```

Expected: FAIL because registries do not exist.

- [ ] **Step 4: Implement runtime metrics**

Implement `RuntimeMetricsRegistry` with `std::sync::Mutex<HashMap<String, KeyRuntimeMetrics>>`. Use alpha `0.2`; first error sample sets the value directly, later samples use `new = old * 0.8 + sample * 0.2`. TTFT updates only when `first_token_ms > 0`.

- [ ] **Step 5: Implement capacity registry**

Implement `CapacityRegistry` with per-key counters:

```rust
pub fn effective_load_capacity(max_concurrency: i64, load_factor: Option<i64>) -> i64 {
    if let Some(load_factor) = load_factor {
        if load_factor > 0 {
            return load_factor;
        }
    }
    if max_concurrency > 0 {
        max_concurrency
    } else {
        1
    }
}
```

`try_acquire(key, max_concurrency)` returns acquired for unlimited `max_concurrency <= 0`, rejects when `in_flight >= max_concurrency`, and releases exactly once through a guard or release function.

- [ ] **Step 6: Add waiting counter tests**

Add tests that `try_enter_wait("key", max_waiting)` increments until max and `WaitPermit::drop()` decrements.

- [ ] **Step 7: Run GREEN**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml metrics::tests capacity::tests --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add -- src-tauri/src/services/proxy/scheduler/mod.rs src-tauri/src/services/proxy/scheduler/metrics.rs src-tauri/src/services/proxy/scheduler/capacity.rs
git commit -m "feat: add scheduler metrics and capacity"
```

---

### Task 6: Group-Scoped Affinity

**Files:**
- Create: `src-tauri/src/services/proxy/scheduler/affinity.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/mod.rs`
- Modify: `src-tauri/src/services/proxy/routing_affinity.rs`

- [ ] **Step 1: Write failing affinity tests**

Add to `affinity.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_affinity_is_scoped_by_group() {
        let store = AffinityStore::default();
        store.bind_session("group:gpt", "session", "key-gpt", 1_000, 3_600);

        assert_eq!(store.lookup_session("group:gpt", "session", 2_000), Some("key-gpt".to_string()));
        assert_eq!(store.lookup_session("group:claude", "session", 2_000), None);
    }

    #[test]
    fn previous_response_has_precedence_over_session() {
        let store = AffinityStore::default();
        store.bind_session("group:gpt", "session", "key-session", 1_000, 3_600);
        store.bind_response("group:gpt", "response", "key-response", 1_000, 3_600);

        assert_eq!(
            store.resolve("group:gpt", Some("response"), Some("session"), 2_000),
            Some(AffinityHit { kind: AffinityKind::PreviousResponse, station_key_id: "key-response".to_string() })
        );
    }
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml affinity::tests --lib
```

Expected: FAIL because `AffinityStore` does not exist.

- [ ] **Step 3: Implement affinity store**

Use `Mutex<HashMap<AffinityKey, AffinityValue>>` with key fields `{ scope: String, id: String, kind: AffinityKind }`. Expire by `expires_at_ms`. Do not persist raw prompts. Implement `resolve()` to check previous response before session.

- [ ] **Step 4: Replace or wrap old affinity key**

In `routing_affinity.rs`, change `RouteAffinityKey` to include `routing_group_scope: String`, or mark the module as a compatibility wrapper that calls `scheduler::affinity::AffinityStore`.

- [ ] **Step 5: Run GREEN**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml affinity::tests --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/proxy/scheduler/mod.rs src-tauri/src/services/proxy/scheduler/affinity.rs src-tauri/src/services/proxy/routing_affinity.rs
git commit -m "feat: scope scheduler affinity by group"
```

---

### Task 7: End-To-End Scheduler And Route Simulation

**Files:**
- Create: `src-tauri/src/services/proxy/scheduler/explanation.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/mod.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/types.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/proxy/routing_snapshot.rs`
- Modify: `src/lib/types/routing.ts`
- Test: existing Rust tests in `router.rs`

- [ ] **Step 1: Write failing end-to-end scheduler tests**

Add tests in `router.rs`:

```rust
#[test]
fn automatic_router_rejects_all_keys_above_multiplier_ceiling() {
    let mut request = route_request(
        RouteEndpointKind::ChatCompletions,
        Some("gpt-5.4"),
        false,
        RoutingPolicy::AutomaticBalanced,
    );
    request.max_rate_multiplier = Some(1.0);
    request.routing_group_filter = RoutingGroupFilter::AllGroups;

    let candidates = vec![rich_candidate_with_economics(
        "expensive",
        0,
        capabilities(|_| {}),
        economics_with_multiplier(1.2, "normal"),
    )];

    let result = select_route_candidates(&request, candidates, &[]).unwrap_err();

    assert_eq!(result.code, "routing_no_candidate_within_multiplier_limit");
}

#[test]
fn automatic_router_group_type_filter_does_not_cross_groups() {
    let mut request = route_request(
        RouteEndpointKind::ChatCompletions,
        Some("gpt-5.4"),
        false,
        RoutingPolicy::AutomaticBalanced,
    );
    request.max_rate_multiplier = Some(2.0);
    request.routing_group_filter = RoutingGroupFilter::GroupType(PricingGroupType::Gpt);

    let candidates = vec![
        rich_candidate_with_group("cheap-claude", PricingGroupType::Claude, 0.2),
        rich_candidate_with_group("gpt", PricingGroupType::Gpt, 1.0),
    ];

    let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

    assert_eq!(selected.accepted[0].candidate.station_key_id, "gpt");
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml automatic_router_ --lib
```

Expected: FAIL because the old router lacks automatic policy, multiplier ceiling, and group filter fields.

- [ ] **Step 3: Add automatic route request fields**

Update `RouteRequest` in `router.rs`:

```rust
pub max_rate_multiplier: Option<f64>,
pub routing_group_filter: RoutingGroupFilter,
pub session_hash: Option<String>,
pub previous_response_id: Option<String>,
```

Add `RoutingPolicy::AutomaticBalanced` as an internal persisted value while keeping old policy parsing readable for migration.

- [ ] **Step 4: Build scheduler facade**

In `scheduler/mod.rs`, expose:

```rust
pub fn schedule_once(
    request: ScheduleRequest,
    candidates: Vec<SchedulerCandidate>,
    metrics: &RuntimeMetricsRegistry,
    capacity: &CapacityRegistry,
    affinity: &AffinityStore,
    settings: &SchedulerAdvancedSettings,
) -> Result<ScheduleDecision, ScheduleError>
```

`schedule_once` must run eligibility, scoring, TopK, weighted order, direct or weighted affinity, immediate acquisition, fresh-load rebuild hook, and wait plan construction. It must not forward HTTP.

- [ ] **Step 5: Route simulation through scheduler**

In `router.rs`, convert each `RichRouteCandidate` to `SchedulerCandidate`, call `schedule_once`, and convert `ScheduleDecision` into existing `RouteSimulationResult` plus new explanation fields.

- [ ] **Step 6: Add explanation JSON**

In `explanation.rs`, define:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDecisionSnapshot {
    pub station_key_id: String,
    pub accepted: bool,
    pub rejection_reasons: Vec<String>,
    pub routing_group_scope: String,
    pub group_match: bool,
    pub effective_multiplier: Option<f64>,
    pub multiplier_source: Option<String>,
    pub factors: SchedulerFactorBreakdown,
    pub base_score: f64,
    pub sticky_score: f64,
    pub in_top_k: bool,
    pub slot_result: String,
}
```

Ensure snapshots store no API key, authorization header, cookie, raw prompt, or full upstream body.

- [ ] **Step 7: Run GREEN**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml automatic_router_ --lib
cargo test --manifest-path .\src-tauri\Cargo.toml scheduler:: --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add -- src-tauri/src/services/proxy/scheduler/mod.rs src-tauri/src/services/proxy/scheduler/types.rs src-tauri/src/services/proxy/scheduler/explanation.rs src-tauri/src/services/proxy/router.rs src-tauri/src/services/proxy/routing_snapshot.rs src/lib/types/routing.ts
git commit -m "feat: route simulation through automatic scheduler"
```

---

### Task 8: Real Proxy Attempt Loop And Forward-Time Rechecks

**Files:**
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/observability.rs`
- Modify: `src-tauri/src/services/proxy/routing_failure.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/proxy.rs`

- [ ] **Step 1: Write failing runtime contract test**

Add a Rust unit test in `runtime.rs` around the pure helper that builds an attempt plan:

```rust
#[test]
fn selected_key_is_rechecked_against_latest_group_and_multiplier_before_forwarding() {
    let planned = PlannedProxyAttempt {
        station_key_id: "key".to_string(),
        routing_group_filter: RoutingGroupFilter::GroupType(PricingGroupType::Gpt),
        max_rate_multiplier: 1.0,
    };
    let latest = LatestAttemptFacts {
        station_key_id: "key".to_string(),
        group_type: Some(PricingGroupType::Gpt),
        effective_multiplier: Some(1.2),
    };

    let err = validate_forward_time_attempt(&planned, &latest).unwrap_err();

    assert_eq!(err.code, "routing_no_candidate_within_multiplier_limit");
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml selected_key_is_rechecked --lib
```

Expected: FAIL because the forward-time helper does not exist.

- [ ] **Step 3: Implement forward-time validation**

Add pure helper types in `runtime.rs` or `scheduler/types.rs`:

```rust
pub struct PlannedProxyAttempt {
    pub station_key_id: String,
    pub routing_group_filter: RoutingGroupFilter,
    pub max_rate_multiplier: f64,
}

pub struct LatestAttemptFacts {
    pub station_key_id: String,
    pub group_type: Option<PricingGroupType>,
    pub effective_multiplier: Option<f64>,
}
```

`validate_forward_time_attempt()` must reject group mismatch, missing multiplier, invalid multiplier, and multiplier above ceiling before any upstream bytes are sent.

- [ ] **Step 4: Integrate scheduler in real requests**

Replace `routing_policy(context)` usage in `runtime.rs` with settings-backed automatic scheduler inputs:

```rust
let settings = context.database.get_settings()?;
let scheduler_settings = settings.automatic_scheduler_settings()?;
let request = build_schedule_request(parsed_request, scheduler_settings, now_millis_for_services());
let decision = scheduler.schedule_once(request, candidates, metrics, capacity, affinity, &scheduler_settings.advanced)?;
```

Keep upstream forwarding code unchanged after the selected key has passed forward-time validation.

- [ ] **Step 5: Add retry exclusions**

On retryable pre-output failures, add selected key ID to `excluded_key_ids`, update EWMA with failure sample `1.0`, and reschedule with the same model, group scope, and multiplier ceiling. Do not retry after output starts.

- [ ] **Step 6: Add log metadata**

Extend request log metadata to include:

```json
{
  "schedulerLayer": "load_balance",
  "routingGroupFilter": "group_type:gpt",
  "routingGroupScope": "group_type:gpt",
  "configuredMultiplierLimit": 1.0,
  "selectedEffectiveMultiplier": 0.8,
  "topK": 7,
  "candidateCount": 12,
  "affinityKind": "session",
  "affinityEscaped": false,
  "decisionSnapshotJson": []
}
```

- [ ] **Step 7: Run GREEN**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml selected_key_is_rechecked --lib
cargo test --manifest-path .\src-tauri\Cargo.toml scheduler:: --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add -- src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/observability.rs src-tauri/src/services/proxy/routing_failure.rs src-tauri/src/services/database.rs src-tauri/src/models/proxy.rs
git commit -m "feat: use scheduler for proxy attempts"
```

---

### Task 9: UI, Logs, Legacy Removal, And Verification

**Files:**
- Create: `scripts/local-routing-automatic-settings.test.mjs`
- Create: `scripts/local-routing-scheduler-log-contract.test.mjs`
- Modify: `src/features/routing/LocalRoutingEditTab.tsx`
- Modify: `src/features/routing/LocalRoutingStatusTab.tsx`
- Modify: `src/features/routing/LocalRoutingCandidateRow.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/lib/types/localRouting.ts`
- Modify: `src/lib/types/settings.ts`
- Modify: `docs/PROJECT_PLAN.md`

- [ ] **Step 1: Write failing UI contract script**

Create `scripts/local-routing-automatic-settings.test.mjs`:

```js
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const read = (path) => readFileSync(join(root, path), "utf8");

function assertIncludes(source, needle, label) {
  if (!source.includes(needle)) {
    throw new Error(`${label} should include ${needle}`);
  }
}

function assertExcludes(source, needle, label) {
  if (source.includes(needle)) {
    throw new Error(`${label} should not include ${needle}`);
  }
}

const editTab = read("src/features/routing/LocalRoutingEditTab.tsx");
const statusTab = read("src/features/routing/LocalRoutingStatusTab.tsx");
const routingTypes = read("src/lib/types/routing.ts");

assertIncludes(editTab, "最高倍率", "routing edit tab");
assertIncludes(editTab, "分组范围", "routing edit tab");
assertIncludes(editTab, "全部分组", "routing group selector");
assertIncludes(editTab, "GPT", "routing group selector");
assertIncludes(statusTab, "当前分组没有可用 Key 时会拒绝请求", "routing status copy");
assertIncludes(routingTypes, "RoutingGroupFilter", "routing type contract");

for (const legacy of ["priority_fallback", "stable_first", "backup_only", "cheap_first", "cost_stable_first"]) {
  assertExcludes(editTab, legacy, "local routing edit tab");
}

console.log("local routing automatic settings contract ok");
```

- [ ] **Step 2: Write failing log contract script**

Create `scripts/local-routing-scheduler-log-contract.test.mjs`:

```js
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const read = (path) => readFileSync(join(root, path), "utf8");

function assertIncludes(source, needle, label) {
  if (!source.includes(needle)) {
    throw new Error(`${label} should include ${needle}`);
  }
}

const localRoutingTypes = read("src/lib/types/localRouting.ts");
const runtime = read("src-tauri/src/services/proxy/runtime.rs");
const observability = read("src-tauri/src/services/proxy/observability.rs");

for (const field of [
  "routingGroupFilter",
  "routingGroupScope",
  "configuredMultiplierLimit",
  "selectedEffectiveMultiplier",
  "decisionSnapshotJson",
]) {
  assertIncludes(localRoutingTypes, field, "local routing types");
}

assertIncludes(runtime, "validate_forward_time_attempt", "proxy runtime");
assertIncludes(observability, "decision_snapshot", "observability metadata");

console.log("local routing scheduler log contract ok");
```

- [ ] **Step 3: Run RED**

```powershell
node scripts\local-routing-automatic-settings.test.mjs
node scripts\local-routing-scheduler-log-contract.test.mjs
```

Expected: FAIL because UI copy, TS fields, and runtime/log fields are not complete.

- [ ] **Step 4: Implement Local Routing UI**

In `LocalRoutingEditTab.tsx`, replace old strategy selector with:

- max multiplier numeric input;
- group scope segmented/select control with `全部分组`, `GPT`, `Claude`, `Gemini`, `Grok`, `图片生成`, and exact bindings when available;
- eligible-key preview count;
- advanced collapsed scheduler defaults.

Do not add a marketing-style page. Keep the existing compact desktop-tool layout.

- [ ] **Step 5: Implement status and candidate rows**

In `LocalRoutingStatusTab.tsx`, show:

- selected group scope;
- max multiplier;
- eligible key count;
- blocking reason when zero candidates qualify;
- last selected key;
- last wait/fallback result.

In `LocalRoutingCandidateRow.tsx`, show:

- group match;
- effective multiplier value/source/freshness;
- load, queue, error EWMA, TTFT;
- TopK membership and final decision.

- [ ] **Step 6: Implement TS log fields**

Update `src/lib/types/localRouting.ts` with:

```ts
export type SchedulerDecisionSnapshot = {
  stationKeyId: string;
  accepted: boolean;
  rejectionReasons: string[];
  routingGroupScope: string;
  groupMatch: boolean;
  effectiveMultiplier: number | null;
  multiplierSource: string | null;
  baseScore: number;
  stickyScore: number;
  inTopK: boolean;
  slotResult: string;
};
```

Add scheduler fields to `RouteDecisionSummary` and `RouteDecisionEvent`.

- [ ] **Step 7: Add attribution**

In `docs/PROJECT_PLAN.md`, add:

```markdown
### Scheduler Attribution

The automatic local routing scheduler is an independent Rust implementation inspired by Sub2API's account scheduler at commit `e316ebf52838a89d57fc790981cce7520f819ac8`. Relay Pool does not copy or link Sub2API core code. The hard multiplier ceiling and multiplier scoring factor are Relay Pool-specific extensions.
```

- [ ] **Step 8: Delete legacy router surface**

After all simulation and real proxy tests pass, remove dead branches from `routing_policy.rs` and old five-strategy UI text. Keep parsing legacy strategy values only in migration/log compatibility code.

- [ ] **Step 9: Run GREEN**

```powershell
node scripts\local-routing-automatic-settings.test.mjs
node scripts\local-routing-scheduler-log-contract.test.mjs
node scripts\local-routing-query-service.test.mjs
node scripts\routing-query-service.test.mjs
pnpm.cmd exec tsc --noEmit
pnpm.cmd build
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected: PASS. If `pnpm.cmd build` fails with a transient `EPERM` on `dist/assets`, rerun the exact same command once before diagnosing source code.

- [ ] **Step 10: Commit**

```powershell
git add -- scripts/local-routing-automatic-settings.test.mjs scripts/local-routing-scheduler-log-contract.test.mjs src/features/routing/LocalRoutingEditTab.tsx src/features/routing/LocalRoutingStatusTab.tsx src/features/routing/LocalRoutingCandidateRow.tsx src/features/routing/RoutingPage.tsx src/features/settings/SettingsPage.tsx src/lib/types/localRouting.ts src/lib/types/settings.ts docs/PROJECT_PLAN.md
git commit -m "feat: expose automatic routing controls"
```

---

## Final Verification

Run the full verification chain after Task 9:

```powershell
git status --short
node scripts\local-routing-automatic-settings.test.mjs
node scripts\local-routing-scheduler-log-contract.test.mjs
node scripts\local-routing-query-service.test.mjs
node scripts\routing-query-service.test.mjs
pnpm.cmd exec tsc --noEmit
pnpm.cmd build
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
git diff --check
```

Expected final behavior:

- A user can set a maximum multiplier and optionally narrow routing to a group such as `gpt`.
- A specific group filter is a hard pool boundary and never falls back to another group.
- All selected keys have trusted, current multipliers inside the configured ceiling.
- Unknown, invalid, expired, unbound, or over-ceiling multiplier facts reject.
- Previous-response affinity wins over session affinity and cannot cross group boundaries.
- TopK ordering, tie-breaks, weighted order, concurrency, waiting, fresh-load retry, and EWMA match the Sub2API-style contract.
- Real proxy attempts recheck latest group binding and multiplier facts immediately before forwarding.
- No automatic replay happens after downstream output starts.
- Request logs and route simulation expose the same scheduler facts without secrets.

## Self-Review Checklist

- Spec coverage: Tasks 1-3 cover contracts/schema, group filter, multiplier facts, and eligibility. Tasks 4-6 cover scoring, TopK, metrics, capacity, and affinity. Tasks 7-8 cover route simulation and real proxy lifecycle. Task 9 covers UI, logs, attribution, and legacy removal.
- Placeholder scan: The plan contains no placeholder markers or unnamed deferred work. Each implementation step names files, functions, commands, and expected results.
- Type consistency: `RoutingGroupFilter`, `PricingGroupType`, `AutomaticSchedulerSettings`, `ScheduleRequest`, `SchedulerCandidate`, and `EffectiveMultiplierFact` are introduced before later tasks use them.
