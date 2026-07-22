use crate::models::routing::{RoutingGroupFilter, SchedulerAdvancedSettings};

use super::{
    scoring::ScoreFactors,
    types::{CandidateRejectionCode, SchedulerCandidateDecision},
};

pub(crate) fn routing_group_scope_key(filter: &RoutingGroupFilter) -> String {
    match filter {
        RoutingGroupFilter::AllGroups => "all_groups".to_string(),
        RoutingGroupFilter::UngroupedOnly => "ungrouped_only".to_string(),
        RoutingGroupFilter::GroupBindingId(value) => format!("group_binding_id:{value}"),
        RoutingGroupFilter::GroupIdHash(value) => format!("group_id_hash:{value}"),
        RoutingGroupFilter::GroupType(value) => format!("group_type:{value:?}"),
    }
}

pub(crate) fn score_factor_labels(
    factors: &ScoreFactors,
    weights: &SchedulerAdvancedSettings,
    base_score: f64,
) -> Vec<String> {
    vec![
        format!("multiplier={:.4}", factors.multiplier),
        format!(
            "multiplier_contribution={:.4}",
            factors.multiplier * weights.multiplier
        ),
        format!("priority={:.4}", factors.priority),
        format!(
            "priority_contribution={:.4}",
            factors.priority * weights.priority
        ),
        format!("load={:.4}", factors.load),
        format!("load_contribution={:.4}", factors.load * weights.load),
        format!("queue={:.4}", factors.queue),
        format!("queue_contribution={:.4}", factors.queue * weights.queue),
        format!("error_rate={:.4}", factors.error_rate),
        format!(
            "error_rate_contribution={:.4}",
            factors.error_rate * weights.error_rate
        ),
        format!("ttft={:.4}", factors.ttft),
        format!("ttft_contribution={:.4}", factors.ttft * weights.ttft),
        format!("quota_headroom={:.4}", factors.quota_headroom),
        format!(
            "quota_headroom_contribution={:.4}",
            factors.quota_headroom * weights.quota_headroom
        ),
        format!("base_score={base_score:.4}"),
        format!("sticky_score={base_score:.4}"),
    ]
}

pub(crate) fn rejection_reason_codes(decision: &SchedulerCandidateDecision) -> Vec<String> {
    decision
        .rejection
        .as_ref()
        .map(|rejection| {
            rejection
                .reasons
                .iter()
                .copied()
                .map(rejection_code_label)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn decision_reasons(decision: &SchedulerCandidateDecision) -> Vec<String> {
    let mut reasons = Vec::new();
    if decision.routing_group_match {
        reasons.push("routing_group_match".to_string());
    }
    if let Some(multiplier) = &decision.effective_multiplier {
        reasons.push(format!(
            "effective_multiplier={:.4} source={}",
            multiplier.value, multiplier.source
        ));
    }
    if let Some(score) = decision.score {
        reasons.push(format!("scheduler_score={score:.4}"));
    }
    if let Some(rank) = decision.top_k_rank {
        reasons.push(format!("top_k_rank={rank}"));
    }
    if let Some(slot_result) = &decision.slot_result {
        reasons.push(format!("slot_result={slot_result}"));
    }
    reasons.push(format!(
        "selection_result={}",
        if decision.accepted {
            if decision.top_k_rank == Some(1) {
                "selected"
            } else {
                "not_selected"
            }
        } else {
            "rejected"
        }
    ));
    reasons
}

pub(crate) fn rejection_code_label(code: CandidateRejectionCode) -> &'static str {
    match code {
        CandidateRejectionCode::AssetUnavailable => "asset_unavailable",
        CandidateRejectionCode::RoutingGroupMismatch => "routing_group_mismatch",
        CandidateRejectionCode::CapabilityMismatch => "capability_mismatch",
        CandidateRejectionCode::ModelMismatch => "model_mismatch",
        CandidateRejectionCode::HealthBlocked => "health_blocked",
        CandidateRejectionCode::BalanceDepleted => "balance_depleted",
        CandidateRejectionCode::NoMultiplierEvidence => "no_multiplier_evidence",
        CandidateRejectionCode::MultiplierEvidenceInvalid => "multiplier_evidence_invalid",
        CandidateRejectionCode::MultiplierEvidenceNegative => "multiplier_evidence_negative",
        CandidateRejectionCode::MultiplierEvidenceExpired => "multiplier_evidence_expired",
        CandidateRejectionCode::MultiplierEvidenceUnboundGroup => {
            "multiplier_evidence_unbound_group"
        }
        CandidateRejectionCode::MultiplierEvidenceLowConfidence => {
            "multiplier_evidence_low_confidence"
        }
        CandidateRejectionCode::MultiplierOverCeiling => "multiplier_over_ceiling",
    }
}
