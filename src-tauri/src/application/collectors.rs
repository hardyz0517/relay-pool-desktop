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
        collection_state::{
            reduce_collection, CollectionPlan, CollectionReducerError, CollectionStatus,
            CollectionTaskKind, Completion, FailureClass, Freshness, ReasonCode, RevisionFence,
            TaskOutcome,
        },
        error::ApplicationError,
        ids::IdGenerator,
        pagination::{PageLimit, MAX_PAGE_LIMIT},
    },
    models::{
        alerting::{AlertEventType, ObservationKind, Severity},
        collector::{CollectorEvent, CollectorRunResult, MutationReceipt},
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
                collector_operation_key, BalanceWrite, CollectorRunFinish, CollectorRunStart,
                CollectorSnapshotWrite, CollectorStore, GroupTransition, GroupWrite,
                RateTransition, RateWrite, StationCollectionProjectionWrite,
                StationGroupBindingWrite, StoredCollectorApply,
            },
            credential_store::CredentialStore,
            station_catalog::StationCatalogStore,
            station_published_status_store::{
                PublishedMonitorSampleWrite, PublishedMonitorWrite, PublishedStatusSourceWrite,
                StationPublishedStatusStore,
            },
        },
    },
    services::group_categories::normalize_group_category,
};

#[cfg(test)]
use crate::application::queries::collector_history::CollectorHistoryQuery;
#[cfg(test)]
use crate::models::{collector::CollectorSnapshot, collector_runs::CollectorRun};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CollectorApplyOutcome {
    pub run_id: String,
    pub snapshot_id: String,
    pub inserted: bool,
    pub operation_id: Option<String>,
    pub collection_revision: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CollectorFullApplyOutcome {
    pub(crate) parent: CollectorApplyOutcome,
    pub(crate) children: Vec<CollectorApplyOutcome>,
}

impl From<StoredCollectorApply> for CollectorApplyOutcome {
    fn from(stored: StoredCollectorApply) -> Self {
        Self {
            run_id: stored.run_id,
            snapshot_id: stored.snapshot_id,
            inserted: stored.inserted,
            operation_id: None,
            collection_revision: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CollectorApplyRequest {
    pub run_key: String,
    pub station_id: String,
    pub endpoint_revision: i64,
    pub credential_revision: i64,
    pub intent_sequence: i64,
    /// Legacy history linkage retained for compatibility tests/fixtures only.
    /// Production authorization/collection control flow never branches on it.
    #[cfg(test)]
    #[serde(skip)]
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
    pub balance_kind: String,
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
    pub evidence_confidence: String,
    pub spendability_authority: String,
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
    credentials: CredentialStore,
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
        let committed_at_ms = snapshot
            .created_at
            .parse::<i64>()
            .unwrap_or_default()
            .max(0);
        let mutation_id = outcome
            .operation_id
            .clone()
            .unwrap_or_else(|| outcome.run_id.clone());
        let receipt = match outcome.collection_revision {
            Some(revision) => MutationReceipt::for_scope(
                mutation_id.clone(),
                committed_at_ms,
                format!("station_collection:{}", snapshot.station_id),
                revision,
            ),
            None => MutationReceipt::without_revision(mutation_id, committed_at_ms),
        };
        Ok(CollectorRunResult {
            snapshot,
            events: vec![CollectorEvent {
                event_type: task_type.to_string(),
                message,
                status,
            }],
            receipt,
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
            credentials: CredentialStore,
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

    /// Reserve a station-scoped collection intent before provider I/O.
    /// Endpoint and credential revisions are checked in the same transaction
    /// as the allocator so a prepared operation cannot start against a fence
    /// that was already superseded.
    pub(crate) async fn allocate_station_collection_intent(
        &self,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
    ) -> Result<i64, ApplicationError> {
        let mut write = self.runtime.begin_write().await?;
        let sequence = self
            .collectors
            .allocate_station_collection_intent(
                &mut write,
                station_id,
                endpoint_revision,
                credential_revision,
                self.clock.now_utc().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
        Ok(sequence)
    }

    pub(crate) async fn start_capture_operation(
        &self,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
    ) -> Result<(String, i64), ApplicationError> {
        let mut write = self.runtime.begin_write().await?;
        let operation = self
            .collectors
            .start_capture_operation(
                &mut write,
                station_id,
                endpoint_revision,
                credential_revision,
                self.clock.now_utc().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
        Ok(operation)
    }

    pub(crate) async fn finish_capture_operation(
        &self,
        operation_id: &str,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
        intent_sequence: i64,
        terminal_status: &str,
        reason_code: Option<&str>,
    ) -> Result<(), ApplicationError> {
        let mut write = self.runtime.begin_write().await?;
        self.collectors
            .finish_capture_operation(
                &mut write,
                operation_id,
                station_id,
                endpoint_revision,
                credential_revision,
                intent_sequence,
                terminal_status,
                reason_code,
                self.clock.now_utc().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
        Ok(())
    }

    pub(crate) async fn interrupt_active_capture_operations(
        &self,
    ) -> Result<u64, ApplicationError> {
        let mut write = self.runtime.begin_write().await?;
        let interrupted = self
            .collectors
            .interrupt_active_capture_operations(
                &mut write,
                self.clock.now_utc().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
        Ok(interrupted)
    }

    /// Reconcile ordinary collector operations left queued/running by a
    /// previous process.  This is intentionally separate from capture
    /// recovery because capture has a distinct user-visible lifecycle.
    pub(crate) async fn recover_active_collector_operations(
        &self,
    ) -> Result<u64, ApplicationError> {
        let mut write = self.runtime.begin_write().await?;
        let recovered = self
            .collectors
            .recover_active_collector_operations(
                &mut write,
                self.clock.now_utc().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
        Ok(recovered)
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

    pub(crate) async fn record_capture_snapshot(
        &self,
        request: CaptureSnapshotRequest,
    ) -> Result<CollectorRunResult, ApplicationError> {
        if request.station_id.trim().is_empty()
            || request.endpoint_revision < 1
            || request.event_count < 0
            || !matches!(request.task_type.as_str(), "capture" | "recharge")
            || !matches!(
                request.status.as_str(),
                "success" | "partial" | "failed" | "manual_required" | "needs_confirmation"
            )
        {
            return Err(ApplicationError::ConstraintViolation);
        }

        let request_hash = canonical_hash(&request)?;
        let result_task_type = request.task_type.clone();
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
                    // Capture evidence is not a collector task. Keep task
                    // state and compatibility Station fields untouched here;
                    // collection and authorization projections are owned by
                    // their typed commit paths and must not be inferred from
                    // WebView payloads.
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

        let mut result = self.result_for_apply(&outcome, &result_task_type).await?;
        result.events.clear();
        result.receipt.affected_scopes.clear();
        result.receipt.revision_vector.clear();
        Ok(result)
    }

    pub(crate) async fn apply_result(
        &self,
        request: CollectorApplyRequest,
    ) -> Result<CollectorApplyOutcome, ApplicationError> {
        let station_id = request.station_id.clone();
        let mut write = self.runtime.begin_write().await?;
        let terminal_at_ms = self.clock.now_utc().timestamp_millis().max(0);
        if !self
            .collectors
            .fence_collector_operation_for_commit(
                &mut write,
                &request.station_id,
                request.endpoint_revision,
                request.credential_revision,
                request.intent_sequence,
                terminal_at_ms,
            )
            .await?
        {
            write.commit().await?;
            return Err(ApplicationError::StaleRevision);
        }
        let (outcome, alerting_changed, _) = self
            .apply_result_in_session(&mut write, request, None, true, true, true)
            .await?;
        write.commit().await?;
        if alerting_changed {
            self.alerting_updates.notify_after_commit();
        }
        publish_station_collection_revision_notice(&outcome, &station_id);
        Ok(outcome)
    }

    /// Commits a Full parent and all of its task results as one visible unit.
    ///
    /// The outbound work has already completed at this boundary. Keeping the
    /// parent, children, canonical facts, task state, alert transitions and
    /// final station compatibility projection in one SQLite transaction
    /// prevents readers from observing a parent result combined with stale
    /// child state.
    pub(crate) async fn apply_full_result(
        &self,
        parent: CollectorApplyRequest,
        children: Vec<CollectorApplyRequest>,
    ) -> Result<CollectorFullApplyOutcome, ApplicationError> {
        if parent.task_type != "full"
            || children.is_empty()
            || children.iter().any(|child| {
                child.station_id != parent.station_id
                    || child.endpoint_revision != parent.endpoint_revision
                    || child.credential_revision != parent.credential_revision
                    || child.intent_sequence != parent.intent_sequence
                    || child.task_type == "full"
            })
        {
            return Err(ApplicationError::ConstraintViolation);
        }
        let station_id = parent.station_id.clone();
        let endpoint_revision = parent.endpoint_revision;
        let credential_revision = parent.credential_revision;
        let intent_sequence = parent.intent_sequence;
        // The parent result is persisted first so child run ids can point at
        // it, but its collector observation must be deferred until every
        // typed child task state has been written below. Keeping the parent
        // request also gives the final observation a stable operation summary
        // without reading diagnostic `summary_json.childRuns`.
        let parent_for_observation = parent.clone();
        // Derive the typed collection status from the complete child outcome
        // set. The parent summary is diagnostic only and may be provider-
        // specific; it must never decide current state.
        let (projection_status, projection_reasons) =
            collection_projection_for_full(&parent, &children)?;
        let collected_at = self.clock.now_utc().timestamp_millis().to_string();
        let projection_updated_at_ms = collected_at.parse::<i64>().unwrap_or_default();
        let mut write = self.runtime.begin_write().await?;
        if !self
            .collectors
            .fence_collector_operation_for_commit(
                &mut write,
                &parent.station_id,
                parent.endpoint_revision,
                parent.credential_revision,
                parent.intent_sequence,
                projection_updated_at_ms.max(0),
            )
            .await?
        {
            write.commit().await?;
            return Err(ApplicationError::StaleRevision);
        }
        let (mut parent_outcome, mut alerting_changed, mut revision) = self
            .apply_result_in_session(&mut write, parent.clone(), None, false, false, true)
            .await?;
        let mut child_outcomes = Vec::with_capacity(children.len());
        for child in children {
            let (outcome, child_alerting_changed, child_revision) = self
                // Child rows are part of the Full transaction. They persist
                // task history and facts, but the parent owns the single
                // terminal collection projection/observation commit below.
                .apply_result_in_session(
                    &mut write,
                    child,
                    Some(parent_outcome.run_id.as_str()),
                    false,
                    false,
                    false,
                )
                .await?;
            alerting_changed |= child_alerting_changed;
            if child_revision.is_some() {
                revision = child_revision;
            }
            child_outcomes.push(outcome);
        }

        // Full collector alerting is reduced from the typed task-state rows
        // now that all children are present in this transaction. This avoids
        // treating the parent diagnostic payload as a current-state protocol
        // and guarantees the observation reflects the complete Full result.
        if collector_task_side_effect_policy("full").emits_collector_observation {
            let failed_task_types =
                collector_failed_task_types(&self.collectors, &mut write, &parent_for_observation)
                    .await?;
            let kind = if failed_task_types.is_empty() {
                ObservationKind::Healthy
            } else {
                ObservationKind::Abnormal
            };
            alerting_changed |= self
                .alerting
                .record_in_session(
                    &mut write,
                    collector_observation(
                        &parent_for_observation,
                        &collector_failure_key(&station_id),
                        &parent_outcome.run_id,
                        kind,
                        &failed_task_types,
                        &collected_at,
                    ),
                )
                .await?
                .inserted;
        }

        let persisted_revision = self
            .collectors
            .station_collection_revision(&mut write, &station_id)
            .await?
            .unwrap_or(endpoint_revision.max(1));
        // Replays do not advance the durable watermark. Never let an
        // idempotent Full apply replace a newer projection revision with the
        // endpoint baseline (which was the previous fallback when every run
        // already existed).
        let projection_revision = revision
            .unwrap_or(persisted_revision)
            .max(persisted_revision)
            .max(1);
        self.collectors
            .upsert_station_collection_projection(
                &mut write,
                &StationCollectionProjectionWrite {
                    station_id: station_id.clone(),
                    status: collection_status_to_storage(projection_status).to_string(),
                    reason_codes_json: serde_json::to_string(&projection_reasons)
                        .map_err(|_| ApplicationError::Internal)?,
                    revision: projection_revision,
                    endpoint_revision,
                    credential_revision,
                    intent_sequence,
                    operation_id: parent_outcome
                        .operation_id
                        .clone()
                        .unwrap_or_else(|| parent_outcome.run_id.clone()),
                    updated_at_ms: projection_updated_at_ms,
                },
            )
            .await?;
        let (operation_status, operation_reason) = collector_operation_terminal(&parent);
        self.collectors
            .finish_collector_operation(
                &mut write,
                &station_id,
                endpoint_revision,
                credential_revision,
                intent_sequence,
                operation_status,
                operation_reason,
                projection_updated_at_ms.max(0),
            )
            .await?;
        parent_outcome.collection_revision = Some(projection_revision);
        write.commit().await?;
        if alerting_changed {
            self.alerting_updates.notify_after_commit();
        }
        publish_station_collection_revision_notice(&parent_outcome, &station_id);
        Ok(CollectorFullApplyOutcome {
            parent: parent_outcome,
            children: child_outcomes,
        })
    }

    async fn apply_result_in_session(
        &self,
        write: &mut crate::persistence::WriteSession,
        request: CollectorApplyRequest,
        history_parent_run_id: Option<&str>,
        emit_collection_observation: bool,
        finalize_operation: bool,
        record_authorization_observation: bool,
    ) -> Result<(CollectorApplyOutcome, bool, Option<i64>), ApplicationError> {
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

        if let Some(existing) = collectors.existing_apply(write, &request.run_key).await? {
            if existing.request_hash != request_hash {
                return Err(
                    crate::persistence::error::PersistenceError::InvariantViolation(
                        "collector run key was reused for a different canonical result".to_string(),
                    )
                    .into(),
                );
            }
            let mut outcome = CollectorApplyOutcome::from(existing.outcome);
            outcome.operation_id = Some(collector_operation_key(
                &request.station_id,
                request.endpoint_revision,
                request.credential_revision,
                request.intent_sequence,
            ));
            outcome.collection_revision = collectors
                .station_collection_revision(write, &request.station_id)
                .await?;
            return Ok((outcome, false, None));
        }

        collectors
            .assert_endpoint_revision(write, &request.station_id, request.endpoint_revision)
            .await?;
        collectors
            .assert_station_credential_revision(
                write,
                &request.station_id,
                request.credential_revision,
            )
            .await?;
        collectors
            .assert_station_collection_intent(
                write,
                &request.station_id,
                request.endpoint_revision,
                request.credential_revision,
                request.intent_sequence,
            )
            .await?;
        collectors
            .mark_collector_operation_running(
                write,
                &request.station_id,
                request.endpoint_revision,
                request.credential_revision,
                request.intent_sequence,
                &request.task_type,
                started_ms.max(0),
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
                    // Parent linkage is retained solely for historical run
                    // navigation. It never participates in current-state
                    // authority, fencing, scheduling, or authorization.
                    parent_run_id: history_parent_run_id.map(ToOwned::to_owned),
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
                        balance_kind: balance.balance_kind.clone(),
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
                        evidence_confidence: balance.evidence_confidence.clone(),
                        spendability_authority: balance.spendability_authority.clone(),
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
        let mut collection_scopes = HashMap::<String, (HashSet<String>, HashSet<String>)>::new();
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
                        last_seen_at: rate.checked_at.clone().or_else(|| Some(now.clone())),
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
                .and_modify(|remembered| remembered.current = transition.current.clone())
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
                        checked_at: rate.checked_at.clone().unwrap_or_else(|| now.clone()),
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
            .filter(|transition| transition.current.binding_kind == BINDING_KIND_STATION_GROUP)
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
            if let Some(observation) = group_transition_observation(transition, &now, &run_id) {
                alerting_changed |= alerting
                    .record_in_session(write, observation)
                    .await?
                    .inserted;
            }
        }
        for transition in rate_transitions
            .iter()
            .filter(|transition| should_emit_rate_change(transition))
        {
            alerting_changed |= alerting
                .record_in_session(
                    write,
                    rate_change_observation(&request.station_id, transition, &run_id, &now),
                )
                .await?
                .inserted;
        }

        // A full collection owns the lifecycle of its child tasks. Child
        // runs still persist facts and run history, but must not create a
        // second incident for the same collection operation.
        let emit_collector_observation = collector_task_side_effect_policy(&request.task_type)
            .emits_collector_observation
            && emit_collection_observation;
        if emit_collection_observation
            && emit_collector_observation
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

        // A reauthorization requirement is a distinct station
        // condition. Keep it independent from the collector
        // failure projection, including optional root tasks such
        // as published-status collection.
        if record_authorization_observation && should_record_collector_observation(&request.status)
        {
            let authorization_expired = request_requires_manual_authorization(&request);
            let authorization_operation_id = collector_operation_key(
                &request.station_id,
                request.endpoint_revision,
                request.credential_revision,
                request.intent_sequence,
            );
            if authorization_expired
                && !self
                    .credentials
                    .record_collector_reauthorization_requirement(
                        write,
                        &request.station_id,
                        request.credential_revision,
                        &authorization_operation_id,
                        finished_ms.max(0),
                    )
                    .await?
            {
                return Err(ApplicationError::StaleRevision);
            }
            let authorization_recovery_task = if authorization_expired
                || !matches!(request.status.as_str(), "success" | "partial")
            {
                None
            } else {
                collectors
                    .authorization_expiry_task_type(write, &request.station_id)
                    .await?
            };
            let authorization_recovered =
                authorization_recovery_task
                    .as_deref()
                    .is_some_and(|task_type| {
                        request_confirms_authorization_recovery(&request, task_type)
                    });
            if authorization_recovered
                && !self
                    .credentials
                    .record_collector_authorization_recovery(
                        write,
                        &request.station_id,
                        request.credential_revision,
                        &authorization_operation_id,
                        finished_ms.max(0),
                    )
                    .await?
            {
                return Err(ApplicationError::StaleRevision);
            }
            if authorization_expired || authorization_recovered {
                alerting_changed |= alerting
                    .record_in_session(
                        write,
                        authorization_expired_observation(
                            &request,
                            &run_id,
                            if authorization_expired {
                                ObservationKind::Abnormal
                            } else {
                                ObservationKind::Healthy
                            },
                            &now,
                        ),
                    )
                    .await?
                    .inserted;
            }
        }

        #[cfg(test)]
        collectors
            .update_task_state_for_test(
                write,
                &crate::persistence::stores::collector_store::CollectorTaskStateWrite {
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
                    status: request.status.clone(),
                    finished_at: now.clone(),
                    duration_ms,
                    endpoint_count: request.endpoint_count,
                    success_count: request.success_count,
                    failure_count: request.failure_count,
                    manual_action_required: request.manual_action_required,
                    error_code: request.error_code.clone(),
                    error_message: request.error_message.clone(),
                    snapshot_id,
                },
            )
            .await?;
        let revision = collectors
            .advance_station_collection_revision(write, &request.station_id, applied_at_ms)
            .await?;
        // Persist the typed collection projection alongside the run/snapshot
        // terminal state. Child rows in a Full operation are intentionally
        // excluded; the parent operation writes the final aggregate below.
        if emit_collection_observation && task_owns_collection_projection(&request.task_type) {
            let status = typed_collection_status_for_request(&request)
                .map(collection_status_to_storage)
                .unwrap_or("degraded");
            let reasons = if status == "healthy" {
                Vec::<&str>::new()
            } else if request_requires_manual_authorization(&request) {
                vec!["authorization_required"]
            } else {
                vec!["core_task_failed"]
            };
            collectors
                .upsert_station_collection_projection(
                    write,
                    &StationCollectionProjectionWrite {
                        station_id: request.station_id.clone(),
                        status: status.to_string(),
                        reason_codes_json: serde_json::to_string(&reasons)
                            .map_err(|_| ApplicationError::Internal)?,
                        revision,
                        endpoint_revision: request.endpoint_revision,
                        credential_revision: request.credential_revision,
                        intent_sequence: request.intent_sequence,
                        operation_id: request.run_key.clone(),
                        updated_at_ms: applied_at_ms.max(0),
                    },
                )
                .await?;
        }
        if finalize_operation {
            let (operation_status, operation_reason) = collector_operation_terminal(&request);
            collectors
                .finish_collector_operation(
                    write,
                    &request.station_id,
                    request.endpoint_revision,
                    request.credential_revision,
                    request.intent_sequence,
                    operation_status,
                    operation_reason,
                    finished_ms.max(0),
                )
                .await?;
        }
        let mut outcome = CollectorApplyOutcome::from(stored);
        outcome.operation_id = Some(collector_operation_key(
            &request.station_id,
            request.endpoint_revision,
            request.credential_revision,
            request.intent_sequence,
        ));
        outcome.collection_revision = Some(revision);
        Ok((outcome, alerting_changed, Some(revision)))
    }
}

fn publish_station_collection_revision_notice(outcome: &CollectorApplyOutcome, station_id: &str) {
    let Some(revision) = outcome.collection_revision else {
        return;
    };
    crate::application::queries::read_model_revision::publish_domain_revision_notice(
        crate::application::queries::read_model_revision::DomainRevisionNotice::for_mutation_scope(
            outcome
                .operation_id
                .clone()
                .unwrap_or_else(|| outcome.run_id.clone()),
            format!("station_collection:{station_id}"),
            revision,
        ),
    );
}

fn collector_operation_terminal(request: &CollectorApplyRequest) -> (&'static str, Option<&str>) {
    match request.status.as_str() {
        "success" => ("succeeded", None),
        "partial" => ("partially_succeeded", Some("partial")),
        "cancelled" => ("cancelled", Some("cancelled")),
        "interrupted" => ("interrupted", Some("interrupted")),
        "manual_required" | "needs_confirmation" => ("failed", Some("authorization_required")),
        _ => ("failed", Some("collection_failed")),
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
    updates_collection_projection: bool,
    emits_collector_observation: bool,
}

fn collector_task_side_effect_policy(task_type: &str) -> CollectorTaskSideEffectPolicy {
    match task_type {
        "detect" | "balance" => CollectorTaskSideEffectPolicy {
            updates_collection_projection: true,
            emits_collector_observation: true,
        },
        "groups" | "full" => CollectorTaskSideEffectPolicy {
            updates_collection_projection: true,
            emits_collector_observation: true,
        },
        "published_status" => CollectorTaskSideEffectPolicy {
            updates_collection_projection: false,
            emits_collector_observation: false,
        },
        // `validate_request` rejects unknown tasks. Defaulting to no side effects
        // keeps any future task isolated until it receives an explicit policy.
        _ => CollectorTaskSideEffectPolicy {
            updates_collection_projection: false,
            emits_collector_observation: false,
        },
    }
}

fn task_owns_collection_projection(task_type: &str) -> bool {
    collector_task_side_effect_policy(task_type).updates_collection_projection
}

/// Reduce all child task outcomes before publishing the typed collection
/// projection. The parent summary is diagnostic only and may be provider-
/// specific; it must never decide current state.
fn collection_projection_for_full(
    parent: &CollectorApplyRequest,
    children: &[CollectorApplyRequest],
) -> Result<(CollectionStatus, Vec<String>), ApplicationError> {
    if children.is_empty() {
        return Err(ApplicationError::ConstraintViolation);
    }
    let fence = RevisionFence::new(
        parent.endpoint_revision,
        parent.credential_revision,
        parent.intent_sequence,
    )
    .map_err(|_| ApplicationError::ConstraintViolation)?;
    let mut specs = Vec::with_capacity(children.len());
    let mut outcomes = Vec::with_capacity(children.len());
    for child in children {
        let task = CollectionTaskKind::parse(&child.task_type)
            .ok_or(ApplicationError::ConstraintViolation)?;
        let role = if task == CollectionTaskKind::PublishedStatus {
            crate::application::collection_state::TaskRole::Optional
        } else {
            crate::application::collection_state::TaskRole::Core
        };
        specs.push(crate::application::collection_state::TaskSpec {
            task,
            role,
            fact_families: match task {
                CollectionTaskKind::Balance => {
                    vec![crate::application::collection_state::FactFamily::Balance]
                }
                CollectionTaskKind::Groups => {
                    vec![crate::application::collection_state::FactFamily::Groups]
                }
                CollectionTaskKind::PublishedStatus => {
                    vec![crate::application::collection_state::FactFamily::PublishedStatus]
                }
                CollectionTaskKind::Detect => {
                    vec![crate::application::collection_state::FactFamily::Endpoint]
                }
            },
        });
        let completion = match child.status.as_str() {
            "success" => Completion::Succeeded,
            "partial" => Completion::Partial,
            "failed" | "manual_required" | "needs_confirmation" => Completion::Failed,
            _ => return Err(ApplicationError::ConstraintViolation),
        };
        let failure_class = (completion != Completion::Succeeded).then(|| {
            classify_failure_code(child.error_code.as_deref(), child.manual_action_required)
        });
        let auth_effect = (failure_class == Some(FailureClass::AuthRejected))
            .then_some(crate::application::collection_state::AuthEffect::RequiresReauthorization)
            .unwrap_or(crate::application::collection_state::AuthEffect::None);
        let observed_at_ms = child.execution_started_at_ms.unwrap_or(0).max(0);
        let outcome = if completion == Completion::Succeeded {
            TaskOutcome::succeeded(
                task,
                parent.run_key.clone(),
                fence,
                Freshness::Fresh,
                observed_at_ms,
            )
        } else if completion == Completion::Failed {
            TaskOutcome::failed(
                task,
                parent.run_key.clone(),
                fence,
                failure_class.unwrap_or(FailureClass::Internal),
                auth_effect,
                observed_at_ms,
            )
        } else {
            TaskOutcome::new(
                task,
                parent.run_key.clone(),
                fence,
                completion,
                failure_class,
                auth_effect,
                Freshness::Unknown,
                if failure_class == Some(FailureClass::AuthRejected) {
                    ReasonCode::AuthorizationRequired
                } else {
                    ReasonCode::None
                },
                observed_at_ms,
            )
        };
        outcomes.push(outcome);
    }
    let plan = if specs.len() == 3
        && specs
            .iter()
            .any(|spec| spec.task == CollectionTaskKind::Balance)
        && specs
            .iter()
            .any(|spec| spec.task == CollectionTaskKind::Groups)
        && specs
            .iter()
            .any(|spec| spec.task == CollectionTaskKind::PublishedStatus)
    {
        CollectionPlan::full_v1()
    } else {
        CollectionPlan::new(
            crate::application::collection_state::CURRENT_COLLECTION_PLAN_VERSION,
            specs,
        )
    };
    let projection = match reduce_collection(&plan, &outcomes, None)
        .map_err(|_| ApplicationError::ConstraintViolation)?
    {
        crate::application::collection_state::CollectionReduction::Applied(projection) => {
            projection
        }
        crate::application::collection_state::CollectionReduction::Stale { .. } => {
            return Ok((CollectionStatus::Stale, vec!["stale_data".to_string()]))
        }
    };
    let reasons = projection
        .reasons
        .into_iter()
        .filter(|reason| *reason != ReasonCode::None)
        .map(reason_code_to_storage)
        .collect::<Vec<_>>();
    Ok((projection.status, reasons))
}

fn reason_code_to_storage(reason: ReasonCode) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "internal_error".to_string())
}

fn typed_collection_status_for_request(
    request: &CollectorApplyRequest,
) -> Result<CollectionStatus, CollectionReducerError> {
    if request.task_type == "full" {
        return Ok(match request.status.as_str() {
            "success" => CollectionStatus::Healthy,
            "partial" => CollectionStatus::Degraded,
            "failed" | "manual_required" => CollectionStatus::Failed,
            _ => CollectionStatus::Degraded,
        });
    }
    let task = CollectionTaskKind::parse(&request.task_type).ok_or(
        CollectionReducerError::UnexpectedTask {
            task: CollectionTaskKind::Detect,
        },
    )?;
    let completion = match request.status.as_str() {
        "success" => Completion::Succeeded,
        "partial" => Completion::Partial,
        "failed" | "manual_required" => Completion::Failed,
        _ => Completion::Failed,
    };
    let failure_class = if completion == Completion::Succeeded {
        None
    } else {
        Some(classify_failure_code(
            request.error_code.as_deref(),
            request.manual_action_required,
        ))
    };
    let fence = RevisionFence::new(
        request.endpoint_revision,
        request.credential_revision,
        request.intent_sequence,
    )
    .map_err(|_| CollectionReducerError::NegativeObservedAt)?;
    let outcome = TaskOutcome::new(
        task,
        request.run_key.clone(),
        fence,
        completion,
        failure_class,
        if failure_class == Some(FailureClass::AuthRejected) {
            crate::application::collection_state::AuthEffect::RequiresReauthorization
        } else {
            crate::application::collection_state::AuthEffect::None
        },
        if completion == Completion::Succeeded {
            Freshness::Fresh
        } else {
            Freshness::Unknown
        },
        if completion == Completion::Succeeded {
            ReasonCode::None
        } else if failure_class == Some(FailureClass::AuthRejected) {
            ReasonCode::AuthorizationRequired
        } else {
            ReasonCode::CoreTaskFailed
        },
        request.execution_started_at_ms.unwrap_or(0),
    );
    let plan = CollectionPlan::single_v1(task);
    match reduce_collection(&plan, &[outcome], None)? {
        crate::application::collection_state::CollectionReduction::Applied(projection) => {
            Ok(projection.status)
        }
        crate::application::collection_state::CollectionReduction::Stale { .. } => {
            Ok(CollectionStatus::Stale)
        }
    }
}

fn collection_status_to_storage(status: CollectionStatus) -> &'static str {
    match status {
        CollectionStatus::NotCollected => "not_collected",
        CollectionStatus::Collecting => "collecting",
        CollectionStatus::Healthy => "healthy",
        CollectionStatus::Degraded => "degraded",
        CollectionStatus::Failed => "failed",
        CollectionStatus::Stale => "stale",
    }
}

fn classify_failure_code(error_code: Option<&str>, manual_action_required: bool) -> FailureClass {
    if manual_action_required
        || error_code == Some(crate::models::collector::MANUAL_AUTHORIZATION_ERROR_CODE)
    {
        return FailureClass::AuthRejected;
    }
    match error_code.unwrap_or_default() {
        code if code.contains("timeout") => FailureClass::Timeout,
        code if code.contains("unsupported") => FailureClass::Unsupported,
        code if code.contains("rate") => FailureClass::RateLimited,
        code if code.contains("transport") => FailureClass::Transport,
        _ => FailureClass::Internal,
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
        || request.credential_revision < 1
        || request.intent_sequence < 1
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
        || (request.execution_started_at_ms.is_none() && request.execution_duration_ms.is_some())
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

fn authorization_expired_observation(
    request: &CollectorApplyRequest,
    source_run_key: &str,
    kind: ObservationKind,
    now: &str,
) -> ObservationIngress {
    let observed_at_ms = parse_now_ms(now);
    ObservationIngress {
        source_observation_key: format!(
            "collector:{}:authorization_expired:{}",
            source_run_key, request.station_id
        ),
        event_type: AlertEventType::AuthorizationExpired,
        condition_key: crate::models::alerting::ConditionKey::new(format!(
            "collector:{}:authorization_expired",
            request.station_id
        ))
        .expect("authorization expiry condition key is bounded"),
        kind,
        severity: Severity::Warning,
        object_type: "station".to_string(),
        object_id: Some(request.station_id.clone()),
        station_id: Some(request.station_id.clone()),
        station_key_id: None,
        source: "collector".to_string(),
        reason_code: Some(
            if kind == ObservationKind::Abnormal {
                "authorization_expired"
            } else {
                "authorization_recovered"
            }
            .to_string(),
        ),
        summary_json: json!({
            "taskType": request.task_type,
            "status": request.status,
            "errorCode": request.error_code,
            "manualActionRequired": kind == ObservationKind::Abnormal,
            "recommendedAction": (kind == ObservationKind::Abnormal)
                .then_some("reauthorize"),
        })
        .to_string(),
        observed_at_ms,
        fact_fresh_until_ms: observed_at_ms.saturating_add(900_000),
    }
}

fn request_requires_manual_authorization(request: &CollectorApplyRequest) -> bool {
    request.status == "manual_required"
        || request.manual_action_required
        || request.error_code.as_deref()
            == Some(crate::models::collector::MANUAL_AUTHORIZATION_ERROR_CODE)
}

fn request_confirms_authorization_recovery(
    request: &CollectorApplyRequest,
    affected_task_type: &str,
) -> bool {
    request.task_type == affected_task_type || request.task_type == "full"
}

fn collector_balance_reason(status: &str) -> Option<String> {
    match status.trim().to_ascii_lowercase().as_str() {
        "depleted" | "exhausted" | "empty" => Some("balance_depleted".to_string()),
        "low" | "warning" => Some("balance_low".to_string()),
        "normal" | "available" | "usable" => Some("balance_usable".to_string()),
        _ => None,
    }
}

fn collector_balance_observed_at_ms(value: &str) -> Option<i64> {
    value
        .trim()
        .parse::<i64>()
        .ok()
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|parsed| parsed.timestamp_millis())
        })
        .filter(|value| *value >= 0)
}

fn parse_now_ms(value: &str) -> i64 {
    value.parse::<i64>().unwrap_or_default().max(0)
}
fn collector_failure_key(station_id: &str) -> String {
    format!("collector:{station_id}:collector_failed")
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
    Ok(merge_typed_failed_task_types(failed, request))
}

/// Merge a just-finished typed task outcome into the current failure set.
///
/// Full operations are reduced from the persisted child task rows after all
/// children have been committed; the parent status is deliberately removed
/// from this compatibility alert summary. No diagnostic JSON is interpreted
/// here, so provider-specific `summary_json` fields cannot become a second
/// current-state protocol.
fn merge_typed_failed_task_types(
    current: impl IntoIterator<Item = String>,
    request: &CollectorApplyRequest,
) -> Vec<String> {
    let mut failed = current.into_iter().collect::<BTreeSet<_>>();

    if request.task_type == "full" {
        // A Full parent is only an operation envelope. Its child task rows
        // own the failure set and are already present when this helper runs.
        failed.remove("full");
    } else {
        apply_collector_task_status(
            &mut failed,
            &request.task_type,
            &request.status,
            request.error_code.as_deref(),
        );
    }

    ["balance", "groups", "detect", "full"]
        .into_iter()
        .filter(|task_type| failed.contains(*task_type))
        .map(str::to_string)
        .collect()
}

fn apply_collector_task_status(
    failed: &mut BTreeSet<String>,
    task_type: &str,
    status: &str,
    error_code: Option<&str>,
) {
    if error_code == Some(crate::models::collector::MANUAL_AUTHORIZATION_ERROR_CODE) {
        failed.remove(task_type);
    } else if status == "failed" {
        failed.insert(task_type.to_string());
    } else if matches!(status, "success" | "partial") {
        failed.remove(task_type);
    } else if status == "manual_required" {
        failed.remove(task_type);
    }
}

fn should_emit_rate_change(transition: &RateTransition) -> bool {
    transition.old_effective_rate_multiplier.is_some()
        || transition.new_effective_rate_multiplier.is_some()
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
        persistence::{
            runtime::PersistenceRuntime,
            stores::{collector_store::GroupState, credential_store::CredentialStore},
        },
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

    #[tokio::test]
    async fn capture_operation_cancel_and_restart_recovery_leave_no_active_rows() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("capture-ledger.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Capture ledger fixture".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://capture-ledger.example.test".to_string(),
                api_base_url: "https://capture-ledger.example.test/v1".to_string(),
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

        let (cancelled_id, cancelled_sequence) = collectors
            .start_capture_operation(&station.id, station.endpoint_revision, 1)
            .await
            .expect("start capture operation");
        collectors
            .finish_capture_operation(
                &cancelled_id,
                &station.id,
                station.endpoint_revision,
                1,
                cancelled_sequence,
                "cancelled",
                Some("user_cancelled"),
            )
            .await
            .expect("cancel capture operation");

        let (interrupted_id, _) = collectors
            .start_capture_operation(&station.id, station.endpoint_revision, 1)
            .await
            .expect("start abandoned capture operation");
        assert_eq!(
            collectors
                .interrupt_active_capture_operations()
                .await
                .expect("recover capture operations"),
            1
        );

        let mut read = runtime.begin_read().await.expect("read capture ledger");
        let active_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_operations
             WHERE task_type = 'capture' AND status IN ('queued', 'running')",
        )
        .fetch_one(read.connection())
        .await
        .expect("count active capture operations");
        let cancelled_status: String =
            sqlx::query_scalar("SELECT status FROM collector_operations WHERE operation_id = ?1")
                .bind(cancelled_id)
                .fetch_one(read.connection())
                .await
                .expect("cancelled status");
        let interrupted_status: String =
            sqlx::query_scalar("SELECT status FROM collector_operations WHERE operation_id = ?1")
                .bind(interrupted_id)
                .fetch_one(read.connection())
                .await
                .expect("interrupted status");

        assert_eq!(active_count, 0);
        assert_eq!(cancelled_status, "cancelled");
        assert_eq!(interrupted_status, "interrupted");
        drop(read);
        runtime.close().await.expect("close runtime");
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
    fn rate_change_events_include_first_effective_rate_but_skip_empty_rate() {
        assert!(should_emit_rate_change(&RateTransition {
            group_binding_id: "binding-1".to_string(),
            group_name: "first group".to_string(),
            old_effective_rate_multiplier: None,
            new_effective_rate_multiplier: Some(0.2),
        }));
        assert!(should_emit_rate_change(&RateTransition {
            group_binding_id: "binding-2".to_string(),
            group_name: "cleared group".to_string(),
            old_effective_rate_multiplier: Some(0.2),
            new_effective_rate_multiplier: None,
        }));
        assert!(!should_emit_rate_change(&RateTransition {
            group_binding_id: "binding-3".to_string(),
            group_name: "empty group".to_string(),
            old_effective_rate_multiplier: None,
            new_effective_rate_multiplier: None,
        }));
    }

    #[test]
    fn producer_observations_cover_failure_recovery_and_audit_change_contracts() {
        let request = CollectorApplyRequest {
            run_key: "failed-run".to_string(),
            station_id: "station-1".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            intent_sequence: 1,
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
    fn manual_authorization_results_are_recorded_for_collector_recovery() {
        assert!(should_record_collector_observation("manual_required"));
        assert!(should_record_collector_observation("success"));
        assert!(should_record_collector_observation("partial"));
        assert!(should_record_collector_observation("failed"));
        assert!(!should_record_collector_observation("unsupported"));
    }

    #[test]
    fn authorization_expiry_is_a_distinct_observation_and_not_a_collector_failure() {
        let request = CollectorApplyRequest {
            run_key: "authorization-expired-run".to_string(),
            station_id: "station-1".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            intent_sequence: 1,
            parent_run_id: None,
            adapter: "newapi".to_string(),
            task_type: "groups".to_string(),
            status: "manual_required".to_string(),
            facts: CanonicalCollectorFacts::default(),
            summary_json: json!({}),
            normalized_json: json!({}),
            raw_json_redacted: None,
            error_code: Some(crate::models::collector::MANUAL_AUTHORIZATION_ERROR_CODE.to_string()),
            error_message: Some("当前登录状态已失效，请重新进行窗口授权".to_string()),
            endpoint_count: 1,
            success_count: 0,
            failure_count: 1,
            manual_action_required: true,
            next_due_at: None,
            execution_started_at_ms: None,
            execution_duration_ms: None,
        };
        let observation = authorization_expired_observation(
            &request,
            "run-1",
            ObservationKind::Abnormal,
            "1700000000000",
        );

        assert_eq!(observation.event_type, AlertEventType::AuthorizationExpired);
        assert_eq!(
            observation.reason_code.as_deref(),
            Some("authorization_expired")
        );
        assert_eq!(
            merge_typed_failed_task_types(Vec::<String>::new(), &request),
            Vec::<String>::new()
        );
    }

    #[test]
    fn published_status_apply_request_is_valid_and_never_updates_core_task_failures() {
        let request = CollectorApplyRequest {
            run_key: "published-status-run".to_string(),
            station_id: "station-1".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            intent_sequence: 1,
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
        assert!(!task_owns_collection_projection(&request.task_type));
        let policy = collector_task_side_effect_policy(&request.task_type);
        assert!(!policy.emits_collector_observation);
        assert!(
            merge_typed_failed_task_types(vec!["published_status".to_string()], &request)
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

        let mut initial_request = published_status_apply_request(
            "published-status-revision-one",
            &initial_station,
            Some(published_status_batch(&initial_station)),
            "success",
        );
        initial_request.intent_sequence = collectors
            .allocate_station_collection_intent(
                &initial_station.id,
                initial_station.endpoint_revision,
                1,
            )
            .await
            .expect("initial collection intent");
        collectors
            .apply_result(initial_request)
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

        let mut failed_request = published_status_apply_request(
            "published-status-revision-two-failed",
            &updated_station,
            None,
            "failed",
        );
        failed_request.intent_sequence = collectors
            .allocate_station_collection_intent(
                &updated_station.id,
                updated_station.endpoint_revision,
                1,
            )
            .await
            .expect("failed replacement intent");
        collectors
            .apply_result(failed_request)
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

        let mut success_request = published_status_apply_request(
            "published-status-revision-two-success",
            &updated_station,
            Some(published_status_batch(&updated_station)),
            "success",
        );
        success_request.intent_sequence = collectors
            .allocate_station_collection_intent(
                &updated_station.id,
                updated_station.endpoint_revision,
                1,
            )
            .await
            .expect("successful replacement intent");
        collectors
            .apply_result(success_request)
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
        let mut request = published_status_apply_request(
            "published-status-idempotent-run",
            &station,
            Some(published_status_batch(&station)),
            "success",
        );
        request.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("collection intent");

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
        let mut complete_request = published_status_apply_request(
            "published-status-complete-inventory",
            &station,
            Some(complete),
            "success",
        );
        complete_request.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("complete inventory intent");
        collectors
            .apply_result(complete_request)
            .await
            .expect("complete inventory");

        let mut partial = published_status_batch(&station);
        partial.source_state = PublishedStatusSourceState::Degraded;
        partial.completeness = PublishedStatusCompleteness::Partial;
        partial.monitors = vec![retained];
        let mut partial_request = published_status_apply_request(
            "published-status-partial-inventory",
            &station,
            Some(partial),
            "partial",
        );
        partial_request.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("partial inventory intent");
        collectors
            .apply_result(partial_request)
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

        let mut failed_request = published_status_apply_request(
            "published-status-failed-inventory",
            &station,
            None,
            "failed",
        );
        failed_request.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("failed inventory intent");
        collectors
            .apply_result(failed_request)
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

        let mut initial_request = published_status_apply_request(
            "published-status-before-sql-failure",
            &station,
            Some(published_status_batch(&station)),
            "success",
        );
        initial_request.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("initial collection intent");
        collectors
            .apply_result(initial_request)
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
        let mut failing_request = published_status_apply_request(
            "published-status-sql-failure",
            &station,
            Some(batch),
            "success",
        );
        failing_request.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("failing collection intent");
        assert!(matches!(
            collectors.apply_result(failing_request).await,
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
        let mut initial_request = published_status_apply_request(
            "published-status-before-revision-change",
            &initial_station,
            Some(published_status_batch(&initial_station)),
            "success",
        );
        initial_request.intent_sequence = collectors
            .allocate_station_collection_intent(
                &initial_station.id,
                initial_station.endpoint_revision,
                1,
            )
            .await
            .expect("initial collection intent");
        collectors
            .apply_result(initial_request)
            .await
            .expect("initial facts");
        let mut stale_request = published_status_apply_request(
            "published-status-stale-revision",
            &initial_station,
            Some(published_status_batch(&initial_station)),
            "success",
        );
        stale_request.intent_sequence = collectors
            .allocate_station_collection_intent(
                &initial_station.id,
                initial_station.endpoint_revision,
                1,
            )
            .await
            .expect("stale collection intent");
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

    #[tokio::test]
    async fn collector_apply_rejects_stale_credential_revision_before_writing_a_run() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("collector-credential-fence.sqlite3"),
        )
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
        let mut stale_request = published_status_apply_request(
            "published-status-stale-credential-revision",
            &station,
            Some(published_status_batch(&station)),
            "success",
        );
        stale_request.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("stale credential collection intent");
        let station_id = station.id.clone();
        runtime
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE domain_revisions
                         SET revision = 2
                         WHERE scope = ?1 AND revision = 1",
                    )
                    .bind(format!("station_account:{station_id}"))
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("advance credential revision");

        assert!(matches!(
            collectors.apply_result(stale_request).await,
            Err(ApplicationError::StaleRevision)
        ));
        assert_eq!(
            collector_apply_row_counts(&runtime, &station.id).await,
            (0, 0, 0)
        );
        runtime.close().await.expect("close runtime");
    }

    #[test]
    fn every_known_collector_task_has_an_explicit_side_effect_policy() {
        for task_type in ["detect", "balance", "groups", "published_status", "full"] {
            let policy = collector_task_side_effect_policy(task_type);
            if task_type == "published_status" {
                assert!(!policy.updates_collection_projection);
                assert!(!policy.emits_collector_observation);
            } else {
                assert!(policy.updates_collection_projection);
                assert!(policy.emits_collector_observation);
            }
        }
        let unknown = collector_task_side_effect_policy("future_task");
        assert!(!unknown.updates_collection_projection);
        assert!(!unknown.emits_collector_observation);
    }

    #[test]
    fn collector_failure_summary_tracks_all_current_failed_tasks() {
        let mut request = CollectorApplyRequest {
            run_key: "groups-failed".to_string(),
            station_id: "station-1".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            intent_sequence: 1,
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
            merge_typed_failed_task_types(vec!["balance".to_string()], &request),
            vec!["balance".to_string(), "groups".to_string()]
        );

        request.status = "success".to_string();
        assert_eq!(
            merge_typed_failed_task_types(
                vec!["balance".to_string(), "groups".to_string()],
                &request,
            ),
            vec!["balance".to_string()]
        );

        request.status = "manual_required".to_string();
        request.manual_action_required = true;
        assert_eq!(
            merge_typed_failed_task_types(vec!["groups".to_string()], &request),
            Vec::<String>::new()
        );

        request.task_type = "full".to_string();
        request.status = "partial".to_string();
        // A Full parent is an envelope; child task rows are the typed source
        // of failures and are reduced after the transaction has written them.
        assert_eq!(
            merge_typed_failed_task_types(
                vec!["balance".to_string(), "full".to_string()],
                &request,
            ),
            vec!["balance".to_string()]
        );

        request.status = "success".to_string();
        assert!(merge_typed_failed_task_types(
            vec!["published_status".to_string(), "full".to_string()],
            &request,
        )
        .is_empty());
        assert_eq!(
            typed_collection_status_for_request(&request).expect("typed status"),
            CollectionStatus::Healthy
        );

        request.status = "partial".to_string();
        assert_eq!(
            typed_collection_status_for_request(&request).expect("typed status"),
            CollectionStatus::Degraded
        );

        request.status = "success".to_string();
        // Child status details are diagnostic JSON only. The typed parent
        // outcome remains the sole collection projection input.
        assert_eq!(
            typed_collection_status_for_request(&request).expect("typed status"),
            CollectionStatus::Healthy
        );
    }

    fn capture_request(station_id: &str, endpoint_revision: i64) -> CaptureSnapshotRequest {
        CaptureSnapshotRequest {
            station_id: station_id.to_string(),
            endpoint_revision,
            task_type: "capture".to_string(),
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
        // WebView capture is evidence-only and must not mutate collection
        // compatibility fields. Collection health is owned by typed task
        // commits, so a fresh station remains unchecked here.
        assert_eq!(collected_station.status, "unchecked");
        assert_eq!(collected_station.last_checked_at, None);
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
    async fn capture_evidence_does_not_mutate_collection_task_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("authorization-capture-recovery.sqlite3"),
        )
        .await
        .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Authorization Capture Recovery".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://authorization-recovery.example.test".to_string(),
                api_base_url: "https://authorization-recovery.example.test/v1".to_string(),
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

        let mut expired = capture_request(&station.id, station.endpoint_revision);
        expired.status = "manual_required".to_string();
        expired.summary_json = json!({ "loginRequired": true });
        collectors
            .record_capture_snapshot(expired)
            .await
            .expect("expired authorization capture");
        assert_eq!(
            stations.list().await.expect("stations after capture")[0].status,
            "unchecked"
        );

        let mut recovered = capture_request(&station.id, station.endpoint_revision);
        recovered.summary_json = json!({ "status": "success", "attempt": "recovered" });
        collectors
            .record_capture_snapshot(recovered)
            .await
            .expect("recovered authorization capture");
        assert_eq!(
            stations
                .list()
                .await
                .expect("stations after second capture")[0]
                .status,
            "unchecked"
        );
        let mut read = runtime.begin_read().await.expect("capture evidence read");
        let task_state_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM collector_task_state WHERE station_id = ?1")
                .bind(&station.id)
                .fetch_one(read.connection())
                .await
                .expect("capture task-state count");
        assert_eq!(task_state_count, 0);
        drop(read);
        let latest = collectors
            .latest_station_snapshot(&station.id)
            .await
            .expect("latest snapshot")
            .expect("recovered snapshot");
        assert_eq!(latest.status, "success");
        runtime.close().await.expect("close runtime");
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

        let intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("collection intent");
        // A freshly allocated operation is queued before the runner marks it
        // running. Scheduling must still suppress that station during this
        // handoff so a second intent cannot be admitted concurrently.
        assert!(collectors
            .due_stations_for_task("balance", 5, PageLimit::new(10).expect("limit"))
            .await
            .expect("queued operation suppression")
            .is_empty());
        collectors
            .apply_result(CollectorApplyRequest {
                run_key: "scheduled-balance-run".to_string(),
                station_id: station.id.clone(),
                endpoint_revision: station.endpoint_revision,
                credential_revision: 1,
                intent_sequence,
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
        let station_id_for_age = station.id.clone();
        runtime
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE collector_runs SET finished_at = '1699999760000' \
                         WHERE station_id = ?1 AND task_type = 'balance'",
                    )
                    .bind(&station_id_for_age)
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
            vec![station.id.clone()]
        );

        // A stale queued claim must not suppress a later due check even if a
        // credential update raced with the worker before it could terminalize
        // the old operation. The scheduler compares the complete fence and
        // treats this row as historical rather than active work.
        let station_id_for_stale_operation = station.id.clone();
        runtime
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE domain_revisions
                         SET revision = 2
                         WHERE scope = 'station_account:' || ?1",
                    )
                    .bind(&station_id_for_stale_operation)
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "UPDATE domain_revisions
                         SET revision = 3
                         WHERE scope = 'station_collection_intent:' || ?1",
                    )
                    .bind(&station_id_for_stale_operation)
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "INSERT INTO collector_operations (
                            operation_id, operation_key, station_id,
                            endpoint_revision, credential_revision, intent_sequence,
                            plan_version, task_type, trigger_kind, status,
                            started_at_ms, finished_at_ms, reason_code, reason_detail,
                            created_at_ms, updated_at_ms
                         ) VALUES (
                            'stale-schedule-operation', 'stale-schedule-operation', ?1,
                            ?2, 1, 2, 'collector-plan-v1', 'unspecified',
                            'unspecified', 'queued', NULL, NULL, NULL, NULL, 1, 1
                         )",
                    )
                    .bind(&station_id_for_stale_operation)
                    .bind(station.endpoint_revision)
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("seed stale queued operation");
        assert_eq!(
            collectors
                .due_stations_for_task("balance", 3, PageLimit::new(10).expect("limit"))
                .await
                .expect("stale operation is ignored")
                .into_iter()
                .map(|station| station.id)
                .collect::<Vec<_>>(),
            vec![station.id.clone()]
        );
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn startup_recovery_unblocks_orphaned_collector_operation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("collector-recovery.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Recovery Station".to_string(),
                station_type: "newapi".to_string(),
                website_url: "https://recovery.example.test".to_string(),
                api_base_url: "https://recovery.example.test/v1".to_string(),
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
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("collection intent");

        // A durable post-authorization worker has a separate recovery owner.
        // Keep it active while exercising ordinary collector recovery so the
        // two startup passes cannot corrupt each other's ledger state.
        {
            let mut write = runtime.begin_write().await.expect("seed post-auth work");
            sqlx::query(
                "INSERT INTO collector_operations (
                    operation_id, operation_key, station_id, endpoint_revision,
                    credential_revision, intent_sequence, plan_version, task_type,
                    trigger_kind, status, started_at_ms, finished_at_ms,
                    reason_code, reason_detail, created_at_ms, updated_at_ms
                 ) VALUES ('post-auth-recovery-operation', 'post-auth-recovery-key', ?1, ?2,
                           1, 99, 'collector-plan-v1', 'post_authorization',
                           'post_authorization', 'running', 100, NULL, NULL, NULL, 100, 100)",
            )
            .bind(&station.id)
            .bind(station.endpoint_revision)
            .execute(write.connection())
            .await
            .expect("insert post-auth operation");
            sqlx::query(
                "INSERT INTO post_authorization_collection_work (
                    station_id, endpoint_revision, credential_revision, state,
                    attempt_count, max_attempts, next_attempt_at_ms, last_error_code,
                    operation_id, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, 1, 'running', 0, 5, 100, NULL,
                           'post-auth-recovery-operation', 100, 100)",
            )
            .bind(&station.id)
            .bind(station.endpoint_revision)
            .execute(write.connection())
            .await
            .expect("insert post-auth work");
            write.commit().await.expect("commit post-auth work");
        }

        // The orphaned queued operation suppresses scheduling until startup
        // recovery terminalizes it.
        let limit = PageLimit::new(10).expect("limit");
        assert!(collectors
            .due_stations_for_task("balance", 5, limit)
            .await
            .expect("queued operation suppression")
            .is_empty());

        assert_eq!(
            collectors
                .recover_active_collector_operations()
                .await
                .expect("recover operations"),
            1
        );
        let due = collectors
            .due_stations_for_task("balance", 5, PageLimit::new(10).expect("limit"))
            .await
            .expect("due stations after recovery");
        assert_eq!(
            due.into_iter()
                .map(|station| station.id)
                .collect::<Vec<_>>(),
            vec![station.id.clone()]
        );

        let mut read = runtime.begin_read().await.expect("read operation");
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM collector_operations
             WHERE station_id = ?1 AND trigger_kind IN ('unspecified', 'collector')",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("operation status");
        assert_eq!(status, "interrupted");
        drop(read);

        let mut read = runtime.begin_read().await.expect("read post-auth state");
        let (operation_status, work_state): (String, String) = sqlx::query_as(
            "SELECT operation.status, work.state
             FROM post_authorization_collection_work AS work
             JOIN collector_operations AS operation
               ON operation.operation_id = work.operation_id
             WHERE work.station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("post-auth state after ordinary recovery");
        assert_eq!(operation_status, "running");
        assert_eq!(work_state, "running");
        drop(read);

        // Its own startup owner can now requeue, claim, and complete the work
        // while keeping the durable work row and operation ledger consistent.
        let claim = {
            let mut write = runtime.begin_write().await.expect("recover post-auth work");
            CredentialStore
                .recover_post_authorization_work(&mut write, 200)
                .await
                .expect("requeue post-auth work");
            let claim = CredentialStore
                .claim_post_authorization_work(&mut write, &station.id, 200)
                .await
                .expect("claim recovered post-auth work")
                .expect("recovered post-auth claim");
            write.commit().await.expect("commit post-auth claim");
            claim
        };
        {
            let mut write = runtime.begin_write().await.expect("finish post-auth work");
            CredentialStore
                .finish_post_authorization_work(&mut write, &claim, true, None, 300)
                .await
                .expect("finish recovered post-auth work");
            write.commit().await.expect("commit post-auth completion");
        }
        let mut read = runtime
            .begin_read()
            .await
            .expect("read post-auth completion");
        let (operation_status, work_state): (String, String) = sqlx::query_as(
            "SELECT operation.status, work.state
             FROM post_authorization_collection_work AS work
             JOIN collector_operations AS operation
               ON operation.operation_id = work.operation_id
             WHERE work.station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("post-auth completion state");
        assert_eq!(operation_status, "succeeded");
        assert_eq!(work_state, "succeeded");
        drop(read);
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn full_projection_is_independent_of_legacy_station_status() {
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
        // Child results are committed only through the atomic Full boundary;
        // a standalone apply cannot use a parent run id as a side-effect
        // authority.
        let intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("full collection intent");
        let mut parent =
            collector_apply_request("parent-status-run", &station, None, "full", "partial");
        parent.intent_sequence = intent_sequence;
        let mut child =
            collector_apply_request("child-status-run", &station, None, "groups", "failed");
        child.intent_sequence = intent_sequence;
        collectors
            .apply_full_result(parent, vec![child])
            .await
            .expect("atomic full apply");

        let collected_station = stations
            .station_for_capture(&station.id)
            .await
            .expect("collected station");
        // Collection health is now owned solely by the typed projection. The
        // legacy station field remains administrative compatibility data.
        assert_eq!(collected_station.status, "unchecked");
        assert_eq!(collected_station.last_pricing_fetched_at, None);
        let mut read = runtime.begin_read().await.expect("projection read");
        let status: String = sqlx::query_scalar(
            "SELECT status FROM station_collection_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("collection projection");
        assert_eq!(status, "failed");
        drop(read);
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn manual_authorization_only_recovers_after_a_matching_task_success() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("manual-authorization.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Manual Authorization".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://manual-authorization.example.test".to_string(),
                api_base_url: "https://manual-authorization.example.test/v1".to_string(),
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

        let mut authorization = collector_apply_request(
            "manual-authorization-run",
            &station,
            Some("legacy-parent-that-must-not-authorize".to_string()),
            "groups",
            "manual_required",
        );
        authorization.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("authorization intent");
        authorization.summary_json = json!({ "loginRequired": true });
        authorization.error_code = Some("manual_authorization_required".to_string());
        authorization.error_message = Some("当前登录状态已失效，请重新进行窗口授权".to_string());
        collectors
            .apply_result(authorization)
            .await
            .expect("manual authorization apply");

        // A single-task request may carry the test-only legacy field, but it
        // cannot grant historical parent authority or alter the authorization
        // transition. Production apply paths always pass `None` as the
        // history linkage and use typed evidence/revision fences instead.
        let mut read = runtime.begin_read().await.expect("authorization run read");
        let parent_run_id: Option<String> = sqlx::query_scalar(
            "SELECT parent_run_id FROM collector_runs
             WHERE station_id = ?1 AND run_key = 'manual-authorization-run'",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("authorization run");
        assert_eq!(parent_run_id, None);

        let event_types = sqlx::query_scalar::<_, String>(
            "SELECT event_type FROM change_incidents
             WHERE station_id = ?1 AND lifecycle_state IN ('pending', 'open', 'recovering')
             ORDER BY event_type",
        )
        .bind(&station.id)
        .fetch_all(read.connection())
        .await
        .expect("active authorization incidents");
        assert_eq!(event_types, vec!["authorization_expired".to_string()]);
        let authorization_projection: (String, i64, String, Option<String>) = sqlx::query_as(
            "SELECT status, credential_revision, authority, reason_code
             FROM station_authorization_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("collector authorization projection");
        assert_eq!(authorization_projection.0, "reauthorization_required");
        assert_eq!(authorization_projection.1, 1);
        assert_eq!(authorization_projection.2, "driver_probe");
        assert_eq!(
            authorization_projection.3.as_deref(),
            Some("authorization_required")
        );
        drop(read);

        let mut balance =
            collector_apply_request("later-balance-run", &station, None, "balance", "success");
        balance.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("balance intent");
        collectors
            .apply_result(balance)
            .await
            .expect("balance apply");

        let collected_station = stations
            .station_for_capture(&station.id)
            .await
            .expect("collected station");
        assert_eq!(collected_station.status, "unchecked");
        let listed_station = stations
            .list()
            .await
            .expect("listed stations")
            .into_iter()
            .find(|listed| listed.id == station.id)
            .expect("listed station");
        // `stations.status` remains a compatibility transport field during
        // the authority cutover; the typed projection is what drives current
        // Station UI state.
        assert_eq!(listed_station.status, "unchecked");
        let latest = collectors
            .latest_station_snapshot(&station.id)
            .await
            .expect("latest station snapshot")
            .expect("latest snapshot");
        // Snapshot history is ordered by recency only. Authorization
        // incidents are projected independently and must not pin an older
        // manual-required row as the current collector evidence.
        assert_eq!(latest.status, "success");

        let mut read = runtime.begin_read().await.expect("balance recovery read");
        let authorization_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_incidents
             WHERE station_id = ?1 AND event_type = 'authorization_expired'
               AND lifecycle_state IN ('pending', 'open', 'recovering')",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("authorization count after unrelated success");
        assert_eq!(authorization_count, 1);
        let authorization_status: String = sqlx::query_scalar(
            "SELECT status FROM station_authorization_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("authorization projection after unrelated success");
        assert_eq!(authorization_status, "reauthorization_required");
        drop(read);

        let mut recovered_groups =
            collector_apply_request("recovered-groups-run", &station, None, "groups", "success");
        let recovered_groups_intent = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("groups recovery intent");
        recovered_groups.intent_sequence = recovered_groups_intent;
        let recovery_operation_id = collector_operation_key(
            &station.id,
            station.endpoint_revision,
            1,
            recovered_groups_intent,
        );
        collectors
            .apply_result(recovered_groups)
            .await
            .expect("groups recovery apply");
        let mut read = runtime.begin_read().await.expect("groups recovery read");
        let authorization_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_incidents
             WHERE station_id = ?1 AND event_type = 'authorization_expired'
               AND lifecycle_state IN ('pending', 'open', 'recovering')",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("authorization count after matching success");
        assert_eq!(authorization_count, 0);
        let authorization_projection: (String, String, Option<String>, String) = sqlx::query_as(
            "SELECT status, authority, reason_code, source_operation_id
             FROM station_authorization_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("authorization projection after matching success");
        assert_eq!(authorization_projection.0, "valid");
        assert_eq!(authorization_projection.1, "driver_probe");
        assert_eq!(authorization_projection.2, None);
        assert_eq!(authorization_projection.3, recovery_operation_id);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn full_apply_commits_parent_and_children_before_refreshing_typed_projection() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("full-atomic-status.sqlite3"))
                .await
                .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Full atomic status".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://full-atomic.example.test".to_string(),
                api_base_url: "https://full-atomic.example.test/v1".to_string(),
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

        let mut old_warning = collector_apply_request(
            "full-old-warning",
            &station,
            None,
            "groups",
            "manual_required",
        );
        old_warning.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("allocate old warning intent");
        old_warning.summary_json = json!({"loginRequired": true});
        collectors
            .apply_result(old_warning)
            .await
            .expect("old warning");

        let full_intent = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("allocate full intent");
        assert_eq!(full_intent, 2);
        let mut parent = collector_apply_request("full-parent", &station, None, "full", "success");
        parent.intent_sequence = full_intent;
        let children: Vec<CollectorApplyRequest> = vec![
            collector_apply_request("full-balance", &station, None, "balance", "success"),
            collector_apply_request("full-groups", &station, None, "groups", "success"),
            collector_apply_request(
                "full-published-status",
                &station,
                None,
                "published_status",
                "failed",
            ),
        ]
        .into_iter()
        .map(|mut child| {
            child.intent_sequence = full_intent;
            child
        })
        .collect();
        let outcome = collectors
            .apply_full_result(parent.clone(), children.clone())
            .await
            .expect("atomic full apply");
        assert_eq!(outcome.children.len(), 3);
        let listed = stations
            .station_for_capture(&station.id)
            .await
            .expect("listed station");
        assert_eq!(listed.status, "unchecked");
        let typed_status: String = sqlx::query_scalar(
            "SELECT status FROM station_collection_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(
            runtime
                .begin_read()
                .await
                .expect("typed projection read")
                .connection(),
        )
        .await
        .expect("typed status");
        assert_eq!(typed_status, "healthy");

        let mut read = runtime.begin_read().await.expect("read");
        let run_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM collector_runs WHERE station_id = ?1")
                .bind(&station.id)
                .fetch_one(read.connection())
                .await
                .expect("run count");
        assert_eq!(run_count, 5);
        let revision: i64 =
            sqlx::query_scalar("SELECT revision FROM domain_revisions WHERE scope = ?1")
                .bind(format!("station_collection:{}", station.id))
                .fetch_one(read.connection())
                .await
                .expect("collection revision");
        assert!(revision >= 4);
        let operation: (String, String, Option<i64>) = sqlx::query_as(
            "SELECT task_type, status, finished_at_ms
             FROM collector_operations
             WHERE station_id = ?1 AND intent_sequence = ?2",
        )
        .bind(&station.id)
        .bind(full_intent)
        .fetch_one(read.connection())
        .await
        .expect("full operation terminal state");
        assert_eq!(operation.0, "full");
        assert_eq!(operation.1, "succeeded");
        assert!(operation.2.is_some());
        drop(read);

        // Replaying the same operation is a no-op for runs, but must not
        // regress the current projection watermark.
        collectors
            .apply_full_result(parent, children)
            .await
            .expect("idempotent full replay");
        let mut read = runtime.begin_read().await.expect("read replay projection");
        let replay_revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM station_collection_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("load replay projection revision");
        assert!(replay_revision >= revision);
        let operation_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_operations
             WHERE station_id = ?1 AND intent_sequence = ?2",
        )
        .bind(&station.id)
        .bind(full_intent)
        .fetch_one(read.connection())
        .await
        .expect("count replayed full operations");
        assert_eq!(operation_count, 1);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn historical_parent_linkage_is_audit_only_and_not_idempotency_input() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("collector-parent-linkage.sqlite3"),
        )
        .await
        .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Collector parent linkage".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://collector-parent.example.test".to_string(),
                api_base_url: "https://collector-parent.example.test/v1".to_string(),
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

        let intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("full collection intent");
        let mut parent = collector_apply_request(
            "parent-linkage-run",
            &station,
            Some("caller-supplied-parent".to_string()),
            "full",
            "success",
        );
        parent.intent_sequence = intent_sequence;
        let mut child = collector_apply_request(
            "child-linkage-run",
            &station,
            Some("another-caller-parent".to_string()),
            "groups",
            "success",
        );
        child.intent_sequence = intent_sequence;

        let first = collectors
            .apply_full_result(parent.clone(), vec![child.clone()])
            .await
            .expect("full apply");
        assert!(first.parent.inserted);
        assert!(first.children[0].inserted);

        let mut read = runtime.begin_read().await.expect("history read");
        let stored_parent: Option<String> =
            sqlx::query_scalar("SELECT parent_run_id FROM collector_runs WHERE id = ?1")
                .bind(&first.children[0].run_id)
                .fetch_one(read.connection())
                .await
                .expect("child history row");
        assert_eq!(stored_parent, Some(first.parent.run_id.clone()));
        let first_parent_hash = canonical_hash(&parent).expect("parent hash");
        drop(read);

        // The compatibility-only request field is deliberately omitted from
        // canonical hashing. Changing it cannot fork the run key or create a
        // second idempotency record; the persisted linkage remains the actual
        // parent run generated by the atomic commit owner.
        parent.parent_run_id = Some("different-parent-after-retry".to_string());
        child.parent_run_id = Some("different-child-after-retry".to_string());
        assert_eq!(
            first_parent_hash,
            canonical_hash(&parent).expect("replay parent hash")
        );
        let replay = collectors
            .apply_full_result(parent, vec![child])
            .await
            .expect("idempotent full replay");
        assert!(!replay.parent.inserted);
        assert!(!replay.children[0].inserted);

        let mut read = runtime.begin_read().await.expect("replay read");
        let run_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM collector_runs WHERE station_id = ?1")
                .bind(&station.id)
                .fetch_one(read.connection())
                .await
                .expect("run count");
        assert_eq!(run_count, 2);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn intent_sequence_orders_results_independently_of_wall_clock_timestamps() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("collection-intent-sequence.sqlite3"),
        )
        .await
        .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Collection intent sequence".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://intent-sequence.example.test".to_string(),
                api_base_url: "https://intent-sequence.example.test/v1".to_string(),
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

        // Allocate intent 1 but hold its outbound result. Allocate and commit
        // intent 2 with an earlier wall-clock timestamp, then deliver intent 1
        // late. Sequence, not clock order, is the authority for concurrent
        // collection results, and the stale operation remains terminal
        // history rather than mutating the current projection.
        let first_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("first intent");
        assert_eq!(first_sequence, 1);

        let second_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("second intent");
        assert_eq!(second_sequence, 2);

        let mut current =
            collector_apply_request("intent-current", &station, None, "groups", "failed");
        current.intent_sequence = second_sequence;
        current.execution_started_at_ms = Some(100);
        collectors
            .apply_result(current)
            .await
            .expect("newer result");

        let mut first =
            collector_apply_request("intent-first", &station, None, "groups", "success");
        first.intent_sequence = first_sequence;
        first.execution_started_at_ms = Some(200);
        let error = collectors
            .apply_result(first)
            .await
            .expect_err("late result must be rejected");
        assert!(matches!(error, ApplicationError::StaleRevision));

        let mut read = runtime.begin_read().await.expect("read");
        let run_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM collector_runs WHERE station_id = ?1")
                .bind(&station.id)
                .fetch_one(read.connection())
                .await
                .expect("run count");
        assert_eq!(run_count, 1);
        let status: String = sqlx::query_scalar(
            "SELECT status FROM station_collection_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("projection status");
        assert_eq!(status, "failed");
        let operation_statuses: Vec<(i64, String)> = sqlx::query_as(
            "SELECT intent_sequence, status FROM collector_operations
             WHERE station_id = ?1 ORDER BY intent_sequence",
        )
        .bind(&station.id)
        .fetch_all(read.connection())
        .await
        .expect("operation statuses");
        assert_eq!(
            operation_statuses,
            vec![(1, "superseded".to_string()), (2, "failed".to_string())]
        );
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn mutation_receipt_keeps_the_committed_revision_after_a_newer_commit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("collection-mutation-receipt.sqlite3"),
        )
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

        let first_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("first intent");
        let mut first =
            collector_apply_request("receipt-first", &station, None, "groups", "success");
        first.intent_sequence = first_sequence;
        let first_outcome = collectors.apply_result(first).await.expect("first result");

        let second_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("second intent");
        let mut second =
            collector_apply_request("receipt-second", &station, None, "groups", "failed");
        second.intent_sequence = second_sequence;
        let second_outcome = collectors
            .apply_result(second)
            .await
            .expect("second result");

        let first_result = collectors
            .result_for_apply(&first_outcome, "groups")
            .await
            .expect("first receipt after newer commit");
        let second_result = collectors
            .result_for_apply(&second_outcome, "groups")
            .await
            .expect("second receipt");
        let first_revision = first_outcome.collection_revision.expect("first revision");
        let second_revision = second_outcome.collection_revision.expect("second revision");

        assert!(first_revision < second_revision);
        assert_eq!(
            first_result.receipt.mutation_id,
            first_outcome.operation_id.expect("first operation id")
        );
        assert_eq!(
            first_result.receipt.revision_vector,
            vec![crate::models::collector::MutationRevision {
                scope: format!("station_collection:{}", station.id),
                revision: first_revision,
            }]
        );
        assert_eq!(
            second_result.receipt.revision_vector[0].revision,
            second_revision
        );
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn equal_intent_from_a_different_operation_rolls_back_every_side_effect() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("collection-intent-conflict.sqlite3"),
        )
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
        let sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("collection intent");

        let mut first =
            collector_apply_request("intent-owner", &station, None, "groups", "success");
        first.intent_sequence = sequence;
        first.execution_started_at_ms = Some(100);
        collectors
            .apply_result(first)
            .await
            .expect("first operation");

        let before = collector_side_effect_counts(&runtime, &station.id).await;
        let mut conflict =
            collector_apply_request("intent-conflict", &station, None, "groups", "failed");
        conflict.intent_sequence = sequence;
        conflict.execution_started_at_ms = Some(100);
        conflict.facts.groups.push(CanonicalGroupFact {
            station_id: station.id.clone(),
            group_id: Some("conflict-group-id".to_string()),
            group_key_hash: "conflict-group-hash".to_string(),
            group_name: "must roll back".to_string(),
            source: "test".to_string(),
            confidence: 1.0,
            inferred_group_category: Some("gpt".to_string()),
            raw_json_redacted: None,
        });
        let error = collectors
            .apply_result(conflict)
            .await
            .expect_err("equal intent must have one operation owner");
        assert!(matches!(error, ApplicationError::Internal));
        assert_eq!(
            collector_side_effect_counts(&runtime, &station.id).await,
            before
        );

        let mut read = runtime.begin_read().await.expect("projection read");
        let operation_id: String = sqlx::query_scalar(
            "SELECT operation_id FROM station_collection_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("current operation");
        assert_eq!(operation_id, "intent-owner");
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn collection_intent_sequence_survives_runtime_restart() {
        let temp = tempfile::tempdir().expect("tempdir");
        let database_path = temp.path().join("collection-intent-restart.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&database_path)
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
        assert_eq!(
            collectors
                .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
                .await
                .expect("first intent"),
            1
        );
        runtime.close().await.expect("close first runtime");

        let reopened = PersistenceRuntime::open_current(&database_path)
            .await
            .expect("reopen runtime");
        let collectors = CollectorService::new(
            reopened.handle(),
            Arc::new(FixedClock),
            Arc::new(SequenceIds::default()),
        );
        assert_eq!(
            collectors
                .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
                .await
                .expect("second intent after restart"),
            2
        );
        reopened.close().await.expect("close reopened runtime");
    }

    async fn collector_side_effect_counts(
        runtime: &PersistenceRuntime,
        station_id: &str,
    ) -> (i64, i64, i64, i64, i64) {
        let mut read = runtime.begin_read().await.expect("side-effect read");
        sqlx::query_as(
            "SELECT
                (SELECT COUNT(*) FROM collector_runs WHERE station_id = ?1),
                (SELECT COUNT(*) FROM collector_snapshots WHERE station_id = ?1),
                (SELECT COUNT(*) FROM collector_task_state WHERE station_id = ?1),
                (SELECT COUNT(*) FROM station_group_bindings WHERE station_id = ?1),
                (SELECT COUNT(*) FROM change_event_occurrences WHERE station_id = ?1)",
        )
        .bind(station_id)
        .fetch_one(read.connection())
        .await
        .expect("side-effect counts")
    }

    #[tokio::test]
    async fn published_status_authorization_recovers_from_a_legacy_stale_projection() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("published-status-authorization.sqlite3"),
        )
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

        let mut authorization = collector_apply_request(
            "published-status-authorization-run",
            &station,
            None,
            "published_status",
            "manual_required",
        );
        authorization.summary_json = json!({
            "loginRequired": true,
            "manualActionRequired": true,
        });
        authorization.error_code = Some("manual_authorization_required".to_string());
        authorization.error_message = Some("当前登录状态已失效，请重新进行窗口授权".to_string());
        authorization.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("authorization intent");
        collectors
            .apply_result(authorization)
            .await
            .expect("published status authorization apply");

        let mut read = runtime
            .begin_read()
            .await
            .expect("published authorization read");
        let event_types = sqlx::query_scalar::<_, String>(
            "SELECT event_type FROM change_incidents
             WHERE station_id = ?1 AND lifecycle_state IN ('pending', 'open', 'recovering')
             ORDER BY event_type",
        )
        .bind(&station.id)
        .fetch_all(read.connection())
        .await
        .expect("published authorization incidents");
        assert_eq!(event_types, vec!["authorization_expired".to_string()]);
        drop(read);

        let mut balance_request = collector_apply_request(
            "balance-after-published-status-authorization",
            &station,
            None,
            "balance",
            "success",
        );
        balance_request.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("balance intent");
        collectors
            .apply_result(balance_request)
            .await
            .expect("balance apply");

        // `stations.status` remains a compatibility transport field during
        // the authority cutover; the typed projection is what drives current
        // Station UI state.
        assert_eq!(
            stations.list().await.expect("listed stations")[0].status,
            "unchecked"
        );
        let typed_status: String = sqlx::query_scalar(
            "SELECT status FROM station_collection_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(
            runtime
                .begin_read()
                .await
                .expect("typed projection read")
                .connection(),
        )
        .await
        .expect("typed collection status");
        assert_eq!(typed_status, "healthy");
        let latest = collectors
            .latest_station_snapshot(&station.id)
            .await
            .expect("latest station snapshot")
            .expect("latest snapshot");
        assert_eq!(latest.status, "success");
        let mut read = runtime
            .begin_read()
            .await
            .expect("published authorization persistence read");
        let authorization_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_incidents
             WHERE station_id = ?1 AND event_type = 'authorization_expired'
               AND lifecycle_state IN ('pending', 'open', 'recovering')",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("published authorization count after unrelated success");
        assert_eq!(authorization_count, 1);
        let authorization_status: String = sqlx::query_scalar(
            "SELECT status FROM station_authorization_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("published authorization projection after unrelated success");
        assert_eq!(authorization_status, "reauthorization_required");
        drop(read);

        // Simulate a successful run written by the pre-fix application. Its
        // collector history advanced, but the authorization projection stayed
        // on the older manual-required operation. The next matching task must
        // recover that durable stale projection instead of relying only on the
        // immediately previous run status.
        let mut write = runtime.begin_write().await.expect("legacy success write");
        sqlx::query(
            "INSERT INTO collector_runs (
                 id, run_key, request_hash, station_id, endpoint_revision,
                 parent_run_id, adapter, task_type, status, started_at,
                 finished_at, duration_ms, endpoint_count, success_count,
                 failure_count, manual_action_required, error_code,
                 error_message, snapshot_id, created_at
             ) VALUES (
                 'legacy-published-success', 'legacy-published-success',
                 'legacy-published-success-hash', ?1, ?2, NULL, 'sub2api',
                 'published_status', 'success', '1700000000001',
                 '1700000000001', 0, 1, 1, 0, 0, NULL, NULL, NULL,
                 '1700000000001'
             )",
        )
        .bind(&station.id)
        .bind(station.endpoint_revision)
        .execute(write.connection())
        .await
        .expect("legacy success history");
        write.commit().await.expect("commit legacy success history");

        let mut recovered_published_status = collector_apply_request(
            "recovered-published-status-run",
            &station,
            None,
            "published_status",
            "success",
        );
        let recovery_intent = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("published status recovery intent");
        recovered_published_status.intent_sequence = recovery_intent;
        let recovery_operation_id =
            collector_operation_key(&station.id, station.endpoint_revision, 1, recovery_intent);
        collectors
            .apply_result(recovered_published_status)
            .await
            .expect("published status recovery apply");

        let mut read = runtime
            .begin_read()
            .await
            .expect("published status recovery read");
        let authorization_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_incidents
             WHERE station_id = ?1 AND event_type = 'authorization_expired'
               AND lifecycle_state IN ('pending', 'open', 'recovering')",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("published authorization count after matching success");
        assert_eq!(authorization_count, 0);
        let authorization_projection: (String, String, Option<String>, String) = sqlx::query_as(
            "SELECT status, authority, reason_code, source_operation_id
             FROM station_authorization_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("published authorization projection after matching success");
        assert_eq!(authorization_projection.0, "valid");
        assert_eq!(authorization_projection.1, "driver_probe");
        assert_eq!(authorization_projection.2, None);
        assert_eq!(authorization_projection.3, recovery_operation_id);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn published_status_only_partial_does_not_degrade_core_collection_status() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("published-status-isolated.sqlite3"),
        )
        .await
        .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Published Status Isolated".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://published-status-isolated.example.test".to_string(),
                api_base_url: "https://published-status-isolated.example.test/v1".to_string(),
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

        // Establish healthy core collection first.  Published-status is an
        // optional axis; its authorization failure must remain actionable
        // without being inferred from a parent summary JSON payload.
        let mut balance = collector_apply_request(
            "published-status-balance-success",
            &station,
            None,
            "balance",
            "success",
        );
        balance.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("balance intent");
        collectors
            .apply_result(balance)
            .await
            .expect("balance apply");
        let mut groups = collector_apply_request(
            "published-status-groups-success",
            &station,
            None,
            "groups",
            "success",
        );
        groups.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("groups intent");
        collectors.apply_result(groups).await.expect("groups apply");
        let mut published_status = collector_apply_request(
            "published-status-partial-run",
            &station,
            None,
            "published_status",
            "manual_required",
        );
        published_status.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("published status intent");
        published_status.summary_json = json!({ "manualActionRequired": true });
        published_status.error_code = Some("manual_authorization_required".to_string());
        collectors
            .apply_result(published_status)
            .await
            .expect("published status apply");
        let mut read = runtime.begin_read().await.expect("authorization incidents");
        let active_events = sqlx::query_scalar::<_, String>(
            "SELECT event_type FROM change_incidents
             WHERE station_id = ?1 AND lifecycle_state IN ('pending', 'open', 'recovering')
             ORDER BY event_type",
        )
        .bind(&station.id)
        .fetch_all(read.connection())
        .await
        .expect("active events");
        assert_eq!(active_events, vec!["authorization_expired".to_string()]);
        drop(read);
        assert_eq!(
            stations
                .station_for_capture(&station.id)
                .await
                .expect("station after full")
                .status,
            "unchecked"
        );

        let mut later_balance = collector_apply_request(
            "later-balance-success-run",
            &station,
            None,
            "balance",
            "success",
        );
        later_balance.intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("later balance intent");
        collectors
            .apply_result(later_balance)
            .await
            .expect("balance apply");
        assert_eq!(
            stations
                .list()
                .await
                .expect("listed stations")
                .into_iter()
                .find(|listed| listed.id == station.id)
                .expect("listed station")
                .status,
            "unchecked"
        );
        runtime.close().await.expect("close runtime");
    }

    fn collector_apply_request(
        run_key: &str,
        station: &Station,
        history_parent_run_id: Option<String>,
        task_type: &str,
        status: &str,
    ) -> CollectorApplyRequest {
        CollectorApplyRequest {
            run_key: run_key.to_string(),
            station_id: station.id.clone(),
            endpoint_revision: station.endpoint_revision,
            credential_revision: 1,
            intent_sequence: 1,
            parent_run_id: history_parent_run_id,
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
            credential_revision: 1,
            intent_sequence: 1,
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

        let intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("groups collection intent");
        collectors
            .apply_result(CollectorApplyRequest {
                run_key: "group-query-run".to_string(),
                station_id: station.id.clone(),
                endpoint_revision: station.endpoint_revision,
                credential_revision: 1,
                intent_sequence,
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
        let first_rate_change = {
            let mut read = runtime.begin_read().await.expect("rate change read");
            sqlx::query_scalar::<_, String>(
                "SELECT new_value_json FROM change_event_occurrences
                 WHERE station_id = ?1 AND event_type = 'group_rate_changed'
                 ORDER BY observed_at_ms DESC, id DESC LIMIT 1",
            )
            .bind(&station.id)
            .fetch_one(read.connection())
            .await
            .expect("first effective rate change")
        };
        let first_rate_change: Value =
            serde_json::from_str(&first_rate_change).expect("rate change json");
        assert_eq!(first_rate_change["oldEffectiveRateMultiplier"], Value::Null);
        assert_eq!(first_rate_change["newEffectiveRateMultiplier"], 0.75);

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
    async fn full_apply_rolls_back_parent_when_a_child_fact_fails() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("full-atomic-child-failure.sqlite3"),
        )
        .await
        .expect("runtime");
        let clock: Arc<dyn Clock> = Arc::new(FixedClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(SequenceIds::default());
        let stations = StationService::new(runtime.handle(), clock.clone(), ids.clone());
        let collectors = CollectorService::new(runtime.handle(), clock, ids);
        let station = stations
            .create(CreateStationInput {
                name: "Full child failure atomic".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://full-child-failure.example.test".to_string(),
                api_base_url: "https://full-child-failure.example.test/v1".to_string(),
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

        let intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("full collection intent");
        let mut parent =
            collector_apply_request("full-failure-parent", &station, None, "full", "success");
        let mut bad_child =
            collector_apply_request("full-failure-balance", &station, None, "balance", "success");
        parent.intent_sequence = intent_sequence;
        bad_child.intent_sequence = intent_sequence;
        // The foreign key is intentionally invalid. The parent and any prior
        // child writes must remain invisible after this terminal error.
        bad_child.facts.balances.push(CanonicalBalanceFact {
            station_id: station.id.clone(),
            station_key_id: Some("missing-station-key".to_string()),
            scope: "account".to_string(),
            balance_kind: "legacy_unknown".to_string(),
            value: Some(1.0),
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
            status: "available".to_string(),
            source: "test".to_string(),
            confidence: 1.0,
            collected_at: Some("1700000000000".to_string()),
            evidence_confidence: "unknown".to_string(),
            spendability_authority: "advisory".to_string(),
        });
        let error = collectors
            .apply_full_result(parent, vec![bad_child])
            .await
            .expect_err("invalid child fact must fail the full transaction");
        assert!(matches!(error, ApplicationError::ConstraintViolation));

        let mut read = runtime.begin_read().await.expect("read rolled-back state");
        let run_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM collector_runs WHERE station_id = ?1")
                .bind(&station.id)
                .fetch_one(read.connection())
                .await
                .expect("count rolled-back runs");
        let snapshot_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM collector_snapshots WHERE station_id = ?1")
                .bind(&station.id)
                .fetch_one(read.connection())
                .await
                .expect("count rolled-back snapshots");
        let projection_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM station_collection_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("count rolled-back projections");
        assert_eq!(run_count, 0);
        assert_eq!(snapshot_count, 0);
        assert_eq!(projection_count, 0);
        drop(read);
        assert_eq!(
            stations
                .station_for_capture(&station.id)
                .await
                .expect("station after rollback")
                .status,
            "unchecked"
        );
        runtime.close().await.expect("close runtime");
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

        let revisions_before = {
            let mut read = runtime.begin_read().await.expect("revision read");
            sqlx::query_as::<_, (String, i64)>(
                "SELECT keys.id, revisions.revision
                 FROM station_keys keys
                 JOIN domain_revisions revisions
                   ON revisions.scope = 'station_key:' || keys.id
                 WHERE keys.station_id = ?1
                 ORDER BY keys.id",
            )
            .bind(&station.id)
            .fetch_all(read.connection())
            .await
            .expect("read key revisions")
        };

        let intent_sequence = collectors
            .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
            .await
            .expect("groups collection intent");
        collectors
            .apply_result(CollectorApplyRequest {
                run_key: "bound-key-rate-refresh".to_string(),
                station_id: station.id.clone(),
                endpoint_revision: station.endpoint_revision,
                credential_revision: 1,
                intent_sequence,
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
            .list_station_keys(station.id.clone())
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

        let revisions_after = {
            let mut read = runtime.begin_read().await.expect("revision read");
            sqlx::query_as::<_, (String, i64)>(
                "SELECT keys.id, revisions.revision
                 FROM station_keys keys
                 JOIN domain_revisions revisions
                   ON revisions.scope = 'station_key:' || keys.id
                 WHERE keys.station_id = ?1
                 ORDER BY keys.id",
            )
            .bind(&station.id)
            .fetch_all(read.connection())
            .await
            .expect("read key revisions")
        };
        assert_eq!(
            revisions_after, revisions_before,
            "derived group/rate projection must not create a new Key lifecycle"
        );
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn collector_terminal_transaction_rolls_back_at_each_critical_write_boundary() {
        enum FaultBoundary {
            Snapshot,
            TaskState,
            Projection,
            OperationTerminal,
        }

        for (case, boundary) in [
            ("snapshot", FaultBoundary::Snapshot),
            ("task-state", FaultBoundary::TaskState),
            ("projection", FaultBoundary::Projection),
            ("operation-terminal", FaultBoundary::OperationTerminal),
        ] {
            let temp = tempfile::tempdir().expect("tempdir");
            let runtime = PersistenceRuntime::initialize_new(
                &temp.path().join(format!("collector-fault-{case}.sqlite3")),
            )
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
            let intent_sequence = collectors
                .allocate_station_collection_intent(&station.id, station.endpoint_revision, 1)
                .await
                .expect("collection intent");

            let trigger_name = format!("fail_collector_{case}").replace('-', "_");
            let trigger_sql = match boundary {
                FaultBoundary::Snapshot => format!(
                    "CREATE TRIGGER {trigger_name} BEFORE INSERT ON collector_snapshots
                     BEGIN SELECT RAISE(ABORT, 'fixture snapshot failure'); END"
                ),
                FaultBoundary::TaskState => format!(
                    "CREATE TRIGGER {trigger_name} BEFORE INSERT ON collector_task_state
                     BEGIN SELECT RAISE(ABORT, 'fixture task state failure'); END"
                ),
                FaultBoundary::Projection => format!(
                    "CREATE TRIGGER {trigger_name} BEFORE INSERT ON station_collection_projection
                     BEGIN SELECT RAISE(ABORT, 'fixture projection failure'); END"
                ),
                FaultBoundary::OperationTerminal => format!(
                    "CREATE TRIGGER {trigger_name}
                     BEFORE UPDATE OF status ON collector_operations
                     WHEN OLD.status = 'running' AND NEW.status = 'succeeded'
                     BEGIN SELECT RAISE(ABORT, 'fixture operation terminal failure'); END"
                ),
            };
            runtime
                .write(move |write| {
                    Box::pin(async move {
                        sqlx::query(&trigger_sql)
                            .execute(write.connection())
                            .await?;
                        Ok(())
                    })
                })
                .await
                .expect("install terminal fault");

            let mut request = collector_apply_request(
                &format!("fault-{case}"),
                &station,
                None,
                "groups",
                "success",
            );
            request.intent_sequence = intent_sequence;
            request.facts.groups.push(CanonicalGroupFact {
                station_id: station.id.clone(),
                group_id: Some(format!("group-{case}")),
                group_key_hash: format!("group-hash-{case}"),
                group_name: format!("Group {case}"),
                source: "fault_fixture".to_string(),
                confidence: 1.0,
                inferred_group_category: Some("gpt".to_string()),
                raw_json_redacted: None,
            });
            assert!(matches!(
                collectors.apply_result(request.clone()).await,
                Err(ApplicationError::Internal)
            ));

            let mut read = runtime.begin_read().await.expect("read rolled-back state");
            let rolled_back_counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT
                    (SELECT COUNT(*) FROM collector_runs WHERE station_id = ?1),
                    (SELECT COUNT(*) FROM collector_snapshots WHERE station_id = ?1),
                    (SELECT COUNT(*) FROM collector_task_state WHERE station_id = ?1),
                    (SELECT COUNT(*) FROM station_group_bindings WHERE station_id = ?1),
                    (SELECT COUNT(*) FROM station_collection_projection WHERE station_id = ?1),
                    (SELECT COUNT(*) FROM change_event_occurrences WHERE station_id = ?1)",
            )
            .bind(&station.id)
            .fetch_one(read.connection())
            .await
            .expect("rolled-back side effects");
            assert_eq!(rolled_back_counts, (0, 0, 0, 0, 0, 0), "fault: {case}");
            let operation_status: String = sqlx::query_scalar(
                "SELECT status FROM collector_operations
                 WHERE station_id = ?1 AND intent_sequence = ?2",
            )
            .bind(&station.id)
            .bind(intent_sequence)
            .fetch_one(read.connection())
            .await
            .expect("retryable operation state");
            assert_eq!(operation_status, "queued", "fault: {case}");
            drop(read);

            runtime
                .write(move |write| {
                    Box::pin(async move {
                        sqlx::query(&format!("DROP TRIGGER {trigger_name}"))
                            .execute(write.connection())
                            .await?;
                        Ok(())
                    })
                })
                .await
                .expect("remove terminal fault");
            collectors
                .apply_result(request)
                .await
                .expect("retry terminal transaction");

            let mut read = runtime.begin_read().await.expect("read retried state");
            let committed_counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
                "SELECT
                    (SELECT COUNT(*) FROM collector_runs WHERE station_id = ?1),
                    (SELECT COUNT(*) FROM collector_snapshots WHERE station_id = ?1),
                    (SELECT COUNT(*) FROM collector_task_state WHERE station_id = ?1),
                    (SELECT COUNT(*) FROM station_group_bindings WHERE station_id = ?1),
                    (SELECT COUNT(*) FROM station_collection_projection WHERE station_id = ?1)",
            )
            .bind(&station.id)
            .fetch_one(read.connection())
            .await
            .expect("committed side effects");
            assert_eq!(committed_counts, (1, 1, 1, 1, 1), "fault: {case}");
            let committed_status: String = sqlx::query_scalar(
                "SELECT status FROM collector_operations
                 WHERE station_id = ?1 AND intent_sequence = ?2",
            )
            .bind(&station.id)
            .bind(intent_sequence)
            .fetch_one(read.connection())
            .await
            .expect("committed operation state");
            assert_eq!(committed_status, "succeeded", "fault: {case}");
            drop(read);
            runtime.close().await.expect("close persistence runtime");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn one_hundred_concurrent_intents_keep_only_the_latest_projection_current() {
        const INTENT_COUNT: usize = 100;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("collection-intent-concurrency.sqlite3"),
        )
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

        let barrier = Arc::new(tokio::sync::Barrier::new(INTENT_COUNT));
        let mut allocations = Vec::with_capacity(INTENT_COUNT);
        for _ in 0..INTENT_COUNT {
            let collector = collectors.clone();
            let station_id = station.id.clone();
            let barrier = barrier.clone();
            allocations.push(tokio::spawn(async move {
                barrier.wait().await;
                collector
                    .allocate_station_collection_intent(&station_id, 1, 1)
                    .await
            }));
        }
        let mut sequences = Vec::with_capacity(INTENT_COUNT);
        for allocation in allocations {
            sequences.push(
                allocation
                    .await
                    .expect("allocator task")
                    .expect("allocated intent"),
            );
        }
        sequences.sort_unstable();
        assert_eq!(sequences, (1_i64..=INTENT_COUNT as i64).collect::<Vec<_>>());

        let latest_sequence = *sequences.last().expect("latest sequence");
        let mut latest =
            collector_apply_request("concurrent-latest", &station, None, "groups", "success");
        latest.intent_sequence = latest_sequence;
        latest.execution_started_at_ms = Some(1);
        collectors
            .apply_result(latest.clone())
            .await
            .expect("latest result");

        let mut stale_sequences = sequences[..INTENT_COUNT - 1].to_vec();
        stale_sequences.reverse();
        for pass in 0..2 {
            if pass == 1 {
                stale_sequences.reverse();
            }
            for sequence in &stale_sequences {
                let mut stale = collector_apply_request(
                    &format!("concurrent-stale-{sequence}"),
                    &station,
                    None,
                    "groups",
                    if sequence % 2 == 0 {
                        "success"
                    } else {
                        "failed"
                    },
                );
                stale.intent_sequence = *sequence;
                stale.execution_started_at_ms = Some(10_000 - sequence);
                assert!(matches!(
                    collectors.apply_result(stale).await,
                    Err(ApplicationError::StaleRevision)
                ));
            }
        }
        let replay = collectors
            .apply_result(latest)
            .await
            .expect("latest idempotent replay");
        assert!(!replay.inserted);

        let mut read = runtime.begin_read().await.expect("read concurrent state");
        let projection: (i64, String, String) = sqlx::query_as(
            "SELECT intent_sequence, status, operation_id
             FROM station_collection_projection WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("latest projection");
        assert_eq!(projection.0, latest_sequence);
        assert_eq!(projection.1, "healthy");
        assert_eq!(projection.2, "concurrent-latest");
        let ledger_counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT
                COUNT(*),
                SUM(CASE WHEN status = 'superseded' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'succeeded' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status IN ('queued', 'running') THEN 1 ELSE 0 END)
             FROM collector_operations WHERE station_id = ?1",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("operation ledger counts");
        assert_eq!(ledger_counts, (100, 99, 1, 0));
        let stale_reasons: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_operations
             WHERE station_id = ?1 AND status = 'superseded'
               AND reason_code = 'stale_revision' AND finished_at_ms IS NOT NULL",
        )
        .bind(&station.id)
        .fetch_one(read.connection())
        .await
        .expect("stale operation reasons");
        assert_eq!(stale_reasons, 99);
        let run_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM collector_runs WHERE station_id = ?1")
                .bind(&station.id)
                .fetch_one(read.connection())
                .await
                .expect("collector run count");
        assert_eq!(run_count, 1);
        drop(read);
        runtime.close().await.expect("close persistence runtime");
    }
}

// The application tests historically construct a CollectorService directly.
// Keep these read-only compatibility shims test-only while production callers
// use CollectorHistoryQuery, so the old service is no longer a history owner.
#[cfg(test)]
impl CollectorService {
    pub(crate) async fn list_collector_runs(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<CollectorRun>, ApplicationError> {
        CollectorHistoryQuery::new(self.runtime.clone())
            .list_collector_runs(station_id, limit)
            .await
    }

    pub(crate) async fn latest_station_snapshot(
        &self,
        station_id: &str,
    ) -> Result<Option<CollectorSnapshot>, ApplicationError> {
        CollectorHistoryQuery::new(self.runtime.clone())
            .latest_station_snapshot(station_id)
            .await
    }

    pub(crate) async fn list_latest_station_snapshots(
        &self,
        station_ids: Vec<String>,
    ) -> Result<Vec<CollectorSnapshot>, ApplicationError> {
        CollectorHistoryQuery::new(self.runtime.clone())
            .list_latest_station_snapshots(station_ids)
            .await
    }
}
