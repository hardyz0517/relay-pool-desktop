use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    application::{
        alerting::{
            AlertingIngress, AlertingReadModelUpdatePublisher,
            NoopAlertingReadModelUpdatePublisher, ObservationIngress,
        },
        clock::Clock,
        error::ApplicationError,
        ids::IdGenerator,
        pagination::{PageLimit, MAX_PAGE_LIMIT},
    },
    models::{
        alerting::{AlertEventType, ObservationKind, Severity},
        collector::{CollectorEvent, CollectorRunResult, CollectorSnapshot},
        collector_runs::CollectorRun,
        group_facts::{
            GroupRateRecord, StationGroupBinding, UpsertStationGroupBindingInput,
            BINDING_KIND_KEY_BINDING, BINDING_KIND_STATION_GROUP, BINDING_STATUS_AVAILABLE,
            BINDING_STATUS_BOUND, BINDING_STATUS_DISABLED, BINDING_STATUS_MANUAL_LEGACY,
            BINDING_STATUS_MISSING,
        },
        shared_capabilities::StationGroupOption,
        station_published_status::{
            PublishedStatusBatch, PublishedStatusCompleteness, PublishedStatusSourceState,
            STATION_PUBLISHED_STATUS_SOURCE_KIND,
        },
        stations::Station,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            collector_store::{
                BalanceWrite, CollectorRunFinish, CollectorRunStart, CollectorSnapshotWrite,
                CollectorStore, CollectorTaskStateWrite, GroupTransition, GroupWrite,
                RateTransition, RateWrite, StationGroupBindingWrite, StoredCollectorApply,
            },
            station_catalog::StationCatalogStore,
            station_published_status_store::{
                PublishedMonitorSampleWrite, PublishedMonitorWrite, PublishedStatusSourceWrite,
                StationPublishedStatusStore,
            },
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
    pub execution_started_at_ms: Option<i64>,
    pub execution_duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CaptureSnapshotRequest {
    pub station_id: String,
    pub endpoint_revision: i64,
    pub task_type: String,
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
    pub published_status: Option<PublishedStatusBatch>,
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
    published_status: StationPublishedStatusStore,
    stations: StationCatalogStore,
    alerting: AlertingIngress,
    alerting_updates: Arc<dyn AlertingReadModelUpdatePublisher>,
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

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=alerting.read-model-update-test-constructor; owner=application/collectors; remove_when=all non-desktop compositions inject a read-model update publisher"
        )
    )]
    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self::new_with_alerting_read_model_updates(
            runtime,
            clock,
            ids,
            Arc::new(NoopAlertingReadModelUpdatePublisher),
        )
    }

    pub(crate) fn new_with_alerting_read_model_updates(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        alerting_updates: Arc<dyn AlertingReadModelUpdatePublisher>,
    ) -> Self {
        Self {
            runtime: runtime.clone(),
            clock,
            ids,
            collectors: CollectorStore,
            published_status: StationPublishedStatusStore,
            stations: StationCatalogStore,
            alerting: AlertingIngress::new(runtime.clone()),
            alerting_updates,
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

    pub(crate) async fn due_stations_for_task(
        &self,
        task_type: &str,
        interval_minutes: u16,
        limit: crate::application::pagination::PageLimit,
    ) -> Result<Vec<Station>, ApplicationError> {
        if !matches!(task_type, "balance" | "groups" | "published_status") || interval_minutes == 0
        {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.stations
            .due_collector_task(
                &mut read,
                task_type,
                interval_minutes,
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
        let alerting = self.alerting.clone();
        // Manual binding edits have no collector run id.  Allocate an
        // operation-scoped source key so a later missing/available episode is
        // not collapsed into the first occurrence for this binding.
        let source_observation_key = self.ids.next_id();

        let (binding, alerting_changed) = self
            .runtime
            .write(move |write| {
                Box::pin(async move {
                    collectors
                        .assert_endpoint_revision(write, &binding.station_id, expected_revision)
                        .await?;
                    let stored = collectors
                        .upsert_station_group_binding(write, &binding)
                        .await?;
                    let alerting_changed = if let Some(observation) = group_transition_observation(
                        &stored.transition,
                        &binding.now,
                        &source_observation_key,
                    ) {
                        alerting
                            .record_in_session(write, observation)
                            .await?
                            .inserted
                    } else {
                        false
                    };
                    Ok((stored.binding, alerting_changed))
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        if alerting_changed {
            self.alerting_updates.notify_after_commit();
        }
        Ok(binding)
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

    pub(crate) async fn list_latest_station_snapshots(
        &self,
        station_ids: Vec<String>,
    ) -> Result<Vec<CollectorSnapshot>, ApplicationError> {
        if station_ids.is_empty() {
            return Ok(Vec::new());
        }
        if station_ids.len() > MAX_PAGE_LIMIT as usize {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut unique = HashSet::with_capacity(station_ids.len());
        for station_id in &station_ids {
            validate_station_id(station_id)?;
            if !unique.insert(station_id.as_str()) {
                return Err(ApplicationError::ConstraintViolation);
            }
        }

        let mut read = self.runtime.begin_read().await?;
        let mut snapshots = Vec::new();
        for station_id in station_ids {
            if let Some(snapshot) = self
                .collectors
                .latest_station_snapshot(&mut read, &station_id)
                .await?
            {
                snapshots.push(snapshot);
            }
        }
        snapshots.sort_by(|left, right| left.station_id.cmp(&right.station_id));
        Ok(snapshots)
    }

    pub(crate) async fn record_capture_snapshot(
        &self,
        request: CaptureSnapshotRequest,
    ) -> Result<CollectorRunResult, ApplicationError> {
        if request.station_id.trim().is_empty()
            || request.endpoint_revision < 1
            || request.event_count < 0
            || !matches!(request.task_type.as_str(), "full" | "recharge")
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
                        .update_station_collection_status(
                            write,
                            &request.station_id,
                            request.endpoint_revision,
                            &run_status,
                            &now,
                            true,
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
        let applied_at_ms = self.clock.now_utc().timestamp_millis();
        let started_ms = request.execution_started_at_ms.unwrap_or(applied_at_ms);
        let duration_ms = request
            .execution_duration_ms
            .unwrap_or_else(|| applied_at_ms.saturating_sub(started_ms))
            .max(0);
        let finished_ms = started_ms.saturating_add(duration_ms);
        let started_at = started_ms.to_string();
        let now = finished_ms.to_string();
        let run_id = self.ids.next_id();
        let snapshot_id = self.ids.next_id();
        let ids = self.ids.clone();
        let collectors = self.collectors;
        let published_status = self.published_status;
        let alerting = self.alerting.clone();

        let (outcome, alerting_changed) = self
            .runtime
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
                        return Ok((existing.outcome.into(), false));
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
                                started_at: started_at.clone(),
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

                    if request.task_type == "published_status" {
                        apply_station_published_status(
                            &published_status,
                            write,
                            &*ids,
                            &request,
                            &run_id,
                            &now,
                            started_ms,
                        )
                        .await?;
                    }

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
                                    evidence_confidence: collector_balance_evidence_confidence(&balance.status),
                                    spendability_authority: collector_balance_authority(
                                        &balance.status,
                                        &balance.source,
                                    ),
                                    observed_at_ms: collector_balance_observed_at_ms(
                                        balance.collected_at.as_deref().unwrap_or(&now),
                                    ),
                                    valid_until_ms: collector_balance_observed_at_ms(
                                        balance.collected_at.as_deref().unwrap_or(&now),
                                    )
                                    .map(|value| value.saturating_add(30 * 60 * 1_000)),
                                    evidence_profile_version: "collector-balance-v1".to_string(),
                                    spendability_reason_code: collector_balance_reason(&balance.status),
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

                    let changed_group_binding_ids = group_transitions
                        .values()
                        .filter(|transition| {
                            transition.current.binding_kind == BINDING_KIND_STATION_GROUP
                        })
                        .map(|transition| transition.current.id.clone())
                        .collect::<HashSet<_>>();
                    collectors
                        .refresh_station_key_group_projections(
                            write,
                            &request.station_id,
                            &changed_group_binding_ids,
                            &now,
                        )
                        .await?;

                    let mut alerting_changed = false;
                    for transition in group_transitions.values() {
                        if let Some(observation) =
                            group_transition_observation(transition, &now, &run_id)
                        {
                            alerting_changed |= alerting
                                .record_in_session(write, observation)
                                .await?
                                .inserted;
                        }
                    }
                    for transition in rate_transitions
                        .iter()
                        .filter(|transition| transition.old_effective_rate_multiplier.is_some())
                    {
                        alerting_changed |= alerting
                            .record_in_session(
                                write,
                                rate_change_observation(
                                    &request.station_id,
                                    transition,
                                    &run_id,
                                    &now,
                                ),
                            )
                            .await?
                            .inserted;
                    }

                    // A full collection owns the lifecycle of its child tasks. Child
                    // runs still persist facts and run history, but must not create a
                    // second incident for the same collection operation.
                    let emit_collector_observation =
                        collector_task_side_effect_policy(&request.task_type)
                            .emits_collector_observation
                            && should_emit_collector_observation(request.parent_run_id.as_deref());
                    if emit_collector_observation
                        && should_record_collector_observation(&request.status)
                    {
                        let failed_task_types =
                            collector_failed_task_types(&collectors, write, &request).await?;
                        let kind = if failed_task_types.is_empty() {
                            ObservationKind::Healthy
                        } else {
                            ObservationKind::Abnormal
                        };
                        alerting_changed |= alerting
                            .record_in_session(
                                write,
                                collector_observation(
                                    &request,
                                    &collector_failure_key(&request.station_id),
                                    &run_id,
                                    kind,
                                    &failed_task_types,
                                    &now,
                                ),
                            )
                            .await?
                            .inserted;
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
                    if request.parent_run_id.is_none()
                        && task_updates_station_collection_status(&request.task_type)
                    {
                        collectors
                            .update_station_collection_status(
                                write,
                                &request.station_id,
                                request.endpoint_revision,
                                station_collection_status_for_request(&request),
                                &now,
                                collector_task_side_effect_policy(&request.task_type)
                                    .refreshes_group_bindings,
                            )
                            .await?;
                    }
                    let stored = collectors
                        .finish_run(
                            write,
                            &CollectorRunFinish {
                                id: run_id,
                                status: request.status,
                                finished_at: now.clone(),
                                duration_ms,
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
                    Ok((stored.into(), alerting_changed))
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        if alerting_changed {
            self.alerting_updates.notify_after_commit();
        }
        Ok(outcome)
    }
}

async fn apply_station_published_status(
    store: &StationPublishedStatusStore,
    write: &mut crate::persistence::WriteSession,
    ids: &dyn IdGenerator,
    request: &CollectorApplyRequest,
    run_id: &str,
    now: &str,
    now_ms: i64,
) -> Result<(), crate::persistence::error::PersistenceError> {
    let batch = request.facts.published_status.as_ref();
    if let Some(batch) = batch {
        batch
            .validate()
            .map_err(|_| crate::persistence::error::PersistenceError::ConstraintViolation)?;
        if batch.station_id != request.station_id
            || batch.endpoint_revision != request.endpoint_revision
            || batch.source_kind != STATION_PUBLISHED_STATUS_SOURCE_KIND
        {
            return Err(crate::persistence::error::PersistenceError::ConstraintViolation);
        }
    }

    let source_state = batch
        .map(|batch| batch.source_state)
        .unwrap_or_else(|| published_status_source_state_for_failed_apply(request));
    let successful_read = is_successful_published_status_read(
        source_state,
        batch.and_then(|batch| batch.safe_error_kind.as_deref()),
    );
    let complete_read = batch.is_some_and(|batch| {
        batch.completeness == PublishedStatusCompleteness::Complete && successful_read
    });
    let source = PublishedStatusSourceWrite {
        station_id: request.station_id.clone(),
        endpoint_revision: request.endpoint_revision,
        source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.to_string(),
        source_state: source_state.as_str().to_string(),
        last_attempt_at: now.to_string(),
        last_success_at: successful_read.then(|| now.to_string()),
        last_complete_at: complete_read.then(|| now.to_string()),
        last_error_kind: batch
            .and_then(|batch| batch.safe_error_kind.clone())
            .or_else(|| request.error_code.clone()),
        monitor_count: successful_read.then(|| {
            batch
                .map(|batch| batch.monitors.len() as i64)
                .unwrap_or_default()
        }),
        created_at: now.to_string(),
        updated_at: now.to_string(),
    };
    store.upsert_source(write, &source).await?;
    // A failed first read for a newly configured endpoint must not erase the
    // last verified facts. A successful read replaces the prior endpoint's
    // display facts in the same transaction.
    if successful_read {
        store
            .purge_other_endpoint_revisions(
                write,
                &request.station_id,
                request.endpoint_revision,
                STATION_PUBLISHED_STATUS_SOURCE_KIND,
            )
            .await?;
    }

    let Some(batch) = batch.filter(|_| successful_read) else {
        return Ok(());
    };

    let mut seen_monitor_ids = Vec::with_capacity(batch.monitors.len());
    for monitor in &batch.monitors {
        let monitor_id = store
            .upsert_monitor(
                write,
                &PublishedMonitorWrite {
                    id: ids.next_id(),
                    station_id: request.station_id.clone(),
                    endpoint_revision: request.endpoint_revision,
                    source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.to_string(),
                    upstream_monitor_id: monitor.upstream_monitor_id.clone(),
                    identity_kind: monitor.identity_kind.as_str().to_string(),
                    name: monitor.name.clone(),
                    provider: monitor.provider.clone(),
                    group_name: monitor.group_name.clone(),
                    primary_model: monitor.primary_model.clone(),
                    extra_models_json: serde_json::to_string(&monitor.extra_models)
                        .expect("validated extra models serialize"),
                    current_outcome: monitor.current_outcome.as_str().to_string(),
                    source_status: monitor.source_status.clone(),
                    current_latency_ms: monitor.current_latency_ms,
                    current_ping_latency_ms: monitor.current_ping_latency_ms,
                    upstream_checked_at_ms: monitor.upstream_checked_at_ms,
                    last_seen_run_id: run_id.to_string(),
                    last_seen_at: now.to_string(),
                    created_at: now.to_string(),
                    updated_at: now.to_string(),
                },
            )
            .await?;
        seen_monitor_ids.push(monitor_id.clone());
        for sample in &monitor.samples {
            store
                .upsert_sample(
                    write,
                    &PublishedMonitorSampleWrite {
                        id: ids.next_id(),
                        monitor_id: monitor_id.clone(),
                        model: sample.model.clone(),
                        checked_at_ms: sample.checked_at_ms,
                        outcome: sample.outcome.as_str().to_string(),
                        source_status: sample.source_status.clone(),
                        latency_ms: sample.latency_ms,
                        ping_latency_ms: sample.ping_latency_ms,
                        safe_message: sample.safe_message.clone(),
                        first_seen_run_id: run_id.to_string(),
                        last_seen_run_id: run_id.to_string(),
                        created_at: now.to_string(),
                        updated_at: now.to_string(),
                    },
                )
                .await?;
        }
    }
    if complete_read {
        store
            .mark_unseen_monitors_missing(
                write,
                &request.station_id,
                request.endpoint_revision,
                STATION_PUBLISHED_STATUS_SOURCE_KIND,
                &seen_monitor_ids,
                now,
            )
            .await?;
    }
    store
        .retain_active_samples(
            write,
            &request.station_id,
            request.endpoint_revision,
            STATION_PUBLISHED_STATUS_SOURCE_KIND,
        )
        .await?;
    let missing_cutoff = now_ms.saturating_sub(30 * 24 * 60 * 60 * 1_000).to_string();
    store
        .delete_missing_before(
            write,
            &request.station_id,
            request.endpoint_revision,
            STATION_PUBLISHED_STATUS_SOURCE_KIND,
            &missing_cutoff,
        )
        .await?;
    Ok(())
}

fn published_status_source_state_for_failed_apply(
    request: &CollectorApplyRequest,
) -> PublishedStatusSourceState {
    if request.manual_action_required || request.status == "manual_required" {
        PublishedStatusSourceState::AuthorizationRequired
    } else if request.error_code.as_deref() == Some("unsupported_task") {
        PublishedStatusSourceState::Unsupported
    } else {
        PublishedStatusSourceState::Failed
    }
}

fn is_successful_published_status_read(
    source_state: PublishedStatusSourceState,
    safe_error_kind: Option<&str>,
) -> bool {
    safe_error_kind.is_none()
        && matches!(
            source_state,
            PublishedStatusSourceState::Available
                | PublishedStatusSourceState::Empty
                | PublishedStatusSourceState::Degraded
        )
}

#[derive(Clone, Copy)]
struct CollectorTaskSideEffectPolicy {
    updates_station_collection_status: bool,
    emits_collector_observation: bool,
    refreshes_group_bindings: bool,
}

fn collector_task_side_effect_policy(task_type: &str) -> CollectorTaskSideEffectPolicy {
    match task_type {
        "detect" | "balance" => CollectorTaskSideEffectPolicy {
            updates_station_collection_status: true,
            emits_collector_observation: true,
            refreshes_group_bindings: false,
        },
        "groups" | "full" => CollectorTaskSideEffectPolicy {
            updates_station_collection_status: true,
            emits_collector_observation: true,
            refreshes_group_bindings: true,
        },
        "published_status" => CollectorTaskSideEffectPolicy {
            updates_station_collection_status: false,
            emits_collector_observation: false,
            refreshes_group_bindings: false,
        },
        // `validate_request` rejects unknown tasks. Defaulting to no side effects
        // keeps any future task isolated until it receives an explicit policy.
        _ => CollectorTaskSideEffectPolicy {
            updates_station_collection_status: false,
            emits_collector_observation: false,
            refreshes_group_bindings: false,
        },
    }
}

fn task_updates_station_collection_status(task_type: &str) -> bool {
    collector_task_side_effect_policy(task_type).updates_station_collection_status
}

/// Published status is a display-only optional child of Full collection. Its
/// failure must stay visible in its own source state without degrading the
/// station's core collector status.
fn station_collection_status_for_request(request: &CollectorApplyRequest) -> &str {
    if request.task_type != "full" {
        return &request.status;
    }

    let Some(children) = request
        .summary_json
        .get("childRuns")
        .and_then(Value::as_array)
    else {
        return &request.status;
    };
    let core_children = children
        .iter()
        .filter(|child| {
            matches!(
                child.get("task").and_then(Value::as_str),
                Some("balance" | "groups" | "detect")
            )
        })
        .collect::<Vec<_>>();
    if !core_children.is_empty()
        && core_children
            .iter()
            .all(|child| child.get("status").and_then(Value::as_str) == Some("success"))
    {
        "success"
    } else {
        &request.status
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
            "detect" | "balance" | "groups" | "published_status" | "full"
        )
        || !matches!(
            request.status.as_str(),
            "success" | "partial" | "failed" | "manual_required"
        )
        || request.endpoint_count < 0
        || request.success_count < 0
        || request.failure_count < 0
        || request.success_count + request.failure_count > request.endpoint_count
        || request.execution_started_at_ms.is_some() != request.execution_duration_ms.is_some()
        || request
            .execution_started_at_ms
            .is_some_and(|value| value < 0)
        || request.execution_duration_ms.is_some_and(|value| value < 0)
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

fn group_transition_observation(
    transition: &GroupTransition,
    now: &str,
    source_run_key: &str,
) -> Option<ObservationIngress> {
    let previous_status = transition
        .previous
        .as_ref()
        .map(|value| value.binding_status.as_str());
    let current = &transition.current;
    let (event_type, kind, severity, reason_code) = match current.binding_kind.as_str() {
        BINDING_KIND_STATION_GROUP => {
            if current.binding_status == BINDING_STATUS_MISSING
                && previous_status != Some(BINDING_STATUS_MISSING)
            {
                (
                    AlertEventType::GroupMissing,
                    ObservationKind::Change,
                    Severity::Info,
                    "group_missing",
                )
            } else if current.binding_status == BINDING_STATUS_AVAILABLE
                && transition.previous.is_none()
            {
                (
                    AlertEventType::GroupAdded,
                    ObservationKind::Change,
                    Severity::Info,
                    "group_added",
                )
            } else {
                return None;
            }
        }
        BINDING_KIND_KEY_BINDING => {
            if current.binding_status == BINDING_STATUS_MISSING
                && previous_status != Some(BINDING_STATUS_MISSING)
            {
                (
                    AlertEventType::KeyGroupUnresolved,
                    ObservationKind::Abnormal,
                    Severity::Warning,
                    "key_group_unresolved",
                )
            } else if current.binding_status == BINDING_STATUS_BOUND
                && previous_status == Some(BINDING_STATUS_MISSING)
            {
                (
                    AlertEventType::KeyGroupUnresolved,
                    ObservationKind::Healthy,
                    Severity::Warning,
                    "key_group_bound",
                )
            } else if current.binding_status == BINDING_STATUS_BOUND
                && transition.previous.is_none()
            {
                (
                    AlertEventType::AuditChange,
                    ObservationKind::Change,
                    Severity::Info,
                    "key_group_bound",
                )
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let condition_key = if current.binding_kind == BINDING_KIND_KEY_BINDING {
        format!(
            "station_key:{}:group",
            current.station_key_id.as_deref().unwrap_or(&current.id)
        )
    } else {
        format!(
            "station_group:{}:{}",
            current.station_id, current.group_key_hash
        )
    };
    let observed_at_ms = parse_now_ms(now);
    Some(ObservationIngress {
        source_observation_key: format!(
            "collector:{}:{}:{}:{}",
            source_run_key,
            event_type.as_str(),
            current.station_id,
            current.group_key_hash
        ),
        event_type,
        condition_key: crate::models::alerting::ConditionKey::new(condition_key).ok()?,
        kind,
        severity,
        object_type: if current.binding_kind == BINDING_KIND_KEY_BINDING {
            "station_key".to_string()
        } else {
            "station_group_binding".to_string()
        },
        object_id: Some(current.id.clone()),
        station_id: Some(current.station_id.clone()),
        station_key_id: current.station_key_id.clone(),
        source: "collector".to_string(),
        reason_code: Some(reason_code.to_string()),
        summary_json: json!({
            "groupName": current.group_name,
            "status": current.binding_status,
            "groupKeyHash": current.group_key_hash,
            "effectiveRateMultiplier": current.effective_rate_multiplier,
        })
        .to_string(),
        observed_at_ms,
        fact_fresh_until_ms: observed_at_ms.saturating_add(900_000),
    })
}

fn rate_change_observation(
    station_id: &str,
    transition: &RateTransition,
    source_run_key: &str,
    now: &str,
) -> ObservationIngress {
    let observed_at_ms = parse_now_ms(now);
    ObservationIngress {
        source_observation_key: format!(
            "collector:{}:group_rate_changed:{}:{}",
            source_run_key, station_id, transition.group_binding_id
        ),
        event_type: AlertEventType::GroupRateChanged,
        condition_key: crate::models::alerting::ConditionKey::new(format!(
            "station_group_rate:{}:{}",
            station_id, transition.group_binding_id
        ))
        .expect("collector rate condition key is bounded"),
        kind: ObservationKind::Change,
        severity: Severity::Info,
        object_type: "station_group_binding".to_string(),
        object_id: Some(transition.group_binding_id.clone()),
        station_id: Some(station_id.to_string()),
        station_key_id: None,
        source: "collector".to_string(),
        reason_code: Some("group_rate_changed".to_string()),
        summary_json: json!({
            "groupName": transition.group_name,
            "oldEffectiveRateMultiplier": transition.old_effective_rate_multiplier,
            "newEffectiveRateMultiplier": transition.new_effective_rate_multiplier,
        })
        .to_string(),
        observed_at_ms,
        fact_fresh_until_ms: observed_at_ms.saturating_add(900_000),
    }
}

fn collector_observation(
    request: &CollectorApplyRequest,
    failure_key: &str,
    source_run_key: &str,
    kind: ObservationKind,
    failed_task_types: &[String],
    now: &str,
) -> ObservationIngress {
    let observed_at_ms = parse_now_ms(now);
    ObservationIngress {
        source_observation_key: format!("collector:{}:{}", source_run_key, failure_key),
        event_type: AlertEventType::CollectorFailed,
        condition_key: crate::models::alerting::ConditionKey::new(failure_key.to_string())
            .expect("collector failure condition key is bounded"),
        kind,
        severity: Severity::Warning,
        object_type: "station".to_string(),
        object_id: Some(request.station_id.clone()),
        station_id: Some(request.station_id.clone()),
        station_key_id: None,
        source: "collector".to_string(),
        reason_code: Some(
            request
                .error_code
                .as_deref()
                .unwrap_or(if kind == ObservationKind::Healthy {
                    "collector_recovered"
                } else {
                    "collector_failed"
                })
                .to_string(),
        ),
        summary_json: json!({
            "taskType": request.task_type,
            "status": request.status,
            "errorCode": request.error_code,
            "failedTaskTypes": failed_task_types,
        })
        .to_string(),
        observed_at_ms,
        fact_fresh_until_ms: observed_at_ms.saturating_add(900_000),
    }
}

fn parse_now_ms(value: &str) -> i64 {
    value.parse::<i64>().unwrap_or_default().max(0)
}
fn collector_failure_key(station_id: &str) -> String {
    format!("collector:{station_id}:collector_failed")
}

fn should_emit_collector_observation(parent_run_id: Option<&str>) -> bool {
    parent_run_id.is_none()
}

/// A manual authorization result is a known station state, not a collector
/// failure. It must still flow through the observation projector so it can
/// clear a failure incident left by an earlier run.
fn should_record_collector_observation(status: &str) -> bool {
    matches!(status, "success" | "partial" | "failed" | "manual_required")
}

async fn collector_failed_task_types(
    collectors: &CollectorStore,
    write: &mut crate::persistence::WriteSession,
    request: &CollectorApplyRequest,
) -> Result<Vec<String>, crate::persistence::error::PersistenceError> {
    let failed = collectors
        .failed_task_types(write, &request.station_id)
        .await?;
    Ok(merge_collector_failed_task_types(failed, request))
}

fn merge_collector_failed_task_types(
    current: impl IntoIterator<Item = String>,
    request: &CollectorApplyRequest,
) -> Vec<String> {
    let mut failed = current.into_iter().collect::<BTreeSet<_>>();

    if request.task_type == "full" {
        let mut applied_child_status = false;
        if let Some(children) = request
            .summary_json
            .get("childRuns")
            .and_then(Value::as_array)
        {
            for child in children {
                let Some(task_type) = child.get("task").and_then(Value::as_str) else {
                    continue;
                };
                let Some(status) = child.get("status").and_then(Value::as_str) else {
                    continue;
                };
                if matches!(task_type, "balance" | "groups" | "detect") {
                    apply_collector_task_status(&mut failed, task_type, status);
                    applied_child_status = true;
                }
            }
        }
        if applied_child_status {
            failed.remove("full");
        } else {
            apply_collector_task_status(&mut failed, "full", &request.status);
        }
    } else {
        apply_collector_task_status(&mut failed, &request.task_type, &request.status);
    }

    ["balance", "groups", "detect", "full"]
        .into_iter()
        .filter(|task_type| failed.contains(*task_type))
        .map(str::to_string)
        .collect()
}

fn apply_collector_task_status(failed: &mut BTreeSet<String>, task_type: &str, status: &str) {
    if status == "failed" {
        failed.insert(task_type.to_string());
    } else if matches!(status, "success" | "partial" | "manual_required") {
        failed.remove(task_type);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };

    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use sqlx::Row;

    use super::*;
    use crate::{
        application::{
            credentials::CredentialService, error::ApplicationError, stations::StationService,
        },
        models::{
            station_keys::CreateStationKeyInput,
            station_published_status::{
                PublishedMonitorFact, PublishedMonitorIdentityKind, PublishedMonitorSampleFact,
                PublishedSampleOutcome,
            },
            stations::{CreateStationInput, UpdateStationInput},
        },
        persistence::{runtime::PersistenceRuntime, stores::collector_store::GroupState},
        services::secrets::vault::DataKeyVault,
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

    #[test]
    fn rate_observation_persists_group_name_for_stable_presentation() {
        let transition = RateTransition {
            group_binding_id: "binding-1".to_string(),
            group_name: "stable-group".to_string(),
            old_effective_rate_multiplier: Some(0.2),
            new_effective_rate_multiplier: Some(0.18),
        };

        let observation =
            rate_change_observation("station-1", &transition, "run-1", "1700000000000");
        let summary: serde_json::Value =
            serde_json::from_str(&observation.summary_json).expect("valid summary");

        assert_eq!(observation.event_type, AlertEventType::GroupRateChanged);
        assert_eq!(observation.severity, Severity::Info);
        assert_eq!(observation.object_type, "station_group_binding");
        assert_eq!(summary["groupName"], "stable-group");
        assert_eq!(summary["oldEffectiveRateMultiplier"], 0.2);
        assert_eq!(summary["newEffectiveRateMultiplier"], 0.18);
    }

    #[test]
    fn producer_observations_cover_failure_recovery_and_audit_change_contracts() {
        let request = CollectorApplyRequest {
            run_key: "failed-run".to_string(),
            station_id: "station-1".to_string(),
            endpoint_revision: 1,
            parent_run_id: None,
            adapter: "newapi".to_string(),
            task_type: "balance".to_string(),
            status: "failed".to_string(),
            facts: CanonicalCollectorFacts::default(),
            summary_json: json!({"status": "failed"}),
            normalized_json: json!({}),
            raw_json_redacted: None,
            error_code: Some("timeout".to_string()),
            error_message: Some("collector timed out".to_string()),
            endpoint_count: 1,
            success_count: 0,
            failure_count: 1,
            manual_action_required: false,
            next_due_at: None,
            execution_started_at_ms: None,
            execution_duration_ms: None,
        };
        let failure_key = collector_failure_key("station-1");
        let abnormal = collector_observation(
            &request,
            &failure_key,
            "failed-run",
            ObservationKind::Abnormal,
            &["balance".to_string()],
            "1700000000000",
        );
        let mut recovered_request = request.clone();
        recovered_request.run_key = "healthy-run".to_string();
        recovered_request.status = "success".to_string();
        recovered_request.error_code = None;
        recovered_request.error_message = None;
        recovered_request.success_count = 1;
        recovered_request.failure_count = 0;
        let healthy = collector_observation(
            &recovered_request,
            &failure_key,
            "healthy-run",
            ObservationKind::Healthy,
            &[],
            "1700000060000",
        );

        assert_eq!(abnormal.event_type, AlertEventType::CollectorFailed);
        assert_eq!(abnormal.kind, ObservationKind::Abnormal);
        assert_eq!(healthy.kind, ObservationKind::Healthy);
        assert_eq!(abnormal.condition_key, healthy.condition_key);
        assert_ne!(
            abnormal.source_observation_key,
            healthy.source_observation_key
        );
        assert_eq!(
            abnormal.source_observation_key,
            collector_observation(
                &request,
                &failure_key,
                "failed-run",
                ObservationKind::Abnormal,
                &["balance".to_string()],
                "1700000000000",
            )
            .source_observation_key
        );
        assert_eq!(failure_key, "collector:station-1:collector_failed");
        assert_eq!(
            serde_json::from_str::<Value>(&abnormal.summary_json).expect("collector summary")
                ["failedTaskTypes"],
            json!(["balance"])
        );

        let transition = GroupTransition {
            previous: None,
            current: GroupState {
                id: "binding-1".to_string(),
                station_id: "station-1".to_string(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                group_key_hash: "group-hash".to_string(),
                group_name: "new-group".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: None,
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(0.07),
                source: "collector".to_string(),
            },
        };
        let audit = group_transition_observation(&transition, "1700000000000", "run-1")
            .expect("new group emits an audit observation");
        assert_eq!(audit.event_type, AlertEventType::GroupAdded);
        assert_eq!(audit.kind, ObservationKind::Change);
        assert_eq!(
            audit.condition_key.as_str(),
            "station_group:station-1:group-hash"
        );
        let summary: Value = serde_json::from_str(&audit.summary_json).expect("group summary");
        assert_eq!(summary["effectiveRateMultiplier"], json!(0.07));

        let available_current = transition.current.clone();
        let mut missing_current = available_current.clone();
        missing_current.binding_status = BINDING_STATUS_MISSING.to_string();
        let missing = group_transition_observation(
            &GroupTransition {
                previous: Some(transition.current),
                current: missing_current,
            },
            "1700000000000",
            "run-2",
        )
        .expect("missing group emits an informational observation");
        assert_eq!(missing.event_type, AlertEventType::GroupMissing);
        assert_eq!(missing.kind, ObservationKind::Change);
        assert_eq!(missing.severity, Severity::Info);

        assert!(group_transition_observation(
            &GroupTransition {
                previous: Some(GroupState {
                    binding_status: BINDING_STATUS_MISSING.to_string(),
                    ..available_current.clone()
                }),
                current: available_current,
            },
            "1700000001000",
            "run-3",
        )
        .is_none());
    }

    #[test]
    fn child_collection_runs_do_not_create_duplicate_collector_incidents() {
        assert!(should_emit_collector_observation(None));
        assert!(!should_emit_collector_observation(Some("full-run-1")));
    }

    #[test]
    fn manual_authorization_results_are_recorded_for_collector_recovery() {
        assert!(should_record_collector_observation("manual_required"));
        assert!(should_record_collector_observation("success"));
        assert!(should_record_collector_observation("partial"));
        assert!(should_record_collector_observation("failed"));
        assert!(!should_record_collector_observation("unsupported"));
    }

    #[test]
    fn published_status_apply_request_is_valid_and_never_updates_core_task_failures() {
        let request = CollectorApplyRequest {
            run_key: "published-status-run".to_string(),
            station_id: "station-1".to_string(),
            endpoint_revision: 1,
            parent_run_id: None,
            adapter: "sub2api".to_string(),
            task_type: "published_status".to_string(),
            status: "partial".to_string(),
            facts: CanonicalCollectorFacts::default(),
            summary_json: json!({ "endpointResults": [] }),
            normalized_json: json!({}),
            raw_json_redacted: None,
            error_code: Some("rate_limited".to_string()),
            error_message: Some("published status rate limited".to_string()),
            endpoint_count: 1,
            success_count: 0,
            failure_count: 1,
            manual_action_required: false,
            next_due_at: None,
            execution_started_at_ms: None,
            execution_duration_ms: None,
        };

        assert!(validate_request(&request).is_ok());
        assert!(!task_updates_station_collection_status(&request.task_type));
        let policy = collector_task_side_effect_policy(&request.task_type);
        assert!(!policy.emits_collector_observation);
        assert!(!policy.refreshes_group_bindings);
        assert!(
            merge_collector_failed_task_types(vec!["published_status".to_string()], &request)
                .is_empty()
        );
    }

    #[test]
    fn only_successful_published_status_reads_replace_superseded_endpoint_facts() {
        assert!(is_successful_published_status_read(
            PublishedStatusSourceState::Available,
            None
        ));
        assert!(is_successful_published_status_read(
            PublishedStatusSourceState::Empty,
            None
        ));
        assert!(is_successful_published_status_read(
            PublishedStatusSourceState::Degraded,
            None
        ));
        assert!(!is_successful_published_status_read(
            PublishedStatusSourceState::Unsupported,
            None
        ));
        assert!(!is_successful_published_status_read(
            PublishedStatusSourceState::AuthorizationRequired,
            None
        ));
        assert!(!is_successful_published_status_read(
            PublishedStatusSourceState::Failed,
            None
        ));
        assert!(!is_successful_published_status_read(
            PublishedStatusSourceState::Available,
            Some("malformed_payload")
        ));
    }

    #[tokio::test]
    async fn failed_new_endpoint_revision_preserves_prior_published_status_facts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("published-status.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let initial_station = stations
            .create(CreateStationInput {
                name: "Published status fixture".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://published-status.example.test".to_string(),
                api_base_url: "https://published-status.example.test/v1".to_string(),
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

        collectors
            .apply_result(published_status_apply_request(
                "published-status-revision-one",
                &initial_station,
                Some(published_status_batch(&initial_station)),
                "success",
            ))
            .await
            .expect("initial published facts");
        assert_eq!(
            published_status_fact_counts(&runtime, &initial_station.id, 1).await,
            (1, 1, 1)
        );

        let updated_station = stations
            .update_station(UpdateStationInput {
                id: initial_station.id.clone(),
                name: initial_station.name.clone(),
                station_type: initial_station.station_type.clone(),
                website_url: initial_station.website_url.clone(),
                api_base_url: "https://replacement.example.test/v1".to_string(),
                api_key: None,
                collector_proxy_mode: initial_station.collector_proxy_mode.clone(),
                collector_proxy_url: initial_station.collector_proxy_url.clone(),
                enabled: initial_station.enabled,
                credit_per_cny: initial_station.credit_per_cny,
                low_balance_threshold_cny: initial_station.low_balance_threshold_cny,
                collection_interval_minutes: initial_station.collection_interval_minutes,
                note: initial_station.note.clone(),
            })
            .await
            .expect("station endpoint update");
        assert_eq!(updated_station.endpoint_revision, 2);

        collectors
            .apply_result(published_status_apply_request(
                "published-status-revision-two-failed",
                &updated_station,
                None,
                "failed",
            ))
            .await
            .expect("failed published-status run is recorded");
        assert_eq!(
            published_status_fact_counts(&runtime, &initial_station.id, 1).await,
            (1, 1, 1)
        );
        assert_eq!(
            published_status_fact_counts(&runtime, &initial_station.id, 2).await,
            (1, 0, 0)
        );

        collectors
            .apply_result(published_status_apply_request(
                "published-status-revision-two-success",
                &updated_station,
                Some(published_status_batch(&updated_station)),
                "success",
            ))
            .await
            .expect("successful replacement facts");
        assert_eq!(
            published_status_fact_counts(&runtime, &initial_station.id, 1).await,
            (0, 0, 0)
        );
        assert_eq!(
            published_status_fact_counts(&runtime, &initial_station.id, 2).await,
            (1, 1, 1)
        );

        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_apply_replay_is_idempotent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("published-status.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(published_status_station_input())
            .await
            .expect("station");
        let request = published_status_apply_request(
            "published-status-idempotent-run",
            &station,
            Some(published_status_batch(&station)),
            "success",
        );

        let first = collectors
            .apply_result(request.clone())
            .await
            .expect("initial apply");
        let replay = collectors
            .apply_result(request)
            .await
            .expect("idempotent replay");

        assert!(first.inserted);
        assert!(!replay.inserted);
        assert_eq!(first.run_id, replay.run_id);
        assert_eq!(first.snapshot_id, replay.snapshot_id);
        assert_eq!(
            published_status_fact_counts(&runtime, &station.id, station.endpoint_revision).await,
            (1, 1, 1)
        );
        assert_eq!(
            collector_apply_row_counts(&runtime, &station.id).await,
            (1, 1, 1)
        );

        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_partial_and_failed_applies_preserve_prior_facts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("published-status.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(published_status_station_input())
            .await
            .expect("station");

        let mut complete = published_status_batch(&station);
        let mut retained = complete.monitors[0].clone();
        retained.upstream_monitor_id = "monitor-retained".to_string();
        retained.name = "Retained Monitor".to_string();
        complete.monitors.push(retained.clone());
        collectors
            .apply_result(published_status_apply_request(
                "published-status-complete-inventory",
                &station,
                Some(complete),
                "success",
            ))
            .await
            .expect("complete inventory");

        let mut partial = published_status_batch(&station);
        partial.source_state = PublishedStatusSourceState::Degraded;
        partial.completeness = PublishedStatusCompleteness::Partial;
        partial.monitors = vec![retained];
        collectors
            .apply_result(published_status_apply_request(
                "published-status-partial-inventory",
                &station,
                Some(partial),
                "partial",
            ))
            .await
            .expect("partial inventory");

        assert_eq!(
            published_status_current_monitor_ids(&runtime, &station.id, station.endpoint_revision)
                .await,
            vec![
                "monitor-fixture".to_string(),
                "monitor-retained".to_string()
            ]
        );

        collectors
            .apply_result(published_status_apply_request(
                "published-status-failed-inventory",
                &station,
                None,
                "failed",
            ))
            .await
            .expect("failed attempt is persisted without clearing facts");

        assert_eq!(
            published_status_fact_counts(&runtime, &station.id, station.endpoint_revision).await,
            (1, 2, 2)
        );
        assert_eq!(
            published_status_current_monitor_ids(&runtime, &station.id, station.endpoint_revision)
                .await,
            vec![
                "monitor-fixture".to_string(),
                "monitor-retained".to_string()
            ]
        );
        let source =
            published_status_source_metadata(&runtime, &station.id, station.endpoint_revision)
                .await;
        assert_eq!(source.0, "failed");
        assert_eq!(source.1.as_deref(), Some("1700000000000"));
        assert_eq!(source.2.as_deref(), Some("1700000000000"));

        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_apply_sql_failure_rolls_back_run_snapshot_and_facts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("published-status.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(published_status_station_input())
            .await
            .expect("station");

        collectors
            .apply_result(published_status_apply_request(
                "published-status-before-sql-failure",
                &station,
                Some(published_status_batch(&station)),
                "success",
            ))
            .await
            .expect("initial facts");
        runtime
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        r#"
                        CREATE TRIGGER fail_published_status_sample_insert
                        BEFORE INSERT ON station_published_monitor_samples
                        BEGIN
                            SELECT RAISE(ABORT, 'fixture published status sample failure');
                        END
                        "#,
                    )
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("install failure trigger");

        let mut batch = published_status_batch(&station);
        batch.monitors[0].samples[0].checked_at_ms += 1;
        assert!(matches!(
            collectors
                .apply_result(published_status_apply_request(
                    "published-status-sql-failure",
                    &station,
                    Some(batch),
                    "success",
                ))
                .await,
            Err(ApplicationError::Internal)
        ));

        assert_eq!(
            published_status_fact_counts(&runtime, &station.id, station.endpoint_revision).await,
            (1, 1, 1)
        );
        assert_eq!(
            collector_apply_row_counts(&runtime, &station.id).await,
            (1, 1, 1)
        );

        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_apply_rejects_stale_revision_before_writing_a_run() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("published-status.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let initial_station = stations
            .create(published_status_station_input())
            .await
            .expect("station");
        collectors
            .apply_result(published_status_apply_request(
                "published-status-before-revision-change",
                &initial_station,
                Some(published_status_batch(&initial_station)),
                "success",
            ))
            .await
            .expect("initial facts");
        let stale_request = published_status_apply_request(
            "published-status-stale-revision",
            &initial_station,
            Some(published_status_batch(&initial_station)),
            "success",
        );
        let updated_station = stations
            .update_station(UpdateStationInput {
                id: initial_station.id.clone(),
                name: initial_station.name.clone(),
                station_type: initial_station.station_type.clone(),
                website_url: initial_station.website_url.clone(),
                api_base_url: "https://replacement.example.test/v1".to_string(),
                api_key: None,
                collector_proxy_mode: initial_station.collector_proxy_mode.clone(),
                collector_proxy_url: initial_station.collector_proxy_url.clone(),
                enabled: initial_station.enabled,
                credit_per_cny: initial_station.credit_per_cny,
                low_balance_threshold_cny: initial_station.low_balance_threshold_cny,
                collection_interval_minutes: initial_station.collection_interval_minutes,
                note: initial_station.note.clone(),
            })
            .await
            .expect("station endpoint update");

        assert!(matches!(
            collectors.apply_result(stale_request).await,
            Err(ApplicationError::StaleRevision)
        ));
        assert_eq!(
            published_status_fact_counts(
                &runtime,
                &initial_station.id,
                initial_station.endpoint_revision,
            )
            .await,
            (1, 1, 1)
        );
        assert_eq!(
            published_status_fact_counts(
                &runtime,
                &initial_station.id,
                updated_station.endpoint_revision
            )
            .await,
            (0, 0, 0)
        );
        assert_eq!(
            collector_apply_row_counts(&runtime, &initial_station.id).await,
            (1, 1, 1)
        );

        runtime.close().await.expect("close runtime");
    }

    #[test]
    fn every_known_collector_task_has_an_explicit_side_effect_policy() {
        for task_type in ["detect", "balance", "groups", "published_status", "full"] {
            let policy = collector_task_side_effect_policy(task_type);
            if task_type == "published_status" {
                assert!(!policy.updates_station_collection_status);
                assert!(!policy.emits_collector_observation);
            } else {
                assert!(policy.updates_station_collection_status);
                assert!(policy.emits_collector_observation);
            }
        }
        let unknown = collector_task_side_effect_policy("future_task");
        assert!(!unknown.updates_station_collection_status);
        assert!(!unknown.emits_collector_observation);
        assert!(!unknown.refreshes_group_bindings);
    }

    #[test]
    fn collector_failure_summary_tracks_all_current_failed_tasks() {
        let mut request = CollectorApplyRequest {
            run_key: "groups-failed".to_string(),
            station_id: "station-1".to_string(),
            endpoint_revision: 1,
            parent_run_id: None,
            adapter: "newapi".to_string(),
            task_type: "groups".to_string(),
            status: "failed".to_string(),
            facts: CanonicalCollectorFacts::default(),
            summary_json: json!({}),
            normalized_json: json!({}),
            raw_json_redacted: None,
            error_code: Some("timeout".to_string()),
            error_message: Some("collector timed out".to_string()),
            endpoint_count: 1,
            success_count: 0,
            failure_count: 1,
            manual_action_required: false,
            next_due_at: None,
            execution_started_at_ms: None,
            execution_duration_ms: None,
        };

        assert_eq!(
            merge_collector_failed_task_types(vec!["balance".to_string()], &request),
            vec!["balance".to_string(), "groups".to_string()]
        );

        request.status = "success".to_string();
        assert_eq!(
            merge_collector_failed_task_types(
                vec!["balance".to_string(), "groups".to_string()],
                &request,
            ),
            vec!["balance".to_string()]
        );

        request.status = "manual_required".to_string();
        request.manual_action_required = true;
        assert!(
            merge_collector_failed_task_types(vec!["groups".to_string()], &request,).is_empty()
        );

        request.task_type = "full".to_string();
        request.status = "partial".to_string();
        request.summary_json = json!({
            "childRuns": [
                { "task": "balance", "status": "success" },
                { "task": "groups", "status": "failed" },
            ],
        });
        assert_eq!(
            merge_collector_failed_task_types(
                vec!["balance".to_string(), "full".to_string()],
                &request,
            ),
            vec!["groups".to_string()]
        );

        request.status = "success".to_string();
        request.summary_json = json!({
            "childRuns": [
                { "task": "balance", "status": "success" },
                { "task": "groups", "status": "success" },
                { "task": "published_status", "status": "failed" },
            ],
        });
        assert!(merge_collector_failed_task_types(
            vec!["published_status".to_string(), "full".to_string()],
            &request,
        )
        .is_empty());
        assert_eq!(station_collection_status_for_request(&request), "success");

        request.summary_json = json!({
            "childRuns": [
                { "task": "balance", "status": "success" },
                { "task": "groups", "status": "partial" },
                { "task": "published_status", "status": "failed" },
            ],
        });
        request.status = "partial".to_string();
        assert_eq!(station_collection_status_for_request(&request), "partial");

        request.summary_json = json!({
            "childRuns": [
                { "task": "balance", "status": "success" },
                { "task": "groups", "status": "failed" },
                { "task": "published_status", "status": "failed" },
            ],
        });
        assert_eq!(station_collection_status_for_request(&request), "partial");
    }

    fn capture_request(station_id: &str, endpoint_revision: i64) -> CaptureSnapshotRequest {
        CaptureSnapshotRequest {
            station_id: station_id.to_string(),
            endpoint_revision,
            task_type: "full".to_string(),
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
        let collected_station = stations
            .station_for_capture(&station.id)
            .await
            .expect("collected station");
        assert_eq!(collected_station.status, "healthy");
        assert_eq!(
            collected_station.last_checked_at.as_deref(),
            Some("1700000000000")
        );
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
    async fn list_latest_station_snapshots_returns_one_latest_row_per_requested_station() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("latest-snapshots.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let first_station = stations
            .create(CreateStationInput {
                name: "Latest Snapshot A".to_string(),
                station_type: "newapi".to_string(),
                website_url: "https://latest-a.example.test".to_string(),
                api_base_url: "https://latest-a.example.test/v1".to_string(),
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
            .expect("first station");
        let second_station = stations
            .create(CreateStationInput {
                name: "Latest Snapshot B".to_string(),
                station_type: "newapi".to_string(),
                website_url: "https://latest-b.example.test".to_string(),
                api_base_url: "https://latest-b.example.test/v1".to_string(),
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
            .expect("second station");

        let mut first_old = capture_request(&first_station.id, first_station.endpoint_revision);
        first_old.summary_json = json!({ "attempt": "old" });
        collectors
            .record_capture_snapshot(first_old)
            .await
            .expect("first old snapshot");
        let mut first_new = capture_request(&first_station.id, first_station.endpoint_revision);
        first_new.status = "manual_required".to_string();
        first_new.summary_json = json!({ "attempt": "new", "loginRequired": true });
        collectors
            .record_capture_snapshot(first_new)
            .await
            .expect("first new snapshot");
        let mut second = capture_request(&second_station.id, second_station.endpoint_revision);
        second.summary_json = json!({ "attempt": "second" });
        collectors
            .record_capture_snapshot(second)
            .await
            .expect("second snapshot");

        assert!(collectors
            .list_latest_station_snapshots(Vec::new())
            .await
            .expect("empty list")
            .is_empty());
        let latest = collectors
            .list_latest_station_snapshots(vec![
                second_station.id.clone(),
                first_station.id.clone(),
                "missing-station".to_string(),
            ])
            .await
            .expect("latest snapshots");
        assert_eq!(latest.len(), 2);
        let by_station = latest
            .into_iter()
            .map(|snapshot| (snapshot.station_id.clone(), snapshot))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            by_station
                .get(&first_station.id)
                .expect("first latest")
                .summary_json,
            json!({ "attempt": "new", "loginRequired": true })
        );
        assert_eq!(
            by_station
                .get(&second_station.id)
                .expect("second latest")
                .summary_json,
            json!({ "attempt": "second" })
        );
        let duplicate_error = collectors
            .list_latest_station_snapshots(vec![first_station.id.clone(), first_station.id.clone()])
            .await
            .expect_err("duplicates should fail closed");
        assert!(matches!(
            duplicate_error,
            ApplicationError::ConstraintViolation
        ));
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn latest_station_snapshots_query_plan_uses_station_created_index() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("latest-snapshot-plan.sqlite3"))
                .await
                .expect("runtime");
        let mut read = runtime.begin_read().await.expect("read session");
        let rows = sqlx::query(
            r#"
            EXPLAIN QUERY PLAN
            WITH ranked AS (
                SELECT id, station_id, endpoint_revision, source, status, fetched_at,
                       summary_json, normalized_json, raw_json_redacted, error_message, created_at,
                       ROW_NUMBER() OVER (
                           PARTITION BY station_id
                           ORDER BY created_at DESC, id DESC
                       ) AS station_snapshot_rank
                FROM collector_snapshots
                WHERE station_id IN (?1, ?2)
            )
            SELECT id, station_id, endpoint_revision, source, status, fetched_at,
                   summary_json, normalized_json, raw_json_redacted, error_message, created_at
            FROM ranked
            WHERE station_snapshot_rank = 1
            ORDER BY station_id ASC
            "#,
        )
        .bind("station-1")
        .bind("station-2")
        .fetch_all(read.connection())
        .await
        .expect("query plan");
        let details = rows
            .into_iter()
            .map(|row| row.get::<String, _>("detail"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            details.contains("idx_collector_snapshots_station_created"),
            "latest snapshot aggregate should use station/created index, got:\n{details}"
        );
        drop(read);
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn due_station_tasks_use_the_requested_global_interval() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("collector-schedule.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Scheduled Balance".to_string(),
                station_type: "newapi".to_string(),
                website_url: "https://schedule.example.test".to_string(),
                api_base_url: "https://schedule.example.test/v1".to_string(),
                api_key: String::new(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 99,
                note: None,
            })
            .await
            .expect("station");

        collectors
            .apply_result(CollectorApplyRequest {
                run_key: "scheduled-balance-run".to_string(),
                station_id: station.id.clone(),
                endpoint_revision: station.endpoint_revision,
                parent_run_id: None,
                adapter: "newapi".to_string(),
                task_type: "balance".to_string(),
                status: "success".to_string(),
                facts: CanonicalCollectorFacts::default(),
                summary_json: json!({ "balance": null }),
                normalized_json: json!({ "balance": null }),
                raw_json_redacted: None,
                error_code: None,
                error_message: None,
                endpoint_count: 1,
                success_count: 1,
                failure_count: 0,
                manual_action_required: false,
                next_due_at: None,
                execution_started_at_ms: Some(1_699_999_995_000),
                execution_duration_ms: Some(5_000),
            })
            .await
            .expect("collector apply");
        let runs = collectors
            .list_collector_runs(&station.id, PageLimit::new(1).expect("run limit"))
            .await
            .expect("collector runs");
        assert_eq!(runs[0].started_at, "1699999995000");
        assert_eq!(runs[0].finished_at.as_deref(), Some("1700000000000"));
        assert_eq!(runs[0].duration_ms, Some(5_000));
        runtime
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE collector_task_state SET updated_at = '1699999760000' \
                         WHERE task_type = 'balance'",
                    )
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("age task state by four minutes");

        let limit = PageLimit::new(10).expect("limit");
        assert!(collectors
            .due_stations_for_task("balance", 5, limit)
            .await
            .expect("five minute schedule")
            .is_empty());
        assert_eq!(
            collectors
                .due_stations_for_task("balance", 3, limit)
                .await
                .expect("three minute schedule")
                .into_iter()
                .map(|station| station.id)
                .collect::<Vec<_>>(),
            vec![station.id]
        );
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn child_run_does_not_override_top_level_station_collection_status() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("station-status.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Collection Status".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://status.example.test".to_string(),
                api_base_url: "https://status.example.test/v1".to_string(),
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
        let parent = collectors
            .apply_result(collector_apply_request(
                "parent-status-run",
                &station,
                None,
                "full",
                "partial",
            ))
            .await
            .expect("parent apply");
        collectors
            .apply_result(collector_apply_request(
                "child-status-run",
                &station,
                Some(parent.run_id),
                "groups",
                "failed",
            ))
            .await
            .expect("child apply");

        let collected_station = stations
            .station_for_capture(&station.id)
            .await
            .expect("collected station");
        assert_eq!(collected_station.status, "warning");
        assert_eq!(
            collected_station.last_pricing_fetched_at.as_deref(),
            Some("1700000000000")
        );
        runtime.close().await.expect("close persistence runtime");
    }

    fn collector_apply_request(
        run_key: &str,
        station: &Station,
        parent_run_id: Option<String>,
        task_type: &str,
        status: &str,
    ) -> CollectorApplyRequest {
        CollectorApplyRequest {
            run_key: run_key.to_string(),
            station_id: station.id.clone(),
            endpoint_revision: station.endpoint_revision,
            parent_run_id,
            adapter: "sub2api".to_string(),
            task_type: task_type.to_string(),
            status: status.to_string(),
            facts: CanonicalCollectorFacts::default(),
            summary_json: json!({ "status": status }),
            normalized_json: json!({}),
            raw_json_redacted: None,
            error_code: (status == "failed").then(|| "fixture_failure".to_string()),
            error_message: (status == "failed").then(|| "fixture failed".to_string()),
            endpoint_count: 1,
            success_count: i64::from(status != "failed"),
            failure_count: i64::from(status == "failed"),
            manual_action_required: status == "manual_required",
            next_due_at: None,
            execution_started_at_ms: None,
            execution_duration_ms: None,
        }
    }

    fn published_status_station_input() -> CreateStationInput {
        CreateStationInput {
            name: "Published status fixture".to_string(),
            station_type: "sub2api".to_string(),
            website_url: "https://published-status.example.test".to_string(),
            api_base_url: "https://published-status.example.test/v1".to_string(),
            api_key: String::new(),
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            collection_interval_minutes: 5,
            note: None,
        }
    }

    fn published_status_apply_request(
        run_key: &str,
        station: &Station,
        batch: Option<PublishedStatusBatch>,
        status: &str,
    ) -> CollectorApplyRequest {
        CollectorApplyRequest {
            run_key: run_key.to_string(),
            station_id: station.id.clone(),
            endpoint_revision: station.endpoint_revision,
            parent_run_id: None,
            adapter: "sub2api".to_string(),
            task_type: "published_status".to_string(),
            status: status.to_string(),
            facts: CanonicalCollectorFacts {
                published_status: batch,
                ..CanonicalCollectorFacts::default()
            },
            summary_json: json!({}),
            normalized_json: json!({}),
            raw_json_redacted: None,
            error_code: (status == "failed").then(|| "rate_limited".to_string()),
            error_message: (status == "failed").then(|| "fixture failure".to_string()),
            endpoint_count: 1,
            success_count: i64::from(status != "failed"),
            failure_count: i64::from(status == "failed"),
            manual_action_required: false,
            next_due_at: None,
            execution_started_at_ms: None,
            execution_duration_ms: None,
        }
    }

    fn published_status_batch(station: &Station) -> PublishedStatusBatch {
        PublishedStatusBatch {
            station_id: station.id.clone(),
            endpoint_revision: station.endpoint_revision,
            source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.to_string(),
            source_state: PublishedStatusSourceState::Available,
            completeness: PublishedStatusCompleteness::Complete,
            monitors: vec![PublishedMonitorFact {
                upstream_monitor_id: "monitor-fixture".to_string(),
                identity_kind: PublishedMonitorIdentityKind::UpstreamId,
                name: "Fixture Monitor".to_string(),
                provider: "openai".to_string(),
                group_name: Some("default".to_string()),
                primary_model: "fixture-model".to_string(),
                extra_models: Vec::new(),
                current_outcome: PublishedSampleOutcome::Available,
                source_status: "healthy".to_string(),
                current_latency_ms: Some(20),
                current_ping_latency_ms: Some(3),
                upstream_checked_at_ms: Some(1_700_000_000_000),
                samples: vec![PublishedMonitorSampleFact {
                    model: "fixture-model".to_string(),
                    outcome: PublishedSampleOutcome::Available,
                    source_status: "healthy".to_string(),
                    latency_ms: Some(20),
                    ping_latency_ms: Some(3),
                    checked_at_ms: 1_700_000_000_000,
                    safe_message: None,
                }],
            }],
            collected_at_ms: 1_700_000_000_000,
            safe_error_kind: None,
        }
    }

    async fn collector_apply_row_counts(
        runtime: &PersistenceRuntime,
        station_id: &str,
    ) -> (i64, i64, i64) {
        let mut read = runtime.begin_read().await.expect("read session");
        let runs = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM collector_runs WHERE station_id = ?1",
        )
        .bind(station_id)
        .fetch_one(read.connection())
        .await
        .expect("run count");
        let snapshots = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM collector_snapshots WHERE station_id = ?1",
        )
        .bind(station_id)
        .fetch_one(read.connection())
        .await
        .expect("snapshot count");
        let task_states = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM collector_task_state WHERE station_id = ?1 AND task_type = 'published_status'",
        )
        .bind(station_id)
        .fetch_one(read.connection())
        .await
        .expect("task state count");
        (runs, snapshots, task_states)
    }

    async fn published_status_current_monitor_ids(
        runtime: &PersistenceRuntime,
        station_id: &str,
        endpoint_revision: i64,
    ) -> Vec<String> {
        let mut read = runtime.begin_read().await.expect("read session");
        sqlx::query_scalar::<_, String>(
            r#"
            SELECT upstream_monitor_id
            FROM station_published_monitors
            WHERE station_id = ?1
              AND endpoint_revision = ?2
              AND presence_status = 'current'
            ORDER BY upstream_monitor_id ASC
            "#,
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .fetch_all(read.connection())
        .await
        .expect("current monitor ids")
    }

    async fn published_status_source_metadata(
        runtime: &PersistenceRuntime,
        station_id: &str,
        endpoint_revision: i64,
    ) -> (String, Option<String>, Option<String>) {
        let mut read = runtime.begin_read().await.expect("read session");
        let row = sqlx::query(
            r#"
            SELECT source_state, last_success_at, last_complete_at
            FROM station_published_status_sources
            WHERE station_id = ?1 AND endpoint_revision = ?2
            "#,
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .fetch_one(read.connection())
        .await
        .expect("source metadata");
        (
            row.get("source_state"),
            row.get("last_success_at"),
            row.get("last_complete_at"),
        )
    }

    async fn published_status_fact_counts(
        runtime: &PersistenceRuntime,
        station_id: &str,
        endpoint_revision: i64,
    ) -> (i64, i64, i64) {
        let mut read = runtime.begin_read().await.expect("read session");
        let sources = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM station_published_status_sources WHERE station_id = ?1 AND endpoint_revision = ?2",
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .fetch_one(read.connection())
        .await
        .expect("source count");
        let monitors = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM station_published_monitors WHERE station_id = ?1 AND endpoint_revision = ?2",
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .fetch_one(read.connection())
        .await
        .expect("monitor count");
        let samples = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM station_published_monitor_samples samples JOIN station_published_monitors monitors ON monitors.id = samples.monitor_id WHERE monitors.station_id = ?1 AND monitors.endpoint_revision = ?2",
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .fetch_one(read.connection())
        .await
        .expect("sample count");
        (sources, monitors, samples)
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
                execution_started_at_ms: None,
                execution_duration_ms: None,
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

    #[tokio::test]
    async fn collected_group_rate_refreshes_bound_key_projection_and_preserves_manual_override() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("bound-key-rate-projection.sqlite3"),
        )
        .await
        .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock.clone(), ids.clone());
        let credentials = CredentialService::new(
            runtime.handle(),
            Arc::new(DataKeyVault::for_test([37; 32])),
            clock,
            ids,
        );
        let station = stations
            .create(CreateStationInput {
                name: "Bound key rate projection".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://projection.example.test".to_string(),
                api_base_url: "https://projection.example.test/v1".to_string(),
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
        let initial_binding = collectors
            .upsert_station_group_binding(group_binding_input(&station.id))
            .await
            .expect("initial group binding");

        for (name, manual_rate_multiplier) in [("automatic", None), ("manual override", Some(0.08))]
        {
            credentials
                .create_station_key(CreateStationKeyInput {
                    station_id: station.id.clone(),
                    name: name.to_string(),
                    api_key: format!("sk-fixture-{name}"),
                    enabled: true,
                    priority: None,
                    max_concurrency: None,
                    load_factor: None,
                    schedulable: None,
                    group_name: Some(initial_binding.group_name.clone()),
                    tier_label: None,
                    group_binding_id: Some(initial_binding.id.clone()),
                    group_id_hash: initial_binding.group_id_hash.clone(),
                    rate_multiplier: Some(0.1),
                    manual_rate_multiplier,
                    rate_source: Some("manual_legacy".to_string()),
                    balance_scope: Some("station_key".to_string()),
                    note: None,
                })
                .await
                .expect("bound station key");
        }
        credentials
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: "unbound".to_string(),
                api_key: "sk-fixture-unbound".to_string(),
                enabled: true,
                priority: None,
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: Some(0.7),
                manual_rate_multiplier: Some(0.7),
                rate_source: Some("manual".to_string()),
                balance_scope: Some("station_key".to_string()),
                note: None,
            })
            .await
            .expect("unbound station key");

        collectors
            .apply_result(CollectorApplyRequest {
                run_key: "bound-key-rate-refresh".to_string(),
                station_id: station.id.clone(),
                endpoint_revision: station.endpoint_revision,
                parent_run_id: None,
                adapter: "sub2api".to_string(),
                task_type: "groups".to_string(),
                status: "success".to_string(),
                facts: CanonicalCollectorFacts {
                    rates: vec![CanonicalRateFact {
                        station_id: station.id.clone(),
                        station_key_id: None,
                        group_id: initial_binding.group_id_hash.clone(),
                        group_key_hash: initial_binding.group_key_hash.clone(),
                        group_name: initial_binding.group_name.clone(),
                        default_rate_multiplier: Some(0.05),
                        user_rate_multiplier: Some(0.05),
                        effective_rate_multiplier: Some(0.05),
                        inferred_group_category: Some("gpt".to_string()),
                        source: "sub2api_groups_rates".to_string(),
                        confidence: 0.95,
                        checked_at: Some("1700000000000".to_string()),
                        raw_json_redacted: None,
                    }],
                    ..CanonicalCollectorFacts::default()
                },
                summary_json: json!({"groups": 1}),
                normalized_json: json!({"groups": ["Manual Group"]}),
                raw_json_redacted: None,
                error_code: None,
                error_message: None,
                endpoint_count: 2,
                success_count: 2,
                failure_count: 0,
                manual_action_required: false,
                next_due_at: None,
                execution_started_at_ms: None,
                execution_duration_ms: None,
            })
            .await
            .expect("collector apply");

        let keys = credentials
            .list_station_keys(station.id)
            .await
            .expect("station keys");
        let automatic = keys
            .iter()
            .find(|key| key.name == "automatic")
            .expect("automatic key");
        assert_eq!(automatic.rate_multiplier, Some(0.05));
        assert_eq!(automatic.manual_rate_multiplier, None);
        assert_eq!(
            automatic.rate_source.as_deref(),
            Some("sub2api_groups_rates")
        );
        assert_eq!(
            automatic.rate_collected_at.as_deref(),
            Some("1700000000000")
        );

        let manual = keys
            .iter()
            .find(|key| key.name == "manual override")
            .expect("manual key");
        assert_eq!(manual.rate_multiplier, Some(0.05));
        assert_eq!(manual.manual_rate_multiplier, Some(0.08));

        let unbound = keys
            .iter()
            .find(|key| key.name == "unbound")
            .expect("unbound key");
        assert_eq!(unbound.rate_multiplier, Some(0.7));
        assert_eq!(unbound.rate_source.as_deref(), Some("manual"));
        runtime.close().await.expect("close persistence runtime");
    }
}
