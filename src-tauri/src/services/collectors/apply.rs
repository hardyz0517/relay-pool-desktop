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
            facts::{CollectedBalanceFact, CollectedModelFact, CollectorFacts},
        },
        database::AppDatabase,
    },
};
use std::collections::{HashMap, HashSet};

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
                message: finish_error_message
                    .unwrap_or_else(|| format!("{adapter} {}", task.as_str())),
                status: finish_status,
            }],
        },
        run: finished_run,
    })
}

pub fn apply_collector_facts(
    database: &AppDatabase,
    mut facts: CollectorFacts,
) -> Result<(), String> {
    append_station_balance_aggregates(&mut facts.balances);
    let station_group_collection_scopes = station_group_collection_scopes(&facts);

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

    for (station_id, scope) in station_group_collection_scopes {
        database.mark_missing_station_group_bindings(
            &station_id,
            scope.sources.into_iter().collect(),
            scope.group_key_hashes.into_iter().collect(),
        )?;
    }

    apply_model_facts(facts.models);

    Ok(())
}

#[derive(Debug, Default)]
struct StationGroupCollectionScope {
    sources: HashSet<String>,
    group_key_hashes: HashSet<String>,
}

fn station_group_collection_scopes(
    facts: &CollectorFacts,
) -> HashMap<String, StationGroupCollectionScope> {
    let mut scopes = HashMap::<String, StationGroupCollectionScope>::new();
    for group in &facts.groups {
        let source = group.source.trim();
        if source.is_empty() {
            continue;
        }
        let scope = scopes.entry(group.station_id.clone()).or_default();
        add_station_group_scope_source(scope, source);
        scope.group_key_hashes.insert(group.group_key_hash.clone());
    }
    for rate in &facts.rates {
        if rate.station_key_id.is_some() {
            continue;
        }
        let source = rate.source.trim();
        if source.is_empty() {
            continue;
        }
        let scope = scopes.entry(rate.station_id.clone()).or_default();
        add_station_group_scope_source(scope, source);
        scope.group_key_hashes.insert(rate.group_key_hash.clone());
    }
    scopes
}

fn add_station_group_scope_source(scope: &mut StationGroupCollectionScope, source: &str) {
    scope.sources.insert(source.to_string());
    if source.starts_with("sub2api_groups_") {
        scope.sources.extend(
            [
                "sub2api_groups_available",
                "sub2api_groups_rates",
                "remote_scan",
            ]
            .map(String::from),
        );
    }
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

        let Some((value, used_value, total_value, currency, credit_unit, confidence, collected_at)) =
            ({
                let key_balances = balances
                    .iter()
                    .filter(|balance| {
                        balance.station_id == station_id && balance.scope == "station_key"
                    })
                    .collect::<Vec<_>>();
                let value = sum_present_values(key_balances.iter().map(|balance| balance.value));
                let used_value =
                    sum_present_values(key_balances.iter().map(|balance| balance.used_value));
                let total_value =
                    sum_present_values(key_balances.iter().map(|balance| balance.total_value));
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
                let confidence = key_balances
                    .iter()
                    .map(|balance| balance.confidence)
                    .fold(1.0_f64, f64::min);
                let collected_at = key_balances
                    .iter()
                    .filter_map(|balance| balance.collected_at.as_ref())
                    .max()
                    .cloned();
                value.map(|value| {
                    (
                        value,
                        used_value,
                        total_value,
                        currency,
                        credit_unit,
                        confidence,
                        collected_at,
                    )
                })
            })
        else {
            continue;
        };

        balances.push(CollectedBalanceFact {
            station_id,
            station_key_id: None,
            scope: "station".to_string(),
            value: Some(value),
            used_value,
            total_value,
            currency,
            credit_unit,
            status: if value == 0.0 { "depleted" } else { "normal" }.to_string(),
            source: "station_key_balance_aggregate".to_string(),
            confidence,
            collected_at,
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        group_facts::{
            UpsertStationGroupBindingInput, BINDING_KIND_STATION_GROUP, BINDING_STATUS_AVAILABLE,
        },
        station_keys::CreateStationKeyInput,
        stations::CreateStationInput,
    };

    fn create_test_station(database: &AppDatabase) -> crate::models::stations::Station {
        database
            .create_station(CreateStationInput {
                name: "balance aggregate relay".to_string(),
                station_type: "sub2api".to_string(),
                base_url: "https://relay.example.test".to_string(),
                api_key: "sk-test".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station")
    }

    fn create_test_key(
        database: &AppDatabase,
        station_id: &str,
        name: &str,
    ) -> crate::models::station_keys::StationKey {
        database
            .create_station_key(CreateStationKeyInput {
                station_id: station_id.to_string(),
                name: name.to_string(),
                api_key: format!("sk-{name}"),
                enabled: true,
                priority: None,
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: Some("station_key".to_string()),
                note: None,
            })
            .expect("station key")
    }

    fn key_balance(
        station_id: &str,
        station_key_id: &str,
        value: f64,
    ) -> crate::services::collectors::facts::CollectedBalanceFact {
        crate::services::collectors::facts::CollectedBalanceFact {
            station_id: station_id.to_string(),
            station_key_id: Some(station_key_id.to_string()),
            scope: "station_key".to_string(),
            value: Some(value),
            used_value: None,
            total_value: None,
            currency: "CNY".to_string(),
            credit_unit: None,
            status: "normal".to_string(),
            source: "sub2api_usage".to_string(),
            confidence: 0.9,
            collected_at: Some("1000".to_string()),
        }
    }

    #[test]
    fn key_balance_facts_create_station_scope_balance_snapshot() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = create_test_station(&database);
        let first_key = create_test_key(&database, &station.id, "first");
        let second_key = create_test_key(&database, &station.id, "second");

        let mut facts = crate::services::collectors::facts::CollectorFacts::default();
        facts
            .balances
            .push(key_balance(&station.id, &first_key.id, 4.25));
        facts
            .balances
            .push(key_balance(&station.id, &second_key.id, 6.75));

        apply_collector_facts(&database, facts).expect("apply facts");

        let balances = database.list_balance_snapshots().expect("balances");
        let station_balance = balances
            .iter()
            .find(|balance| balance.station_id == station.id && balance.scope == "station")
            .expect("station balance");
        assert_eq!(station_balance.value, Some(11.0));
        assert_eq!(station_balance.source, "station_key_balance_aggregate");
    }

    #[test]
    fn group_facts_mark_missing_station_groups_absent_from_latest_collection() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = create_test_station(&database);
        let stale = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "stale-hash".to_string(),
                group_id_hash: Some("stale".to_string()),
                group_name: "stale".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(0.5),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(0.5),
                rate_source: Some("sub2api_groups_rates".to_string()),
                confidence: 0.9,
                last_seen_at: Some("1000".to_string()),
                raw_json_redacted: None,
            })
            .expect("stale binding");

        let mut facts = crate::services::collectors::facts::CollectorFacts::default();
        facts
            .groups
            .push(crate::services::collectors::facts::CollectedGroupFact {
                station_id: station.id.clone(),
                group_id: Some("fresh".to_string()),
                group_key_hash: "fresh-hash".to_string(),
                group_name: "fresh".to_string(),
                visibility: "available".to_string(),
                source: "sub2api_groups_rates".to_string(),
                confidence: 0.9,
                raw_json_redacted: None,
            });

        apply_collector_facts(&database, facts).expect("apply group facts");

        let bindings = database
            .list_station_group_bindings(station.id.clone())
            .expect("bindings");
        let stale = bindings
            .iter()
            .find(|binding| binding.id == stale.id)
            .expect("stale binding remains for history");
        let fresh = bindings
            .iter()
            .find(|binding| binding.group_key_hash == "fresh-hash")
            .expect("fresh binding");

        assert_eq!(fresh.binding_status, "available");
        assert_eq!(stale.binding_status, "missing");
        assert!(database
            .list_change_events()
            .expect("events")
            .iter()
            .any(|event| event.event_type == "group_missing"
                && event.object_id.as_deref() == Some(stale.id.as_str())));
    }

    #[test]
    fn group_added_event_is_enriched_when_rate_arrives_after_group_name() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = create_test_station(&database);
        let mut facts = crate::services::collectors::facts::CollectorFacts::default();
        facts
            .groups
            .push(crate::services::collectors::facts::CollectedGroupFact {
                station_id: station.id.clone(),
                group_id: Some("image-universal".to_string()),
                group_key_hash: "group-image-universal".to_string(),
                group_name: "万能生图".to_string(),
                visibility: "available".to_string(),
                source: "sub2api_groups_available".to_string(),
                confidence: 0.9,
                raw_json_redacted: None,
            });
        facts
            .rates
            .push(crate::services::collectors::facts::CollectedRateFact {
                station_id: station.id.clone(),
                station_key_id: None,
                group_id: Some("image-universal".to_string()),
                group_key_hash: "group-image-universal".to_string(),
                group_name: "万能生图".to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(1.0),
                source: "sub2api_groups_rates".to_string(),
                confidence: 0.9,
                checked_at: Some("1000".to_string()),
                raw_json_redacted: None,
            });

        apply_collector_facts(&database, facts).expect("apply group and rate facts");

        let event = database
            .list_change_events()
            .expect("events")
            .into_iter()
            .find(|event| event.event_type == "group_added")
            .expect("group added event");
        let new_value: serde_json::Value =
            serde_json::from_str(event.new_value_json.as_deref().expect("new value"))
                .expect("new value json");

        assert_eq!(new_value["groupName"], "万能生图");
        assert_eq!(new_value["effectiveRateMultiplier"], 1.0);
    }

    #[test]
    fn sub2api_group_facts_mark_remote_scan_groups_missing_when_absent_from_latest_collection() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = create_test_station(&database);
        let stale = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "remote-scan-claude-aws".to_string(),
                group_id_hash: Some("claude-aws".to_string()),
                group_name: "claude-aws".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(0.22),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(0.22),
                rate_source: Some("remote_scan".to_string()),
                confidence: 0.9,
                last_seen_at: Some("1000".to_string()),
                raw_json_redacted: None,
            })
            .expect("remote scan binding");

        let mut facts = crate::services::collectors::facts::CollectorFacts::default();
        facts
            .groups
            .push(crate::services::collectors::facts::CollectedGroupFact {
                station_id: station.id.clone(),
                group_id: Some("codex-0".to_string()),
                group_key_hash: "latest-codex-0".to_string(),
                group_name: "codex-0号池".to_string(),
                visibility: "available".to_string(),
                source: "sub2api_groups_available".to_string(),
                confidence: 0.9,
                raw_json_redacted: Some(serde_json::json!({
                    "id": 42,
                    "name": "codex-0号池",
                    "platform": "openai"
                })),
            });

        apply_collector_facts(&database, facts).expect("apply group facts");

        let bindings = database
            .list_station_group_bindings(station.id.clone())
            .expect("bindings");
        let stale = bindings
            .iter()
            .find(|binding| binding.id == stale.id)
            .expect("stale binding remains for history");

        assert_eq!(stale.binding_status, "missing");
    }
}
