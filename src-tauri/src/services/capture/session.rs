use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use crate::models::capture::{CaptureSessionStatus, CapturedHttpEvent};

static NEXT_CAPTURE_SESSION_GENERATION: AtomicU64 = AtomicU64::new(1);
static NEXT_CAPTURE_COMMIT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebAuthorizationCandidate {
    pub user_id: String,
    generation: u64,
}

#[derive(Debug)]
pub(crate) struct CaptureCommit {
    pub endpoint_revision: i64,
    pub events: Vec<CapturedHttpEvent>,
    generation: u64,
    commit_id: u64,
}

pub(crate) struct CaptureEventReceipt {
    pub status: CaptureSessionStatus,
    pub endpoint_revision: i64,
}

#[derive(Clone, Default)]
pub struct CaptureSessionStore {
    sessions: Arc<Mutex<HashMap<String, CaptureSession>>>,
}

impl CaptureSessionStore {
    pub fn start(
        &self,
        station_id: String,
        window_label: String,
        endpoint_revision: i64,
    ) -> Result<CaptureSessionStatus, String> {
        let mut sessions = self.sessions()?;
        if sessions
            .get(&station_id)
            .is_some_and(|session| session.phase != CaptureSessionPhase::Capturing)
        {
            return Err("capture session is currently being committed".to_string());
        }
        let session = CaptureSession {
            station_id: station_id.clone(),
            window_label,
            generation: NEXT_CAPTURE_SESSION_GENERATION.fetch_add(1, Ordering::Relaxed),
            endpoint_revision,
            phase: CaptureSessionPhase::Capturing,
            active_commit_id: None,
            events: Vec::new(),
            web_authorization_user_id: None,
            last_error: None,
        };
        let status = session.status();
        sessions.insert(station_id, session);
        Ok(status)
    }

    pub(crate) fn push_event(
        &self,
        station_id: &str,
        event: CapturedHttpEvent,
        web_authorization_user_id: Option<String>,
    ) -> Result<CaptureEventReceipt, String> {
        let mut sessions = self.sessions()?;
        let Some(session) = sessions.get_mut(station_id) else {
            return Err("捕获会话不存在，请先点击网页登录 / 捕获。".to_string());
        };
        if session.phase != CaptureSessionPhase::Capturing {
            return Err("capture session is currently being committed".to_string());
        }
        if session.window_label != event.source_window_id {
            return Err("捕获事件来源窗口不匹配，已忽略。".to_string());
        }
        session.events.push(event);
        if let Some(user_id) = web_authorization_user_id.filter(|value| !value.trim().is_empty()) {
            session.web_authorization_user_id = Some(user_id.trim().to_string());
        }
        Ok(CaptureEventReceipt {
            status: session.status(),
            endpoint_revision: session.endpoint_revision,
        })
    }

    pub fn web_authorization_candidate(
        &self,
        station_id: &str,
    ) -> Result<Option<WebAuthorizationCandidate>, String> {
        let sessions = self.sessions()?;
        Ok(sessions.get(station_id).and_then(|session| {
            if session.phase != CaptureSessionPhase::Capturing {
                return None;
            }
            session
                .web_authorization_user_id
                .clone()
                .map(|user_id| WebAuthorizationCandidate {
                    user_id,
                    generation: session.generation,
                })
        }))
    }

    pub(crate) fn begin_finish(&self, station_id: &str) -> Result<CaptureCommit, String> {
        self.begin_commit(station_id, None, CaptureSessionPhase::Finishing)
    }

    pub(crate) fn begin_web_authorization_commit(
        &self,
        station_id: &str,
        candidate: &WebAuthorizationCandidate,
    ) -> Result<CaptureCommit, String> {
        self.begin_commit(
            station_id,
            Some(candidate),
            CaptureSessionPhase::Authorizing,
        )
    }

    pub(crate) fn complete_commit(
        &self,
        station_id: &str,
        commit: &CaptureCommit,
    ) -> Result<(), String> {
        let mut sessions = self.sessions()?;
        let is_current = sessions.get(station_id).is_some_and(|session| {
            session.generation == commit.generation
                && session.active_commit_id == Some(commit.commit_id)
        });
        if !is_current {
            return Err("capture session changed while persistence was committing".to_string());
        }
        sessions.remove(station_id);
        Ok(())
    }

    pub(crate) fn abort_commit(
        &self,
        station_id: &str,
        commit: &CaptureCommit,
    ) -> Result<(), String> {
        let mut sessions = self.sessions()?;
        let session = sessions
            .get_mut(station_id)
            .filter(|session| {
                session.generation == commit.generation
                    && session.active_commit_id == Some(commit.commit_id)
            })
            .ok_or_else(|| "capture commit is stale".to_string())?;
        session.phase = CaptureSessionPhase::Capturing;
        session.active_commit_id = None;
        Ok(())
    }

    pub fn status(&self, station_id: &str) -> Result<CaptureSessionStatus, String> {
        let sessions = self.sessions()?;
        Ok(sessions
            .get(station_id)
            .map(CaptureSession::status)
            .unwrap_or_else(|| CaptureSessionStatus {
                station_id: station_id.to_string(),
                status: "idle".to_string(),
                capture_count: 0,
                recognized_field_count: 0,
                pending_confirmation_count: 0,
                web_authorization_candidate: false,
                last_error: None,
            }))
    }

    pub fn clear(&self, station_id: &str) -> Result<CaptureSessionStatus, String> {
        let mut sessions = self.sessions()?;
        if sessions
            .get(station_id)
            .is_some_and(|session| session.phase != CaptureSessionPhase::Capturing)
        {
            return Err("capture session is currently being committed".to_string());
        }
        sessions.remove(station_id);
        Ok(CaptureSessionStatus {
            station_id: station_id.to_string(),
            status: "idle".to_string(),
            capture_count: 0,
            recognized_field_count: 0,
            pending_confirmation_count: 0,
            web_authorization_candidate: false,
            last_error: None,
        })
    }

    fn begin_commit(
        &self,
        station_id: &str,
        candidate: Option<&WebAuthorizationCandidate>,
        phase: CaptureSessionPhase,
    ) -> Result<CaptureCommit, String> {
        let mut sessions = self.sessions()?;
        let session = sessions
            .get_mut(station_id)
            .ok_or_else(|| "capture session does not exist".to_string())?;
        if session.phase != CaptureSessionPhase::Capturing {
            return Err("capture session is currently being committed".to_string());
        }
        if let Some(candidate) = candidate {
            if session.generation != candidate.generation
                || session.web_authorization_user_id.as_deref() != Some(&candidate.user_id)
            {
                return Err("web authorization capture session is stale".to_string());
            }
        }
        let commit_id = NEXT_CAPTURE_COMMIT_ID.fetch_add(1, Ordering::Relaxed);
        session.phase = phase;
        session.active_commit_id = Some(commit_id);
        Ok(CaptureCommit {
            endpoint_revision: session.endpoint_revision,
            events: session.events.clone(),
            generation: session.generation,
            commit_id,
        })
    }

    fn sessions(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, CaptureSession>>, String> {
        self.sessions
            .lock()
            .map_err(|_| "捕获会话状态锁已损坏。".to_string())
    }
}

struct CaptureSession {
    station_id: String,
    window_label: String,
    generation: u64,
    endpoint_revision: i64,
    phase: CaptureSessionPhase,
    active_commit_id: Option<u64>,
    events: Vec<CapturedHttpEvent>,
    web_authorization_user_id: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureSessionPhase {
    Capturing,
    Finishing,
    Authorizing,
}

impl CaptureSessionPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Capturing => "capturing",
            Self::Finishing => "finishing",
            Self::Authorizing => "authorizing",
        }
    }
}

impl CaptureSession {
    fn status(&self) -> CaptureSessionStatus {
        let (recognized_field_count, pending_confirmation_count) =
            super::event_field_counts(&self.events);
        CaptureSessionStatus {
            station_id: self.station_id.clone(),
            status: self.phase.as_str().to_string(),
            capture_count: self.events.len(),
            recognized_field_count,
            pending_confirmation_count,
            web_authorization_candidate: self.web_authorization_user_id.is_some(),
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captured_event() -> CapturedHttpEvent {
        CapturedHttpEvent {
            id: "event-1".to_string(),
            station_id: "station-1".to_string(),
            source_window_id: "capture-station-1".to_string(),
            page_url: "https://relay.example/dashboard".to_string(),
            request_url: "https://relay.example/api/oauth/oidc".to_string(),
            request_path: "/api/oauth/oidc".to_string(),
            method: "GET".to_string(),
            status: Some(200),
            content_type: "application/json".to_string(),
            started_at: None,
            finished_at: None,
            duration_ms: None,
            response_kind: "json".to_string(),
            response_size: 0,
            response_json_redacted: None,
            response_text_preview_redacted: None,
            classification: "auth".to_string(),
            confidence: 1.0,
            error_message: None,
        }
    }

    fn push_authorization_candidate(
        store: &CaptureSessionStore,
        event: CapturedHttpEvent,
        user_id: &str,
    ) -> CaptureSessionStatus {
        store
            .push_event("station-1", event, Some(user_id.to_string()))
            .expect("push authorization candidate")
            .status
    }

    #[test]
    fn authorization_candidate_is_retained_in_native_session_state() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string(), 4)
            .expect("start capture");

        let status = push_authorization_candidate(&store, captured_event(), "42");

        assert!(status.web_authorization_candidate);
    }

    #[test]
    fn event_from_another_window_is_rejected_before_session_mutation() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string(), 4)
            .expect("start capture");
        let mut event = captured_event();
        event.source_window_id = "capture-station-2".to_string();

        assert!(store
            .push_event("station-1", event, Some("42".to_string()))
            .is_err());
        let status = store.status("station-1").expect("status");
        assert_eq!(status.capture_count, 0);
        assert!(!status.web_authorization_candidate);
    }

    #[test]
    fn stale_authorization_candidate_cannot_commit_after_session_restart() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string(), 4)
            .expect("start first capture");
        push_authorization_candidate(&store, captured_event(), "42");
        let candidate = store
            .web_authorization_candidate("station-1")
            .expect("read candidate")
            .expect("candidate exists");

        store
            .start("station-1".to_string(), "capture-station-1".to_string(), 4)
            .expect("replace capture");
        let error = store
            .begin_web_authorization_commit("station-1", &candidate)
            .expect_err("stale candidate should fail");

        assert!(error.contains("stale"));
    }

    #[test]
    fn current_authorization_candidate_commits_and_returns_its_events() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string(), 4)
            .expect("start capture");
        push_authorization_candidate(&store, captured_event(), "42");
        let candidate = store
            .web_authorization_candidate("station-1")
            .expect("read candidate")
            .expect("candidate exists");

        let commit = store
            .begin_web_authorization_commit("station-1", &candidate)
            .expect("begin current candidate commit");

        assert_eq!(commit.events.len(), 1);
        assert_eq!(
            store.status("station-1").expect("status").status,
            "authorizing"
        );
        store
            .complete_commit("station-1", &commit)
            .expect("complete current candidate commit");
        assert_eq!(store.status("station-1").expect("status").status, "idle");
    }

    #[test]
    fn capture_session_retains_its_start_revision() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string(), 4)
            .expect("start capture");

        let receipt = store
            .push_event("station-1", captured_event(), None)
            .expect("accept event");
        assert_eq!(receipt.endpoint_revision, 4);
    }

    #[test]
    fn capture_commit_blocks_mutation_and_abort_restores_the_session() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string(), 4)
            .expect("start capture");
        let commit = store.begin_finish("station-1").expect("begin finish");

        assert_eq!(commit.endpoint_revision, 4);
        assert!(store
            .push_event("station-1", captured_event(), None)
            .is_err());
        assert!(store.clear("station-1").is_err());

        store
            .abort_commit("station-1", &commit)
            .expect("abort finish");
        store
            .push_event("station-1", captured_event(), None)
            .expect("capture resumes after abort");
    }

    #[test]
    fn stale_commit_token_cannot_complete_a_new_attempt() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string(), 4)
            .expect("start capture");
        let first = store.begin_finish("station-1").expect("first commit");
        store
            .abort_commit("station-1", &first)
            .expect("abort first commit");
        let second = store.begin_finish("station-1").expect("second commit");

        assert!(store.complete_commit("station-1", &first).is_err());
        assert_eq!(
            store.status("station-1").expect("status").status,
            "finishing"
        );
        store
            .complete_commit("station-1", &second)
            .expect("complete second commit");
    }
}
