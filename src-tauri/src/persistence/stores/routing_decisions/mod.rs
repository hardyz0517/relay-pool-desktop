pub(crate) mod queries;
pub(crate) mod retention;
pub(crate) mod write;

pub(crate) const MAX_ROUTE_CANDIDATE_DECISION_DETAILS: usize = 32;
pub(crate) const ROUTING_DECISION_RETENTION_MAX_COUNT: u32 = 10_000;
pub(crate) const ROUTING_DECISION_RETENTION_MAX_AGE_DAYS: i64 = 30;
