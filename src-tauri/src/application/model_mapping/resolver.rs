use crate::models::model_mapping::{
    normalize_model_name, Action, ConditionRequirement, EndpointKind, FallbackTrigger,
    ModelRequestFacts, RejectionKind, TargetRef, UnmatchedModelBehavior,
};

use super::compiler::{CompiledMatcher, CompiledModelMappingConfiguration, CompiledRule};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Disposition {
    Bypass,
    Preserve,
    Mapped,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetPolicy {
    None,
    Preserve,
    Fixed,
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolutionReason {
    UnmatchedPreserve,
    RuleMatch,
    ProfileKeyBinding,
    ProfileStationBinding,
    ProfileDefault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTarget {
    pub(crate) target_rank: u16,
    pub(crate) route_model: String,
    pub(crate) resolution_reason: ResolutionReason,
    pub(crate) binding_revision: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CandidateResolutionContext<'a> {
    pub(crate) station_key_id: &'a str,
    pub(crate) station_id: &'a str,
    pub(crate) endpoint: EndpointKind,
    pub(crate) credential_revision: u64,
    pub(crate) endpoint_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateModelVariant {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) upstream_model: String,
    pub(crate) target_rank: u16,
    pub(crate) binding_revision: Option<u64>,
    pub(crate) model_resolution_fence: String,
    pub(crate) endpoint: EndpointKind,
    pub(crate) credential_revision: u64,
    pub(crate) endpoint_revision: u64,
}

impl CandidateModelVariant {
    /// Stable, non-secret identity for request-local attempt de-duplication.
    /// Capacity remains keyed by `station_key_id`; this identity only scopes
    /// model selection, retry exclusion and trace evidence.
    pub(crate) fn identity_key(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{:?}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.station_key_id,
            self.upstream_model,
            self.endpoint,
            self.credential_revision,
            self.endpoint_revision,
            self.model_resolution_fence,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecisionEvidence {
    pub(crate) code: &'static str,
    pub(crate) rule_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedModelPlan {
    pub(crate) requested_model: Option<String>,
    pub(crate) disposition: Disposition,
    pub(crate) matched_rule_id: Option<String>,
    pub(crate) mapping_revision: u64,
    pub(crate) model_resolution_fence: String,
    pub(crate) target_policy: TargetPolicy,
    pub(crate) fallback_trigger: Option<FallbackTrigger>,
    pub(crate) target_models: Vec<ResolvedTarget>,
    pub(crate) rejection_kind: Option<RejectionKind>,
    pub(crate) rejection_message: Option<String>,
    pub(crate) decision_evidence: Vec<DecisionEvidence>,
}

/// Phase 1 has exactly one runtime target.  Keep the cardinality check at the
/// boundary where a plan becomes an execution model so a future multi-target
/// action cannot be silently truncated by callers taking `first()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetSelectionError {
    MultipleTargetsUnsupported { target_count: usize },
}

impl TargetSelectionError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::MultipleTargetsUnsupported { .. } => "model_mapping_multiple_targets_unsupported",
        }
    }
}

impl std::fmt::Display for TargetSelectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MultipleTargetsUnsupported { target_count } => write!(
                formatter,
                "model mapping produced {target_count} targets but runtime supports exactly one"
            ),
        }
    }
}

impl std::error::Error for TargetSelectionError {}

impl ResolvedModelPlan {
    pub(crate) fn execution_target(&self) -> Result<Option<&ResolvedTarget>, TargetSelectionError> {
        match self.target_models.as_slice() {
            [] => Ok(None),
            [target] => Ok(Some(target)),
            targets => Err(TargetSelectionError::MultipleTargetsUnsupported {
                target_count: targets.len(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModelMappingResolutionError {
    InvalidModelName,
    TargetRequiresCandidateContext,
    ProfileNotFound,
    ProfileHasNoOffering,
    NoResolvedTargets,
}

impl std::fmt::Display for ModelMappingResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidModelName => formatter.write_str("requested model is invalid"),
            Self::TargetRequiresCandidateContext => {
                formatter.write_str("model profile target requires a station candidate context")
            }
            Self::ProfileNotFound => formatter.write_str("model profile target does not exist"),
            Self::ProfileHasNoOffering => {
                formatter.write_str("model profile has no eligible offering")
            }
            Self::NoResolvedTargets => formatter.write_str("mapping produced no resolved targets"),
        }
    }
}

impl std::error::Error for ModelMappingResolutionError {}

pub(crate) fn resolve(
    configuration: &CompiledModelMappingConfiguration,
    facts: &ModelRequestFacts,
) -> Result<ResolvedModelPlan, ModelMappingResolutionError> {
    let base = |disposition, matched_rule_id, target_policy, target_models, evidence| {
        Ok(ResolvedModelPlan {
            requested_model: facts
                .requested_model
                .as_deref()
                .and_then(normalize_model_name),
            disposition,
            matched_rule_id,
            mapping_revision: configuration.mapping_revision,
            model_resolution_fence: configuration.model_resolution_fence.clone(),
            target_policy,
            fallback_trigger: None,
            target_models,
            rejection_kind: None,
            rejection_message: None,
            decision_evidence: evidence,
        })
    };

    if facts.endpoint.is_mapping_bypass() {
        return base(
            Disposition::Bypass,
            None,
            TargetPolicy::None,
            Vec::new(),
            vec![DecisionEvidence {
                code: "model_mapping_bypass_endpoint",
                rule_id: None,
            }],
        );
    }
    let Some(requested_model) = facts.requested_model.as_deref() else {
        return base(
            Disposition::Bypass,
            None,
            TargetPolicy::None,
            Vec::new(),
            vec![DecisionEvidence {
                code: "model_mapping_bypass_missing_model",
                rule_id: None,
            }],
        );
    };
    let requested_model = normalize_model_name(requested_model)
        .ok_or(ModelMappingResolutionError::InvalidModelName)?;

    let matched = configuration
        .rules
        .iter()
        .find(|rule| rule_matches(rule, &requested_model, facts));
    match matched {
        Some(rule) => resolve_action(configuration, requested_model, rule, None),
        None => match configuration.policy.unmatched_model_behavior {
            UnmatchedModelBehavior::Preserve => base(
                Disposition::Preserve,
                None,
                TargetPolicy::Preserve,
                vec![ResolvedTarget {
                    target_rank: 0,
                    route_model: requested_model,
                    resolution_reason: ResolutionReason::UnmatchedPreserve,
                    binding_revision: None,
                }],
                vec![DecisionEvidence {
                    code: "model_mapping_unmatched_preserve",
                    rule_id: None,
                }],
            ),
            UnmatchedModelBehavior::Reject => base(
                Disposition::Reject,
                None,
                TargetPolicy::None,
                Vec::new(),
                vec![DecisionEvidence {
                    code: "model_mapping_unmatched_reject",
                    rule_id: None,
                }],
            )
            .map(|mut plan| {
                plan.rejection_kind = Some(RejectionKind::UnsupportedModel);
                plan
            }),
        },
    }
}

pub(crate) fn resolve_for_candidate(
    configuration: &CompiledModelMappingConfiguration,
    facts: &ModelRequestFacts,
    context: CandidateResolutionContext<'_>,
) -> Result<ResolvedModelPlan, ModelMappingResolutionError> {
    if facts.endpoint.is_mapping_bypass() || facts.requested_model.is_none() {
        return resolve(configuration, facts);
    }
    let requested_model = facts
        .requested_model
        .as_deref()
        .and_then(normalize_model_name)
        .ok_or(ModelMappingResolutionError::InvalidModelName)?;
    let matched = configuration
        .rules
        .iter()
        .find(|rule| rule_matches(rule, &requested_model, facts));
    match matched {
        Some(rule) => resolve_action(configuration, requested_model, rule, Some(&context)),
        None => resolve(configuration, facts),
    }
}

pub(crate) fn candidate_variants(
    plan: &ResolvedModelPlan,
    context: CandidateResolutionContext<'_>,
) -> Vec<CandidateModelVariant> {
    let mut variants: Vec<CandidateModelVariant> = Vec::new();
    for target in &plan.target_models {
        let variant = CandidateModelVariant {
            station_key_id: context.station_key_id.to_owned(),
            station_id: context.station_id.to_owned(),
            upstream_model: target.route_model.clone(),
            target_rank: target.target_rank,
            binding_revision: target.binding_revision,
            model_resolution_fence: plan.model_resolution_fence.clone(),
            endpoint: context.endpoint,
            credential_revision: context.credential_revision,
            endpoint_revision: context.endpoint_revision,
        };
        if !variants.iter().any(|existing| {
            existing.station_key_id == variant.station_key_id
                && existing.upstream_model == variant.upstream_model
                && existing.endpoint == variant.endpoint
                && existing.credential_revision == variant.credential_revision
                && existing.endpoint_revision == variant.endpoint_revision
                && existing.model_resolution_fence == variant.model_resolution_fence
        }) {
            variants.push(variant);
        }
    }
    variants
}

fn resolve_action(
    configuration: &CompiledModelMappingConfiguration,
    requested_model: String,
    rule: &CompiledRule,
    context: Option<&CandidateResolutionContext<'_>>,
) -> Result<ResolvedModelPlan, ModelMappingResolutionError> {
    let evidence = vec![DecisionEvidence {
        code: "model_mapping_rule_match",
        rule_id: Some(rule.id.clone()),
    }];
    match &rule.action {
        Action::MapFixed { target } => {
            let (upstream_model, binding_revision, resolution_reason) =
                resolve_target(configuration, target, context)?;
            Ok(ResolvedModelPlan {
                requested_model: Some(requested_model),
                disposition: Disposition::Mapped,
                matched_rule_id: Some(rule.id.clone()),
                mapping_revision: configuration.mapping_revision,
                model_resolution_fence: configuration.model_resolution_fence.clone(),
                target_policy: TargetPolicy::Fixed,
                fallback_trigger: None,
                target_models: vec![ResolvedTarget {
                    target_rank: 0,
                    route_model: upstream_model,
                    resolution_reason,
                    binding_revision,
                }],
                rejection_kind: None,
                rejection_message: None,
                decision_evidence: evidence,
            })
        }
        Action::MapFallbackChain {
            targets,
            fallback_trigger,
        } => {
            let mut resolved_targets = Vec::with_capacity(targets.len());
            for (target_rank, target) in targets.iter().enumerate() {
                match resolve_target(configuration, target, context) {
                    Ok((route_model, binding_revision, resolution_reason)) => {
                        resolved_targets.push(ResolvedTarget {
                            target_rank: target_rank as u16,
                            route_model,
                            resolution_reason,
                            binding_revision,
                        });
                    }
                    Err(ModelMappingResolutionError::ProfileHasNoOffering) => continue,
                    Err(error) => return Err(error),
                }
            }
            if resolved_targets.is_empty() {
                return Err(ModelMappingResolutionError::NoResolvedTargets);
            }
            Ok(ResolvedModelPlan {
                requested_model: Some(requested_model),
                disposition: Disposition::Mapped,
                matched_rule_id: Some(rule.id.clone()),
                mapping_revision: configuration.mapping_revision,
                model_resolution_fence: configuration.model_resolution_fence.clone(),
                target_policy: TargetPolicy::Fallback,
                fallback_trigger: Some(*fallback_trigger),
                target_models: resolved_targets,
                rejection_kind: None,
                rejection_message: None,
                decision_evidence: evidence,
            })
        }
        Action::Preserve => Ok(ResolvedModelPlan {
            requested_model: Some(requested_model.clone()),
            disposition: Disposition::Preserve,
            matched_rule_id: Some(rule.id.clone()),
            mapping_revision: configuration.mapping_revision,
            model_resolution_fence: configuration.model_resolution_fence.clone(),
            target_policy: TargetPolicy::Preserve,
            fallback_trigger: None,
            target_models: vec![ResolvedTarget {
                target_rank: 0,
                route_model: requested_model,
                resolution_reason: ResolutionReason::RuleMatch,
                binding_revision: None,
            }],
            rejection_kind: None,
            rejection_message: None,
            decision_evidence: evidence,
        }),
        Action::Reject {
            rejection_kind,
            message,
        } => Ok(ResolvedModelPlan {
            requested_model: Some(requested_model),
            disposition: Disposition::Reject,
            matched_rule_id: Some(rule.id.clone()),
            mapping_revision: configuration.mapping_revision,
            model_resolution_fence: configuration.model_resolution_fence.clone(),
            target_policy: TargetPolicy::None,
            fallback_trigger: None,
            target_models: Vec::new(),
            rejection_kind: Some(*rejection_kind),
            rejection_message: message.clone(),
            decision_evidence: evidence,
        }),
    }
}

fn rule_matches(rule: &CompiledRule, model: &str, facts: &ModelRequestFacts) -> bool {
    let matcher_matches = match &rule.matcher {
        CompiledMatcher::Exact(expected) => expected == model,
        CompiledMatcher::Glob(pattern) => pattern.matches(model),
        CompiledMatcher::Default => true,
    };
    matcher_matches
        && rule
            .conditions
            .endpoint_kinds
            .as_ref()
            .is_none_or(|endpoints| endpoints.contains(&facts.endpoint))
        && requirement_matches(rule.conditions.stream, facts.stream)
        && requirement_matches(rule.conditions.tools, facts.tools)
        && requirement_matches(rule.conditions.vision, facts.vision)
        && requirement_matches(rule.conditions.reasoning, facts.reasoning)
}

fn requirement_matches(requirement: ConditionRequirement, actual: bool) -> bool {
    match requirement {
        ConditionRequirement::Any => true,
        ConditionRequirement::Required => actual,
        ConditionRequirement::Forbidden => !actual,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::model_mapping::compiler::compile;
    use crate::models::model_mapping::{
        Action, EndpointKind, FallbackTrigger, Matcher, ModelBindingSource, ModelMappingDocumentV1,
        ModelMappingPolicy, ModelMappingRule, ModelOfferingBinding, ModelProfile,
        ModelProfileStatus, RejectionKind, RuleConditions, TargetRef,
    };

    fn rule(id: &str, priority: u32, matcher: Matcher, action: Action) -> ModelMappingRule {
        ModelMappingRule {
            id: id.to_string(),
            priority,
            enabled: true,
            matcher,
            conditions: RuleConditions::default(),
            action,
            note: None,
            revision: 1,
        }
    }

    fn fixed(model: &str) -> Action {
        Action::MapFixed {
            target: TargetRef::Literal {
                upstream_model: model.to_string(),
            },
        }
    }

    fn document(rules: Vec<ModelMappingRule>) -> ModelMappingDocumentV1 {
        ModelMappingDocumentV1 {
            format_version: 1,
            base_revision: 7,
            policy: ModelMappingPolicy::default(),
            rules,
            ..Default::default()
        }
    }

    fn profile(id: &str, default_upstream_model: Option<&str>) -> ModelProfile {
        ModelProfile {
            id: id.to_string(),
            canonical_model: format!("canonical-{id}"),
            display_name: id.to_string(),
            default_upstream_model: default_upstream_model.map(ToOwned::to_owned),
            status: ModelProfileStatus::Active,
            note: None,
            revision: 3,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    fn binding(
        id: &str,
        profile_id: &str,
        station_key_id: Option<&str>,
        station_id: Option<&str>,
        upstream_model: &str,
        enabled: bool,
    ) -> ModelOfferingBinding {
        ModelOfferingBinding {
            id: id.to_string(),
            model_profile_id: profile_id.to_string(),
            station_key_id: station_key_id.map(ToOwned::to_owned),
            station_id: station_id.map(ToOwned::to_owned),
            upstream_model: upstream_model.to_string(),
            source: ModelBindingSource::Manual,
            enabled,
            note: None,
            revision: 11,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    fn candidate_context<'a>(
        station_key_id: &'a str,
        station_id: &'a str,
    ) -> CandidateResolutionContext<'a> {
        CandidateResolutionContext {
            station_key_id,
            station_id,
            endpoint: EndpointKind::Responses,
            credential_revision: 1,
            endpoint_revision: 1,
        }
    }

    #[test]
    fn fixed_mapping_preserves_requested_and_resolves_trimmed_model() {
        let configuration = compile(&document(vec![rule(
            "fixed",
            10,
            Matcher::Exact {
                model: "codex-5.4".into(),
            },
            fixed("deepseek-v4-flash"),
        )]))
        .unwrap();
        let plan = resolve(
            &configuration,
            &ModelRequestFacts::inference(
                "  codex-5.4 ",
                EndpointKind::Responses,
                false,
                false,
                false,
                false,
            ),
        )
        .unwrap();
        assert_eq!(plan.requested_model.as_deref(), Some("codex-5.4"));
        assert_eq!(plan.disposition, Disposition::Mapped);
        assert_eq!(plan.target_models[0].route_model, "deepseek-v4-flash");
        assert_eq!(plan.mapping_revision, 7);
    }

    #[test]
    fn matching_is_case_sensitive_and_conditions_are_enforced() {
        let mut rule = rule(
            "tools",
            10,
            Matcher::Exact {
                model: "codex".into(),
            },
            fixed("target"),
        );
        rule.conditions.tools = ConditionRequirement::Required;
        let configuration = compile(&document(vec![rule])).unwrap();
        let no_tools = resolve(
            &configuration,
            &ModelRequestFacts::inference(
                "codex",
                EndpointKind::Responses,
                false,
                false,
                false,
                false,
            ),
        )
        .unwrap();
        assert_eq!(no_tools.disposition, Disposition::Preserve);
        let wrong_case = resolve(
            &configuration,
            &ModelRequestFacts::inference(
                "Codex",
                EndpointKind::Responses,
                false,
                true,
                false,
                false,
            ),
        )
        .unwrap();
        assert_eq!(wrong_case.disposition, Disposition::Preserve);
        let with_tools = resolve(
            &configuration,
            &ModelRequestFacts::inference(
                "codex",
                EndpointKind::Responses,
                false,
                true,
                false,
                false,
            ),
        )
        .unwrap();
        assert_eq!(with_tools.disposition, Disposition::Mapped);
    }

    #[test]
    fn preserve_reject_and_bypass_do_not_touch_upstream() {
        let mut reject = rule(
            "reject",
            20,
            Matcher::Exact {
                model: "blocked".into(),
            },
            Action::Reject {
                rejection_kind: RejectionKind::PolicyDenied,
                message: None,
            },
        );
        reject.conditions = RuleConditions::default();
        let configuration = compile(&document(vec![reject])).unwrap();
        let rejected = resolve(
            &configuration,
            &ModelRequestFacts::inference(
                "blocked",
                EndpointKind::ChatCompletions,
                false,
                false,
                false,
                false,
            ),
        )
        .unwrap();
        assert_eq!(rejected.disposition, Disposition::Reject);
        assert_eq!(rejected.rejection_kind, Some(RejectionKind::PolicyDenied));
        let bypass = resolve(
            &configuration,
            &ModelRequestFacts::bypass(EndpointKind::Models),
        )
        .unwrap();
        assert_eq!(bypass.disposition, Disposition::Bypass);
        assert!(bypass.target_models.is_empty());
    }

    #[test]
    fn candidate_resolution_keeps_bypass_and_missing_model_semantics() {
        let configuration = compile(&document(vec![rule(
            "default",
            10,
            Matcher::Default,
            fixed("native"),
        )]))
        .unwrap();
        let context = candidate_context("key", "station");
        let bypass = resolve_for_candidate(
            &configuration,
            &ModelRequestFacts::bypass(EndpointKind::Models),
            context,
        )
        .unwrap();
        assert_eq!(bypass.disposition, Disposition::Bypass);
        let missing_model = resolve_for_candidate(
            &configuration,
            &ModelRequestFacts {
                requested_model: None,
                endpoint: EndpointKind::Responses,
                stream: false,
                tools: false,
                vision: false,
                reasoning: false,
            },
            context,
        )
        .unwrap();
        assert_eq!(missing_model.disposition, Disposition::Bypass);
    }

    #[test]
    fn profile_resolution_prefers_key_then_station_then_default_and_ignores_disabled() {
        let action = Action::MapFixed {
            target: TargetRef::ModelProfile {
                model_profile_id: "profile-a".into(),
            },
        };
        let mut document = document(vec![rule(
            "profile",
            10,
            Matcher::Exact {
                model: "codex".into(),
            },
            action,
        )]);
        document.profiles = vec![profile("profile-a", Some("profile-default"))];
        document.bindings = vec![
            binding(
                "key",
                "profile-a",
                Some("key-1"),
                None,
                "key-upstream",
                true,
            ),
            binding(
                "station",
                "profile-a",
                None,
                Some("station-1"),
                "station-upstream",
                true,
            ),
            binding(
                "disabled",
                "profile-a",
                Some("key-2"),
                None,
                "disabled-upstream",
                false,
            ),
        ];
        let configuration = compile(&document).unwrap();
        let facts = ModelRequestFacts::inference(
            "codex",
            EndpointKind::Responses,
            false,
            false,
            false,
            false,
        );

        let key_plan = resolve_for_candidate(
            &configuration,
            &facts,
            candidate_context("key-1", "station-1"),
        )
        .unwrap();
        assert_eq!(key_plan.target_models[0].route_model, "key-upstream");
        assert_eq!(
            key_plan.target_models[0].resolution_reason,
            ResolutionReason::ProfileKeyBinding
        );

        let station_plan = resolve_for_candidate(
            &configuration,
            &facts,
            candidate_context("other", "station-1"),
        )
        .unwrap();
        assert_eq!(
            station_plan.target_models[0].route_model,
            "station-upstream"
        );
        assert_eq!(
            station_plan.target_models[0].resolution_reason,
            ResolutionReason::ProfileStationBinding
        );

        let default_plan =
            resolve_for_candidate(&configuration, &facts, candidate_context("key-2", "other"))
                .unwrap();
        assert_eq!(default_plan.target_models[0].route_model, "profile-default");
        assert_eq!(
            default_plan.target_models[0].resolution_reason,
            ResolutionReason::ProfileDefault
        );
    }

    #[test]
    fn fallback_resolution_preserves_rank_and_deduplicates_actual_variants() {
        let mut document = document(vec![rule(
            "fallback",
            10,
            Matcher::Exact {
                model: "codex".into(),
            },
            Action::MapFallbackChain {
                targets: vec![
                    TargetRef::ModelProfile {
                        model_profile_id: "profile-a".into(),
                    },
                    TargetRef::Literal {
                        upstream_model: "same-upstream".into(),
                    },
                ],
                fallback_trigger: FallbackTrigger::NoEligibleTarget,
            },
        )]);
        document.profiles = vec![profile("profile-a", Some("same-upstream"))];
        let configuration = compile(&document).unwrap();
        let facts = ModelRequestFacts::inference(
            "codex",
            EndpointKind::Responses,
            false,
            false,
            false,
            false,
        );
        let plan =
            resolve_for_candidate(&configuration, &facts, candidate_context("key", "station"))
                .unwrap();
        assert_eq!(plan.target_policy, TargetPolicy::Fallback);
        assert_eq!(plan.target_models.len(), 2);
        assert_eq!(plan.target_models[0].target_rank, 0);
        assert_eq!(plan.target_models[1].target_rank, 1);

        let variants = candidate_variants(&plan, candidate_context("key", "station"));
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].target_rank, 0);
        assert_eq!(variants[0].upstream_model, "same-upstream");
    }

    #[test]
    fn fallback_skips_profile_without_offering_but_keeps_original_rank() {
        let mut document = document(vec![rule(
            "fallback",
            10,
            Matcher::Exact {
                model: "codex".into(),
            },
            Action::MapFallbackChain {
                targets: vec![
                    TargetRef::ModelProfile {
                        model_profile_id: "empty".into(),
                    },
                    TargetRef::Literal {
                        upstream_model: "second".into(),
                    },
                ],
                fallback_trigger: FallbackTrigger::NoEligibleTarget,
            },
        )]);
        document.profiles = vec![profile("empty", None)];
        let configuration = compile(&document).unwrap();
        let facts = ModelRequestFacts::inference(
            "codex",
            EndpointKind::Responses,
            false,
            false,
            false,
            false,
        );
        let plan =
            resolve_for_candidate(&configuration, &facts, candidate_context("key", "station"))
                .unwrap();
        assert_eq!(plan.target_models.len(), 1);
        assert_eq!(plan.target_models[0].target_rank, 1);
        assert_eq!(plan.target_models[0].route_model, "second");
    }

    #[test]
    fn execution_target_rejects_unconsumed_multi_target_plans() {
        let plan = ResolvedModelPlan {
            requested_model: Some("codex".to_string()),
            disposition: Disposition::Mapped,
            matched_rule_id: Some("future-fallback".to_string()),
            mapping_revision: 7,
            model_resolution_fence: "mapping-fence".to_string(),
            target_policy: TargetPolicy::Fixed,
            fallback_trigger: None,
            target_models: vec![
                ResolvedTarget {
                    target_rank: 0,
                    route_model: "native-a".to_string(),
                    resolution_reason: ResolutionReason::RuleMatch,
                    binding_revision: None,
                },
                ResolvedTarget {
                    target_rank: 1,
                    route_model: "native-b".to_string(),
                    resolution_reason: ResolutionReason::RuleMatch,
                    binding_revision: None,
                },
            ],
            rejection_kind: None,
            rejection_message: None,
            decision_evidence: Vec::new(),
        };
        let error = plan
            .execution_target()
            .expect_err("runtime must not silently select target rank zero");
        assert_eq!(error.code(), "model_mapping_multiple_targets_unsupported");
        assert_eq!(
            error.to_string(),
            "model mapping produced 2 targets but runtime supports exactly one"
        );
    }

    #[test]
    fn execution_target_accepts_single_target_and_empty_rejection() {
        let mut plan = ResolvedModelPlan {
            requested_model: Some("codex".to_string()),
            disposition: Disposition::Mapped,
            matched_rule_id: None,
            mapping_revision: 1,
            model_resolution_fence: "mapping-fence".to_string(),
            target_policy: TargetPolicy::Fixed,
            fallback_trigger: None,
            target_models: vec![ResolvedTarget {
                target_rank: 0,
                route_model: "native".to_string(),
                resolution_reason: ResolutionReason::RuleMatch,
                binding_revision: None,
            }],
            rejection_kind: None,
            rejection_message: None,
            decision_evidence: Vec::new(),
        };
        assert_eq!(
            plan.execution_target()
                .expect("single target is supported")
                .map(|target| target.route_model.as_str()),
            Some("native")
        );
        plan.target_models.clear();
        assert!(plan
            .execution_target()
            .expect("reject/bypass plans have no execution target")
            .is_none());
    }
}

fn resolve_target(
    configuration: &CompiledModelMappingConfiguration,
    target: &TargetRef,
    context: Option<&CandidateResolutionContext<'_>>,
) -> Result<(String, Option<u64>, ResolutionReason), ModelMappingResolutionError> {
    match target {
        TargetRef::Literal { upstream_model } => {
            Ok((upstream_model.clone(), None, ResolutionReason::RuleMatch))
        }
        TargetRef::ModelProfile { model_profile_id } => {
            let profile = configuration
                .profiles
                .iter()
                .find(|profile| profile.id == *model_profile_id)
                .ok_or(ModelMappingResolutionError::ProfileNotFound)?;
            if matches!(
                profile.status,
                crate::models::model_mapping::ModelProfileStatus::Archived
            ) {
                return Err(ModelMappingResolutionError::ProfileHasNoOffering);
            }
            let context =
                context.ok_or(ModelMappingResolutionError::TargetRequiresCandidateContext)?;
            if let Some(binding) = configuration.bindings.iter().find(|binding| {
                binding.enabled
                    && binding.model_profile_id == profile.id
                    && binding.station_key_id.as_deref() == Some(context.station_key_id)
            }) {
                return Ok((
                    normalize_model_name(&binding.upstream_model)
                        .ok_or(ModelMappingResolutionError::ProfileHasNoOffering)?,
                    Some(binding.revision),
                    ResolutionReason::ProfileKeyBinding,
                ));
            }
            if let Some(binding) = configuration.bindings.iter().find(|binding| {
                binding.enabled
                    && binding.model_profile_id == profile.id
                    && binding.station_id.as_deref() == Some(context.station_id)
            }) {
                return Ok((
                    normalize_model_name(&binding.upstream_model)
                        .ok_or(ModelMappingResolutionError::ProfileHasNoOffering)?,
                    Some(binding.revision),
                    ResolutionReason::ProfileStationBinding,
                ));
            }
            profile
                .default_upstream_model
                .as_deref()
                .and_then(normalize_model_name)
                .map(|model| {
                    (
                        model,
                        Some(profile.revision),
                        ResolutionReason::ProfileDefault,
                    )
                })
                .ok_or(ModelMappingResolutionError::ProfileHasNoOffering)
        }
    }
}
