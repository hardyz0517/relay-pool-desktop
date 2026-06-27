use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::models::capture::{CapturedHttpEvent, CaptureSessionStatus};

#[derive(Clone, Default)]
pub struct CaptureSessionStore {
    sessions: Arc<Mutex<HashMap<String, CaptureSession>>>,
}

impl CaptureSessionStore {
    pub fn start(&self, station_id: String, window_label: String) -> Result<CaptureSessionStatus, String> {
        let mut sessions = self.sessions()?;
        let session = CaptureSession {
            station_id: station_id.clone(),
            window_label,
            status: "capturing".to_string(),
            events: Vec::new(),
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
    ) -> Result<CaptureSessionStatus, String> {
        let mut sessions = self.sessions()?;
        let Some(session) = sessions.get_mut(station_id) else {
            return Err("捕获会话不存在，请先点击网页登录 / 捕获。".to_string());
        };
        if session.window_label != event.source_window_id {
            return Err("捕获事件来源窗口不匹配，已忽略。".to_string());
        }
        session.events.push(event);
        Ok(session.status())
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
            last_error: None,
        })
    }

    fn sessions(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, CaptureSession>>, String> {
        self.sessions
            .lock()
            .map_err(|_| "捕获会话状态锁已损坏".to_string())
    }
}

struct CaptureSession {
    station_id: String,
    window_label: String,
    status: String,
    events: Vec<CapturedHttpEvent>,
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
            last_error: self.last_error.clone(),
        }
    }
}
