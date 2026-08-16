use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use tauri::{ExitRequestApi, Runtime};

#[derive(Clone)]
pub struct ExitCoordinator {
    inner: Arc<Mutex<ExitState>>,
    drain_timeout: Duration,
}

impl ExitCoordinator {
    pub fn new(drain_timeout: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ExitState::default())),
            drain_timeout,
        }
    }

    pub fn request_exit<R: Runtime>(
        &self,
        app: tauri::AppHandle<R>,
        reason: ExitReason,
        code: i32,
    ) {
        self.record_request(reason);
        app.exit(code);
    }

    pub fn handle_exit_requested<R, F, Fut>(
        &self,
        app: tauri::AppHandle<R>,
        code: Option<i32>,
        api: &ExitRequestApi,
        drain: F,
    ) where
        R: Runtime,
        F: FnOnce(tauri::AppHandle<R>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        match self.begin_exit_requested() {
            ExitRequestDecision::AllowFinalExit => {}
            ExitRequestDecision::PreventAlreadyDraining => {
                api.prevent_exit();
            }
            ExitRequestDecision::PreventAndDrain => {
                api.prevent_exit();
                let coordinator = self.clone();
                let timeout = self.drain_timeout;
                tauri::async_runtime::spawn(async move {
                    if tokio::time::timeout(timeout, drain(app.clone()))
                        .await
                        .is_err()
                    {
                        crate::observability::runtime::bootstrap::emit(
                            crate::app_runtime_events::exit_drain_timeout(),
                        );
                    }
                    coordinator.mark_final_exit_requested();
                    app.exit(code.unwrap_or(0));
                });
            }
        }
    }

    fn record_request(&self, reason: ExitReason) {
        let mut state = self.inner.lock().expect("exit coordinator mutex poisoned");
        if matches!(state.phase, ExitPhase::Idle) {
            state.reason = Some(reason);
            state.phase = ExitPhase::Requested;
        }
    }

    fn begin_exit_requested(&self) -> ExitRequestDecision {
        let mut state = self.inner.lock().expect("exit coordinator mutex poisoned");
        match state.phase {
            ExitPhase::FinalExitRequested => ExitRequestDecision::AllowFinalExit,
            ExitPhase::Draining => ExitRequestDecision::PreventAlreadyDraining,
            ExitPhase::Idle | ExitPhase::Requested => {
                if matches!(state.phase, ExitPhase::Idle) {
                    state.reason = Some(ExitReason::RuntimeExitRequested);
                }
                state.phase = ExitPhase::Draining;
                ExitRequestDecision::PreventAndDrain
            }
        }
    }

    fn mark_final_exit_requested(&self) {
        let mut state = self.inner.lock().expect("exit coordinator mutex poisoned");
        state.phase = ExitPhase::FinalExitRequested;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    TrayQuit,
    MainWindowClose,
    RuntimeExitRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitRequestDecision {
    AllowFinalExit,
    PreventAlreadyDraining,
    PreventAndDrain,
}

#[derive(Default)]
struct ExitState {
    phase: ExitPhase,
    reason: Option<ExitReason>,
}

#[derive(Default)]
enum ExitPhase {
    #[default]
    Idle,
    Requested,
    Draining,
    FinalExitRequested,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_requested_prevents_until_final_exit_is_requested() {
        let coordinator = ExitCoordinator::new(Duration::from_secs(1));

        assert_eq!(
            coordinator.begin_exit_requested(),
            ExitRequestDecision::PreventAndDrain
        );
        assert_eq!(
            coordinator.begin_exit_requested(),
            ExitRequestDecision::PreventAlreadyDraining
        );

        coordinator.mark_final_exit_requested();

        assert_eq!(
            coordinator.begin_exit_requested(),
            ExitRequestDecision::AllowFinalExit
        );
    }

    #[test]
    fn explicit_exit_request_records_first_reason_only() {
        let coordinator = ExitCoordinator::new(Duration::from_secs(1));

        coordinator.record_request(ExitReason::TrayQuit);
        coordinator.record_request(ExitReason::MainWindowClose);

        let state = coordinator
            .inner
            .lock()
            .expect("exit coordinator mutex poisoned");
        assert_eq!(state.reason, Some(ExitReason::TrayQuit));
    }
}
