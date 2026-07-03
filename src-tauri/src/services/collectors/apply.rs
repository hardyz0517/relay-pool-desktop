use crate::{
    models::{
        group_facts::{InsertGroupRateRecordInput, UpsertStationGroupBindingInput},
        pricing::UpsertBalanceSnapshotInput,
    },
    services::{collectors::facts::CollectorFacts, database::AppDatabase},
};

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

    Ok(())
}
