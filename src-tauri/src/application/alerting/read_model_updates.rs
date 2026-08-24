/// Publishes a best-effort freshness signal only after an alerting write has
/// committed. The database remains the source of truth; consumers must treat
/// this as an invalidation hint rather than a durable event stream.
pub(crate) trait AlertingReadModelUpdatePublisher: Send + Sync {
    fn notify_after_commit(&self);
}

#[derive(Default)]
pub(crate) struct NoopAlertingReadModelUpdatePublisher;

impl AlertingReadModelUpdatePublisher for NoopAlertingReadModelUpdatePublisher {
    fn notify_after_commit(&self) {}
}
