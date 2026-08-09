use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    models::alerting::AlertPolicy,
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::alerting::{AlertingSettingsStore, PolicyStore, ALERTING_SETTINGS_KEY},
    },
};

/// Versioned global delivery controls.  This is deliberately separate from
/// the incident engine: disabling a channel cannot stop observations or
/// recovery transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertingSettings {
    #[serde(default = "default_revision")]
    pub revision: u64,
    #[serde(
        rename = "enabled",
        alias = "alertingEnabled",
        default = "default_true"
    )]
    pub alerting_enabled: bool,
    #[serde(default = "default_true")]
    pub in_app_enabled: bool,
    #[serde(default)]
    pub desktop_enabled: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub global_pause_until_ms: Option<i64>,
    #[serde(default)]
    pub quiet_hours_enabled: bool,
    #[serde(rename = "quietHoursStart", alias = "quiet_hours_start_local", default)]
    pub quiet_hours_start_local: Option<String>,
    #[serde(rename = "quietHoursEnd", alias = "quiet_hours_end_local", default)]
    pub quiet_hours_end_local: Option<String>,
    #[serde(
        rename = "quietHoursTimezone",
        alias = "quiet_hours_time_zone",
        default = "default_timezone"
    )]
    pub quiet_hours_time_zone: String,
    #[serde(default = "default_true")]
    pub critical_bypasses_quiet_hours: bool,
    #[serde(default = "default_history_retention")]
    pub history_retention_days: u32,
    #[serde(default = "default_delivery_retention")]
    pub delivery_retention_days: u32,
    #[serde(default)]
    pub updated_at_ms: i64,
}

fn default_revision() -> u64 {
    1
}

fn default_true() -> bool {
    true
}

fn default_timezone() -> String {
    "local".to_string()
}

fn default_history_retention() -> u32 {
    90
}

fn default_delivery_retention() -> u32 {
    30
}

impl Default for AlertingSettings {
    fn default() -> Self {
        Self {
            revision: 1,
            alerting_enabled: true,
            in_app_enabled: true,
            desktop_enabled: false,
            paused: false,
            global_pause_until_ms: None,
            quiet_hours_enabled: false,
            quiet_hours_start_local: Some("22:00".to_string()),
            quiet_hours_end_local: Some("08:00".to_string()),
            quiet_hours_time_zone: "local".to_string(),
            critical_bypasses_quiet_hours: true,
            history_retention_days: 90,
            delivery_retention_days: 30,
            updated_at_ms: 0,
        }
    }
}

impl AlertingSettings {
    pub(crate) fn validate(&self) -> Result<(), PersistenceError> {
        if self.revision == 0
            || !(1..=3_650).contains(&self.history_retention_days)
            || !(1..=3_650).contains(&self.delivery_retention_days)
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        if self.global_pause_until_ms.is_some_and(|value| value < 0) {
            return Err(PersistenceError::ConstraintViolation);
        }
        if self.quiet_hours_enabled {
            let start = self
                .quiet_hours_start_local
                .as_deref()
                .ok_or(PersistenceError::ConstraintViolation)?;
            let end = self
                .quiet_hours_end_local
                .as_deref()
                .ok_or(PersistenceError::ConstraintViolation)?;
            validate_local_time(start)?;
            validate_local_time(end)?;
            if self.quiet_hours_time_zone != "local" {
                self.quiet_hours_time_zone
                    .parse::<chrono_tz::Tz>()
                    .map_err(|_| PersistenceError::ConstraintViolation)?;
            }
        }
        Ok(())
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.settings-pause; owner=application/alerting; remove_when=scheduler no longer evaluates global pause"
    )]
    pub(crate) fn is_paused(&self) -> bool {
        self.paused || self.global_pause_until_ms.is_some()
    }

    pub(crate) fn is_paused_at(&self, now_ms: i64) -> bool {
        self.paused
            || self
                .global_pause_until_ms
                .is_some_and(|until| now_ms < until)
    }

    pub(crate) fn is_quiet_at(&self, now_ms: i64) -> bool {
        if !self.quiet_hours_enabled {
            return false;
        }
        let (Some(start), Some(end)) = (
            self.quiet_hours_start_local.as_deref(),
            self.quiet_hours_end_local.as_deref(),
        ) else {
            return false;
        };
        let Ok(start) = parse_minutes(start) else {
            return false;
        };
        let Ok(end) = parse_minutes(end) else {
            return false;
        };
        let Some(utc) = DateTime::<Utc>::from_timestamp_millis(now_ms) else {
            return false;
        };
        let minute = if self.quiet_hours_time_zone == "local" {
            let local = utc.with_timezone(&chrono::Local);
            local.hour() as u16 * 60 + local.minute() as u16
        } else {
            let zone = self
                .quiet_hours_time_zone
                .parse::<chrono_tz::Tz>()
                .unwrap_or(chrono_tz::UTC);
            let local = utc.with_timezone(&zone);
            local.hour() as u16 * 60 + local.minute() as u16
        };
        if start == end {
            return true;
        }
        if start < end {
            (start..end).contains(&minute)
        } else {
            minute >= start || minute < end
        }
    }
}

fn validate_local_time(value: &str) -> Result<(), PersistenceError> {
    parse_minutes(value)
        .map(|_| ())
        .map_err(|_| PersistenceError::ConstraintViolation)
}

fn parse_minutes(value: &str) -> Result<u16, ()> {
    let (hours, minutes) = value.split_once(':').ok_or(())?;
    if hours.len() != 2 || minutes.len() != 2 {
        return Err(());
    }
    let hours = hours.parse::<u16>().map_err(|_| ())?;
    let minutes = minutes.parse::<u16>().map_err(|_| ())?;
    if hours >= 24 || minutes >= 60 {
        return Err(());
    }
    Ok(hours * 60 + minutes)
}

#[derive(Clone)]
pub(crate) struct PolicyService {
    runtime: PersistenceHandle,
    store: PolicyStore,
}

impl PolicyService {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            store: PolicyStore,
        }
    }

    pub(crate) async fn list_policies(&self) -> Result<Vec<AlertPolicy>, PersistenceError> {
        let mut read = self.runtime.begin_read().await?;
        self.store.list(&mut read).await
    }

    pub(crate) async fn get_policy(
        &self,
        id: &str,
    ) -> Result<Option<AlertPolicy>, PersistenceError> {
        let mut read = self.runtime.begin_read().await?;
        self.store.get(&mut read, id).await
    }

    pub(crate) async fn save_policy(
        &self,
        policy: AlertPolicy,
        expected_revision: Option<u64>,
    ) -> Result<(), PersistenceError> {
        self.runtime
            .write(|write| {
                Box::pin(async move { PolicyStore.save(write, &policy, expected_revision).await })
            })
            .await
    }

    pub(crate) async fn set_policy_state(
        &self,
        id: &str,
        state: crate::models::alerting::PolicyState,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let id = id.to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    PolicyStore
                        .mark_state(write, &id, state, expected_revision, now_ms)
                        .await
                })
            })
            .await
    }

    pub(crate) async fn load_settings(&self) -> Result<AlertingSettings, PersistenceError> {
        let mut read = self.runtime.begin_read().await?;
        let value = AlertingSettingsStore.load_json(&mut read).await?;
        let settings = value
            .map(|value| serde_json::from_str::<AlertingSettings>(&value))
            .transpose()
            .map_err(|_| PersistenceError::InvariantViolation("invalid alerting settings".into()))?
            .unwrap_or_default();
        settings.validate()?;
        Ok(settings)
    }

    pub(crate) async fn update_settings(
        &self,
        settings: AlertingSettings,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<AlertingSettings, PersistenceError> {
        if now_ms < 0 || settings.revision != expected_revision.saturating_add(1) {
            return Err(PersistenceError::RevisionConflict(
                ALERTING_SETTINGS_KEY.to_string(),
            ));
        }
        let mut settings = settings;
        settings.updated_at_ms = now_ms;
        settings.validate()?;
        let encoded = serde_json::to_string(&settings).map_err(|_| {
            PersistenceError::InvariantViolation("cannot encode alerting settings".into())
        })?;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let store = AlertingSettingsStore;
                    let previous = store.load_json_for_write(write).await?;
                    let affected = match previous {
                        Some(expected_json) => {
                            store
                                .update_json_if_matches(write, &expected_json, &encoded, now_ms)
                                .await?
                        }
                        None if expected_revision <= 1 => {
                            store.insert_json_if_absent(write, &encoded, now_ms).await?
                        }
                        None => {
                            return Err(PersistenceError::RevisionConflict(
                                ALERTING_SETTINGS_KEY.to_string(),
                            ))
                        }
                    };
                    if !affected {
                        return Err(PersistenceError::RevisionConflict(
                            ALERTING_SETTINGS_KEY.to_string(),
                        ));
                    }
                    Ok(settings)
                })
            })
            .await
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.settings-cas-helper; owner=application/alerting; remove_when=settings mutation adapters use explicit CAS only"
    )]
    pub(crate) async fn update_settings_from_current(
        &self,
        mutator: impl FnOnce(&mut AlertingSettings),
        now_ms: i64,
    ) -> Result<AlertingSettings, PersistenceError> {
        let current = self.load_settings().await?;
        let expected = current.revision;
        let mut next = current;
        mutator(&mut next);
        next.revision = expected.saturating_add(1);
        self.update_settings(next, expected, now_ms).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::alerting::Severity;
    use crate::persistence::runtime::PersistenceRuntime;

    #[test]
    fn settings_validate_cross_midnight_quiet_hours() {
        let settings = AlertingSettings {
            quiet_hours_enabled: true,
            quiet_hours_start_local: Some("22:00".into()),
            quiet_hours_end_local: Some("07:00".into()),
            quiet_hours_time_zone: "UTC".into(),
            ..Default::default()
        };
        settings.validate().expect("valid quiet hours");
        assert!(settings.is_quiet_at(0));
    }

    #[test]
    fn malformed_local_time_is_rejected() {
        let settings = AlertingSettings {
            quiet_hours_enabled: true,
            quiet_hours_start_local: Some("8:00".into()),
            quiet_hours_end_local: Some("09:00".into()),
            ..Default::default()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn settings_accept_frontend_camel_case_contract() {
        let settings: AlertingSettings = serde_json::from_value(serde_json::json!({
            "enabled": false,
            "paused": true,
            "quietHoursEnabled": true,
            "quietHoursStart": "22:00",
            "quietHoursEnd": "08:00",
            "quietHoursTimezone": "local",
            "revision": 4
        }))
        .expect("frontend settings contract");
        assert!(!settings.alerting_enabled);
        assert!(settings.paused);
        assert!(settings.validate().is_ok());
        let encoded = serde_json::to_value(settings).expect("encode settings");
        assert_eq!(encoded["enabled"], false);
        assert_eq!(encoded["quietHoursStart"], "22:00");
    }

    #[tokio::test]
    async fn settings_and_policies_round_trip_with_cas() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("alerting.sqlite3"))
            .await
            .expect("runtime");
        let service = PolicyService::new(runtime.handle());

        let initial = service.load_settings().await.expect("default settings");
        assert_eq!(initial.revision, 1);
        let saved = service
            .update_settings_from_current(|settings| settings.desktop_enabled = true, 1_000)
            .await
            .expect("save settings");
        assert_eq!(saved.revision, 2);
        assert!(
            service
                .load_settings()
                .await
                .expect("reload settings")
                .desktop_enabled
        );

        let mut policy = AlertPolicy::system_default(Severity::Warning);
        policy.id = "custom-policy".into();
        policy.name = "Custom policy".into();
        service
            .save_policy(policy.clone(), None)
            .await
            .expect("insert policy");
        let loaded = service
            .get_policy("custom-policy")
            .await
            .expect("load policy")
            .expect("policy exists");
        assert_eq!(loaded.revision, 1);
        policy.revision = 2;
        policy.updated_at_ms = 2_000;
        service
            .save_policy(policy, Some(1))
            .await
            .expect("cas update policy");
        assert!(service
            .save_policy(
                AlertPolicy {
                    revision: 3,
                    ..loaded
                },
                Some(1),
            )
            .await
            .is_err());
        runtime.close().await.expect("close runtime");
    }
}
