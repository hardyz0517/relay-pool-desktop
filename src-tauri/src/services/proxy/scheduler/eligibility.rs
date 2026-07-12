use crate::models::routing::{RouteEndpointKind, RoutingGroupFilter};
use crate::services::proxy::scheduler::types::{
    CandidateRejection, CandidateRejectionCode, MultiplierRejectReason, ScheduleRequest,
    SchedulerCandidate,
};

pub fn group_matches(filter: &RoutingGroupFilter, candidate: &SchedulerCandidate) -> bool {
    match filter {
        RoutingGroupFilter::AllGroups => true,
        RoutingGroupFilter::UngroupedOnly => {
            candidate.group_binding_id.is_none() && candidate.group_id_hash.is_none()
        }
        RoutingGroupFilter::GroupBindingId(expected) => {
            candidate.group_binding_id.as_deref() == Some(expected.as_str())
        }
        RoutingGroupFilter::GroupIdHash(expected) => {
            candidate.group_id_hash.as_deref() == Some(expected.as_str())
        }
        RoutingGroupFilter::GroupType(expected) => candidate.group_type.as_ref() == Some(expected),
    }
}

pub fn evaluate_candidate(
    request: &ScheduleRequest,
    candidate: &SchedulerCandidate,
) -> Result<(), CandidateRejection> {
    let mut reasons = Vec::new();

    if !candidate.station_enabled
        || !candidate.key_enabled
        || !candidate.schedulable
        || request
            .excluded_key_ids
            .iter()
            .any(|key_id| key_id == &candidate.station_key_id)
    {
        reasons.push(CandidateRejectionCode::AssetUnavailable);
    }

    if !group_matches(&request.routing_group_filter, candidate) {
        reasons.push(CandidateRejectionCode::RoutingGroupMismatch);
    }

    if capability_mismatch(request, candidate) {
        reasons.push(CandidateRejectionCode::CapabilityMismatch);
    }

    if model_mismatch(request, candidate) {
        reasons.push(CandidateRejectionCode::ModelMismatch);
    }

    if candidate.health_blocked {
        reasons.push(CandidateRejectionCode::HealthBlocked);
    }

    if candidate.balance_depleted {
        reasons.push(CandidateRejectionCode::BalanceDepleted);
    }

    match &candidate.effective_multiplier {
        Some(fact) if fact.value > request.max_rate_multiplier => {
            reasons.push(CandidateRejectionCode::MultiplierOverCeiling);
        }
        Some(_) => {}
        None => reasons.push(multiplier_rejection_code(
            candidate.multiplier_reject_reason,
        )),
    }

    if let Some(primary_code) = reasons.first().copied() {
        Err(CandidateRejection {
            primary_code,
            reasons,
        })
    } else {
        Ok(())
    }
}

fn multiplier_rejection_code(reason: Option<MultiplierRejectReason>) -> CandidateRejectionCode {
    match reason {
        Some(MultiplierRejectReason::Invalid) => CandidateRejectionCode::MultiplierEvidenceInvalid,
        Some(MultiplierRejectReason::Negative) => {
            CandidateRejectionCode::MultiplierEvidenceNegative
        }
        Some(MultiplierRejectReason::Expired) => CandidateRejectionCode::MultiplierEvidenceExpired,
        Some(MultiplierRejectReason::UnboundGroup) => {
            CandidateRejectionCode::MultiplierEvidenceUnboundGroup
        }
        Some(MultiplierRejectReason::LowConfidence) => {
            CandidateRejectionCode::MultiplierEvidenceLowConfidence
        }
        Some(MultiplierRejectReason::Missing) | None => {
            CandidateRejectionCode::NoMultiplierEvidence
        }
    }
}

fn capability_mismatch(request: &ScheduleRequest, candidate: &SchedulerCandidate) -> bool {
    (match request.endpoint {
        RouteEndpointKind::Models => false,
        RouteEndpointKind::ChatCompletions => !candidate.supports_chat_completions,
        RouteEndpointKind::Responses => !candidate.supports_responses,
        RouteEndpointKind::Embeddings => !candidate.supports_embeddings,
    }) || (request.stream && !candidate.supports_stream)
        || (request.uses_tools && !candidate.supports_tools)
        || (request.uses_vision && !candidate.supports_vision)
        || (request.uses_reasoning && !candidate.supports_reasoning)
}

fn model_mismatch(request: &ScheduleRequest, candidate: &SchedulerCandidate) -> bool {
    let Some(model) = request
        .mapped_model
        .as_deref()
        .or(request.requested_model.as_deref())
        .map(normalize_model)
    else {
        return false;
    };

    if candidate
        .model_blocklist
        .iter()
        .map(|item| normalize_model(item))
        .any(|blocked| blocked == model)
    {
        return true;
    }

    !candidate.model_allowlist.is_empty()
        && !candidate
            .model_allowlist
            .iter()
            .map(|item| normalize_model(item))
            .any(|allowed| allowed == model)
}

fn normalize_model(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::routing::{PricingGroupType, RouteEndpointKind, RoutingGroupFilter};
    use crate::services::proxy::scheduler::types::{
        CandidateRejectionCode, EffectiveMultiplierFact, ScheduleRequest, SchedulerCandidate,
    };

    #[test]
    fn group_type_filter_rejects_out_of_group_cheap_candidate_before_budget() {
        let request = request(|request| {
            request.routing_group_filter = RoutingGroupFilter::GroupType(PricingGroupType::Gpt);
            request.max_rate_multiplier = 1.0;
        });
        let candidate = candidate(|candidate| {
            candidate.group_type = Some(PricingGroupType::Claude);
            candidate.effective_multiplier = Some(multiplier(0.01));
        });

        let rejection = evaluate_candidate(&request, &candidate)
            .expect_err("out-of-group candidate should reject");

        assert_eq!(
            rejection.primary_code,
            CandidateRejectionCode::RoutingGroupMismatch
        );
        assert!(rejection
            .reasons
            .contains(&CandidateRejectionCode::RoutingGroupMismatch));
        assert!(!rejection
            .reasons
            .contains(&CandidateRejectionCode::MultiplierOverCeiling));
    }

    #[test]
    fn over_ceiling_matching_group_rejects() {
        let request = request(|request| {
            request.routing_group_filter =
                RoutingGroupFilter::GroupBindingId("binding-pro".to_string());
            request.max_rate_multiplier = 1.0;
        });
        let candidate = candidate(|candidate| {
            candidate.group_binding_id = Some("binding-pro".to_string());
            candidate.effective_multiplier = Some(multiplier(1.01));
        });

        let rejection =
            evaluate_candidate(&request, &candidate).expect_err("over-ceiling candidate rejects");

        assert_eq!(
            rejection.primary_code,
            CandidateRejectionCode::MultiplierOverCeiling
        );
    }

    #[test]
    fn all_groups_admits_grouped_and_ungrouped_candidates_when_other_gates_pass() {
        let request = request(|request| {
            request.routing_group_filter = RoutingGroupFilter::AllGroups;
        });
        let grouped = candidate(|candidate| {
            candidate.group_binding_id = Some("binding-pro".to_string());
            candidate.group_id_hash = Some("hash-pro".to_string());
        });
        let ungrouped = candidate(|candidate| {
            candidate.group_binding_id = None;
            candidate.group_id_hash = None;
        });

        assert!(group_matches(&request.routing_group_filter, &grouped));
        assert!(group_matches(&request.routing_group_filter, &ungrouped));
        evaluate_candidate(&request, &grouped).expect("grouped candidate should pass");
        evaluate_candidate(&request, &ungrouped).expect("ungrouped candidate should pass");
    }

    #[test]
    fn ungrouped_only_rejects_grouped_candidate() {
        let request = request(|request| {
            request.routing_group_filter = RoutingGroupFilter::UngroupedOnly;
        });
        let grouped = candidate(|candidate| {
            candidate.group_binding_id = Some("binding-pro".to_string());
        });

        let rejection = evaluate_candidate(&request, &grouped)
            .expect_err("grouped candidate should reject for ungrouped-only filter");

        assert_eq!(
            rejection.primary_code,
            CandidateRejectionCode::RoutingGroupMismatch
        );
    }

    #[test]
    fn group_id_hash_filter_matches_exact_hash_only() {
        let request = request(|request| {
            request.routing_group_filter = RoutingGroupFilter::GroupIdHash("hash-pro".to_string());
        });
        let matching = candidate(|candidate| {
            candidate.group_id_hash = Some("hash-pro".to_string());
        });
        let different = candidate(|candidate| {
            candidate.group_id_hash = Some("hash-pro-extra".to_string());
        });

        assert!(group_matches(&request.routing_group_filter, &matching));
        evaluate_candidate(&request, &matching).expect("exact hash candidate should pass");

        let rejection = evaluate_candidate(&request, &different)
            .expect_err("different hash candidate should reject");
        assert_eq!(
            rejection.primary_code,
            CandidateRejectionCode::RoutingGroupMismatch
        );
    }

    #[test]
    fn individual_candidate_gates_reject_with_expected_primary_reason() {
        assert_rejection_code(
            |request| {
                request.excluded_key_ids = vec!["key-1".to_string()];
            },
            |_| {},
            CandidateRejectionCode::AssetUnavailable,
        );
        assert_rejection_code(
            |request| {
                request.endpoint = RouteEndpointKind::Responses;
                request.stream = true;
                request.uses_tools = true;
            },
            |candidate| {
                candidate.supports_responses = false;
                candidate.supports_stream = false;
                candidate.supports_tools = false;
            },
            CandidateRejectionCode::CapabilityMismatch,
        );
        assert_rejection_code(
            |request| {
                request.mapped_model = Some("gpt-4o".to_string());
            },
            |candidate| {
                candidate.model_allowlist = vec!["claude-3-5-sonnet".to_string()];
            },
            CandidateRejectionCode::ModelMismatch,
        );
        assert_rejection_code(
            |_| {},
            |candidate| {
                candidate.health_blocked = true;
            },
            CandidateRejectionCode::HealthBlocked,
        );
        assert_rejection_code(
            |_| {},
            |candidate| {
                candidate.balance_depleted = true;
            },
            CandidateRejectionCode::BalanceDepleted,
        );
    }

    #[test]
    fn primary_reason_is_first_reason_and_all_simultaneous_failures_are_reported() {
        let request = request(|request| {
            request.routing_group_filter = RoutingGroupFilter::GroupType(PricingGroupType::Gpt);
            request.endpoint = RouteEndpointKind::Responses;
            request.mapped_model = Some("gpt-4o".to_string());
        });
        let candidate = candidate(|candidate| {
            candidate.station_enabled = false;
            candidate.group_type = Some(PricingGroupType::Claude);
            candidate.supports_responses = false;
            candidate.model_blocklist = vec!["gpt-4o".to_string()];
            candidate.health_blocked = true;
            candidate.balance_depleted = true;
            candidate.effective_multiplier = None;
        });

        let rejection = evaluate_candidate(&request, &candidate)
            .expect_err("candidate with multiple gate failures should reject");

        assert_eq!(rejection.primary_code, rejection.reasons[0]);
        assert_eq!(
            rejection.reasons,
            vec![
                CandidateRejectionCode::AssetUnavailable,
                CandidateRejectionCode::RoutingGroupMismatch,
                CandidateRejectionCode::CapabilityMismatch,
                CandidateRejectionCode::ModelMismatch,
                CandidateRejectionCode::HealthBlocked,
                CandidateRejectionCode::BalanceDepleted,
                CandidateRejectionCode::NoMultiplierEvidence,
            ]
        );
    }

    #[test]
    fn missing_multiplier_evidence_rejects() {
        let request = request(|request| {
            request.routing_group_filter = RoutingGroupFilter::AllGroups;
        });
        let candidate = candidate(|candidate| {
            candidate.effective_multiplier = None;
        });

        let rejection = evaluate_candidate(&request, &candidate)
            .expect_err("missing multiplier evidence should reject");

        assert_eq!(
            rejection.primary_code,
            CandidateRejectionCode::NoMultiplierEvidence
        );
    }

    #[test]
    fn expired_multiplier_evidence_preserves_specific_rejection_code() {
        let request = request(|request| {
            request.routing_group_filter = RoutingGroupFilter::AllGroups;
        });
        let candidate = candidate(|candidate| {
            candidate.effective_multiplier = None;
            candidate.multiplier_reject_reason = Some(MultiplierRejectReason::Expired);
        });

        let rejection = evaluate_candidate(&request, &candidate)
            .expect_err("expired multiplier evidence should reject");

        assert_eq!(
            rejection.primary_code,
            CandidateRejectionCode::MultiplierEvidenceExpired
        );
    }

    fn request(mut customize: impl FnMut(&mut ScheduleRequest)) -> ScheduleRequest {
        let mut request = ScheduleRequest {
            endpoint: RouteEndpointKind::ChatCompletions,
            requested_model: Some("gpt-4o".to_string()),
            mapped_model: Some("gpt-4o".to_string()),
            routing_group_filter: RoutingGroupFilter::AllGroups,
            stream: false,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            max_rate_multiplier: 1.0,
            session_hash: None,
            previous_response_id: None,
            excluded_key_ids: Vec::new(),
            now_ms: 1_000_000,
        };
        customize(&mut request);
        request
    }

    fn candidate(mut customize: impl FnMut(&mut SchedulerCandidate)) -> SchedulerCandidate {
        let mut candidate = SchedulerCandidate {
            station_key_id: "key-1".to_string(),
            station_id: "station-1".to_string(),
            priority: 0,
            max_concurrency: 0,
            load_factor: None,
            group_binding_id: None,
            group_id_hash: None,
            group_type: None,
            station_enabled: true,
            key_enabled: true,
            schedulable: true,
            supports_chat_completions: true,
            supports_responses: true,
            supports_embeddings: true,
            supports_stream: true,
            supports_tools: true,
            supports_vision: true,
            supports_reasoning: true,
            model_allowlist: Vec::new(),
            model_blocklist: Vec::new(),
            health_blocked: false,
            balance_depleted: false,
            effective_multiplier: Some(multiplier(0.5)),
            multiplier_reject_reason: None,
        };
        customize(&mut candidate);
        candidate
    }

    fn multiplier(value: f64) -> EffectiveMultiplierFact {
        EffectiveMultiplierFact {
            station_key_id: "key-1".to_string(),
            value,
            source: "test".to_string(),
            collected_at_ms: Some(1),
            valid_until_ms: Some(2),
            confidence: 1.0,
            group_binding_id: None,
        }
    }

    fn assert_rejection_code(
        customize_request: impl FnMut(&mut ScheduleRequest),
        customize_candidate: impl FnMut(&mut SchedulerCandidate),
        expected: CandidateRejectionCode,
    ) {
        let request = request(customize_request);
        let candidate = candidate(customize_candidate);

        let rejection =
            evaluate_candidate(&request, &candidate).expect_err("candidate should reject");
        assert_eq!(rejection.primary_code, expected);
        assert_eq!(rejection.reasons, vec![expected]);
    }
}
