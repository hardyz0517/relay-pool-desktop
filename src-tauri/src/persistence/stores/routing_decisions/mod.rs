pub(crate) mod queries;
#[cfg(test)]
pub(crate) mod retention;
#[cfg(test)]
pub(crate) mod write;

#[cfg(test)]
pub(crate) const MAX_ROUTE_CANDIDATE_DECISION_DETAILS: usize = 32;
#[cfg(test)]
pub(crate) const ROUTING_DECISION_RETENTION_MAX_COUNT: u32 = 10_000;
#[cfg(test)]
pub(crate) const ROUTING_DECISION_RETENTION_MAX_AGE_DAYS: i64 = 30;
