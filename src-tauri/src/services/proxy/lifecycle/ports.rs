use futures_util::future::BoxFuture;

use super::{
    attempt::AttemptTerminalRecord,
    request::{FinalRequestRecord, RequestStartRecord},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LifecycleWriteError {
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
pub(crate) struct RequestCommitAck {
    pub finalized: bool,
}

pub(crate) trait RequestLifecycleStore: Send + Sync + 'static {
    fn start_request(
        &self,
        record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>>;

    fn finish_attempt(
        &self,
        record: AttemptTerminalRecord,
    ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>>;

    fn finish_request(
        &self,
        record: FinalRequestRecord,
    ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>>;
}
