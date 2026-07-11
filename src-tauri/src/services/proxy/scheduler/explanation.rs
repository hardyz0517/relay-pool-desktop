use crate::{
    models::routing::RoutingGroupFilter,
    services::proxy::scheduler::{
        scoring::ScoreFactors,
        types::{CandidateRejectionCode, SchedulerCandidateDecision},
    },
};

pub fn routing_group_scope_key(filter: &RoutingGroupFilter) -> String {
    match filter {
        RoutingGroupFilter::AllGroups => "all_groups".to_string(),
        RoutingGroupFilter::UngroupedOnly => "ungrouped_only".to_string(),
        RoutingGroupFilter::GroupBindingId(value) => format!("group_binding_id:{value}"),
        RoutingGroupFilter::GroupIdHash(value) => format!("group_id_hash:{value}"),
        RoutingGroupFilter::GroupType(value) => format!("group_type:{value:?}"),
    }
}

pub fn score_factor_labels(factors: &ScoreFactors) -> Vec<String> {
    vec![
        format!("multiplier={:.4}", factors.multiplier),
        format!("priority={:.4}", factors.priority),
        format!("load={:.4}", factors.load),
        format!("queue={:.4}", factors.queue),
        format!("error_rate={:.4}", factors.error_rate),
        format!("ttft={:.4}", factors.ttft),
        format!("quota_headroom={:.4}", factors.quota_headroom),
    ]
}

pub fn rejection_reason_codes(decision: &SchedulerCandidateDecision) -> Vec<String> {
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

pub fn decision_reasons(decision: &SchedulerCandidateDecision) -> Vec<String> {
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
    reasons
}

fn rejection_code_label(code: CandidateRejectionCode) -> &'static str {
    match code {
        CandidateRejectionCode::AssetUnavailable => "asset_unavailable",
        CandidateRejectionCode::RoutingGroupMismatch => "routing_group_mismatch",
        CandidateRejectionCode::CapabilityMismatch => "capability_mismatch",
        CandidateRejectionCode::ModelMismatch => "model_mismatch",
        CandidateRejectionCode::HealthBlocked => "health_blocked",
        CandidateRejectionCode::BalanceDepleted => "balance_depleted",
        CandidateRejectionCode::NoMultiplierEvidence => "no_multiplier_evidence",
        CandidateRejectionCode::MultiplierOverCeiling => "multiplier_over_ceiling",
    }
}
