//! Durable routing diagnostics read owner.
//!
//! This reader owns only persistence-backed diagnostics queries. It does not
//! read proxy runtime state, perform outbound I/O, or rebuild routing
//! candidates. The routing service can continue to expose the same methods
//! during the caller migration; this type is the narrow replacement owner.

use crate::{
    application::{
        error::ApplicationError,
        error_rate_protection::{ErrorRateHistoryPageV1, ErrorRateProtectionService},
        queries::request_decision_trace::{
            append_durable_attempt_trace, append_durable_decision_events, decision_cursor,
            decision_trace_from_decision, decision_trace_from_durable_outcome,
            recent_route_decisions_from_page, RecentRouteDecisionsInput, RecentRouteDecisionsPage,
            RequestDecisionTrace,
        },
    },
    models::{
        pricing::BalanceSnapshot,
        routing::{ModelAlias, StationKeyHealth},
        stations::StationEndpointHealth,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            request_outcome_store::RequestOutcomeStore,
            routing_decisions::queries::RoutingDecisionQueries,
            routing_error_rate_history_store::RoutingErrorRateHistoryStore,
            routing_policy_store::RoutingPolicyStore, routing_store::RoutingStore,
        },
    },
};

/// Read-only application owner for durable routing diagnostics.
///
/// `RoutingStore` remains the persistence adapter for station, endpoint, and
/// balance facts. `ErrorRateProtectionService` is retained only to project the
/// current policy switch into the history query configuration; it is not a
/// second protection state machine.
#[derive(Clone)]
pub(crate) struct RoutingDiagnosticsReader {
    runtime: PersistenceHandle,
    store: RoutingStore,
    error_rate: ErrorRateProtectionService,
}

impl RoutingDiagnosticsReader {
    pub(crate) fn new(runtime: PersistenceHandle, error_rate: ErrorRateProtectionService) -> Self {
        Self {
            runtime,
            store: RoutingStore,
            error_rate,
        }
    }

    pub(crate) async fn list_recent_route_decisions(
        &self,
        input: RecentRouteDecisionsInput,
    ) -> Result<RecentRouteDecisionsPage, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let page = RoutingDecisionQueries
            .list_decisions(
                read.connection(),
                decision_cursor(input.cursor.as_deref()).as_ref(),
                input.limit.unwrap_or(50).clamp(1, 200) as u32,
            )
            .await
            .map_err(ApplicationError::from)?;
        Ok(recent_route_decisions_from_page(page))
    }

    pub(crate) async fn get_request_decision_trace(
        &self,
        decision_id: String,
    ) -> Result<RequestDecisionTrace, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        if let Some(summary) = RequestOutcomeStore
            .routing_outcome_summary(read.connection(), &decision_id)
            .await
            .map_err(ApplicationError::from)?
        {
            let attempts = RequestOutcomeStore
                .routing_attempt_trace(read.connection(), &decision_id, 4)
                .await
                .map_err(ApplicationError::from)?;
            let trace = append_durable_attempt_trace(
                decision_trace_from_durable_outcome(summary),
                attempts,
            );
            let events = RequestOutcomeStore
                .routing_decision_events(read.connection(), &decision_id, 64)
                .await
                .map_err(ApplicationError::from)?;
            return Ok(append_durable_decision_events(trace, events));
        }

        let queries = RoutingDecisionQueries;
        let summary = queries
            .get_decision(read.connection(), &decision_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or(ApplicationError::NotFound)?;
        let candidates = queries
            .list_candidate_details(read.connection(), &summary.id, 500)
            .await
            .map_err(ApplicationError::from)?;
        Ok(decision_trace_from_decision(summary, candidates))
    }

    pub(crate) async fn list_error_rate_history(
        &self,
        before_ms: Option<i64>,
        limit: usize,
    ) -> Result<ErrorRateHistoryPageV1, ApplicationError> {
        let now_ms = chrono::Utc::now().timestamp_millis().max(0);
        let mut read = self.runtime.begin_read().await?;
        let policy_enabled = self.load_protection_enabled(&mut read).await?;
        let config = self.error_rate.config_for_policy(policy_enabled);
        RoutingErrorRateHistoryStore
            .list_page(read.connection(), before_ms, limit, &config, now_ms)
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn list_station_key_health(
        &self,
    ) -> Result<Vec<StationKeyHealth>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_station_key_health(&mut read)
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_model_aliases(&mut read)
            .await
            .map_err(ApplicationError::from)
    }

    #[cfg(test)]
    pub(crate) async fn list_model_alias_pairs(
        &self,
    ) -> Result<Vec<(String, String)>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_model_alias_pairs(&mut read)
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn station_key_health_by_id(
        &self,
        station_key_id: &str,
    ) -> Result<StationKeyHealth, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .station_key_health_by_id(&mut read, station_key_id)
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn list_station_endpoint_health(
        &self,
    ) -> Result<Vec<StationEndpointHealth>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_station_endpoint_health(&mut read)
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn list_balance_snapshots_for_station(
        &self,
        station_id: &str,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_balance_snapshots_for_station(&mut read, station_id)
            .await
            .map_err(ApplicationError::from)
    }

    async fn load_protection_enabled(
        &self,
        read: &mut crate::persistence::ReadSession,
    ) -> Result<bool, ApplicationError> {
        let stored = RoutingPolicyStore
            .load(read.connection())
            .await
            .map_err(ApplicationError::from)?
            .ok_or(ApplicationError::NotFound)?;
        let policy =
            crate::models::routing_policy::RoutingPolicyConfigV2::from_stored_value(&stored.config)
                .map_err(|_| ApplicationError::ConstraintViolation)?;
        Ok(policy.protection_profile.enabled)
    }
}
