//! Read-only projection for routing protection facts.
//!
//! Durable health verdicts, legacy health snapshots, and process-local
//! capacity protection are intentionally represented as separate persistence
//! kinds. This keeps a runtime capacity cooldown from being presented as a
//! durable circuit breaker and gives callers an explicit unavailable state.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    application::health_protection::HealthProtectionStatus,
    application::routing_engine::failure_domains::ProviderCapacityDomain,
    models::routing::StationKeyHealth,
    persistence::stores::routing_health_verdict_store::{
        DurableHealthVerdict, ScopedHealthVerdictRow,
    },
};

pub(crate) const ROUTING_PROTECTION_STATUS_VERSION: &str = "routing_protection_status_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProtectionPersistenceKind {
    Durable,
    LegacyCompatibility,
    RuntimeCapacity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProtectionState {
    NoProtection,
    Degraded,
    Cooldown,
    Blocked,
    Open,
    HalfOpen,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProtectionReadModelStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProtectionStatusEntry {
    /// This is a non-secret commitment or a bounded compatibility hash. It
    /// never contains an endpoint, credential, request, or account value.
    pub(crate) scope: String,
    pub(crate) scope_kind: Option<String>,
    pub(crate) state: ProtectionState,
    /// Stable UI explanation key. The public `Degraded` state also carries
    /// whether it came from reducer `Closed` monitoring or an actual degraded
    /// verdict; consumers must not infer that distinction from `state` alone.
    pub(crate) explanation_key: String,
    pub(crate) persistence_kind: Option<ProtectionPersistenceKind>,
    pub(crate) cooldown_until_ms: Option<i64>,
    pub(crate) cooldown_remaining_ms: Option<i64>,
    pub(crate) recent_failure_code: Option<String>,
    pub(crate) updated_at_ms: Option<i64>,
    pub(crate) detail_available: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingProtectionStatus {
    pub(crate) status_version: &'static str,
    pub(crate) generated_at_ms: i64,
    pub(crate) entries: Vec<ProtectionStatusEntry>,
    /// Aggregated, low-sensitivity Provider/capacity-domain diagnostics.
    ///
    /// This is projected together with `entries` so callers do not need to
    /// infer health by joining independent read models in the UI. The vector
    /// is omitted when no candidate identity is available for the query.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) failure_domains: Vec<RoutingFailureDomainDiagnostic>,
    pub(crate) read_model_status: ProtectionReadModelStatus,
    /// Effective proxy timeout facts. These are read-only runtime facts and
    /// intentionally do not belong to the editable routing policy document.
    pub(crate) timeouts: Option<ProxyTimeoutFacts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingFailureDomainDiagnostic {
    /// Present only when a concrete model and trusted identity resolved to a
    /// canonical capacity-domain commitment. It is a digest, never a URL or
    /// credential/account identifier.
    pub(crate) commitment: Option<String>,
    pub(crate) resolution: String,
    pub(crate) provider_family: Option<String>,
    pub(crate) deployment_identity: Option<String>,
    pub(crate) region_identity: Option<String>,
    pub(crate) revision: Option<i64>,
    pub(crate) candidate_count: u32,
    pub(crate) schedulable_candidate_count: u32,
    pub(crate) status: ProtectionState,
    pub(crate) persistence_kind: Option<ProtectionPersistenceKind>,
    pub(crate) recent_failure_code: Option<String>,
    pub(crate) explanation_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FailureDomainCandidateFact {
    pub(crate) provider_family: Option<String>,
    pub(crate) deployment_identity: Option<String>,
    pub(crate) region_identity: Option<String>,
    pub(crate) revision: Option<i64>,
    pub(crate) schedulable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxyTimeoutFacts {
    pub(crate) connect_seconds: f64,
    pub(crate) first_byte_seconds: f64,
    pub(crate) precommit_seconds: f64,
    pub(crate) buffered_execution_seconds: f64,
    pub(crate) stream_idle_seconds: f64,
    pub(crate) owner: String,
}

/// The runtime registry is process-local. The caller supplies a deliberately
/// narrow fact instead of exposing registry locks or mutable state to queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapacityProtectionFact {
    pub(crate) scope: String,
    pub(crate) state: String,
    pub(crate) cooldown_until_ms: Option<i64>,
    pub(crate) recent_failure_code: Option<String>,
    pub(crate) updated_at_ms: Option<i64>,
}

/// Projects the currently available protection inputs into one public read
/// model. The projection has no side effects and never writes health state.
///
/// Production callers must use `project_routing_protection_status_with_reducer`
/// so reducer facts and runtime capacity facts share one projector. This
/// no-reducer convenience is retained only for unit fixtures.
#[cfg(test)]
pub(crate) fn project_routing_protection_status(
    generated_at_ms: i64,
    durable: &[ScopedHealthVerdictRow],
    legacy: &[StationKeyHealth],
    capacity: &[CapacityProtectionFact],
    runtime_capacity_available: bool,
) -> RoutingProtectionStatus {
    project_routing_protection_status_with_reducer(
        generated_at_ms,
        durable,
        legacy,
        capacity,
        runtime_capacity_available,
        &[],
    )
}

#[cfg(test)]
pub(crate) fn project_routing_protection_status_with_reducer(
    generated_at_ms: i64,
    durable: &[ScopedHealthVerdictRow],
    legacy: &[StationKeyHealth],
    capacity: &[CapacityProtectionFact],
    runtime_capacity_available: bool,
    reducer_statuses: &[HealthProtectionStatus],
) -> RoutingProtectionStatus {
    project_routing_protection_status_with_reducer_and_domains(
        generated_at_ms,
        durable,
        legacy,
        capacity,
        runtime_capacity_available,
        reducer_statuses,
        &[],
        None,
    )
}

pub(crate) fn project_routing_protection_status_with_reducer_and_domains(
    generated_at_ms: i64,
    durable: &[ScopedHealthVerdictRow],
    legacy: &[StationKeyHealth],
    capacity: &[CapacityProtectionFact],
    runtime_capacity_available: bool,
    reducer_statuses: &[HealthProtectionStatus],
    domain_facts: &[FailureDomainCandidateFact],
    requested_model: Option<&str>,
) -> RoutingProtectionStatus {
    let mut entries = Vec::new();

    for row in durable {
        let reducer_status = reducer_statuses
            .iter()
            .filter(|status| {
                status.persistence_kind
                    == crate::application::health_protection::HealthProtectionPersistenceKind::Durable
            })
            .find(|status| status.scope.commitment == row.subject_scope);
        let (state, explanation_key, cooldown_until_ms, cooldown_remaining_ms, recent_failure_code) =
            if let Some(status) = reducer_status {
                let (state, explanation_key) = reducer_projection(status.state);
                (
                    state,
                    explanation_key,
                    status.cooldown_until_ms,
                    status.cooldown_remaining_ms,
                    status
                        .recent_failure_code
                        .map(|code| bounded_code(code.as_str(), "durable_failure")),
                )
            } else {
                (
                    durable_state(row.verdict),
                    durable_explanation_key(row.verdict),
                    row.cooldown_until_ms,
                    remaining_ms(row.cooldown_until_ms, generated_at_ms),
                    Some(bounded_code(&row.evidence_code, "durable_failure")),
                )
            };
        entries.push(ProtectionStatusEntry {
            scope: bounded_scope(&row.subject_scope, "durable"),
            scope_kind: Some(row.scope_kind.as_str().to_string()),
            state,
            explanation_key: explanation_key.to_string(),
            persistence_kind: Some(ProtectionPersistenceKind::Durable),
            cooldown_until_ms,
            cooldown_remaining_ms,
            recent_failure_code,
            updated_at_ms: reducer_status
                .map(|status| status.updated_at_ms)
                .or_else(|| non_negative(row.updated_at_ms)),
            detail_available: true,
        });
    }

    // Error-rate protection can exist before an explicit scoped verdict row
    // (it is driven by the reducer window itself). Project those durable
    // reducer entries as first-class status facts instead of silently hiding
    // them behind the legacy verdict join.
    for status in reducer_statuses.iter().filter(|status| {
        status.persistence_kind
            == crate::application::health_protection::HealthProtectionPersistenceKind::Durable
    }) {
        if durable
            .iter()
            .any(|row| row.subject_scope == status.scope.commitment)
        {
            continue;
        }
        let (state, explanation_key) = reducer_projection(status.state);
        entries.push(ProtectionStatusEntry {
            scope: bounded_scope(&status.scope.commitment, "durable"),
            scope_kind: Some(reducer_scope_kind(status.scope.kind).to_string()),
            state,
            explanation_key: explanation_key.to_string(),
            persistence_kind: Some(ProtectionPersistenceKind::Durable),
            cooldown_until_ms: status.cooldown_until_ms,
            cooldown_remaining_ms: status.cooldown_remaining_ms,
            recent_failure_code: status
                .recent_failure_code
                .map(|code| bounded_code(code.as_str(), "durable_failure")),
            updated_at_ms: non_negative(status.updated_at_ms),
            detail_available: status.detail_available,
        });
    }

    for health in legacy {
        let has_protection = health.consecutive_failures > 0 || health.cooldown_until.is_some();
        if !has_protection {
            continue;
        }
        let cooldown_until_ms = health
            .cooldown_until
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value >= 0);
        entries.push(ProtectionStatusEntry {
            scope: legacy_scope(&health.station_key_id),
            scope_kind: Some("legacy_station_key".to_string()),
            state: if cooldown_until_ms.is_some() {
                ProtectionState::Cooldown
            } else {
                ProtectionState::Degraded
            },
            explanation_key: if cooldown_until_ms.is_some() {
                "routing.protection.legacy_cooldown"
            } else {
                "routing.protection.legacy_degraded"
            }
            .to_string(),
            persistence_kind: Some(ProtectionPersistenceKind::LegacyCompatibility),
            cooldown_until_ms,
            cooldown_remaining_ms: remaining_ms(cooldown_until_ms, generated_at_ms),
            recent_failure_code: health
                .last_error_summary
                .as_deref()
                .filter(|summary| !summary.trim().is_empty())
                .map(|_| "legacy_failure".to_string()),
            updated_at_ms: health
                .updated_at
                .parse::<i64>()
                .ok()
                .filter(|value| *value >= 0),
            detail_available: true,
        });
    }

    if runtime_capacity_available {
        for fact in capacity {
            entries.push(capacity_entry(fact, generated_at_ms));
        }
    } else {
        entries.push(ProtectionStatusEntry {
            scope: "runtime_capacity".to_string(),
            scope_kind: Some("capacity_domain".to_string()),
            state: ProtectionState::Unavailable,
            explanation_key: "routing.protection.unavailable".to_string(),
            persistence_kind: Some(ProtectionPersistenceKind::RuntimeCapacity),
            cooldown_until_ms: None,
            cooldown_remaining_ms: None,
            recent_failure_code: None,
            updated_at_ms: None,
            detail_available: false,
        });
    }

    entries.sort_by(|left, right| {
        left.scope
            .cmp(&right.scope)
            .then_with(|| left.persistence_kind.cmp(&right.persistence_kind))
            .then_with(|| left.scope_kind.cmp(&right.scope_kind))
    });

    if entries.is_empty() {
        entries.push(ProtectionStatusEntry {
            scope: "routing".to_string(),
            scope_kind: None,
            state: ProtectionState::NoProtection,
            explanation_key: "routing.protection.none_active".to_string(),
            persistence_kind: None,
            cooldown_until_ms: None,
            cooldown_remaining_ms: None,
            recent_failure_code: None,
            updated_at_ms: Some(generated_at_ms),
            detail_available: true,
        });
    }

    let failure_domains =
        project_failure_domain_diagnostics(domain_facts, requested_model, &entries);

    RoutingProtectionStatus {
        status_version: ROUTING_PROTECTION_STATUS_VERSION,
        generated_at_ms,
        entries,
        failure_domains,
        read_model_status: ProtectionReadModelStatus::Available,
        timeouts: None,
    }
}

/// Aggregate candidate identity facts by the same canonical capacity-domain
/// commitment used by planner admission. Unresolved identities remain visible
/// with a bounded explanation, but never get a guessed commitment or health
/// status. This keeps the diagnostics useful while preserving fail-closed
/// routing semantics.
fn project_failure_domain_diagnostics(
    facts: &[FailureDomainCandidateFact],
    requested_model: Option<&str>,
    entries: &[ProtectionStatusEntry],
) -> Vec<RoutingFailureDomainDiagnostic> {
    #[derive(Debug, Clone)]
    struct Aggregate {
        commitment: Option<String>,
        resolution: String,
        provider_family: Option<String>,
        deployment_identity: Option<String>,
        region_identity: Option<String>,
        revision: Option<i64>,
        candidate_count: u32,
        schedulable_candidate_count: u32,
    }

    let mut aggregates = std::collections::BTreeMap::<String, Aggregate>::new();
    for fact in facts {
        let provider = fact
            .provider_family
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(bounded_identity);
        let deployment = fact
            .deployment_identity
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(bounded_identity);
        let region = fact
            .region_identity
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(bounded_identity);
        let resolution = if provider.is_none() {
            "not_configured"
        } else if requested_model.is_none() {
            "model_required"
        } else if ProviderCapacityDomain::from_trusted_identity(
            provider.as_deref().unwrap_or_default(),
            requested_model.unwrap_or_default(),
            deployment.as_deref(),
            region.as_deref(),
        )
        .is_none()
        {
            "invalid_identity"
        } else {
            "resolved"
        };
        let commitment = if resolution == "resolved" {
            ProviderCapacityDomain::from_trusted_identity(
                provider.as_deref().unwrap_or_default(),
                requested_model.unwrap_or_default(),
                deployment.as_deref(),
                region.as_deref(),
            )
            .map(|domain| {
                let commitment = domain.commitment();
                format!("v{}:{}", commitment.schema_version, commitment.digest_hex)
            })
        } else {
            None
        };
        let key = commitment.clone().unwrap_or_else(|| {
            format!(
                "unresolved:{resolution}:{}:{}:{}",
                provider.as_deref().unwrap_or("-").to_ascii_lowercase(),
                deployment.as_deref().unwrap_or("-").to_ascii_lowercase(),
                region.as_deref().unwrap_or("-").to_ascii_lowercase()
            )
        });
        let aggregate = aggregates.entry(key).or_insert_with(|| Aggregate {
            commitment,
            resolution: resolution.to_string(),
            provider_family: provider,
            deployment_identity: deployment,
            region_identity: region,
            revision: fact.revision,
            candidate_count: 0,
            schedulable_candidate_count: 0,
        });
        aggregate.candidate_count = aggregate.candidate_count.saturating_add(1);
        if fact.schedulable {
            aggregate.schedulable_candidate_count =
                aggregate.schedulable_candidate_count.saturating_add(1);
        }
        if aggregate.revision.is_none() {
            aggregate.revision = fact.revision;
        }
    }

    aggregates
        .into_values()
        .map(|aggregate| {
            let protection = aggregate
                .commitment
                .as_deref()
                .and_then(|commitment| entries.iter().find(|entry| entry.scope == commitment));
            let (status, persistence_kind, recent_failure_code, explanation_key) =
                if let Some(entry) = protection {
                    (
                        entry.state,
                        entry.persistence_kind,
                        entry.recent_failure_code.clone(),
                        entry.explanation_key.clone(),
                    )
                } else {
                    (
                        ProtectionState::NoProtection,
                        None,
                        None,
                        format!("routing.failure_domain.{}", aggregate.resolution),
                    )
                };
            RoutingFailureDomainDiagnostic {
                commitment: aggregate.commitment,
                resolution: aggregate.resolution,
                provider_family: aggregate.provider_family,
                deployment_identity: aggregate.deployment_identity,
                region_identity: aggregate.region_identity,
                revision: aggregate.revision,
                candidate_count: aggregate.candidate_count,
                schedulable_candidate_count: aggregate.schedulable_candidate_count,
                status,
                persistence_kind,
                recent_failure_code,
                explanation_key,
            }
        })
        .collect()
}

fn bounded_identity(value: &str) -> String {
    let value = value.trim();
    if value.len() <= 128
        && !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/' | b' ')
        })
    {
        value.to_string()
    } else {
        format!("identity:v1:{}", digest_hex(value.as_bytes()))
    }
}

/// Maps the reducer's internal state to the versioned public read model.
///
/// `Closed` means that protection is not open, but the scope has observations
/// and is still being evaluated. The v1 public enum has no separate
/// `Monitoring` value, so it is intentionally projected as `Degraded` with a
/// distinct explanation key. Keep this pair together so state and copy cannot
/// drift into contradictory UI semantics.
fn reducer_projection(
    state: crate::application::health_protection::HealthProtectionState,
) -> (ProtectionState, &'static str) {
    match state {
        crate::application::health_protection::HealthProtectionState::Closed => (
            ProtectionState::Degraded,
            "routing.protection.closed_monitoring",
        ),
        crate::application::health_protection::HealthProtectionState::Open => {
            (ProtectionState::Open, "routing.protection.open")
        }
        crate::application::health_protection::HealthProtectionState::HalfOpen => {
            (ProtectionState::HalfOpen, "routing.protection.half_open")
        }
    }
}

fn durable_explanation_key(verdict: DurableHealthVerdict) -> &'static str {
    match verdict {
        DurableHealthVerdict::Degraded => "routing.protection.degraded",
        DurableHealthVerdict::Cooldown => "routing.protection.cooldown",
        DurableHealthVerdict::Blocked => "routing.protection.blocked",
    }
}

fn reducer_scope_kind(
    kind: crate::application::health_protection::HealthProtectionScopeKind,
) -> &'static str {
    match kind {
        crate::application::health_protection::HealthProtectionScopeKind::Credential => {
            "credential"
        }
        crate::application::health_protection::HealthProtectionScopeKind::Account => "account",
        crate::application::health_protection::HealthProtectionScopeKind::Group => "group",
        crate::application::health_protection::HealthProtectionScopeKind::Endpoint => "endpoint",
        crate::application::health_protection::HealthProtectionScopeKind::Model => "model",
        crate::application::health_protection::HealthProtectionScopeKind::CapacityDomain => {
            "capacity_domain"
        }
    }
}

fn durable_state(verdict: DurableHealthVerdict) -> ProtectionState {
    match verdict {
        DurableHealthVerdict::Degraded => ProtectionState::Degraded,
        DurableHealthVerdict::Cooldown => ProtectionState::Cooldown,
        DurableHealthVerdict::Blocked => ProtectionState::Blocked,
    }
}

fn capacity_entry(fact: &CapacityProtectionFact, generated_at_ms: i64) -> ProtectionStatusEntry {
    let normalized_state = fact.state.trim().to_ascii_lowercase();
    let (state, detail_available) = match normalized_state.as_str() {
        "open" => (ProtectionState::Open, true),
        "half_open" | "half-open" => (ProtectionState::HalfOpen, true),
        _ => (ProtectionState::Unavailable, false),
    };
    ProtectionStatusEntry {
        scope: bounded_scope(&fact.scope, "capacity"),
        scope_kind: Some("capacity_domain".to_string()),
        state,
        explanation_key: match state {
            ProtectionState::Open => "routing.protection.open",
            ProtectionState::HalfOpen => "routing.protection.half_open",
            _ => "routing.protection.unavailable",
        }
        .to_string(),
        persistence_kind: Some(ProtectionPersistenceKind::RuntimeCapacity),
        cooldown_until_ms: fact.cooldown_until_ms,
        cooldown_remaining_ms: remaining_ms(fact.cooldown_until_ms, generated_at_ms),
        recent_failure_code: fact
            .recent_failure_code
            .as_deref()
            .map(|code| bounded_code(code, "capacity_failure")),
        updated_at_ms: fact.updated_at_ms.filter(|value| *value >= 0),
        detail_available,
    }
}

fn remaining_ms(until_ms: Option<i64>, now_ms: i64) -> Option<i64> {
    until_ms.map(|until| until.saturating_sub(now_ms).max(0))
}

fn non_negative(value: i64) -> Option<i64> {
    (value >= 0).then_some(value)
}

fn bounded_scope(value: &str, prefix: &str) -> String {
    let value = value.trim();
    if value.len() <= 128
        && !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
    {
        return value.to_string();
    }
    format!("{prefix}:v1:{}", digest_hex(value.as_bytes()))
}

fn legacy_scope(station_key_id: &str) -> String {
    format!(
        "legacy_station_key:v1:{}",
        digest_hex(station_key_id.trim().as_bytes())
    )
}

fn bounded_code(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.len() <= 96
        && !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        value.to_string()
    } else {
        fallback.to_string()
    }
}

fn digest_hex(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::stores::routing_health_verdict_store::{
        DurableHealthVerdict, FailureDimension, HealthScopeKind,
    };

    fn durable_row() -> ScopedHealthVerdictRow {
        ScopedHealthVerdictRow {
            subject_scope: "station_key_credential:v1:abc".to_string(),
            scope_kind: HealthScopeKind::StationKeyCredential,
            dimension: FailureDimension::Credential,
            verdict: DurableHealthVerdict::Cooldown,
            cooldown_until_ms: Some(1_500),
            evidence_code: "auth_invalid".to_string(),
            updated_at_ms: 1_000,
        }
    }

    fn legacy_health() -> StationKeyHealth {
        StationKeyHealth {
            station_key_id: "secret-ish-key".to_string(),
            last_success_at: None,
            last_failure_at: Some("1000".to_string()),
            consecutive_failures: 2,
            success_count: 1,
            failure_count: 2,
            avg_latency_ms: None,
            last_error_summary: Some("Authorization header: secret".to_string()),
            cooldown_until: None,
            updated_at: "1000".to_string(),
        }
    }

    #[test]
    fn projection_keeps_durable_legacy_and_runtime_kinds_distinct() {
        let status = project_routing_protection_status(
            1_200,
            &[durable_row()],
            &[legacy_health()],
            &[CapacityProtectionFact {
                scope: "v1:deadbeef".to_string(),
                state: "half_open".to_string(),
                cooldown_until_ms: Some(1_400),
                recent_failure_code: Some("capacity_exhausted".to_string()),
                updated_at_ms: Some(1_100),
            }],
            true,
        );

        assert_eq!(
            status.read_model_status,
            ProtectionReadModelStatus::Available
        );
        assert_eq!(status.entries.len(), 3);
        assert!(status.entries.iter().any(|entry| {
            entry.persistence_kind == Some(ProtectionPersistenceKind::Durable)
                && entry.state == ProtectionState::Cooldown
                && entry.cooldown_remaining_ms == Some(300)
        }));
        assert!(status.entries.iter().any(|entry| {
            entry.persistence_kind == Some(ProtectionPersistenceKind::LegacyCompatibility)
                && entry.recent_failure_code.as_deref() == Some("legacy_failure")
        }));
        assert!(status.entries.iter().any(|entry| {
            entry.persistence_kind == Some(ProtectionPersistenceKind::RuntimeCapacity)
                && entry.state == ProtectionState::HalfOpen
        }));
        assert!(status
            .entries
            .iter()
            .all(|entry| !entry.scope.contains("secret-ish-key")));
    }

    #[test]
    fn no_protection_is_explicit_and_runtime_restart_is_unavailable() {
        let available = project_routing_protection_status(100, &[], &[], &[], true);
        assert_eq!(available.entries[0].state, ProtectionState::NoProtection);
        assert_eq!(available.entries[0].persistence_kind, None);

        let unavailable = project_routing_protection_status(100, &[], &[], &[], false);
        assert_eq!(unavailable.entries[0].state, ProtectionState::Unavailable);
        assert_eq!(
            unavailable.entries[0].persistence_kind,
            Some(ProtectionPersistenceKind::RuntimeCapacity)
        );
    }

    #[test]
    fn malformed_capacity_state_fails_closed_to_unavailable() {
        let status = project_routing_protection_status(
            100,
            &[],
            &[],
            &[CapacityProtectionFact {
                scope: "v1:abc".to_string(),
                state: "future_state".to_string(),
                cooldown_until_ms: None,
                recent_failure_code: None,
                updated_at_ms: None,
            }],
            true,
        );
        assert_eq!(status.entries[0].state, ProtectionState::Unavailable);
        assert!(!status.entries[0].detail_available);
    }

    #[test]
    fn reducer_only_scope_is_projected_without_a_legacy_verdict_row() {
        let scope = crate::application::health_protection::HealthProtectionScope::new(
            crate::application::health_protection::HealthProtectionScopeKind::Endpoint,
            "a".repeat(64),
        )
        .expect("valid committed scope");
        let reducer_status = crate::application::health_protection::HealthProtectionStatus {
            version: crate::application::health_protection::HEALTH_PROTECTION_VERSION.to_string(),
            scope,
            state: crate::application::health_protection::HealthProtectionState::Open,
            persistence_kind:
                crate::application::health_protection::HealthProtectionPersistenceKind::Durable,
            state_revision: 2,
            opened_at_ms: Some(10),
            cooldown_until_ms: Some(1_000),
            cooldown_remaining_ms: Some(900),
            half_open_probe_in_flight: false,
            recent_failure_code: Some(
                crate::application::health_protection::HealthProtectionFailureCode::EndpointUnavailable,
            ),
            sample_count: 5,
            failure_rate_percent: 100,
            updated_at_ms: 100,
            detail_available: true,
        };

        let projected = project_routing_protection_status_with_reducer(
            100,
            &[],
            &[],
            &[],
            true,
            &[reducer_status],
        );
        assert_eq!(projected.entries.len(), 1);
        let entry = &projected.entries[0];
        assert_eq!(entry.state, ProtectionState::Open);
        assert_eq!(
            entry.persistence_kind,
            Some(ProtectionPersistenceKind::Durable)
        );
        assert_eq!(entry.scope_kind.as_deref(), Some("endpoint"));
        assert_eq!(
            entry.recent_failure_code.as_deref(),
            Some("endpoint_unavailable")
        );
        assert_eq!(entry.cooldown_remaining_ms, Some(900));
        assert!(!entry.scope.contains("https://"));
    }

    #[test]
    fn reducer_closed_projects_as_degraded_with_monitoring_explanation() {
        let scope = crate::application::health_protection::HealthProtectionScope::new(
            crate::application::health_protection::HealthProtectionScopeKind::Endpoint,
            "b".repeat(64),
        )
        .expect("valid committed scope");
        let reducer_status = crate::application::health_protection::HealthProtectionStatus {
            version: crate::application::health_protection::HEALTH_PROTECTION_VERSION.to_string(),
            scope,
            state: crate::application::health_protection::HealthProtectionState::Closed,
            persistence_kind:
                crate::application::health_protection::HealthProtectionPersistenceKind::Durable,
            state_revision: 3,
            opened_at_ms: None,
            cooldown_until_ms: None,
            cooldown_remaining_ms: None,
            half_open_probe_in_flight: false,
            recent_failure_code: Some(
                crate::application::health_protection::HealthProtectionFailureCode::Upstream5xx,
            ),
            sample_count: 4,
            failure_rate_percent: 25,
            updated_at_ms: 200,
            detail_available: true,
        };

        let projected = project_routing_protection_status_with_reducer(
            200,
            &[],
            &[],
            &[],
            true,
            &[reducer_status],
        );
        let entry = &projected.entries[0];
        assert_eq!(entry.state, ProtectionState::Degraded);
        assert_eq!(
            entry.explanation_key,
            "routing.protection.closed_monitoring"
        );
        assert!(entry.cooldown_until_ms.is_none());
    }

    #[test]
    fn reducer_projection_keeps_state_and_explanation_key_in_lockstep() {
        assert_eq!(
            reducer_projection(
                crate::application::health_protection::HealthProtectionState::Closed
            ),
            (
                ProtectionState::Degraded,
                "routing.protection.closed_monitoring"
            )
        );
        assert_eq!(
            reducer_projection(crate::application::health_protection::HealthProtectionState::Open),
            (ProtectionState::Open, "routing.protection.open")
        );
        assert_eq!(
            reducer_projection(
                crate::application::health_protection::HealthProtectionState::HalfOpen
            ),
            (ProtectionState::HalfOpen, "routing.protection.half_open")
        );
    }

    #[test]
    fn no_protection_is_only_emitted_when_no_observation_source_has_an_entry() {
        let scope = crate::application::health_protection::HealthProtectionScope::new(
            crate::application::health_protection::HealthProtectionScopeKind::Endpoint,
            "c".repeat(64),
        )
        .expect("valid committed scope");
        let closed = crate::application::health_protection::HealthProtectionStatus {
            version: crate::application::health_protection::HEALTH_PROTECTION_VERSION.to_string(),
            scope,
            state: crate::application::health_protection::HealthProtectionState::Closed,
            persistence_kind:
                crate::application::health_protection::HealthProtectionPersistenceKind::Durable,
            state_revision: 1,
            opened_at_ms: None,
            cooldown_until_ms: None,
            cooldown_remaining_ms: None,
            half_open_probe_in_flight: false,
            recent_failure_code: None,
            sample_count: 1,
            failure_rate_percent: 0,
            updated_at_ms: 1,
            detail_available: true,
        };
        let projected =
            project_routing_protection_status_with_reducer(1, &[], &[], &[], true, &[closed]);
        assert_eq!(projected.entries.len(), 1);
        assert_ne!(projected.entries[0].state, ProtectionState::NoProtection);
        assert_eq!(
            projected.entries[0].explanation_key,
            "routing.protection.closed_monitoring"
        );
    }

    #[test]
    fn runtime_outlier_reducer_state_is_not_exposed_as_durable_protection() {
        let scope = crate::application::health_protection::HealthProtectionScope::new(
            crate::application::health_protection::HealthProtectionScopeKind::Endpoint,
            "d".repeat(64),
        )
        .expect("valid committed scope");
        let runtime_status = crate::application::health_protection::HealthProtectionStatus {
            version: crate::application::health_protection::HEALTH_PROTECTION_VERSION.to_string(),
            scope,
            state: crate::application::health_protection::HealthProtectionState::Open,
            persistence_kind:
                crate::application::health_protection::HealthProtectionPersistenceKind::RuntimeOutlier,
            state_revision: 1,
            opened_at_ms: Some(1),
            cooldown_until_ms: Some(2),
            cooldown_remaining_ms: Some(1),
            half_open_probe_in_flight: false,
            recent_failure_code: None,
            sample_count: 1,
            failure_rate_percent: 100,
            updated_at_ms: 1,
            detail_available: true,
        };
        let projected = project_routing_protection_status_with_reducer(
            1,
            &[],
            &[],
            &[],
            true,
            &[runtime_status],
        );
        assert_eq!(projected.entries[0].state, ProtectionState::NoProtection);
        assert_eq!(
            projected.entries[0].explanation_key,
            "routing.protection.none_active"
        );
    }

    #[test]
    fn failure_domain_diagnostics_aggregate_candidates_and_join_protection() {
        let domain = ProviderCapacityDomain::from_trusted_identity(
            "OpenAI",
            "gpt-test",
            Some("primary"),
            Some("us"),
        )
        .expect("valid domain");
        let commitment = domain.commitment();
        let commitment = format!("v{}:{}", commitment.schema_version, commitment.digest_hex);
        let status = project_routing_protection_status_with_reducer_and_domains(
            10,
            &[],
            &[],
            &[CapacityProtectionFact {
                scope: commitment.clone(),
                state: "open".to_string(),
                cooldown_until_ms: None,
                recent_failure_code: Some("capacity_exhausted".to_string()),
                updated_at_ms: Some(10),
            }],
            true,
            &[],
            &[
                FailureDomainCandidateFact {
                    provider_family: Some("OpenAI".to_string()),
                    deployment_identity: Some("primary".to_string()),
                    region_identity: Some("us".to_string()),
                    revision: Some(3),
                    schedulable: true,
                },
                FailureDomainCandidateFact {
                    provider_family: Some(" openai ".to_string()),
                    deployment_identity: Some("PRIMARY".to_string()),
                    region_identity: Some("US".to_string()),
                    revision: Some(3),
                    schedulable: false,
                },
            ],
            Some("gpt-test"),
        );

        assert_eq!(status.failure_domains.len(), 1);
        let diagnostic = &status.failure_domains[0];
        assert_eq!(diagnostic.commitment.as_deref(), Some(commitment.as_str()));
        assert_eq!(diagnostic.resolution, "resolved");
        assert_eq!(diagnostic.candidate_count, 2);
        assert_eq!(diagnostic.schedulable_candidate_count, 1);
        assert_eq!(diagnostic.status, ProtectionState::Open);
        assert_eq!(
            diagnostic.recent_failure_code.as_deref(),
            Some("capacity_exhausted")
        );
        assert_eq!(diagnostic.explanation_key, "routing.protection.open");
    }

    #[test]
    fn unresolved_domain_diagnostics_are_visible_without_guessing_commitment() {
        let status = project_routing_protection_status_with_reducer_and_domains(
            10,
            &[],
            &[],
            &[],
            true,
            &[],
            &[
                FailureDomainCandidateFact {
                    provider_family: Some("OpenAI".to_string()),
                    deployment_identity: None,
                    region_identity: None,
                    revision: Some(1),
                    schedulable: true,
                },
                FailureDomainCandidateFact {
                    provider_family: None,
                    deployment_identity: None,
                    region_identity: None,
                    revision: None,
                    schedulable: false,
                },
            ],
            None,
        );

        assert_eq!(status.failure_domains.len(), 2);
        let model_required = status
            .failure_domains
            .iter()
            .find(|domain| domain.resolution == "model_required")
            .expect("model-required domain");
        assert!(model_required.commitment.is_none());
        assert_eq!(model_required.candidate_count, 1);
        assert_eq!(
            model_required.explanation_key,
            "routing.failure_domain.model_required"
        );
        let not_configured = status
            .failure_domains
            .iter()
            .find(|domain| domain.resolution == "not_configured")
            .expect("not-configured domain");
        assert_eq!(
            not_configured.explanation_key,
            "routing.failure_domain.not_configured"
        );
        assert!(status
            .failure_domains
            .iter()
            .all(|domain| domain.commitment.is_none()));
    }

    #[test]
    fn domain_diagnostic_serialization_is_bounded_and_secret_free() {
        let status = project_routing_protection_status_with_reducer_and_domains(
            10,
            &[],
            &[],
            &[],
            true,
            &[],
            &[FailureDomainCandidateFact {
                provider_family: Some("openai".to_string()),
                deployment_identity: Some("primary".to_string()),
                region_identity: Some("us".to_string()),
                revision: Some(1),
                schedulable: true,
            }],
            Some("gpt-test"),
        );
        let serialized = serde_json::to_string(&status).expect("serialize status");
        assert!(serialized.contains("failureDomains"));
        assert!(serialized.contains("candidateCount"));
        assert!(!serialized.contains("api_key"));
        assert!(!serialized.contains("Authorization"));
        assert!(!serialized.contains("https://"));
    }
}
