use crate::{
    application::{error::ApplicationError, routing::RoutingService},
    models::pricing::BalanceSnapshot,
};

pub(crate) async fn load_station_detail_balances(
    routing: &RoutingService,
    station_id: &str,
) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
    routing.list_balance_snapshots_for_station(station_id).await
}
