use crate::{
    application::{
        collectors::{
            CanonicalBalanceFact, CanonicalCollectorFacts, CanonicalGroupFact, CanonicalRateFact,
            CollectorApplyOutcome, CollectorApplyRequest, CollectorFullApplyOutcome,
            CollectorService,
        },
        error::ApplicationError,
    },
    observability::correlation,
    services::collectors::output::AdapterOutput,
};

pub(crate) trait CollectorApplyPort: Send + Sync {
    fn apply<'a>(
        &'a self,
        request: CollectorApplyRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<CollectorApplyOutcome, ApplicationError>>
                + Send
                + 'a,
        >,
    >;

    /// Apply a Full operation as one atomic write.
    ///
    /// The method is intentionally required on every port implementation:
    /// silently falling back to parent-then-child writes would reintroduce a
    /// partially visible Full operation and violate the terminal commit
    /// invariant. Test/dry-run ports that do not support Full must return a
    /// typed error instead of emulating it sequentially.
    fn apply_full<'a>(
        &'a self,
        parent: CollectorApplyRequest,
        children: Vec<CollectorApplyRequest>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<CollectorFullApplyOutcome, ApplicationError>>
                + Send
                + 'a,
        >,
    >;
}

impl CollectorApplyPort for CollectorService {
    fn apply<'a>(
        &'a self,
        request: CollectorApplyRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<CollectorApplyOutcome, ApplicationError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(self.apply_result(request))
    }

    fn apply_full<'a>(
        &'a self,
        parent: CollectorApplyRequest,
        children: Vec<CollectorApplyRequest>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<CollectorFullApplyOutcome, ApplicationError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(self.apply_full_result(parent, children))
    }
}

/// Applies one collector result through the atomic application port.
///
/// Station discovery and upstream calls remain outside the terminal write;
/// this boundary guarantees that run, snapshot, facts, projections and
/// revisions are committed by the canonical collector owner.
pub(crate) async fn apply_station_output(
    port: &dyn CollectorApplyPort,
    station_id: String,
    endpoint_revision: i64,
    credential_revision: i64,
    intent_sequence: i64,
    output: AdapterOutput,
) -> Result<crate::application::collectors::CollectorApplyOutcome, ApplicationError> {
    if station_id.trim().is_empty()
        || endpoint_revision < 1
        || credential_revision < 1
        || intent_sequence < 1
    {
        return Err(ApplicationError::ConstraintViolation);
    }
    let run_key = run_key_for_current_intent(
        &station_id,
        endpoint_revision,
        credential_revision,
        intent_sequence,
        &output,
    );
    let request = collector_apply_request_from_output(
        run_key,
        station_id,
        endpoint_revision,
        credential_revision,
        intent_sequence,
        None,
        output,
    )?;
    port.apply(request).await
}

pub(crate) fn collector_apply_request_from_output(
    run_key: String,
    station_id: String,
    endpoint_revision: i64,
    credential_revision: i64,
    intent_sequence: i64,
    next_due_at: Option<String>,
    output: AdapterOutput,
) -> Result<CollectorApplyRequest, ApplicationError> {
    let mut facts = output.facts;
    let published_status = facts.published_status.take();
    let endpoint_counts = endpoint_counts_from_summary(&output.summary_json);
    Ok(CollectorApplyRequest {
        run_key,
        station_id,
        endpoint_revision,
        credential_revision,
        intent_sequence,
        #[cfg(test)]
        parent_run_id: None,
        adapter: output.adapter,
        task_type: output.task.as_str().to_string(),
        status: output.status.clone(),
        facts: CanonicalCollectorFacts {
            balances: facts
                .balances
                .into_iter()
                .map(|fact| CanonicalBalanceFact {
                    station_id: fact.station_id,
                    station_key_id: fact.station_key_id,
                    scope: fact.scope,
                    balance_kind: fact.balance_kind,
                    value: fact.value,
                    used_value: fact.used_value,
                    total_value: fact.total_value,
                    today_request_count: fact.today_request_count,
                    total_request_count: fact.total_request_count,
                    today_consumption: fact.today_consumption,
                    total_consumption: fact.total_consumption,
                    today_base_consumption: fact.today_base_consumption,
                    total_base_consumption: fact.total_base_consumption,
                    today_token_count: fact.today_token_count,
                    total_token_count: fact.total_token_count,
                    today_input_token_count: fact.today_input_token_count,
                    today_output_token_count: fact.today_output_token_count,
                    total_input_token_count: fact.total_input_token_count,
                    total_output_token_count: fact.total_output_token_count,
                    account_concurrency_limit: fact.account_concurrency_limit,
                    currency: fact.currency,
                    credit_unit: fact.credit_unit,
                    status: fact.status,
                    source: fact.source,
                    confidence: fact.confidence,
                    collected_at: fact.collected_at,
                    evidence_confidence: fact.evidence_confidence,
                    spendability_authority: fact.spendability_authority,
                })
                .collect(),
            groups: facts
                .groups
                .into_iter()
                .map(|fact| CanonicalGroupFact {
                    station_id: fact.station_id,
                    group_id: fact.group_id,
                    group_key_hash: fact.group_key_hash,
                    group_name: fact.group_name,
                    source: fact.source,
                    confidence: fact.confidence,
                    inferred_group_category: fact.inferred_group_category,
                    raw_json_redacted: fact.raw_json_redacted,
                })
                .collect(),
            rates: facts
                .rates
                .into_iter()
                .map(|fact| CanonicalRateFact {
                    station_id: fact.station_id,
                    station_key_id: fact.station_key_id,
                    group_id: fact.group_id,
                    group_key_hash: fact.group_key_hash,
                    group_name: fact.group_name,
                    default_rate_multiplier: fact.default_rate_multiplier,
                    user_rate_multiplier: fact.user_rate_multiplier,
                    effective_rate_multiplier: fact.effective_rate_multiplier,
                    inferred_group_category: fact.inferred_group_category,
                    source: fact.source,
                    confidence: fact.confidence,
                    checked_at: fact.checked_at,
                    raw_json_redacted: fact.raw_json_redacted,
                })
                .collect(),
            models: Vec::new(),
            published_status,
        },
        summary_json: output.summary_json,
        normalized_json: output.normalized_json,
        raw_json_redacted: output.raw_json_redacted,
        error_code: output.error_code,
        error_message: output.error_message,
        endpoint_count: endpoint_counts.0,
        success_count: endpoint_counts.1,
        failure_count: endpoint_counts.2,
        manual_action_required: output.status == "manual_required",
        next_due_at,
        execution_started_at_ms: output.execution_started_at_ms,
        execution_duration_ms: output.execution_duration_ms,
    })
}

fn endpoint_counts_from_summary(summary: &serde_json::Value) -> (i64, i64, i64) {
    let Some(endpoints) = summary
        .get("endpointResults")
        .and_then(serde_json::Value::as_array)
    else {
        return (0, 0, 0);
    };
    let endpoint_count = endpoints.len() as i64;
    let success_count = endpoints
        .iter()
        .filter(|endpoint| {
            endpoint
                .get("ok")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .count() as i64;
    (
        endpoint_count,
        success_count,
        endpoint_count.saturating_sub(success_count),
    )
}

pub(crate) fn run_key_for_current_intent(
    station_id: &str,
    endpoint_revision: i64,
    credential_revision: i64,
    intent_sequence: i64,
    output: &AdapterOutput,
) -> String {
    let intent_id = correlation::current_id_string()
        .unwrap_or_else(|| uuid::Uuid::now_v7().simple().to_string());
    format!(
        "collector:{station_id}:{endpoint_revision}:{credential_revision}:{intent_sequence}:{}:{intent_id}",
        output.task.as_str()
    )
}

/// Full operations use the same intent-scoped key derivation as single-task
/// operations, but construct requests before the atomic apply boundary.
pub(crate) fn run_key_for_current_intent_for_full(
    station_id: &str,
    endpoint_revision: i64,
    credential_revision: i64,
    intent_sequence: i64,
    output: &AdapterOutput,
) -> String {
    run_key_for_current_intent(
        station_id,
        endpoint_revision,
        credential_revision,
        intent_sequence,
        output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::collectors::{
        facts::{CollectedBalanceFact, CollectorFacts},
        output::CollectorTask,
    };

    fn output(task: CollectorTask) -> AdapterOutput {
        AdapterOutput {
            adapter: "fixture".to_string(),
            task,
            status: "success".to_string(),
            facts: CollectorFacts::default(),
            summary_json: serde_json::json!({"success": 1}),
            normalized_json: serde_json::json!({"models": []}),
            raw_json_redacted: None,
            error_code: None,
            error_message: None,
            execution_started_at_ms: None,
            execution_duration_ms: None,
        }
    }

    #[test]
    fn run_key_is_fresh_without_an_active_work_intent() {
        let first =
            run_key_for_current_intent("station-1", 4, 1, 1, &output(CollectorTask::Balance));
        let second =
            run_key_for_current_intent("station-1", 4, 1, 1, &output(CollectorTask::Balance));

        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn run_key_is_stable_only_within_the_same_command_intent() {
        let (first, repeat, other_revision, other_task) =
            correlation::in_command_scope("collect_station_task", async {
                (
                    run_key_for_current_intent(
                        "station-1",
                        4,
                        1,
                        1,
                        &output(CollectorTask::Balance),
                    ),
                    run_key_for_current_intent(
                        "station-1",
                        4,
                        1,
                        1,
                        &output(CollectorTask::Balance),
                    ),
                    run_key_for_current_intent(
                        "station-1",
                        5,
                        1,
                        1,
                        &output(CollectorTask::Balance),
                    ),
                    run_key_for_current_intent(
                        "station-1",
                        4,
                        1,
                        1,
                        &output(CollectorTask::Groups),
                    ),
                )
            })
            .await;
        let next_click = correlation::in_command_scope("collect_station_task", async {
            run_key_for_current_intent("station-1", 4, 1, 1, &output(CollectorTask::Balance))
        })
        .await;

        assert_eq!(first, repeat);
        assert_ne!(first, other_revision);
        assert_ne!(first, other_task);
        assert_ne!(first, next_click);
    }

    #[test]
    fn collector_apply_preserves_key_balances_without_creating_station_sum() {
        let key_balance = |key_id: &str| CollectedBalanceFact {
            station_id: "station-1".to_string(),
            station_key_id: Some(key_id.to_string()),
            scope: "station_key".to_string(),
            balance_kind: "station_key_quota".to_string(),
            value: Some(2.8),
            used_value: None,
            total_value: None,
            today_request_count: None,
            total_request_count: None,
            today_consumption: None,
            total_consumption: None,
            today_base_consumption: None,
            total_base_consumption: None,
            today_token_count: None,
            total_token_count: None,
            today_input_token_count: None,
            today_output_token_count: None,
            total_input_token_count: None,
            total_output_token_count: None,
            account_concurrency_limit: None,
            currency: "USD".to_string(),
            credit_unit: None,
            status: "normal".to_string(),
            source: "sub2api_usage".to_string(),
            confidence: 1.0,
            collected_at: Some("100".to_string()),
            evidence_confidence: "confirmed".to_string(),
            spendability_authority: "authoritative".to_string(),
        };
        let mut facts = CollectorFacts::default();
        facts.balances = vec![key_balance("key-a"), key_balance("key-b")];
        let mut collected = output(CollectorTask::Balance);
        collected.facts = facts;

        let request = collector_apply_request_from_output(
            "run-1".to_string(),
            "station-1".to_string(),
            1,
            1,
            1,
            None,
            collected,
        )
        .expect("collector request");

        assert_eq!(request.facts.balances.len(), 2);
        assert!(request
            .facts
            .balances
            .iter()
            .all(|balance| balance.scope == "station_key"));
        assert_eq!(
            request
                .facts
                .balances
                .iter()
                .filter_map(|balance| balance.value)
                .sum::<f64>(),
            5.6
        );
    }
}
