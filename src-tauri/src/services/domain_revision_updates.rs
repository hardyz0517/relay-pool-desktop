use tauri::{AppHandle, Emitter, Runtime};

use crate::application::queries::read_model_revision::subscribe_domain_revision_notices;

/// Best-effort native bridge for committed domain revision hints. The
/// database transaction remains authoritative; a dropped or lagged event only
/// causes the frontend to reconcile on its normal freshness path.
pub(crate) const DOMAIN_REVISION_UPDATED_EVENT: &str = "domain-revision-updated";

pub(crate) fn spawn_domain_revision_event_bridge<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut receiver = subscribe_domain_revision_notices();
        loop {
            match receiver.recv().await {
                Ok(notice) => {
                    let _ = app.emit(DOMAIN_REVISION_UPDATED_EVENT, notice);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // The next foreground reconciliation reads durable
                    // revisions and therefore does not need synthetic events.
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::DOMAIN_REVISION_UPDATED_EVENT;

    #[test]
    fn event_name_is_stable() {
        assert_eq!(DOMAIN_REVISION_UPDATED_EVENT, "domain-revision-updated");
    }
}
