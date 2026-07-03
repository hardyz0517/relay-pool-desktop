use crate::{
    models::{
        group_facts::UpsertStationGroupBindingInput,
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

    Ok(())
}
