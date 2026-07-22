use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_millis_for_services() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
