use futures_util::future::BoxFuture;

use super::{
    attempt::AttemptTerminalRecord,
    request::{FinalRequestRecord, RequestLogAnnotations, RequestStartRecord},
};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "only the fault-matrix integration contract injects unknown commit outcomes"
    )
)]
pub(crate) enum LifecycleWriteError {
    DatabaseBusy,
    Unavailable(String),
    CommitOutcomeUnknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestStartAck {
    pub inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptCommitAck {
    pub inserted: bool,
    pub health_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptCostCommitRecord {
    pub(crate) request_id: String,
    pub(crate) ordinal: u16,
    pub(crate) pricing_context_id: String,
    pub(crate) pricing_basis: String,
    pub(crate) pricing_status_label: String,
    pub(crate) usage_status: String,
    pub(crate) input_tokens: Option<i64>,
    pub(crate) output_tokens: Option<i64>,
    pub(crate) total_tokens: Option<i64>,
    pub(crate) cache_creation_tokens: Option<i64>,
    pub(crate) cache_read_tokens: Option<i64>,
    pub(crate) cost_status: String,
    pub(crate) currency: Option<String>,
    pub(crate) total_cost_micro: Option<i64>,
    pub(crate) created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptCostCommitAck {
    pub inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestCommitAck {
    pub finalized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestCostAggregateCommitRecord {
    pub(crate) request_id: String,
    pub(crate) status: String,
    pub(crate) totals_by_currency_json: String,
    pub(crate) compatibility_currency: Option<String>,
    pub(crate) compatibility_total_cost_micro: Option<i64>,
    pub(crate) incomplete_attempts_json: String,
    pub(crate) written_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestCostAggregateCommitAck {
    pub inserted: bool,
}

pub(crate) trait RequestLifecycleStore: Send + Sync + 'static {
    fn start_request(
        &self,
        record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>>;

    fn start_request_with_annotations(
        &self,
        record: RequestStartRecord,
        annotations: RequestLogAnnotations,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        let _ = annotations;
        self.start_request(record)
    }

    fn finish_attempt(
        &self,
        record: AttemptTerminalRecord,
    ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>>;

    fn finish_request(
        &self,
        record: FinalRequestRecord,
    ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>>;

    fn finish_attempt_cost(
        &self,
        _record: AttemptCostCommitRecord,
    ) -> BoxFuture<'static, Result<AttemptCostCommitAck, LifecycleWriteError>> {
        Box::pin(async {
            Err(LifecycleWriteError::Unavailable(
                "attempt cost persistence is not wired for this store".to_string(),
            ))
        })
    }

    fn finish_request_cost_aggregate(
        &self,
        _record: RequestCostAggregateCommitRecord,
    ) -> BoxFuture<'static, Result<RequestCostAggregateCommitAck, LifecycleWriteError>> {
        Box::pin(async {
            Err(LifecycleWriteError::Unavailable(
                "request cost aggregate persistence is not wired for this store".to_string(),
            ))
        })
    }
}
