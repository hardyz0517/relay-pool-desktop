use futures_util::future::BoxFuture;

use crate::{
    application::error::ApplicationError,
    models::stations::StationEndpointHealth,
    persistence::stores::routing_store::StationEndpointProbeTarget,
};

/// Reads the immutable station endpoint identity needed before an endpoint
/// probe starts. The returned revision is the fence that must be carried into
/// the eventual health write.
pub(crate) trait RoutingEndpointTargetReadPort: Send + Sync + 'static {
    fn station_endpoint_probe_target(
        &self,
        station_id: String,
    ) -> BoxFuture<'static, Result<StationEndpointProbeTarget, ApplicationError>>;
}

/// Persists the result of an endpoint-only probe. Implementations must use
/// `expected_endpoint_revision` as a compare-and-fence value and return
/// `ApplicationError::StaleRevision` without replacing the newer endpoint's
/// health when the station changed while the probe was in flight.
pub(crate) trait RoutingEndpointHealthWritePort: Send + Sync + 'static {
    fn record_station_endpoint_health(
        &self,
        station_id: String,
        expected_endpoint_revision: i64,
        status: String,
        latency_ms: Option<i64>,
        checked_at: String,
        error_summary: Option<String>,
    ) -> BoxFuture<'static, Result<StationEndpointHealth, ApplicationError>>;
}

/// Records a station-key connectivity diagnostic separately from endpoint
/// snapshot health. The endpoint revision remains an explicit fence so a
/// diagnostic from an old station endpoint cannot be attributed to its new
/// endpoint/key state.
pub(crate) trait RoutingStationKeyDiagnosticWritePort: Send + Sync + 'static {
    fn record_station_key_connectivity(
        &self,
        station_key_id: String,
        station_id: String,
        expected_endpoint_revision: i64,
        ok: bool,
        duration_ms: i64,
        error_summary: String,
    ) -> BoxFuture<'static, Result<(), ApplicationError>>;
}
