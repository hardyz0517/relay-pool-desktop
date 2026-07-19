use std::sync::Arc;

use crate::{
    application::{
        clock::{Clock, SystemClock},
        ids::{IdGenerator, UuidV7Generator},
        stations::StationService,
    },
    persistence::runtime::PersistenceHandle,
};

pub(crate) fn station_service_for_v2_tests(runtime: PersistenceHandle) -> StationService {
    StationService::new(
        runtime,
        Arc::new(SystemClock) as Arc<dyn Clock>,
        Arc::new(UuidV7Generator) as Arc<dyn IdGenerator>,
    )
}
