//! Narrow application ports for endpoint probing and station-key diagnostics.
//!
//! These contracts deliberately live above the persistence and outbound
//! adapters.  Callers depend on the capability they need instead of taking a
//! dependency on the transitional [`RoutingService`](super::routing::RoutingService)
//! owner.  The concrete adapter is implemented by `RoutingService` for now;
//! moving that adapter to a dedicated persistence owner is a later, behaviour
//! preserving step.

use futures_util::future::BoxFuture;

use crate::{
    application::{
        error::ApplicationError, queries::routing_runtime::RoutingMonitoringTargetSnapshot,
    },
    models::stations::StationEndpointHealth,
};

/// Endpoint target captured before an outbound probe starts.
///
/// `api_base_url` is normalized at the application boundary.  It is not a
/// credential-bearing URL and should not be logged as an arbitrary database
/// value.  `endpoint_revision` must accompany every subsequent write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingEndpointProbeTarget {
    pub(crate) station_id: String,
    pub(crate) api_base_url: String,
    pub(crate) endpoint_revision: i64,
}

/// Endpoint snapshot state accepted by the endpoint write port.
///
/// The persistence schema stores these values as strings for compatibility,
/// but application callers use this enum so invalid states cannot cross the
/// port boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingEndpointHealthStatus {
    Unchecked,
    Success,
    Failed,
}

impl RoutingEndpointHealthStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, ApplicationError> {
        match value {
            "unchecked" => Ok(Self::Unchecked),
            "success" => Ok(Self::Success),
            "failed" => Ok(Self::Failed),
            _ => Err(ApplicationError::ConstraintViolation),
        }
    }
}

/// Revision-fenced endpoint snapshot write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingEndpointHealthWrite {
    pub(crate) station_id: String,
    pub(crate) expected_endpoint_revision: i64,
    pub(crate) status: RoutingEndpointHealthStatus,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) checked_at: String,
    pub(crate) error_summary: Option<String>,
}

impl RoutingEndpointHealthWrite {
    /// Validate values before invoking a persistence adapter.  The adapter
    /// repeats the revision check atomically; this validation only prevents
    /// malformed commands from reaching it.
    pub(crate) fn validate(&self) -> Result<(), ApplicationError> {
        if self.station_id.trim().is_empty()
            || self.expected_endpoint_revision < 1
            || self.checked_at.trim().is_empty()
            || self.latency_ms.is_some_and(|latency| latency < 0)
        {
            return Err(ApplicationError::ConstraintViolation);
        }
        Ok(())
    }
}

/// Read the station endpoint target and its revision for a probe.
pub(crate) trait RoutingEndpointTargetReadPort: Send + Sync {
    fn read_endpoint_probe_target(
        &self,
        station_id: &str,
    ) -> BoxFuture<'_, Result<RoutingEndpointProbeTarget, ApplicationError>>;
}

/// Read the immutable target set captured at the start of a monitoring run.
/// This remains separate from the single-station target port used by manual
/// endpoint pings.
pub(crate) trait RoutingMonitoringTargetReadPort: Send + Sync {
    fn read_monitoring_target_snapshots(
        &self,
    ) -> BoxFuture<'_, Result<Vec<RoutingMonitoringTargetSnapshot>, ApplicationError>>;
}

/// Persist an endpoint snapshot with an atomic expected-revision fence.
pub(crate) trait RoutingEndpointHealthWritePort: Send + Sync {
    fn write_endpoint_health(
        &self,
        write: RoutingEndpointHealthWrite,
    ) -> BoxFuture<'_, Result<StationEndpointHealth, ApplicationError>>;
}

/// Station-key connectivity diagnostics are a separate fact from endpoint
/// health and proxy traffic.  The current command facade keeps these results
/// in its bounded operation store; this port documents the seam that a
/// durable diagnostic owner can implement without changing endpoint writes.
///
/// It is intentionally not implemented by `RoutingService`: doing so would
/// conflate key/model diagnostics with endpoint snapshots and violate the
/// routing fact-ownership boundary.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=routing.station-key-diagnostic-port; owner=application/routing_endpoint_ports; remove_when=station-key diagnostic persistence owner is introduced"
    )
)]
pub(crate) trait RoutingStationKeyDiagnosticWritePort: Send + Sync {
    fn write_station_key_diagnostic(
        &self,
        observation: StationKeyDiagnosticObservation,
    ) -> BoxFuture<'_, Result<StationKeyDiagnosticReceipt, ApplicationError>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=routing.station-key-diagnostic-observation; owner=application/routing_endpoint_ports; remove_when=station-key diagnostic persistence owner is introduced"
    )
)]
pub(crate) struct StationKeyDiagnosticObservation {
    pub(crate) station_key_id: String,
    pub(crate) model: String,
    pub(crate) ok: bool,
    pub(crate) status_code: u16,
    pub(crate) duration_ms: i64,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=routing.station-key-diagnostic-receipt; owner=application/routing_endpoint_ports; remove_when=station-key diagnostic persistence owner is introduced"
    )
)]
pub(crate) struct StationKeyDiagnosticReceipt {
    pub(crate) station_key_id: String,
    pub(crate) recorded_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_status_mapping_is_closed_and_stable() {
        assert_eq!(
            RoutingEndpointHealthStatus::parse("success").unwrap(),
            RoutingEndpointHealthStatus::Success
        );
        assert_eq!(RoutingEndpointHealthStatus::Failed.as_str(), "failed");
        assert!(matches!(
            RoutingEndpointHealthStatus::parse("healthy"),
            Err(ApplicationError::ConstraintViolation)
        ));
    }

    #[test]
    fn endpoint_write_rejects_invalid_revision_and_latency() {
        let mut write = RoutingEndpointHealthWrite {
            station_id: "station-a".to_string(),
            expected_endpoint_revision: 1,
            status: RoutingEndpointHealthStatus::Success,
            latency_ms: Some(1),
            checked_at: "1".to_string(),
            error_summary: None,
        };
        assert!(write.validate().is_ok());

        write.expected_endpoint_revision = 0;
        assert!(matches!(
            write.validate(),
            Err(ApplicationError::ConstraintViolation)
        ));

        write.expected_endpoint_revision = 1;
        write.latency_ms = Some(-1);
        assert!(matches!(
            write.validate(),
            Err(ApplicationError::ConstraintViolation)
        ));

        write.latency_ms = None;
        write.checked_at.clear();
        assert!(matches!(
            write.validate(),
            Err(ApplicationError::ConstraintViolation)
        ));
    }

    #[tokio::test]
    async fn stale_revision_error_is_not_collapsed_by_the_port_contract() {
        #[derive(Clone)]
        struct Stale;

        impl RoutingEndpointHealthWritePort for Stale {
            fn write_endpoint_health(
                &self,
                _write: RoutingEndpointHealthWrite,
            ) -> BoxFuture<'_, Result<StationEndpointHealth, ApplicationError>> {
                Box::pin(async { Err(ApplicationError::StaleRevision) })
            }
        }

        let result = Stale
            .write_endpoint_health(RoutingEndpointHealthWrite {
                station_id: "station-a".to_string(),
                expected_endpoint_revision: 1,
                status: RoutingEndpointHealthStatus::Failed,
                latency_ms: None,
                checked_at: "1".to_string(),
                error_summary: Some("stale probe".to_string()),
            })
            .await;
        assert!(matches!(result, Err(ApplicationError::StaleRevision)));
    }

    #[tokio::test]
    async fn stale_endpoint_write_does_not_replace_newer_snapshot() {
        use crate::application::routing::RoutingService;
        use crate::persistence::runtime::PersistenceRuntime;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("routing.sqlite3"))
            .await
            .expect("runtime");
        let service = RoutingService::new(runtime.handle());

        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO stations (id, name, station_type, website_url, api_base_url, endpoint_revision, created_at, updated_at) VALUES ('port-test', 'Port Test', 'sub2api', 'https://port.test', 'https://port.test/v1', 1, '1', '1')",
                    )
                    .execute(write.connection())
                    .await
                    .map(|_| ())
                    .map_err(Into::into)
                })
            })
            .await
            .expect("station");

        let first = service
            .write_endpoint_health(RoutingEndpointHealthWrite {
                station_id: "port-test".to_string(),
                expected_endpoint_revision: 1,
                status: RoutingEndpointHealthStatus::Success,
                latency_ms: Some(7),
                checked_at: "1".to_string(),
                error_summary: None,
            })
            .await
            .expect("initial snapshot");
        assert_eq!(first.status, "success");

        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query("UPDATE stations SET endpoint_revision = 2 WHERE id = 'port-test'")
                        .execute(write.connection())
                        .await
                        .map(|_| ())
                        .map_err(Into::into)
                })
            })
            .await
            .expect("bump endpoint revision");

        let stale = service
            .write_endpoint_health(RoutingEndpointHealthWrite {
                station_id: "port-test".to_string(),
                expected_endpoint_revision: 1,
                status: RoutingEndpointHealthStatus::Failed,
                latency_ms: None,
                checked_at: "2".to_string(),
                error_summary: Some("stale".to_string()),
            })
            .await;
        assert!(matches!(stale, Err(ApplicationError::StaleRevision)));

        let rows = service
            .station_endpoint_probe_target("port-test")
            .await
            .expect("target");
        assert_eq!(rows.endpoint_revision, 2);

        let mut read = runtime.handle().begin_read().await.expect("read");
        let status: String = sqlx::query_scalar(
            "SELECT status FROM endpoint_health_snapshot WHERE station_id = 'port-test'",
        )
        .fetch_one(read.connection())
        .await
        .expect("snapshot remains");
        assert_eq!(status, "success");
        drop(read);
        runtime.close().await.expect("close persistence runtime");
    }
}
