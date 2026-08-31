use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RoutingGenerationStatus {
    Building,
    Ready,
    CutoverFencing,
    Active,
    Retired,
    Failed,
}

impl RoutingGenerationStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Building => "building",
            Self::Ready => "ready",
            Self::CutoverFencing => "cutover_fencing",
            Self::Active => "active",
            Self::Retired => "retired",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "building" => Some(Self::Building),
            "ready" => Some(Self::Ready),
            "cutover_fencing" => Some(Self::CutoverFencing),
            "active" => Some(Self::Active),
            "retired" => Some(Self::Retired),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingCutoverMode {
    PreCutover,
    V3Active,
}

impl RoutingCutoverMode {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=v3-cutover-mode-serialization; owner=models/routing_generation; remove_when=cutover mode is emitted only through the generated status DTO"
        )
    )]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::PreCutover => "pre_cutover",
            Self::V3Active => "v3_active",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "pre_cutover" => Some(Self::PreCutover),
            "v3_active" => Some(Self::V3Active),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingRuntimeGeneration {
    pub(crate) runtime_generation_id: String,
    pub(crate) policy_generation_id: String,
    pub(crate) quality_generation_id: String,
    pub(crate) circuit_generation_id: String,
    pub(crate) policy_revision: u64,
    pub(crate) quality_policy_revision: u64,
    pub(crate) circuit_policy_revision: u64,
    pub(crate) algorithm_version: String,
    pub(crate) status: RoutingGenerationStatus,
    pub(crate) input_observation_watermark: u64,
    pub(crate) input_circuit_event_watermark: u64,
    pub(crate) policy_input_hash: String,
    pub(crate) quality_input_hash: String,
    pub(crate) circuit_input_hash: String,
    pub(crate) policy_content_hash: String,
    pub(crate) quality_content_hash: String,
    pub(crate) circuit_content_hash: String,
    pub(crate) checkpoint_ref: String,
    pub(crate) cutover_fence_revision: Option<u64>,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewRoutingRuntimeGeneration {
    pub(crate) runtime_generation_id: String,
    pub(crate) policy_generation_id: String,
    pub(crate) quality_generation_id: String,
    pub(crate) circuit_generation_id: String,
    pub(crate) policy_revision: u64,
    pub(crate) quality_policy_revision: u64,
    pub(crate) circuit_policy_revision: u64,
    pub(crate) algorithm_version: String,
    pub(crate) input_observation_watermark: u64,
    pub(crate) input_circuit_event_watermark: u64,
    pub(crate) policy_input_hash: String,
    pub(crate) quality_input_hash: String,
    pub(crate) circuit_input_hash: String,
    pub(crate) policy_content_hash: String,
    pub(crate) quality_content_hash: String,
    pub(crate) circuit_content_hash: String,
    pub(crate) checkpoint_ref: String,
    pub(crate) policy_checkpoint_ref: String,
    pub(crate) quality_checkpoint_ref: String,
    pub(crate) circuit_checkpoint_ref: String,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingGenerationMarker {
    pub(crate) mode: RoutingCutoverMode,
    pub(crate) active_runtime_generation_id: Option<String>,
    pub(crate) fenced_runtime_generation_id: Option<String>,
    pub(crate) fence_revision: u64,
    pub(crate) updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingGenerationRegistrySnapshot {
    pub(crate) marker: RoutingGenerationMarker,
    pub(crate) active: Option<RoutingRuntimeGeneration>,
    pub(crate) fencing: Option<RoutingRuntimeGeneration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingGenerationEligibility {
    Active,
    Next,
}

impl RoutingGenerationEligibility {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Next => "next",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingGenerationIngestionFence {
    pub(crate) eligibility: RoutingGenerationEligibility,
    pub(crate) active_runtime_generation_id: Option<String>,
    pub(crate) fence_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingGenerationAdmissionGuard {
    pub(crate) active_runtime_generation_id: Option<String>,
    pub(crate) fence_revision: u64,
    pub(crate) fencing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingGenerationFence {
    pub(crate) source_runtime_generation_id: Option<String>,
    pub(crate) target_runtime_generation_id: String,
    pub(crate) fence_revision: u64,
}

pub(crate) const ROUTING_GENERATION_QUALIFICATION_VERSION: &str =
    "routing-generation-qualification-v2";

pub(crate) fn qualification_reports_are_activation_ready(
    runtime_generation_id: &str,
    comparison: &serde_json::Value,
    replay: &serde_json::Value,
) -> bool {
    let Some(comparison_object) = comparison.as_object() else {
        return false;
    };
    let Some(replay_object) = replay.as_object() else {
        return false;
    };
    if comparison_object
        .get("report_version")
        .and_then(serde_json::Value::as_str)
        != Some("routing-generation-comparison-report-v2")
        || replay_object
            .get("report_version")
            .and_then(serde_json::Value::as_str)
            != Some("routing-generation-replay-report-v2")
        || comparison_object
            .get("runtime_generation_id")
            .and_then(serde_json::Value::as_str)
            != Some(runtime_generation_id)
        || replay_object
            .get("runtime_generation_id")
            .and_then(serde_json::Value::as_str)
            != Some(runtime_generation_id)
        || comparison_object
            .get("score_basis")
            .and_then(serde_json::Value::as_str)
            != Some("reliability_and_responsiveness_available_factors_renormalized")
    {
        return false;
    }
    let Some(keys) = comparison_object
        .get("keys")
        .and_then(serde_json::Value::as_array)
    else {
        return false;
    };
    if comparison_object
        .get("key_count")
        .and_then(serde_json::Value::as_u64)
        != Some(keys.len() as u64)
        || keys.iter().any(|key| !valid_key_comparison(key))
    {
        return false;
    }
    let computed_rank_changes = keys
        .iter()
        .filter(|key| {
            key.get("rank_delta")
                .and_then(serde_json::Value::as_i64)
                .is_some_and(|delta| delta != 0)
        })
        .count() as u64;
    if comparison_object
        .get("rank_change_count")
        .and_then(serde_json::Value::as_u64)
        != Some(computed_rank_changes)
    {
        return false;
    }
    let Some(fixtures) = replay_object
        .get("semantic_fixtures")
        .and_then(serde_json::Value::as_array)
    else {
        return false;
    };
    [429_u64, 502_u64].into_iter().all(|status| {
        fixtures.iter().any(|fixture| {
            fixture
                .get("http_status")
                .and_then(serde_json::Value::as_u64)
                == Some(status)
                && fixture
                    .get("failure_sample")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                && fixture
                    .get("retry_next_key")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                && fixture
                    .get("retry_after_ignored")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                && fixture
                    .get("station_key_circuit_opened")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                && fixture.get("passed").and_then(serde_json::Value::as_bool) == Some(true)
                && fixture
                    .get("consecutive_failures")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|count| count > 0)
        })
    })
}

fn valid_key_comparison(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let valid_commitment = object
        .get("key_commitment")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|commitment| {
            commitment.len() == 38
                && commitment.starts_with("keyc1_")
                && commitment[6..].bytes().all(|byte| byte.is_ascii_hexdigit())
        });
    valid_commitment
        && object.get("station_key_id").is_none()
        && object
            .get("source")
            .is_some_and(|metric| metric.is_null() || valid_key_metrics(metric))
        && object
            .get("target")
            .is_some_and(|metric| metric.is_null() || valid_key_metrics(metric))
        && (!object.get("source").is_some_and(serde_json::Value::is_null)
            || !object.get("target").is_some_and(serde_json::Value::is_null))
}

fn valid_key_metrics(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    [
        "reliability_basis_points",
        "qualification_score_basis_points",
        "real_source_weight_basis_points",
        "monitoring_source_weight_basis_points",
    ]
    .into_iter()
    .all(|field| {
        object
            .get(field)
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|value| value <= 10_000)
    }) && object
        .get("weighted_latency_ms")
        .and_then(serde_json::Value::as_u64)
        .is_some()
        && object
            .get("observation_count")
            .and_then(serde_json::Value::as_u64)
            .is_some()
        && object
            .get("quality_basis")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty())
        && object
            .get("circuit_state")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| matches!(value, "closed" | "open" | "half_open" | "not_present"))
        && object
            .get("rank")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|value| value > 0)
}

#[cfg(test)]
pub(crate) fn test_activation_qualification_reports(
    runtime_generation_id: &str,
) -> (serde_json::Value, serde_json::Value) {
    let comparison = serde_json::json!({
        "report_version": "routing-generation-comparison-report-v2",
        "runtime_generation_id": runtime_generation_id,
        "score_basis": "reliability_and_responsiveness_available_factors_renormalized",
        "key_count": 0,
        "rank_change_count": 0,
        "keys": []
    });
    let fixture = |name: &str, status: u16| {
        serde_json::json!({
            "fixture": name,
            "http_status": status,
            "failure_sample": true,
            "retry_next_key": true,
            "retry_after_ignored": true,
            "station_key_circuit_opened": true,
            "consecutive_failures": 3,
            "passed": true
        })
    };
    let replay = serde_json::json!({
        "report_version": "routing-generation-replay-report-v2",
        "runtime_generation_id": runtime_generation_id,
        "semantic_fixtures": [fixture("tntapi_429", 429), fixture("tntapi_502", 502)]
    });
    (comparison, replay)
}

#[cfg(test)]
mod qualification_tests {
    use super::*;

    #[test]
    fn activation_rejects_weak_or_identity_leaking_qualification_reports() {
        let runtime_generation_id = "rg1_qualification-test";
        let (comparison, replay) = test_activation_qualification_reports(runtime_generation_id);
        assert!(qualification_reports_are_activation_ready(
            runtime_generation_id,
            &comparison,
            &replay
        ));

        let mut missing_429 = replay.clone();
        missing_429["semantic_fixtures"] =
            serde_json::json!([replay["semantic_fixtures"][1].clone()]);
        assert!(!qualification_reports_are_activation_ready(
            runtime_generation_id,
            &comparison,
            &missing_429
        ));

        let mut failed_502 = replay.clone();
        failed_502["semantic_fixtures"][1]["passed"] = serde_json::Value::Bool(false);
        assert!(!qualification_reports_are_activation_ready(
            runtime_generation_id,
            &comparison,
            &failed_502
        ));

        let mut leaking_comparison = comparison.clone();
        leaking_comparison["key_count"] = serde_json::json!(1);
        leaking_comparison["keys"] = serde_json::json!([{
            "key_commitment": format!("keyc1_{}", "a".repeat(32)),
            "station_key_id": "must-not-appear",
            "source": null,
            "target": {
                "reliability_basis_points": 9500,
                "weighted_latency_ms": 2500,
                "qualification_score_basis_points": 9500,
                "observation_count": 0,
                "real_source_weight_basis_points": 7000,
                "monitoring_source_weight_basis_points": 3000,
                "quality_basis": "optimistic",
                "circuit_state": "closed",
                "rank": 1
            },
            "score_delta_basis_points": null,
            "rank_delta": null
        }]);
        assert!(!qualification_reports_are_activation_ready(
            runtime_generation_id,
            &leaking_comparison,
            &replay
        ));

        let mut per_key_comparison = comparison.clone();
        per_key_comparison["key_count"] = serde_json::json!(1);
        per_key_comparison["keys"] = serde_json::json!([{
            "key_commitment": format!("keyc1_{}", "b".repeat(32)),
            "source": null,
            "target": {
                "reliability_basis_points": 9500,
                "weighted_latency_ms": 2500,
                "qualification_score_basis_points": 9500,
                "observation_count": 15,
                "real_source_weight_basis_points": 7000,
                "monitoring_source_weight_basis_points": 3000,
                "quality_basis": "observed",
                "circuit_state": "closed",
                "rank": 1
            },
            "score_delta_basis_points": null,
            "rank_delta": null
        }]);
        assert!(qualification_reports_are_activation_ready(
            runtime_generation_id,
            &per_key_comparison,
            &replay
        ));
        per_key_comparison["keys"][0]["target"]
            .as_object_mut()
            .expect("target metrics")
            .remove("weighted_latency_ms");
        assert!(!qualification_reports_are_activation_ready(
            runtime_generation_id,
            &per_key_comparison,
            &replay
        ));
    }

    #[test]
    fn activation_rejects_mismatched_runtime_and_rank_counts() {
        let runtime_generation_id = "rg1_qualification-test";
        let (mut comparison, replay) = test_activation_qualification_reports(runtime_generation_id);
        comparison["runtime_generation_id"] = serde_json::json!("rg1_other");
        assert!(!qualification_reports_are_activation_ready(
            runtime_generation_id,
            &comparison,
            &replay
        ));

        let (mut comparison, replay) = test_activation_qualification_reports(runtime_generation_id);
        comparison["rank_change_count"] = serde_json::json!(1);
        assert!(!qualification_reports_are_activation_ready(
            runtime_generation_id,
            &comparison,
            &replay
        ));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingGenerationQualification {
    pub(crate) runtime_generation_id: String,
    pub(crate) comparison_report_hash: String,
    pub(crate) comparison_report: serde_json::Value,
    pub(crate) replay_report_hash: String,
    pub(crate) replay_report: serde_json::Value,
    pub(crate) qualified_at_ms: i64,
}
