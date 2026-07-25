use std::sync::Arc;

use crate::{
    application::{
        error::ApplicationError, pagination::PageLimit, queries::channel_status::ChannelStatusQuery,
    },
    models::shared_capabilities::{ChannelStatusSummary, ChannelStatusWorkspace},
};

#[derive(Clone)]
pub(crate) struct ChannelStatusCommandFacade {
    channel_status: Arc<ChannelStatusQuery>,
}

impl ChannelStatusCommandFacade {
    pub(crate) fn new(channel_status: Arc<ChannelStatusQuery>) -> Self {
        Self { channel_status }
    }

    pub(crate) async fn list_channel_status_summaries(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelStatusSummary>, ApplicationError> {
        self.channel_status.load(limit).await
    }

    pub(crate) async fn load_channel_status_workspace(
        &self,
        limit: PageLimit,
    ) -> Result<ChannelStatusWorkspace, ApplicationError> {
        self.channel_status.load_workspace(limit).await
    }
}
