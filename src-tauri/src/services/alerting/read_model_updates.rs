use tauri::{AppHandle, Emitter, Runtime};

use crate::application::alerting::AlertingReadModelUpdatePublisher;

pub(crate) const ALERTING_READ_MODEL_UPDATED_EVENT: &str = "alerting-read-model-updated";

/// Tauri bridge for the non-durable frontend cache invalidation signal. The
/// payload is deliberately empty: affected views refetch their authoritative
/// server-side counts and no alert details cross this boundary.
#[derive(Clone)]
pub(crate) struct TauriAlertingReadModelUpdatePublisher<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriAlertingReadModelUpdatePublisher<R> {
    pub(crate) fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> AlertingReadModelUpdatePublisher for TauriAlertingReadModelUpdatePublisher<R> {
    fn notify_after_commit(&self) {
        let _ = self.app.emit(ALERTING_READ_MODEL_UPDATED_EVENT, ());
    }
}

#[cfg(test)]
mod tests {
    use super::ALERTING_READ_MODEL_UPDATED_EVENT;

    #[test]
    fn frontend_event_name_is_stable() {
        assert_eq!(
            ALERTING_READ_MODEL_UPDATED_EVENT,
            "alerting-read-model-updated"
        );
    }
}
