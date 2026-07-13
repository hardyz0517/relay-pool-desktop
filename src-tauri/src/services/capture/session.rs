use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use crate::models::capture::{CaptureSessionStatus, CapturedHttpEvent};

static NEXT_CAPTURE_SESSION_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebAuthorizationCandidate {
    pub user_id: String,
    generation: u64,
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
    ) -> Result<CaptureSessionStatus, String> {
        let mut sessions = self.sessions()?;
        let session = CaptureSession {
            station_id: station_id.clone(),
            window_label,
            generation: NEXT_CAPTURE_SESSION_GENERATION.fetch_add(1, Ordering::Relaxed),
            status: "capturing".to_string(),
            events: Vec::new(),
            web_authorization_user_id: None,
            last_error: None,
        };
        let status = session.status();
        sessions.insert(station_id, session);
        Ok(status)
    }

    pub fn push_event(
        &self,
        station_id: &str,
        event: CapturedHttpEvent,
        web_authorization_user_id: Option<String>,
    ) -> Result<CaptureSessionStatus, String> {
        let mut sessions = self.sessions()?;
        let Some(session) = sessions.get_mut(station_id) else {
            return Err("捕获会话不存在，请先点击网页登录 / 捕获。".to_string());
        };
        if session.window_label != event.source_window_id {
            return Err("捕获事件来源窗口不匹配，已忽略。".to_string());
        }
        session.events.push(event);
        if let Some(user_id) = web_authorization_user_id.filter(|value| !value.trim().is_empty()) {
            session.web_authorization_user_id = Some(user_id.trim().to_string());
        }
        Ok(session.status())
    }

    pub fn web_authorization_candidate(
        &self,
        station_id: &str,
    ) -> Result<Option<WebAuthorizationCandidate>, String> {
        let sessions = self.sessions()?;
        Ok(sessions.get(station_id).and_then(|session| {
            session
                .web_authorization_user_id
                .clone()
                .map(|user_id| WebAuthorizationCandidate {
                    user_id,
                    generation: session.generation,
                })
        }))
    }

    pub fn commit_web_authorization<T>(
        &self,
        station_id: &str,
        candidate: &WebAuthorizationCandidate,
        persist: impl FnOnce() -> Result<T, String>,
    ) -> Result<(T, Vec<CapturedHttpEvent>), String> {
        let mut sessions = self.sessions()?;
        let is_current = sessions.get(station_id).is_some_and(|session| {
            session.generation == candidate.generation
                && session.web_authorization_user_id.as_deref() == Some(&candidate.user_id)
        });
        if !is_current {
            return Err("网页登录授权会话已失效，请重新完成登录。".to_string());
        }

        let result = persist()?;
        let session = sessions
            .remove(station_id)
            .ok_or_else(|| "网页登录授权会话已失效，请重新完成登录。".to_string())?;
        Ok((result, session.events))
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

    pub fn take_events(&self, station_id: &str) -> Result<Vec<CapturedHttpEvent>, String> {
        let mut sessions = self.sessions()?;
        let Some(session) = sessions.remove(station_id) else {
            return Err("捕获会话不存在，请先打开网页登录窗口。".to_string());
        };
        Ok(session.events)
    }

    pub fn clear(&self, station_id: &str) -> Result<CaptureSessionStatus, String> {
        let mut sessions = self.sessions()?;
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

    fn sessions(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, CaptureSession>>, String> {
        self.sessions
            .lock()
            .map_err(|_| "捕获会话状态锁已损坏".to_string())
    }
}

struct CaptureSession {
    station_id: String,
    window_label: String,
    generation: u64,
    status: String,
    events: Vec<CapturedHttpEvent>,
    web_authorization_user_id: Option<String>,
    last_error: Option<String>,
}

impl CaptureSession {
    fn status(&self) -> CaptureSessionStatus {
        let (recognized_field_count, pending_confirmation_count) =
            super::event_field_counts(&self.events);
        CaptureSessionStatus {
            station_id: self.station_id.clone(),
            status: self.status.clone(),
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
    }

    #[test]
    fn authorization_candidate_is_retained_in_native_session_state() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string())
            .expect("start capture");

        let status = push_authorization_candidate(&store, captured_event(), "42");

        assert!(status.web_authorization_candidate);
    }

    #[test]
    fn stale_authorization_candidate_cannot_commit_after_session_restart() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string())
            .expect("start first capture");
        push_authorization_candidate(&store, captured_event(), "42");
        let candidate = store
            .web_authorization_candidate("station-1")
            .expect("read candidate")
            .expect("candidate exists");

        store
            .start("station-1".to_string(), "capture-station-1".to_string())
            .expect("replace capture");
        let mut persisted = false;
        let error = store
            .commit_web_authorization("station-1", &candidate, || {
                persisted = true;
                Ok(())
            })
            .expect_err("stale candidate should fail");

        assert!(!persisted);
        assert!(error.contains("已失效"));
    }

    #[test]
    fn current_authorization_candidate_commits_and_returns_its_events() {
        let store = CaptureSessionStore::default();
        store
            .start("station-1".to_string(), "capture-station-1".to_string())
            .expect("start capture");
        push_authorization_candidate(&store, captured_event(), "42");
        let candidate = store
            .web_authorization_candidate("station-1")
            .expect("read candidate")
            .expect("candidate exists");

        let (stored, events) = store
            .commit_web_authorization("station-1", &candidate, || Ok("stored"))
            .expect("commit current candidate");

        assert_eq!(stored, "stored");
        assert_eq!(events.len(), 1);
        assert_eq!(store.status("station-1").expect("status").status, "idle");
    }
}
