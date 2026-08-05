use tokio_util::sync::CancellationToken;

use crate::{
    application::quality_projection::{rebuild_quality_summary, BetaPrior, QualitySummary},
    models::routing_observation::RoutingObservation,
};

pub(crate) const MAX_ROUTING_PROJECTION_BATCH: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingProjectionBatch {
    pub(crate) summaries: Vec<QualitySummary>,
    pub(crate) processed: usize,
    pub(crate) cancelled: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingProjectionRunner;

impl RoutingProjectionRunner {
    pub(crate) fn project_batch(
        &self,
        scopes: &[String],
        observations: &[RoutingObservation],
        prior: BetaPrior,
        cancellation: &CancellationToken,
    ) -> RoutingProjectionBatch {
        let mut summaries = Vec::with_capacity(scopes.len());
        let mut processed = 0;
        for scope in scopes.iter().take(MAX_ROUTING_PROJECTION_BATCH) {
            if cancellation.is_cancelled() {
                return RoutingProjectionBatch {
                    summaries,
                    processed,
                    cancelled: true,
                };
            }
            summaries.push(rebuild_quality_summary(scope, observations, prior));
            processed += 1;
        }
        RoutingProjectionBatch {
            summaries,
            processed,
            cancelled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runner_honors_cancellation_and_batch_bound() {
        let token = CancellationToken::new();
        token.cancel();
        let scopes = (0..(MAX_ROUTING_PROJECTION_BATCH + 1))
            .map(|index| format!("station_key:key-{index}"))
            .collect::<Vec<_>>();
        let result =
            RoutingProjectionRunner.project_batch(&scopes, &[], BetaPrior::default(), &token);
        assert!(result.cancelled);
        assert_eq!(result.processed, 0);
    }
}
