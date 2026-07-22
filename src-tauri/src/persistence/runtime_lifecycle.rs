use std::sync::{Arc, Mutex};

use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RuntimeState {
    Starting,
    Ready,
    Draining,
    Closed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RuntimeTransitionError {
    #[error("runtime lifecycle cannot move backwards")]
    Reverse,
    #[error("runtime lifecycle transition is invalid")]
    Invalid,
    #[error("persistence runtime close failed")]
    CloseFailed,
}

#[derive(Debug)]
pub(crate) struct RuntimeLifecycle {
    inner: Mutex<RuntimeLifecycleInner>,
    active_work: watch::Sender<usize>,
}

#[derive(Debug)]
struct RuntimeLifecycleInner {
    state: RuntimeState,
    active_work: usize,
}

impl RuntimeLifecycle {
    pub(crate) fn new() -> Self {
        let (active_work, _) = watch::channel(0);
        Self {
            inner: Mutex::new(RuntimeLifecycleInner {
                state: RuntimeState::Starting,
                active_work: 0,
            }),
            active_work,
        }
    }

    pub(crate) fn transition(&self, next: RuntimeState) -> Result<(), RuntimeTransitionError> {
        let mut inner = self.inner.lock().expect("runtime lifecycle poisoned");
        if next == inner.state {
            return Ok(());
        }
        if rank(next) < rank(inner.state) {
            return Err(RuntimeTransitionError::Reverse);
        }
        if !is_valid_transition(inner.state, next) {
            return Err(RuntimeTransitionError::Invalid);
        }
        inner.state = next;
        Ok(())
    }

    pub(crate) fn state(&self) -> RuntimeState {
        self.inner.lock().expect("runtime lifecycle poisoned").state
    }

    #[cfg(test)]
    pub(crate) fn accepts_new_work(&self) -> bool {
        self.state() == RuntimeState::Ready
    }

    pub(crate) fn admit(self: &Arc<Self>) -> Option<RuntimeWorkPermit> {
        let mut inner = self.inner.lock().expect("runtime lifecycle poisoned");
        if inner.state != RuntimeState::Ready {
            return None;
        }
        inner.active_work += 1;
        self.active_work.send_replace(inner.active_work);
        Some(RuntimeWorkPermit {
            lifecycle: Arc::clone(self),
        })
    }

    pub(crate) async fn wait_for_idle(&self) {
        let mut active_work = self.active_work.subscribe();
        while *active_work.borrow_and_update() != 0 {
            if active_work.changed().await.is_err() {
                break;
            }
        }
    }

    fn release_work(&self) {
        let mut inner = self.inner.lock().expect("runtime lifecycle poisoned");
        debug_assert!(inner.active_work > 0, "runtime work permit underflow");
        inner.active_work = inner.active_work.saturating_sub(1);
        self.active_work.send_replace(inner.active_work);
    }
}

#[derive(Debug)]
pub(crate) struct RuntimeWorkPermit {
    lifecycle: Arc<RuntimeLifecycle>,
}

impl Drop for RuntimeWorkPermit {
    fn drop(&mut self) {
        self.lifecycle.release_work();
    }
}

fn rank(state: RuntimeState) -> u8 {
    match state {
        RuntimeState::Starting => 0,
        RuntimeState::Ready => 1,
        RuntimeState::Draining => 2,
        RuntimeState::Closed => 3,
        RuntimeState::Unavailable => 4,
    }
}

fn is_valid_transition(current: RuntimeState, next: RuntimeState) -> bool {
    matches!(
        (current, next),
        (RuntimeState::Starting, RuntimeState::Ready)
            | (RuntimeState::Starting, RuntimeState::Unavailable)
            | (RuntimeState::Ready, RuntimeState::Draining)
            | (RuntimeState::Ready, RuntimeState::Unavailable)
            | (RuntimeState::Draining, RuntimeState::Closed)
            | (RuntimeState::Draining, RuntimeState::Unavailable)
    )
}
