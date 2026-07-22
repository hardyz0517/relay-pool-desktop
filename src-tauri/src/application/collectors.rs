use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    application::{
        clock::Clock,
        error::ApplicationError,
        ids::IdGenerator,
        pagination::{PageLimit, MAX_PAGE_LIMIT},
    },
    models::{
        collector::{CollectorEvent, CollectorRunResult},
        collector_runs::CollectorRun,
        group_facts::{
            GroupRateRecord, StationGroupBinding, UpsertStationGroupBindingInput,
            BINDING_KIND_KEY_BINDING, BINDING_KIND_STATION_GROUP, BINDING_STATUS_AVAILABLE,
            BINDING_STATUS_BOUND, BINDING_STATUS_DISABLED, BINDING_STATUS_MANUAL_LEGACY,
            BINDING_STATUS_MISSING,
        },
        shared_capabilities::StationGroupOption,
        stations::Station,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            change_store::{ChangeStore, NewChangeEvent},
            collector_store::{
                BalanceWrite, CollectorRunFinish, CollectorRunStart, CollectorSnapshotWrite,
                CollectorStore, CollectorTaskStateWrite, GroupState, GroupTransition, GroupWrite,
                ModelWrite, RateTransition, RateWrite, StationGroupBindingWrite,
                StoredCollectorApply,
            },
            station_catalog::StationCatalogStore,
        },
    },
    services::group_categories::normalize_group_category,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CollectorApplyOutcome {
    pub run_id: String,
    pub snapshot_id: String,
    pub inserted: bool,
}

impl From<StoredCollectorApply> for CollectorApplyOutcome {
    fn from(stored: StoredCollectorApply) -> Self {
        Self {
            run_id: stored.run_id,
            snapshot_id: stored.snapshot_id,
            inserted: stored.inserted,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CollectorApplyRequest {
    pub run_key: String,
    pub station_id: String,
    pub endpoint_revision: i64,
    pub parent_run_id: Option<String>,
    pub adapter: String,
    pub task_type: String,
    pub status: String,
    pub facts: CanonicalCollectorFacts,
    pub summary_json: Value,
    pub normalized_json: Value,
    pub raw_json_redacted: Option<Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub endpoint_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub manual_action_required: bool,
    pub next_due_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CaptureSnapshotRequest {
    pub station_id: String,
    pub endpoint_revision: i64,
    pub status: String,
    pub summary_json: Value,
    pub normalized_json: Value,
    pub raw_json_redacted: Option<Value>,
    pub error_message: Option<String>,
    pub event_count: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct CanonicalCollectorFacts {
    pub balances: Vec<CanonicalBalanceFact>,
    pub groups: Vec<CanonicalGroupFact>,
    pub rates: Vec<CanonicalRateFact>,
    pub models: Vec<CanonicalModelFact>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CanonicalBalanceFact {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub scope: String,
    pub value: Option<f64>,
    pub used_value: Option<f64>,
    pub total_value: Option<f64>,
    pub today_request_count: Option<i64>,
    pub total_request_count: Option<i64>,
    pub today_consumption: Option<f64>,
    pub total_consumption: Option<f64>,
    pub today_base_consumption: Option<f64>,
    pub total_base_consumption: Option<f64>,
    pub today_token_count: Option<i64>,
    pub total_token_count: Option<i64>,
    pub today_input_token_count: Option<i64>,
    pub today_output_token_count: Option<i64>,
    pub total_input_token_count: Option<i64>,
    pub total_output_token_count: Option<i64>,
    pub account_concurrency_limit: Option<i64>,
    pub currency: String,
    pub credit_unit: Option<String>,
    pub status: String,
    pub source: String,
    pub confidence: f64,
    pub collected_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CanonicalGroupFact {
    pub station_id: String,
    pub group_id: Option<String>,
    pub group_key_hash: String,
    pub group_name: String,
    pub source: String,
    pub confidence: f64,
    pub inferred_group_category: Option<String>,
    pub raw_json_redacted: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CanonicalRateFact {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub group_id: Option<String>,
    pub group_key_hash: String,
    pub group_name: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub source: String,
    pub confidence: f64,
    pub checked_at: Option<String>,
    pub raw_json_redacted: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CanonicalModelFact {
    pub station_id: String,
    pub model: String,
    pub available: bool,
    pub source: String,
    pub confidence: f64,
}

#[derive(Clone)]
pub(crate) struct CollectorService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    collectors: CollectorStore,
    stations: StationCatalogStore,
    changes: ChangeStore,
}

impl CollectorService {
    pub(crate) async fn result_for_apply(
        &self,
        outcome: &CollectorApplyOutcome,
        task_type: &str,
    ) -> Result<CollectorRunResult, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let snapshot = self
            .collectors
            .snapshot_by_id(&mut read, &outcome.snapshot_id)
            .await?;
        let message = snapshot
            .error_message
            .clone()
            .unwrap_or_else(|| snapshot.source.clone());
        let status = snapshot.status.clone();
        Ok(CollectorRunResult {
            snapshot,
            events: vec![CollectorEvent {
                event_type: task_type.to_string(),
                message,
                status,
            }],
        })
    }

    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            runtime,
            clock,
            ids,
            collectors: CollectorStore,
            stations: StationCatalogStore,
            changes: ChangeStore,
        }
    }

    pub(crate) async fn station_for_collection(
        &self,
        station_id: &str,
    ) -> Result<Station, ApplicationError> {
        if station_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.stations
            .get(&mut read, station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn due_stations(
        &self,
        limit: crate::application::pagination::PageLimit,
    ) -> Result<Vec<Station>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.stations
            .due_collectors(
                &mut read,
                self.clock.now_utc().timestamp_millis(),
                limit.get(),
            )
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_station_group_bindings(
        &self,
        station_id: &str,
    ) -> Result<Vec<StationGroupBinding>, ApplicationError> {
        validate_station_id(station_id)?;
        let limit = PageLimit::new(MAX_PAGE_LIMIT)?;
        let mut read = self.runtime.begin_read().await?;
        self.stations.get(&mut read, station_id).await?;
        self.collectors
            .list_station_group_bindings(&mut read, station_id, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_station_group_options(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<StationGroupOption>, ApplicationError> {
        validate_station_id(station_id)?;
        let mut read = self.runtime.begin_read().await?;
        self.stations.get(&mut read, station_id).await?;
        let bindings = self
            .collectors
            .list_selectable_station_group_bindings(&mut read, station_id, limit.get())
            .await?;
        let rates = self
            .collectors
            .list_latest_station_group_rates(&mut read, station_id, limit.get())
            .await?;
        Ok(crate::services::shared_capabilities::station_group_options_from_facts(bindings, rates))
    }

    pub(crate) async fn upsert_station_group_binding(
        &self,
        input: UpsertStationGroupBindingInput,
    ) -> Result<StationGroupBinding, ApplicationError> {
        let now = self.clock.now_utc().timestamp_millis().to_string();
        let binding = normalize_station_group_binding(input, self.ids.next_id(), now)?;
        let expected_revision = self
            .station_for_collection(&binding.station_id)
            .await?
            .endpoint_revision;
        let collectors = self.collectors;
        let changes = self.changes;
        let ids = self.ids.clone();

        self.runtime
            .write(move |write| {
                Box::pin(async move {
                    collectors
                        .assert_endpoint_revision(write, &binding.station_id, expected_revision)
                        .await?;
                    let stored = collectors
                        .upsert_station_group_binding(write, &binding)
                        .await?;
                    if let Some(event) =
                        binding_upsert_event(ids.as_ref(), &stored.transition, &binding.now)
                    {
                        changes.upsert(write, &event).await?;
                    }
                    Ok(stored.binding)
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_group_rate_records(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<GroupRateRecord>, ApplicationError> {
        validate_station_id(station_id)?;
        let mut read = self.runtime.begin_read().await?;
        self.stations.get(&mut read, station_id).await?;
        self.collectors
            .list_group_rate_records(&mut read, station_id, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_collector_runs(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<CollectorRun>, ApplicationError> {
        validate_station_id(station_id)?;
        let mut read = self.runtime.begin_read().await?;
        self.stations.get(&mut read, station_id).await?;
        self.collectors
            .list_collector_runs(&mut read, station_id, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_station_snapshots(
        &self,
        station_id: &str,
        limit: crate::application::pagination::PageLimit,
    ) -> Result<Vec<crate::models::collector::CollectorSnapshot>, ApplicationError> {
        if station_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.collectors
            .list_station_snapshots(&mut read, station_id, i64::from(limit.get()))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn latest_station_snapshot(
        &self,
        station_id: &str,
    ) -> Result<Option<crate::models::collector::CollectorSnapshot>, ApplicationError> {
        if station_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.collectors
            .latest_station_snapshot(&mut read, station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn record_capture_snapshot(
        &self,
        request: CaptureSnapshotRequest,
    ) -> Result<CollectorRunResult, ApplicationError> {
        if request.station_id.trim().is_empty()
            || request.endpoint_revision < 1
            || request.event_count < 0
            || !matches!(
                request.status.as_str(),
                "success" | "partial" | "failed" | "manual_required" | "needs_confirmation"
            )
        {
            return Err(ApplicationError::ConstraintViolation);
        }

        let request_hash = canonical_hash(&request)?;
        let run_status = match request.status.as_str() {
            "needs_confirmation" => "partial",
            status => status,
        }
        .to_string();
        let run_key = format!(
            "capture:{}:{}:{}",
            request.station_id, request.endpoint_revision, request_hash
        );
        let now = self.clock.now_utc().timestamp_millis().to_string();
        let run_id = self.ids.next_id();
        let snapshot_id = self.ids.next_id();
        let collectors = self.collectors;
        let outcome = self
            .runtime
            .write(move |write| {
                Box::pin(async move {
                    if let Some(existing) = collectors.existing_apply(write, &run_key).await? {
                        if existing.request_hash != request_hash {
                            return Err(
                                crate::persistence::error::PersistenceError::InvariantViolation(
                                    "capture run key was reused for a different snapshot"
                                        .to_string(),
                                ),
                            );
                        }
                        return Ok(CollectorApplyOutcome::from(existing.outcome));
                    }

                    collectors
                        .assert_endpoint_revision(
                            write,
                            &request.station_id,
                            request.endpoint_revision,
                        )
                        .await?;
                    collectors
                        .start_run(
                            write,
                            &CollectorRunStart {
                                id: run_id.clone(),
                                run_key,
                                request_hash,
                                station_id: request.station_id.clone(),
                                endpoint_revision: request.endpoint_revision,
                                parent_run_id: None,
                                adapter: "webview".to_string(),
                                task_type: "full".to_string(),
                                started_at: now.clone(),
                            },
                        )
                        .await?;
                    collectors
                        .insert_snapshot(
                            write,
                            &CollectorSnapshotWrite {
                                id: snapshot_id.clone(),
                                run_id: run_id.clone(),
                                station_id: request.station_id,
                                endpoint_revision: request.endpoint_revision,
                                source: "webview-capture".to_string(),
                                status: request.status.clone(),
                                fetched_at: now.clone(),
                                summary_json: request.summary_json,
                                normalized_json: request.normalized_json,
                                raw_json_redacted: request.raw_json_redacted,
                                error_message: request.error_message.clone(),
                                created_at: now.clone(),
                            },
                        )
                        .await?;
                    collectors
                        .finish_run(
                            write,
                            &CollectorRunFinish {
                                id: run_id,
                                status: run_status.clone(),
                                finished_at: now,
                                duration_ms: 0,
                                endpoint_count: request.event_count,
                                success_count: if run_status == "failed" {
                                    0
                                } else {
                                    request.event_count
                                },
                                failure_count: if run_status == "failed" {
                                    request.event_count
                                } else {
                                    0
                                },
                                manual_action_required: request.event_count == 0
                                    || matches!(
                                        request.status.as_str(),
                                        "manual_required" | "needs_confirmation"
                                    ),
                                error_code: None,
                                error_message: request.error_message,
                                snapshot_id,
                            },
                        )
                        .await
                        .map(CollectorApplyOutcome::from)
                })
            })
            .await?;

        let mut result = self.result_for_apply(&outcome, "full").await?;
        result.events.clear();
        Ok(result)
    }

    pub(crate) async fn apply_result(
        &self,
        request: CollectorApplyRequest,
    ) -> Result<CollectorApplyOutcome, ApplicationError> {
        validate_request(&request)?;
        let request_hash = canonical_hash(&request)?;
        let now = self.clock.now_utc().timestamp_millis().to_string();
        let started_ms = now.parse::<i64>().unwrap_or_default();
        let run_id = self.ids.next_id();
        let snapshot_id = self.ids.next_id();
        let ids = self.ids.clone();
        let collectors = self.collectors;
        let changes = self.changes;

        self.runtime
            .write(move |write| {
                Box::pin(async move {
                    if let Some(existing) =
                        collectors.existing_apply(write, &request.run_key).await?
                    {
                        if existing.request_hash != request_hash {
                            return Err(
                                crate::persistence::error::PersistenceError::InvariantViolation(
                                    "collector run key was reused for a different canonical result"
                                        .to_string(),
                                ),
                            );
                        }
                        return Ok(existing.outcome.into());
                    }

                    collectors
                        .assert_endpoint_revision(
                            write,
                            &request.station_id,
                            request.endpoint_revision,
                        )
                        .await?;
                    collectors
                        .start_run(
                            write,
                            &CollectorRunStart {
                                id: run_id.clone(),
                                run_key: request.run_key.clone(),
                                request_hash,
                                station_id: request.station_id.clone(),
                                endpoint_revision: request.endpoint_revision,
                                parent_run_id: request.parent_run_id.clone(),
                                adapter: request.adapter.clone(),
                                task_type: request.task_type.clone(),
                                started_at: now.clone(),
                            },
                        )
                        .await?;
                    collectors
                        .insert_snapshot(
                            write,
                            &CollectorSnapshotWrite {
                                id: snapshot_id.clone(),
                                run_id: run_id.clone(),
                                station_id: request.station_id.clone(),
                                endpoint_revision: request.endpoint_revision,
                                source: format!("{}-{}", request.adapter, request.task_type),
                                status: request.status.clone(),
                                fetched_at: now.clone(),
                                summary_json: request.summary_json.clone(),
                                normalized_json: request.normalized_json.clone(),
                                raw_json_redacted: request.raw_json_redacted.clone(),
                                error_message: request.error_message.clone(),
                                created_at: now.clone(),
                            },
                        )
                        .await?;

                    for balance in &request.facts.balances {
                        collectors
                            .insert_balance(
                                write,
                                &BalanceWrite {
                                    id: ids.next_id(),
                                    station_id: balance.station_id.clone(),
                                    station_key_id: balance.station_key_id.clone(),
                                    scope: balance.scope.clone(),
                                    value: balance.value,
                                    used_value: balance.used_value,
                                    total_value: balance.total_value,
                                    today_request_count: balance.today_request_count,
                                    total_request_count: balance.total_request_count,
                                    today_consumption: balance.today_consumption,
                                    total_consumption: balance.total_consumption,
                                    today_base_consumption: balance.today_base_consumption,
                                    total_base_consumption: balance.total_base_consumption,
                                    today_token_count: balance.today_token_count,
                                    total_token_count: balance.total_token_count,
                                    today_input_token_count: balance.today_input_token_count,
                                    today_output_token_count: balance.today_output_token_count,
                                    total_input_token_count: balance.total_input_token_count,
                                    total_output_token_count: balance.total_output_token_count,
                                    account_concurrency_limit: balance.account_concurrency_limit,
                                    currency: balance.currency.clone(),
                                    credit_unit: balance.credit_unit.clone(),
                                    status: balance.status.clone(),
                                    source: balance.source.clone(),
                                    confidence: balance.confidence,
                                    collected_at: balance.collected_at.clone(),
                                    now: now.clone(),
                                },
                            )
                            .await?;
                    }

                    let mut group_transitions = HashMap::<String, GroupTransition>::new();
                    let mut collection_scopes =
                        HashMap::<String, (HashSet<String>, HashSet<String>)>::new();
                    for group in &request.facts.groups {
                        let transition = collectors
                            .upsert_group(
                                write,
                                &GroupWrite {
                                    id: ids.next_id(),
                                    station_id: group.station_id.clone(),
                                    station_key_id: None,
                                    binding_kind: "station_group".to_string(),
                                    group_key_hash: group.group_key_hash.clone(),
                                    group_id_hash: group.group_id.clone(),
                                    group_name: group.group_name.clone(),
                                    binding_status: "available".to_string(),
                                    default_rate_multiplier: None,
                                    user_rate_multiplier: None,
                                    effective_rate_multiplier: None,
                                    inferred_group_category: group.inferred_group_category.clone(),
                                    source: group.source.clone(),
                                    confidence: group.confidence,
                                    last_seen_at: Some(now.clone()),
                                    raw_json_redacted: group.raw_json_redacted.clone(),
                                    run_id: run_id.clone(),
                                    now: now.clone(),
                                },
                            )
                            .await?;
                        remember_group_scope(
                            &mut collection_scopes,
                            group.station_id.clone(),
                            &group.source,
                            group.group_key_hash.clone(),
                        );
                        group_transitions.insert(transition.current.id.clone(), transition);
                    }

                    let mut rate_transitions = Vec::<RateTransition>::new();
                    for rate in &request.facts.rates {
                        let binding_kind = if rate.station_key_id.is_some() {
                            "key_binding"
                        } else {
                            "station_group"
                        };
                        let transition = collectors
                            .upsert_group(
                                write,
                                &GroupWrite {
                                    id: ids.next_id(),
                                    station_id: rate.station_id.clone(),
                                    station_key_id: rate.station_key_id.clone(),
                                    binding_kind: binding_kind.to_string(),
                                    group_key_hash: rate.group_key_hash.clone(),
                                    group_id_hash: rate.group_id.clone(),
                                    group_name: rate.group_name.clone(),
                                    binding_status: if rate.station_key_id.is_some() {
                                        "bound".to_string()
                                    } else {
                                        "available".to_string()
                                    },
                                    default_rate_multiplier: rate.default_rate_multiplier,
                                    user_rate_multiplier: rate.user_rate_multiplier,
                                    effective_rate_multiplier: rate.effective_rate_multiplier,
                                    inferred_group_category: rate.inferred_group_category.clone(),
                                    source: rate.source.clone(),
                                    confidence: rate.confidence,
                                    last_seen_at: rate
                                        .checked_at
                                        .clone()
                                        .or_else(|| Some(now.clone())),
                                    raw_json_redacted: rate.raw_json_redacted.clone(),
                                    run_id: run_id.clone(),
                                    now: now.clone(),
                                },
                            )
                            .await?;
                        let binding_id = transition.current.id.clone();
                        if rate.station_key_id.is_none() {
                            remember_group_scope(
                                &mut collection_scopes,
                                rate.station_id.clone(),
                                &rate.source,
                                rate.group_key_hash.clone(),
                            );
                        }
                        group_transitions
                            .entry(binding_id.clone())
                            .and_modify(|remembered| {
                                remembered.current = transition.current.clone()
                            })
                            .or_insert(transition);
                        if let Some(transition) = collectors
                            .insert_rate_if_changed(
                                write,
                                &RateWrite {
                                    id: ids.next_id(),
                                    station_id: rate.station_id.clone(),
                                    station_key_id: rate.station_key_id.clone(),
                                    group_binding_id: binding_id,
                                    binding_kind: binding_kind.to_string(),
                                    group_key_hash: rate.group_key_hash.clone(),
                                    group_name: rate.group_name.clone(),
                                    default_rate_multiplier: rate.default_rate_multiplier,
                                    user_rate_multiplier: rate.user_rate_multiplier,
                                    effective_rate_multiplier: rate.effective_rate_multiplier,
                                    inferred_group_category: rate.inferred_group_category.clone(),
                                    source: rate.source.clone(),
                                    confidence: rate.confidence,
                                    raw_json_redacted: rate.raw_json_redacted.clone(),
                                    checked_at: rate
                                        .checked_at
                                        .clone()
                                        .unwrap_or_else(|| now.clone()),
                                    created_at: now.clone(),
                                },
                            )
                            .await?
                        {
                            rate_transitions.push(transition);
                        }
                    }

                    for (station_id, (sources, hashes)) in collection_scopes {
                        for transition in collectors
                            .mark_missing_groups(write, &station_id, &sources, &hashes, &now)
                            .await?
                        {
                            group_transitions.insert(transition.current.id.clone(), transition);
                        }
                    }

                    for transition in group_transitions.values() {
                        if let Some(event) = group_event(ids.as_ref(), transition, &now) {
                            changes.upsert(write, &event).await?;
                        }
                    }
                    for transition in rate_transitions
                        .iter()
                        .filter(|transition| transition.old_effective_rate_multiplier.is_some())
                    {
                        changes
                            .upsert(
                                write,
                                &rate_event(ids.as_ref(), &request.station_id, transition, &now),
                            )
                            .await?;
                    }

                    if matches!(request.task_type.as_str(), "models" | "full") {
                        let models = request
                            .facts
                            .models
                            .iter()
                            .map(|model| ModelWrite {
                                station_id: model.station_id.clone(),
                                model: model.model.clone(),
                                available: model.available,
                                source: model.source.clone(),
                                confidence: model.confidence,
                                run_id: run_id.clone(),
                                now: now.clone(),
                            })
                            .collect::<Vec<_>>();
                        let transitions = collectors
                            .replace_models(write, &request.station_id, &run_id, &models, &now)
                            .await?;
                        if supports_model_events(&request.adapter) {
                            for model in transitions.added {
                                changes
                                    .upsert(
                                        write,
                                        &model_event(
                                            ids.as_ref(),
                                            &request.station_id,
                                            &model,
                                            true,
                                            &now,
                                        ),
                                    )
                                    .await?;
                            }
                            for model in transitions.removed {
                                changes
                                    .upsert(
                                        write,
                                        &model_event(
                                            ids.as_ref(),
                                            &request.station_id,
                                            &model,
                                            false,
                                            &now,
                                        ),
                                    )
                                    .await?;
                            }
                        }
                    }

                    let failure_key =
                        collector_failure_key(&request.station_id, &request.task_type);
                    if matches!(request.status.as_str(), "success" | "partial") {
                        changes
                            .resolve_by_dedupe_key(write, &failure_key, &now)
                            .await?;
                    } else if request.status == "failed" {
                        changes
                            .upsert(
                                write,
                                &collector_failure_event(
                                    ids.as_ref(),
                                    &request,
                                    &failure_key,
                                    &now,
                                ),
                            )
                            .await?;
                    }

                    collectors
                        .update_task_state(
                            write,
                            &CollectorTaskStateWrite {
                                station_id: request.station_id.clone(),
                                task_type: request.task_type.clone(),
                                run_id: run_id.clone(),
                                status: request.status.clone(),
                                finished_at: now.clone(),
                                next_due_at: request.next_due_at.clone(),
                            },
                        )
                        .await?;
                    let stored = collectors
                        .finish_run(
                            write,
                            &CollectorRunFinish {
                                id: run_id,
                                status: request.status,
                                finished_at: now.clone(),
                                duration_ms: now.parse::<i64>().unwrap_or(started_ms) - started_ms,
                                endpoint_count: request.endpoint_count,
                                success_count: request.success_count,
                                failure_count: request.failure_count,
                                manual_action_required: request.manual_action_required,
                                error_code: request.error_code,
                                error_message: request.error_message,
                                snapshot_id,
                            },
                        )
                        .await?;
                    Ok(stored.into())
                })
            })
            .await
            .map_err(Into::into)
    }
}

fn validate_station_id(station_id: &str) -> Result<(), ApplicationError> {
    if station_id.trim().is_empty() {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(())
}

fn normalize_station_group_binding(
    input: UpsertStationGroupBindingInput,
    id: String,
    now: String,
) -> Result<StationGroupBindingWrite, ApplicationError> {
    let station_id = required_trimmed(input.station_id)?;
    let station_key_id = optional_trimmed(input.station_key_id);
    let binding_kind = match input.binding_kind.trim() {
        BINDING_KIND_STATION_GROUP => BINDING_KIND_STATION_GROUP.to_string(),
        BINDING_KIND_KEY_BINDING => BINDING_KIND_KEY_BINDING.to_string(),
        _ => return Err(ApplicationError::ConstraintViolation),
    };
    if (binding_kind == BINDING_KIND_STATION_GROUP && station_key_id.is_some())
        || (binding_kind == BINDING_KIND_KEY_BINDING && station_key_id.is_none())
    {
        return Err(ApplicationError::ConstraintViolation);
    }
    let binding_status = match input.binding_status.trim() {
        BINDING_STATUS_AVAILABLE => BINDING_STATUS_AVAILABLE.to_string(),
        BINDING_STATUS_BOUND => BINDING_STATUS_BOUND.to_string(),
        BINDING_STATUS_MISSING => BINDING_STATUS_MISSING.to_string(),
        BINDING_STATUS_DISABLED => BINDING_STATUS_DISABLED.to_string(),
        BINDING_STATUS_MANUAL_LEGACY => BINDING_STATUS_MANUAL_LEGACY.to_string(),
        _ => return Err(ApplicationError::ConstraintViolation),
    };
    let default_rate_multiplier = validated_multiplier(input.default_rate_multiplier)?;
    let user_rate_multiplier = validated_multiplier(input.user_rate_multiplier)?;
    let effective_rate_multiplier = validated_multiplier(input.effective_rate_multiplier)?;
    if !input.confidence.is_finite() || !(0.0..=1.0).contains(&input.confidence) {
        return Err(ApplicationError::ConstraintViolation);
    }

    Ok(StationGroupBindingWrite {
        id,
        station_id,
        station_key_id,
        binding_kind,
        parent_group_binding_id: optional_trimmed(input.parent_group_binding_id),
        group_key_hash: required_trimmed(input.group_key_hash)?,
        group_id_hash: optional_trimmed(input.group_id_hash),
        group_name: required_trimmed(input.group_name)?,
        binding_status,
        default_rate_multiplier,
        user_rate_multiplier,
        effective_rate_multiplier,
        inferred_group_category: validated_group_category(input.inferred_group_category)?,
        group_category_override: validated_group_category(input.group_category_override)?,
        rate_source: optional_trimmed(input.rate_source),
        confidence: input.confidence,
        last_seen_at: optional_trimmed(input.last_seen_at),
        raw_json_redacted: input.raw_json_redacted,
        now,
    })
}

fn required_trimmed(value: String) -> Result<String, ApplicationError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(value)
}

fn optional_trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validated_multiplier(value: Option<f64>) -> Result<Option<f64>, ApplicationError> {
    if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(value)
}

fn validated_group_category(value: Option<String>) -> Result<Option<String>, ApplicationError> {
    let value = optional_trimmed(value);
    match value {
        Some(value) => normalize_group_category(Some(&value))
            .map(Some)
            .ok_or(ApplicationError::ConstraintViolation),
        None => Ok(None),
    }
}

fn validate_request(request: &CollectorApplyRequest) -> Result<(), ApplicationError> {
    if request.run_key.trim().is_empty()
        || request.station_id.trim().is_empty()
        || request.endpoint_revision < 1
        || request.adapter.trim().is_empty()
        || !matches!(
            request.task_type.as_str(),
            "detect" | "balance" | "groups" | "models" | "full"
        )
        || !matches!(
            request.status.as_str(),
            "success" | "partial" | "failed" | "manual_required"
        )
        || request.endpoint_count < 0
        || request.success_count < 0
        || request.failure_count < 0
        || request.success_count + request.failure_count > request.endpoint_count
    {
        return Err(ApplicationError::ConstraintViolation);
    }
    let same_station = request
        .facts
        .balances
        .iter()
        .map(|fact| fact.station_id.as_str())
        .chain(
            request
                .facts
                .groups
                .iter()
                .map(|fact| fact.station_id.as_str()),
        )
        .chain(
            request
                .facts
                .rates
                .iter()
                .map(|fact| fact.station_id.as_str()),
        )
        .chain(
            request
                .facts
                .models
                .iter()
                .map(|fact| fact.station_id.as_str()),
        )
        .all(|station_id| station_id == request.station_id);
    if !same_station {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(())
}

fn canonical_hash(request: &impl Serialize) -> Result<String, ApplicationError> {
    let bytes = serde_json::to_vec(request).map_err(|_| ApplicationError::Internal)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn remember_group_scope(
    scopes: &mut HashMap<String, (HashSet<String>, HashSet<String>)>,
    station_id: String,
    source: &str,
    group_key_hash: String,
) {
    let scope = scopes.entry(station_id).or_default();
    scope.0.insert(source.to_string());
    if source.starts_with("sub2api_groups_") {
        scope.0.extend(
            [
                "sub2api_groups_available",
                "sub2api_groups_rates",
                "remote_scan",
            ]
            .map(String::from),
        );
    }
    scope.1.insert(group_key_hash);
}

fn group_event(
    ids: &dyn IdGenerator,
    transition: &GroupTransition,
    now: &str,
) -> Option<NewChangeEvent> {
    if transition.current.binding_kind != "station_group" {
        return None;
    }
    let was_available = transition
        .previous
        .as_ref()
        .is_some_and(|previous| previous.binding_status == "available");
    let is_available = transition.current.binding_status == "available";
    let (event_type, severity, title, message, old_value, new_value) =
        if !was_available && is_available {
            (
                "group_added",
                "info",
                "Group added",
                format!(
                    "Station group {} is available",
                    transition.current.group_name
                ),
                None,
                Some(group_value(&transition.current)),
            )
        } else if was_available && transition.current.binding_status == "missing" {
            (
                "group_missing",
                "warning",
                "Group missing",
                format!("Station group {} is missing", transition.current.group_name),
                transition.previous.as_ref().map(group_value),
                Some(group_value(&transition.current)),
            )
        } else {
            return None;
        };
    Some(NewChangeEvent {
        id: ids.next_id(),
        severity: severity.to_string(),
        event_type: event_type.to_string(),
        title: title.to_string(),
        message,
        object_type: "station_group_binding".to_string(),
        object_id: Some(transition.current.id.clone()),
        station_id: Some(transition.current.station_id.clone()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: old_value,
        new_value_json: new_value,
        impact_json: None,
        dedupe_key: format!(
            "collector:{}:{}:{}",
            transition.current.station_id, event_type, transition.current.group_key_hash
        ),
        source: "collector".to_string(),
        now: now.to_string(),
    })
}

fn binding_upsert_event(
    ids: &dyn IdGenerator,
    transition: &GroupTransition,
    now: &str,
) -> Option<NewChangeEvent> {
    if transition.current.binding_kind == BINDING_KIND_STATION_GROUP {
        return group_event(ids, transition, now);
    }
    if transition.current.binding_kind != BINDING_KIND_KEY_BINDING {
        return None;
    }
    let station_key_id = transition.current.station_key_id.as_ref()?;
    let previous_status = transition
        .previous
        .as_ref()
        .map(|previous| previous.binding_status.as_str());
    let (event_type, severity, title, message, new_value_json, impact_json) =
        if transition.current.binding_status == BINDING_STATUS_BOUND
            && previous_status != Some(BINDING_STATUS_BOUND)
        {
            (
                "key_group_bound",
                "info",
                "Key group bound",
                format!("Key was bound to group {}", transition.current.group_name),
                Some(
                    json!({
                        "groupBindingId": transition.current.id,
                        "groupName": transition.current.group_name
                    })
                    .to_string(),
                ),
                Some(json!({ "cheapFirstConfidence": "improved" }).to_string()),
            )
        } else if transition.current.binding_status == BINDING_STATUS_MISSING
            && previous_status != Some(BINDING_STATUS_MISSING)
        {
            (
                "key_group_unresolved",
                "warning",
                "Key group unresolved",
                "The collector could not resolve this key's group".to_string(),
                None,
                Some(json!({ "cheapFirstConfidence": "reduced" }).to_string()),
            )
        } else {
            return None;
        };

    Some(NewChangeEvent {
        id: ids.next_id(),
        severity: severity.to_string(),
        event_type: event_type.to_string(),
        title: title.to_string(),
        message,
        object_type: "station_key".to_string(),
        object_id: Some(station_key_id.clone()),
        station_id: Some(transition.current.station_id.clone()),
        station_key_id: Some(station_key_id.clone()),
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json,
        impact_json,
        dedupe_key: format!("{event_type}:station_key:{station_key_id}"),
        source: "collector".to_string(),
        now: now.to_string(),
    })
}

fn rate_event(
    ids: &dyn IdGenerator,
    station_id: &str,
    transition: &RateTransition,
    now: &str,
) -> NewChangeEvent {
    NewChangeEvent {
        id: ids.next_id(),
        severity: "warning".to_string(),
        event_type: "group_rate_changed".to_string(),
        title: "Group rate changed".to_string(),
        message: format!("Group {} rate changed", transition.group_name),
        object_type: "station_group_binding".to_string(),
        object_id: Some(transition.group_binding_id.clone()),
        station_id: Some(station_id.to_string()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: Some(
            json!({
                "effectiveRateMultiplier": transition.old_effective_rate_multiplier
            })
            .to_string(),
        ),
        new_value_json: Some(
            json!({
                "effectiveRateMultiplier": transition.new_effective_rate_multiplier
            })
            .to_string(),
        ),
        impact_json: Some(json!({ "routingCostMayChange": true }).to_string()),
        dedupe_key: format!(
            "collector:{}:group_rate_changed:{}",
            station_id, transition.group_binding_id
        ),
        source: "collector".to_string(),
        now: now.to_string(),
    }
}

fn model_event(
    ids: &dyn IdGenerator,
    station_id: &str,
    model: &str,
    added: bool,
    now: &str,
) -> NewChangeEvent {
    let event_type = if added {
        "model_added"
    } else {
        "model_removed"
    };
    NewChangeEvent {
        id: ids.next_id(),
        severity: if added { "info" } else { "warning" }.to_string(),
        event_type: event_type.to_string(),
        title: if added {
            "Model added"
        } else {
            "Model removed"
        }
        .to_string(),
        message: format!(
            "Model {model} was {}",
            if added { "added" } else { "removed" }
        ),
        object_type: "station".to_string(),
        object_id: Some(station_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: (!added).then(|| json!({ "model": model }).to_string()),
        new_value_json: added.then(|| json!({ "model": model }).to_string()),
        impact_json: (!added)
            .then(|| json!({ "routingRisk": "model_candidates_may_change" }).to_string()),
        dedupe_key: format!("collector:{station_id}:{event_type}:{model}"),
        source: "collector".to_string(),
        now: now.to_string(),
    }
}

fn collector_failure_event(
    ids: &dyn IdGenerator,
    request: &CollectorApplyRequest,
    dedupe_key: &str,
    now: &str,
) -> NewChangeEvent {
    NewChangeEvent {
        id: ids.next_id(),
        severity: "warning".to_string(),
        event_type: "collector_failed".to_string(),
        title: "Collector failed".to_string(),
        message: request
            .error_message
            .clone()
            .unwrap_or_else(|| format!("Collector task {} failed", request.task_type)),
        object_type: "station".to_string(),
        object_id: Some(request.station_id.clone()),
        station_id: Some(request.station_id.clone()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json: request
            .error_code
            .as_ref()
            .map(|code| json!({ "errorCode": code }).to_string()),
        impact_json: None,
        dedupe_key: dedupe_key.to_string(),
        source: "collector".to_string(),
        now: now.to_string(),
    }
}

fn collector_failure_key(station_id: &str, task_type: &str) -> String {
    format!("collector:{station_id}:collector_failed:{task_type}")
}

fn group_value(group: &GroupState) -> String {
    json!({
        "groupName": group.group_name,
        "status": group.binding_status,
        "defaultRateMultiplier": group.default_rate_multiplier,
        "userRateMultiplier": group.user_rate_multiplier,
        "effectiveRateMultiplier": group.effective_rate_multiplier
    })
    .to_string()
}

fn supports_model_events(adapter: &str) -> bool {
    matches!(adapter, "sub2api" | "newapi" | "openai-compatible")
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };

    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::*;
    use crate::{
        application::{error::ApplicationError, stations::StationService},
        models::stations::CreateStationInput,
        persistence::runtime::PersistenceRuntime,
    };

    struct FixedClock;

    impl Clock for FixedClock {
        fn now_utc(&self) -> chrono::DateTime<Utc> {
            Utc.timestamp_millis_opt(1_700_000_000_000)
                .single()
                .expect("valid timestamp")
        }
    }

    #[derive(Default)]
    struct SequenceIds(AtomicU64);

    impl IdGenerator for SequenceIds {
        fn next_id(&self) -> String {
            format!("capture-test-{}", self.0.fetch_add(1, Ordering::Relaxed))
        }
    }

    fn capture_request(station_id: &str, endpoint_revision: i64) -> CaptureSnapshotRequest {
        CaptureSnapshotRequest {
            station_id: station_id.to_string(),
            endpoint_revision,
            status: "success".to_string(),
            summary_json: json!({ "status": "success" }),
            normalized_json: json!({ "status": "success", "groups": [] }),
            raw_json_redacted: Some(json!({ "capture": "redacted" })),
            error_message: None,
            event_count: 1,
        }
    }

    fn group_binding_input(station_id: &str) -> UpsertStationGroupBindingInput {
        UpsertStationGroupBindingInput {
            station_id: station_id.to_string(),
            station_key_id: None,
            binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
            parent_group_binding_id: None,
            group_key_hash: "manual-group-hash".to_string(),
            group_id_hash: Some("manual-group-id".to_string()),
            group_name: "Manual Group".to_string(),
            binding_status: BINDING_STATUS_AVAILABLE.to_string(),
            default_rate_multiplier: None,
            user_rate_multiplier: Some(0.9),
            effective_rate_multiplier: Some(0.9),
            inferred_group_category: Some("GPT".to_string()),
            group_category_override: None,
            rate_source: Some("manual".to_string()),
            confidence: 1.0,
            last_seen_at: None,
            raw_json_redacted: None,
        }
    }

    #[tokio::test]
    async fn capture_snapshot_is_idempotent_and_rejects_stale_endpoint_revision() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("capture.sqlite3"))
            .await
            .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Capture Test".to_string(),
                station_type: "newapi".to_string(),
                website_url: "https://capture.example.test".to_string(),
                api_base_url: "https://capture.example.test/v1".to_string(),
                api_key: String::new(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .await
            .expect("station");
        let request = capture_request(&station.id, station.endpoint_revision);

        let first = collectors
            .record_capture_snapshot(request.clone())
            .await
            .expect("first capture snapshot");
        let replay = collectors
            .record_capture_snapshot(request)
            .await
            .expect("idempotent replay");

        assert_eq!(first.snapshot.id, replay.snapshot.id);
        let mut read = runtime.begin_read().await.expect("read session");
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_snapshots WHERE source = 'webview-capture'",
        )
        .fetch_one(read.connection())
        .await
        .expect("capture snapshot count");
        assert_eq!(count, 1);
        drop(read);

        runtime
            .write(|write| {
                let station_id = station.id.clone();
                Box::pin(async move {
                    sqlx::query("UPDATE stations SET endpoint_revision = 2 WHERE id = ?1")
                        .bind(station_id)
                        .execute(write.connection())
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("advance endpoint revision");
        let mut stale = capture_request(&station.id, station.endpoint_revision);
        stale.summary_json = json!({ "status": "success", "attempt": "stale" });

        let error = collectors
            .record_capture_snapshot(stale)
            .await
            .expect_err("stale capture must fail closed");
        assert!(matches!(error, ApplicationError::StaleRevision));
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn group_queries_and_collector_runs_use_bounded_v2_reads() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("group-queries.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Group Query Test".to_string(),
                station_type: "newapi".to_string(),
                website_url: "https://groups.example.test".to_string(),
                api_base_url: "https://groups.example.test/v1".to_string(),
                api_key: String::new(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .await
            .expect("station");

        let saved = collectors
            .upsert_station_group_binding(group_binding_input(&station.id))
            .await
            .expect("manual group binding");
        assert_eq!(saved.inferred_group_category.as_deref(), Some("gpt"));

        let mut invalid_key_binding = group_binding_input(&station.id);
        invalid_key_binding.binding_kind = BINDING_KIND_KEY_BINDING.to_string();
        invalid_key_binding.station_key_id = Some("missing-key".to_string());
        let error = collectors
            .upsert_station_group_binding(invalid_key_binding)
            .await
            .expect_err("foreign station key must be rejected");
        assert!(matches!(error, ApplicationError::ConstraintViolation));

        collectors
            .apply_result(CollectorApplyRequest {
                run_key: "group-query-run".to_string(),
                station_id: station.id.clone(),
                endpoint_revision: station.endpoint_revision,
                parent_run_id: None,
                adapter: "newapi".to_string(),
                task_type: "groups".to_string(),
                status: "success".to_string(),
                facts: CanonicalCollectorFacts {
                    rates: vec![CanonicalRateFact {
                        station_id: station.id.clone(),
                        station_key_id: None,
                        group_id: Some("remote-group-id".to_string()),
                        group_key_hash: "remote-group-hash".to_string(),
                        group_name: "Remote Group".to_string(),
                        default_rate_multiplier: Some(0.75),
                        user_rate_multiplier: None,
                        effective_rate_multiplier: Some(0.75),
                        inferred_group_category: Some("gpt".to_string()),
                        source: "groups_api".to_string(),
                        confidence: 0.95,
                        checked_at: Some("1700000000000".to_string()),
                        raw_json_redacted: None,
                    }],
                    ..CanonicalCollectorFacts::default()
                },
                summary_json: json!({ "groups": 1 }),
                normalized_json: json!({ "groups": ["Remote Group"] }),
                raw_json_redacted: None,
                error_code: None,
                error_message: None,
                endpoint_count: 1,
                success_count: 1,
                failure_count: 0,
                manual_action_required: false,
                next_due_at: None,
            })
            .await
            .expect("collector apply");

        let one = PageLimit::new(1).expect("bounded limit");
        let runs = collectors
            .list_collector_runs(&station.id, one)
            .await
            .expect("collector runs");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].endpoint_revision, station.endpoint_revision);
        assert_eq!(runs[0].status, "success");

        let rates = collectors
            .list_group_rate_records(&station.id, one)
            .await
            .expect("group rate records");
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].effective_rate_multiplier, Some(0.75));

        let options = collectors
            .list_station_group_options(&station.id, PageLimit::new(10).expect("bounded options"))
            .await
            .expect("station group options");
        assert_eq!(options.len(), 2);
        assert!(options.iter().any(|option| {
            option.group_name == "Remote Group" && option.rate_multiplier == Some(0.75)
        }));

        let bindings = collectors
            .list_station_group_bindings(&station.id)
            .await
            .expect("station group bindings");
        assert_eq!(bindings.len(), 2);
        runtime.close().await.expect("close persistence runtime");
    }
}
