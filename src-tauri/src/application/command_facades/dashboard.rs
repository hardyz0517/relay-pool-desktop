use std::sync::Arc;

use crate::{
    application::{error::ApplicationError, queries::dashboard_metrics::DashboardMetricsQuery},
    models::dashboard_metrics::{
        DashboardCumulativeRequestMetricsSnapshot, DashboardLiveRequestMetricsSnapshot,
        DashboardRequestMetricsInput,
    },
};

#[derive(Clone)]
pub(crate) struct DashboardMetricsCommandFacade {
    query: Arc<DashboardMetricsQuery>,
}

impl DashboardMetricsCommandFacade {
    pub(crate) fn new(query: Arc<DashboardMetricsQuery>) -> Self {
        Self { query }
    }

    pub(crate) async fn load_live(
        &self,
        input: DashboardRequestMetricsInput,
    ) -> Result<DashboardLiveRequestMetricsSnapshot, ApplicationError> {
        self.query.load_live(input).await
    }

    pub(crate) async fn load_cumulative(
        &self,
    ) -> Result<DashboardCumulativeRequestMetricsSnapshot, ApplicationError> {
        self.query.load_cumulative().await
    }
}
