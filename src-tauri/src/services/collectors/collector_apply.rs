use sha2::{Digest, Sha256};

use crate::{
    application::{
        collectors::{
            CanonicalBalanceFact, CanonicalCollectorFacts, CanonicalGroupFact, CanonicalModelFact,
            CanonicalRateFact, CollectorApplyOutcome, CollectorApplyRequest, CollectorService,
        },
        error::ApplicationError,
    },
    services::collectors::{adapters::AdapterOutput, facts::CollectedBalanceFact},
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
}

#[derive(Clone)]
pub(crate) struct V2CollectorApplyAdapter {
    service: CollectorService,
}

impl V2CollectorApplyAdapter {
    pub(crate) fn new(service: CollectorService) -> Self {
        Self { service }
    }
}

impl CollectorApplyPort for V2CollectorApplyAdapter {
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
        Box::pin(self.service.apply_result(request))
    }
}

/// Applies one collector result through the V2 application port.
///
/// Station discovery and upstream calls remain owned by the legacy collector
/// coordinator for now; this boundary guarantees that the resulting run,
/// snapshot, facts, health transitions, and change events are written by V2.
pub(crate) async fn apply_station_output_v2(
    port: &dyn CollectorApplyPort,
    station_id: String,
    endpoint_revision: i64,
    parent_run_id: Option<String>,
    output: AdapterOutput,
) -> Result<crate::application::collectors::CollectorApplyOutcome, ApplicationError> {
    if station_id.trim().is_empty() || endpoint_revision < 1 {
        return Err(ApplicationError::ConstraintViolation);
    }
    let run_key = stable_run_key(
        &station_id,
        endpoint_revision,
        &output,
        parent_run_id.as_deref(),
    )?;
    apply_adapter_output(
        port,
        run_key,
        station_id,
        endpoint_revision,
        parent_run_id,
        None,
        output,
    )
    .await
}

async fn apply_adapter_output(
    port: &dyn CollectorApplyPort,
    run_key: String,
    station_id: String,
    endpoint_revision: i64,
    parent_run_id: Option<String>,
    next_due_at: Option<String>,
    output: AdapterOutput,
) -> Result<CollectorApplyOutcome, ApplicationError> {
    let mut facts = output.facts;
    append_station_balance_aggregates(&mut facts.balances);
    let endpoint_counts = endpoint_counts_from_summary(&output.summary_json);
    let request = CollectorApplyRequest {
        run_key,
        station_id,
        endpoint_revision,
        parent_run_id,
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
            models: facts
                .models
                .into_iter()
                .map(|fact| CanonicalModelFact {
                    station_id: fact.station_id,
                    model: fact.model,
                    available: fact.available,
                    source: fact.source,
                    confidence: fact.confidence,
                })
                .collect(),
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
    };
    port.apply(request).await
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
                .unwrap_or("CNY")
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
            status: if value == 0.0 { "depleted" } else { "normal" }.to_string(),
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

fn stable_run_key(
    station_id: &str,
    endpoint_revision: i64,
    output: &AdapterOutput,
    parent_run_id: Option<&str>,
) -> Result<String, ApplicationError> {
    let canonical = serde_json::json!({
        "stationId": station_id,
        "endpointRevision": endpoint_revision,
        "parentRunId": parent_run_id,
        "adapter": output.adapter,
        "task": output.task.as_str(),
        "status": output.status,
        "summary": output.summary_json,
        "normalized": output.normalized_json,
        "errorCode": output.error_code,
    });
    let bytes = serde_json::to_vec(&canonical).map_err(|_| ApplicationError::Internal)?;
    let digest = Sha256::digest(bytes);
    let suffix = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!(
        "collector:{station_id}:{endpoint_revision}:{}:{suffix}",
        output.task.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::collectors::{adapters::CollectorTask, facts::CollectorFacts};

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
        }
    }

    #[test]
    fn run_key_is_deterministic_and_scoped_to_revision_and_task() {
        let first =
            stable_run_key("station-1", 4, &output(CollectorTask::Balance), None).expect("run key");
        let repeat =
            stable_run_key("station-1", 4, &output(CollectorTask::Balance), None).expect("run key");
        let other_revision =
            stable_run_key("station-1", 5, &output(CollectorTask::Balance), None).expect("run key");
        let other_task =
            stable_run_key("station-1", 4, &output(CollectorTask::Groups), None).expect("run key");
        assert_eq!(first, repeat);
        assert_ne!(first, other_revision);
        assert_ne!(first, other_task);
    }
}
