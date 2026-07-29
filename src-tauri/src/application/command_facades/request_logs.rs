use std::sync::Arc;

use crate::{
    application::{
        error::ApplicationError, pagination::PageLimit, request_logs::RequestLogService,
    },
    models::proxy::RequestLog,
};

#[derive(Clone)]
pub(crate) struct RequestLogsCommandFacade {
    request_logs: Arc<RequestLogService>,
}

impl RequestLogsCommandFacade {
    pub(crate) fn new(request_logs: Arc<RequestLogService>) -> Self {
        Self { request_logs }
    }

    pub(crate) async fn list_request_logs(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<RequestLog>, ApplicationError> {
        self.request_logs.list_recent(limit).await
    }

    pub(crate) async fn clear_request_logs(&self) -> Result<(), ApplicationError> {
        self.request_logs.clear().await
    }
}
