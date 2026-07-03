use crate::{
    models::{
        collector::{CollectorEvent, CollectorRunResult},
        collector_runs::{CreateCollectorRunInput, FinishCollectorRunInput},
        group_facts::{InsertGroupRateRecordInput, UpsertStationGroupBindingInput},
        pricing::UpsertBalanceSnapshotInput,
    },
    services::{
        collectors::{
            adapters::AdapterOutput,
            facts::{CollectedModelFact, CollectorFacts},
        },
        database::AppDatabase,
    },
};

#[derive(Debug, Clone)]
pub struct AppliedAdapterOutput {
    pub result: CollectorRunResult,
    pub run: crate::models::collector_runs::CollectorRun,
}

pub fn apply_adapter_output(
    database: &AppDatabase,
    station_id: &str,
    parent_run_id: Option<String>,
    output: AdapterOutput,
) -> Result<AppliedAdapterOutput, String> {
    let adapter = output.adapter.clone();
    let task = output.task;
    let status = output.status.clone();
    let summary_json = output.summary_json.clone();
    let normalized_json = output.normalized_json.clone();
    let raw_json_redacted = output.raw_json_redacted.clone();
    let error_code = output.error_code.clone();
    let error_message = output.error_message.clone();
    let endpoint_counts = endpoint_counts_from_summary(&summary_json, &status);
    let manual_action_required = status == "manual_required";

    let run = database.create_collector_run(CreateCollectorRunInput {
        station_id: station_id.to_string(),
        parent_run_id,
        adapter: adapter.clone(),
        task_type: task.as_str().to_string(),
    })?;
    let snapshot = match database.insert_collector_snapshot(
        station_id,
        &format!("{adapter}-{}", task.as_str()),
        &status,
        summary_json,
        normalized_json,
        raw_json_redacted,
        error_message.clone(),
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = database.finish_collector_run(FinishCollectorRunInput {
                id: run.id,
                status: "failed".to_string(),
                endpoint_count: endpoint_counts.0,
                success_count: endpoint_counts.1,
                failure_count: endpoint_counts.2,
                manual_action_required,
                error_code: Some("snapshot_write_failed".to_string()),
                error_message: Some(error.clone()),
                snapshot_id: None,
            });
            return Err(error);
        }
    };

    let apply_result = apply_collector_facts(database, output.facts);
    let finish_status = if apply_result.is_ok() {
        status.clone()
    } else {
        "failed".to_string()
    };
    let finish_error_message = apply_result.err().or(error_message);
    let finished_run = database.finish_collector_run(FinishCollectorRunInput {
        id: run.id,
        status: finish_status.clone(),
        endpoint_count: endpoint_counts.0,
        success_count: endpoint_counts.1,
        failure_count: endpoint_counts.2,
        manual_action_required,
        error_code,
        error_message: finish_error_message.clone(),
        snapshot_id: Some(snapshot.id.clone()),
    })?;

    Ok(AppliedAdapterOutput {
        result: CollectorRunResult {
            snapshot,
            events: vec![CollectorEvent {
                event_type: task.as_str().to_string(),
                message: finish_error_message.unwrap_or_else(|| format!("{adapter} {}", task.as_str())),
                status: finish_status,
            }],
        },
        run: finished_run,
    })
}

pub fn apply_collector_facts(database: &AppDatabase, facts: CollectorFacts) -> Result<(), String> {
    for balance in facts.balances {
        database.upsert_balance_snapshot(UpsertBalanceSnapshotInput {
            id: None,
            station_id: balance.station_id,
            station_key_id: balance.station_key_id,
            scope: balance.scope,
            value: balance.value,
            currency: balance.currency,
            credit_unit: balance.credit_unit,
            used_value: balance.used_value,
            total_value: balance.total_value,
            low_balance_threshold: None,
            status: balance.status,
            source: balance.source,
            confidence: balance.confidence,
            collected_at: balance.collected_at,
        })?;
    }

    for group in facts.groups {
        database.upsert_station_group_binding(UpsertStationGroupBindingInput {
            station_id: group.station_id,
            station_key_id: None,
            binding_kind: "station_group".to_string(),
            parent_group_binding_id: None,
            group_key_hash: group.group_key_hash,
            group_id_hash: group.group_id,
            group_name: group.group_name,
            binding_status: "available".to_string(),
            default_rate_multiplier: None,
            user_rate_multiplier: None,
            effective_rate_multiplier: None,
            rate_source: Some(group.source),
            confidence: group.confidence,
            last_seen_at: None,
            raw_json_redacted: group.raw_json_redacted,
        })?;
    }

    for rate in facts.rates {
        let is_key_binding = rate.station_key_id.is_some();
        let binding = database.upsert_station_group_binding(UpsertStationGroupBindingInput {
            station_id: rate.station_id.clone(),
            station_key_id: rate.station_key_id.clone(),
            binding_kind: if is_key_binding {
                "key_binding".to_string()
            } else {
                "station_group".to_string()
            },
            parent_group_binding_id: None,
            group_key_hash: rate.group_key_hash.clone(),
            group_id_hash: rate.group_id.clone(),
            group_name: rate.group_name.clone(),
            default_rate_multiplier: rate.default_rate_multiplier,
            user_rate_multiplier: rate.user_rate_multiplier,
            effective_rate_multiplier: rate.effective_rate_multiplier,
            rate_source: Some(rate.source.clone()),
            confidence: rate.confidence,
            binding_status: if is_key_binding {
                "bound".to_string()
            } else {
                "available".to_string()
            },
            last_seen_at: rate.checked_at.clone(),
            raw_json_redacted: rate.raw_json_redacted.clone(),
        })?;
        database.upsert_group_rate_record_if_changed(InsertGroupRateRecordInput {
            station_id: rate.station_id,
            station_key_id: rate.station_key_id,
            group_binding_id: Some(binding.id),
            binding_kind: binding.binding_kind,
            group_key_hash: rate.group_key_hash,
            group_name: rate.group_name,
            default_rate_multiplier: rate.default_rate_multiplier,
            user_rate_multiplier: rate.user_rate_multiplier,
            effective_rate_multiplier: rate.effective_rate_multiplier,
            source: rate.source,
            confidence: rate.confidence,
            raw_json_redacted: rate.raw_json_redacted,
            checked_at: rate.checked_at.unwrap_or_else(|| {
                crate::services::database::now_millis_for_services().to_string()
            }),
        })?;
    }

    apply_model_facts(facts.models);

    Ok(())
}

fn apply_model_facts(models: Vec<CollectedModelFact>) {
    // Models are persisted through adapter snapshots so existing model diff
    // events keep using collector_snapshots.normalized_json.models.
    for model in models {
        let _ = (
            model.station_id,
            model.model,
            model.available,
            model.source,
            model.confidence,
        );
    }
}

fn endpoint_counts_from_summary(summary: &serde_json::Value, status: &str) -> (i64, i64, i64) {
    let endpoints = summary
        .get("endpointResults")
        .and_then(serde_json::Value::as_array);
    let Some(endpoints) = endpoints else {
        return match status {
            "success" => (0, 0, 0),
            "partial" | "failed" => (0, 0, 0),
            _ => (0, 0, 0),
        };
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
    let failure_count = endpoint_count.saturating_sub(success_count);
    (endpoint_count, success_count, failure_count)
}
