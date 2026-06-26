pub mod sub2api;

use crate::{models::collector::CollectorRunResult, services::database::AppDatabase};

pub fn detect_station_info(
    database: &AppDatabase,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    sub2api::detect_station(database, station_id)
}

pub fn collect_station_info(
    database: &AppDatabase,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    sub2api::collect_station(database, station_id)
}
