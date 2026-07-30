use crate::{
    background_tasks::{BlockingExecutor, BlockingExecutorError},
    observability::correlation,
    services::{collectors, remote_keys},
};

pub(super) async fn prepare_collection_plan<S>(
    blocking: &BlockingExecutor,
    source: S,
    data_key: [u8; 32],
    station_id: String,
    task: collectors::output::CollectorTask,
) -> Result<
    Result<collectors::PreparedStationCollectionRoute, crate::application::error::ApplicationError>,
    BlockingExecutorError,
>
where
    S: collectors::CollectorSourcePort + 'static,
{
    blocking
        .submit(
            "draft_collection_plan",
            None,
            current_correlation_id(),
            None,
            move |_| {
                Ok(collectors::prepare_station_collection_route_v2(
                    &source, &data_key, station_id, task,
                ))
            },
        )?
        .result()
        .await
}

pub(super) async fn prepare_newapi_key_scan_plan<S>(
    blocking: &BlockingExecutor,
    source: S,
    data_key: [u8; 32],
    station_id: String,
) -> Result<
    Result<
        Option<remote_keys::PreparedNewApiRemoteKeyDriverContext>,
        remote_keys::RemoteKeyOperationError,
    >,
    BlockingExecutorError,
>
where
    S: collectors::CollectorSourcePort + 'static,
{
    blocking
        .submit(
            "draft_newapi_key_scan_plan",
            None,
            current_correlation_id(),
            None,
            move |_| {
                Ok(remote_keys::prepare_newapi_remote_key_driver_context_v2(
                    &source, &data_key, station_id,
                ))
            },
        )?
        .result()
        .await
}

pub(super) async fn prepare_sub2api_key_scan_plan<S>(
    blocking: &BlockingExecutor,
    source: S,
    data_key: [u8; 32],
    station_id: String,
) -> Result<
    Result<
        Option<remote_keys::PreparedSub2ApiRemoteKeyDriverContext>,
        remote_keys::RemoteKeyOperationError,
    >,
    BlockingExecutorError,
>
where
    S: collectors::CollectorSourcePort + 'static,
{
    blocking
        .submit(
            "draft_sub2api_key_scan_plan",
            None,
            current_correlation_id(),
            None,
            move |_| {
                Ok(remote_keys::prepare_sub2api_remote_key_driver_context_v2(
                    &source, &data_key, station_id,
                ))
            },
        )?
        .result()
        .await
}

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}
