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
    services::collectors::{
        facts::{CollectedBalanceFact, NORMALIZED_BALANCE_CURRENCY},
        output::AdapterOutput,
    },
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
    append_station_balance_aggregates(&mut facts.balances);
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

fn append_station_balance_aggregates(balances: &mut Vec<CollectedBalanceFact>) {
    let mut station_ids = Vec::new();
    for balance in balances.iter() {
        if balance.scope != "station_key" || balance.station_key_id.is_none() {
            continue;
        }
        if !station_ids.contains(&balance.station_id) {
            station_ids.push(balance.station_id.clone());
        }
    }

    for station_id in station_ids {
        if balances
            .iter()
            .any(|balance| balance.station_id == station_id && balance.scope == "station")
        {
            continue;
        }
        let key_balances = balances
            .iter()
            .filter(|balance| balance.station_id == station_id && balance.scope == "station_key")
            .collect::<Vec<_>>();
        let Some(value) = sum_present_values(key_balances.iter().map(|balance| balance.value))
        else {
            continue;
        };
        let currency =
            shared_text_value(key_balances.iter().map(|balance| balance.currency.as_str()))
                .unwrap_or(NORMALIZED_BALANCE_CURRENCY)
                .to_string();
        let credit_unit = shared_optional_text_value(
            key_balances
                .iter()
                .map(|balance| balance.credit_unit.as_deref()),
        )
        .map(ToString::to_string);
        let aggregate = CollectedBalanceFact {
            station_id,
            station_key_id: None,
            scope: "station".to_string(),
            value: Some(value),
            used_value: sum_present_values(key_balances.iter().map(|balance| balance.used_value)),
            total_value: sum_present_values(key_balances.iter().map(|balance| balance.total_value)),
            today_request_count: sum_present_i64_values(
                key_balances
                    .iter()
                    .map(|balance| balance.today_request_count),
            ),
            total_request_count: sum_present_i64_values(
                key_balances
                    .iter()
                    .map(|balance| balance.total_request_count),
            ),
            today_consumption: sum_present_values(
                key_balances.iter().map(|balance| balance.today_consumption),
            ),
            total_consumption: sum_present_values(
                key_balances.iter().map(|balance| balance.total_consumption),
            ),
            today_base_consumption: sum_present_values(
                key_balances
                    .iter()
                    .map(|balance| balance.today_base_consumption),
            ),
            total_base_consumption: sum_present_values(
                key_balances
                    .iter()
                    .map(|balance| balance.total_base_consumption),
            ),
            today_token_count: sum_present_i64_values(
                key_balances.iter().map(|balance| balance.today_token_count),
            ),
            total_token_count: sum_present_i64_values(
                key_balances.iter().map(|balance| balance.total_token_count),
            ),
            today_input_token_count: sum_present_i64_values(
                key_balances
                    .iter()
                    .map(|balance| balance.today_input_token_count),
            ),
            today_output_token_count: sum_present_i64_values(
                key_balances
                    .iter()
                    .map(|balance| balance.today_output_token_count),
            ),
            total_input_token_count: sum_present_i64_values(
                key_balances
                    .iter()
                    .map(|balance| balance.total_input_token_count),
            ),
            total_output_token_count: sum_present_i64_values(
                key_balances
                    .iter()
                    .map(|balance| balance.total_output_token_count),
            ),
            account_concurrency_limit: key_balances
                .iter()
                .find_map(|balance| balance.account_concurrency_limit),
            currency,
            credit_unit,
            status: if value <= 0.0 { "depleted" } else { "normal" }.to_string(),
            source: "station_key_balance_aggregate".to_string(),
            confidence: key_balances
                .iter()
                .map(|balance| balance.confidence)
                .fold(1.0_f64, f64::min),
            collected_at: key_balances
                .iter()
                .filter_map(|balance| balance.collected_at.as_ref())
                .max()
                .cloned(),
        };
        balances.push(aggregate);
    }
}

fn sum_present_values(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let mut total = 0.0;
    let mut has_value = false;
    for value in values.flatten() {
        total += value;
        has_value = true;
    }
    has_value.then_some(total)
}

fn sum_present_i64_values(values: impl Iterator<Item = Option<i64>>) -> Option<i64> {
    let mut total = 0_i64;
    let mut has_value = false;
    for value in values.flatten() {
        total += value;
        has_value = true;
    }
    has_value.then_some(total)
}

fn shared_text_value<'a>(mut values: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let first = values.next()?;
    values.all(|value| value == first).then_some(first)
}

fn shared_optional_text_value<'a>(
    mut values: impl Iterator<Item = Option<&'a str>>,
) -> Option<&'a str> {
    let first = values.next()??;
    values.all(|value| value == Some(first)).then_some(first)
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
    use crate::services::collectors::{facts::CollectorFacts, output::CollectorTask};

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
}
