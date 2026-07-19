use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RuntimeState {
    Starting,
    Ready,
    Draining,
    Closed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeTransitionError {
    Reverse,
}

#[derive(Debug)]
pub(crate) struct RuntimeLifecycle {
    state: Mutex<RuntimeState>,
}

impl RuntimeLifecycle {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(RuntimeState::Starting),
        }
    }

    pub(crate) fn transition(&self, next: RuntimeState) -> Result<(), RuntimeTransitionError> {
        let mut state = self.state.lock().expect("runtime lifecycle poisoned");
        if rank(next) < rank(*state) {
            return Err(RuntimeTransitionError::Reverse);
        }
        *state = next;
        Ok(())
    }

    pub(crate) fn accepts_new_work(&self) -> bool {
        matches!(
            *self.state.lock().expect("runtime lifecycle poisoned"),
            RuntimeState::Ready
        )
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

#[cfg(test)]
mod tests {
    use super::{RuntimeLifecycle, RuntimeState, RuntimeTransitionError};

    #[test]
    fn runtime_lifecycle_is_monotonic() {
        let state = RuntimeLifecycle::new();
        assert_eq!(state.transition(RuntimeState::Ready), Ok(()));
        assert_eq!(
            state.transition(RuntimeState::Starting),
            Err(RuntimeTransitionError::Reverse)
        );
        assert_eq!(state.transition(RuntimeState::Draining), Ok(()));
        assert!(!state.accepts_new_work());
        assert_eq!(state.transition(RuntimeState::Closed), Ok(()));
    }
}
