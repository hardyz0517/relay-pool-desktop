pub(crate) mod attention;
pub(crate) mod delivery;
pub(crate) mod incident;
pub(crate) mod occurrence;
pub(crate) mod policy;
pub(crate) mod upgrade_progress;
pub(crate) mod workspace;

use sqlx::Row;

use crate::{
    models::alerting::{
        AlertEventType, AlertPolicy, ConditionKey, DeliveryKind, DeliveryStatus, EventCategory,
        Incident, IncidentObservation, LifecycleState, NotificationChannel, NotificationDelivery,
        ObservationKind, PolicyState, QuietHoursPolicy, RecoveryMode, RepeatMode, ScopeKind,
        Severity, StateTransition, SuppressionReason, TriggerMode,
    },
    persistence::{error::PersistenceError, ReadSession, WriteSession},
};

pub(crate) const ALERTING_SETTINGS_KEY: &str = "alerting_settings_v1";

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct AlertingSettingsStore;

impl AlertingSettingsStore {
    pub(crate) async fn load_json(
        &self,
        session: &mut ReadSession,
    ) -> Result<Option<String>, PersistenceError> {
        let value = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?1")
            .bind(ALERTING_SETTINGS_KEY)
            .fetch_optional(session.connection())
            .await?;
        Ok(value)
    }

    /// Load the versioned settings payload inside the caller's write
    /// transaction. The persistence boundary returns raw JSON so the
    /// application layer remains the owner of the settings domain type and
    /// validation rules.
    pub(crate) async fn load_json_for_write(
        &self,
        session: &mut WriteSession,
    ) -> Result<Option<String>, PersistenceError> {
        let value = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?1")
            .bind(ALERTING_SETTINGS_KEY)
            .fetch_optional(session.connection())
            .await?;
        Ok(value)
    }

    pub(crate) async fn insert_json_if_absent(
        &self,
        session: &mut WriteSession,
        encoded: &str,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        let affected =
            sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)")
                .bind(ALERTING_SETTINGS_KEY)
                .bind(encoded)
                .bind(now_ms.to_string())
                .execute(session.connection())
                .await?
                .rows_affected();
        Ok(affected == 1)
    }

    pub(crate) async fn update_json_if_matches(
        &self,
        session: &mut WriteSession,
        expected_json: &str,
        encoded: &str,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        let affected = sqlx::query(
            "UPDATE settings SET value = ?2, updated_at = ?3
             WHERE key = ?1 AND value = ?4",
        )
        .bind(ALERTING_SETTINGS_KEY)
        .bind(encoded)
        .bind(now_ms.to_string())
        .bind(expected_json)
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(affected == 1)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OccurrenceInsert {
    pub id: String,
    pub source_observation_key: String,
    pub event_type: AlertEventType,
    pub category: EventCategory,
    pub observation_kind: ObservationKind,
    pub severity: Severity,
    pub condition_key: Option<ConditionKey>,
    pub object_type: String,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub source: String,
    pub reason_code: Option<String>,
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub impact_json: Option<String>,
    pub observed_at_ms: i64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InsertResult {
    pub inserted: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OccurrenceStore;

impl OccurrenceStore {
    pub(crate) async fn insert_ignore(
        &self,
        session: &mut WriteSession,
        occurrence: &OccurrenceInsert,
    ) -> Result<InsertResult, PersistenceError> {
        let affected = sqlx::query(
            "INSERT INTO change_event_occurrences (
                id, source_observation_key, event_type, category, observation_kind, severity,
                condition_key, object_type, object_id, station_id, station_key_id, source,
                reason_code, old_value_json, new_value_json, impact_json, observed_at_ms, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                       ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(source_observation_key) DO NOTHING",
        )
        .bind(&occurrence.id)
        .bind(&occurrence.source_observation_key)
        .bind(occurrence.event_type.as_str())
        .bind(occurrence.category.as_str())
        .bind(observation_kind_str(occurrence.observation_kind))
        .bind(occurrence.severity.as_str())
        .bind(occurrence.condition_key.as_ref().map(ConditionKey::as_str))
        .bind(&occurrence.object_type)
        .bind(&occurrence.object_id)
        .bind(&occurrence.station_id)
        .bind(&occurrence.station_key_id)
        .bind(&occurrence.source)
        .bind(&occurrence.reason_code)
        .bind(&occurrence.old_value_json)
        .bind(&occurrence.new_value_json)
        .bind(&occurrence.impact_json)
        .bind(occurrence.observed_at_ms)
        .bind(occurrence.created_at_ms)
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(InsertResult {
            inserted: affected == 1,
        })
    }

    pub(crate) async fn delete_retained_before(
        &self,
        session: &mut WriteSession,
        cutoff_ms: i64,
        limit: u32,
    ) -> Result<u64, PersistenceError> {
        let affected = sqlx::query(
            "DELETE FROM change_event_occurrences
             WHERE id IN (
                 SELECT o.id FROM change_event_occurrences o
                 LEFT JOIN change_incidents i ON i.id = o.incident_id
                 WHERE o.created_at_ms < ?1
                   AND (o.incident_id IS NULL OR i.lifecycle_state = 'resolved')
                 ORDER BY o.created_at_ms ASC, o.id ASC LIMIT ?2
             )",
        )
        .bind(cutoff_ms)
        .bind(i64::from(limit.clamp(1, 1_000)))
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(affected)
    }
}

#[derive(Debug, Clone)]
#[expect(
    dead_code,
    reason = "contract=alerting.incident-record; owner=persistence/alerting; remove_when=diagnostic and migration reads are retired"
)]
pub(crate) struct IncidentRecord {
    pub id: String,
    pub condition_key: String,
    pub event_type: String,
    pub lifecycle_state: String,
    pub severity: String,
    pub episode_number: i64,
    pub occurrence_count: i64,
    pub version: i64,
    pub updated_at_ms: i64,
}

/// Complete persisted projection used by the application projector.  The read
/// model intentionally remains a smaller DTO; this type is only a persistence
/// boundary value and is never serialized to the frontend.
#[derive(Debug, Clone)]
pub(crate) struct IncidentSnapshot {
    pub incident: Incident,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct IncidentStore;

impl IncidentStore {
    pub(crate) async fn resolve_for_deleted_station(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        now_ms: i64,
    ) -> Result<u64, PersistenceError> {
        if station_id.trim().is_empty() || now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }

        sqlx::query(
            "UPDATE notification_deliveries
             SET status = 'suppressed', suppressed_reason = 'stale_episode',
                 claim_token = NULL, lease_expires_at_ms = NULL, retry_not_before_ms = NULL,
                 updated_at_ms = ?2
             WHERE incident_id IN (
                 SELECT id FROM change_incidents
                 WHERE (station_id = ?1 OR station_key_id IN (
                     SELECT id FROM station_keys WHERE station_id = ?1
                 ))
                   AND lifecycle_state IN ('pending', 'open', 'recovering')
             )
               AND status IN ('scheduled', 'claimed', 'outcome_unknown')",
        )
        .bind(station_id)
        .bind(now_ms)
        .execute(session.connection())
        .await?;

        let affected = sqlx::query(
            "UPDATE change_incidents
             SET lifecycle_state = 'resolved', resolved_at_ms = ?2,
                 recovering_at_ms = NULL, consecutive_abnormal_count = 0,
                 consecutive_healthy_count = 0, pending_since_ms = NULL,
                 healthy_since_ms = NULL, next_state_evaluation_at_ms = NULL,
                 next_notification_at_ms = NULL, version = version + 1,
                 updated_at_ms = ?2
             WHERE (station_id = ?1 OR station_key_id IN (
                 SELECT id FROM station_keys WHERE station_id = ?1
             ))
               AND lifecycle_state IN ('pending', 'open', 'recovering')",
        )
        .bind(station_id)
        .bind(now_ms)
        .execute(session.connection())
        .await?
        .rows_affected();

        Ok(affected)
    }

    pub(crate) async fn resolve_all_active(
        &self,
        session: &mut WriteSession,
        station_id: Option<&str>,
        severity: Option<Severity>,
        now_ms: i64,
    ) -> Result<u64, PersistenceError> {
        if now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        sqlx::query(
            "UPDATE notification_deliveries
             SET status = 'suppressed', suppressed_reason = 'stale_episode',
                 claim_token = NULL, lease_expires_at_ms = NULL, retry_not_before_ms = NULL,
                 updated_at_ms = ?3
             WHERE incident_id IN (
                 SELECT id FROM change_incidents
                 WHERE lifecycle_state IN ('pending', 'open', 'recovering')
                   AND (?1 IS NULL OR station_id = ?1)
                   AND (?2 IS NULL OR severity = ?2)
             )
               AND status IN ('scheduled', 'claimed', 'outcome_unknown')",
        )
        .bind(station_id)
        .bind(severity.map(Severity::as_str))
        .bind(now_ms)
        .execute(session.connection())
        .await?;

        Ok(sqlx::query(
            "UPDATE change_incidents
             SET lifecycle_state = 'resolved', resolved_at_ms = ?3,
                 recovering_at_ms = NULL, consecutive_abnormal_count = 0,
                 consecutive_healthy_count = 0, pending_since_ms = NULL,
                 healthy_since_ms = NULL, next_state_evaluation_at_ms = NULL,
                 next_notification_at_ms = NULL, version = version + 1,
                 updated_at_ms = ?3
             WHERE lifecycle_state IN ('pending', 'open', 'recovering')
               AND (?1 IS NULL OR station_id = ?1)
               AND (?2 IS NULL OR severity = ?2)",
        )
        .bind(station_id)
        .bind(severity.map(Severity::as_str))
        .bind(now_ms)
        .execute(session.connection())
        .await?
        .rows_affected())
    }

    /// Resolve active incidents left behind by an earlier station deletion.
    ///
    /// Older deletions relied on `ON DELETE SET NULL`, so the incident no
    /// longer has a foreign-key station reference even though its condition
    /// key still embeds the deleted station id.
    pub(crate) async fn resolve_orphaned_station_incidents(
        &self,
        session: &mut WriteSession,
        now_ms: i64,
    ) -> Result<u64, PersistenceError> {
        if now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }

        let rows = sqlx::query(
            "SELECT id, episode_number, station_id, station_key_id, object_type, object_id, condition_key
             FROM change_incidents
             WHERE lifecycle_state IN ('pending', 'open', 'recovering')",
        )
        .fetch_all(session.connection())
        .await?;

        let mut resolved = 0;
        for row in rows {
            let station_id = row.try_get::<Option<String>, _>("station_id")?;
            let station_key_id = row.try_get::<Option<String>, _>("station_key_id")?;
            let object_type = row.try_get::<String, _>("object_type")?;
            let object_id = row.try_get::<Option<String>, _>("object_id")?;
            let condition_key = row.try_get::<String, _>("condition_key")?;
            let station_candidate = station_id
                .or_else(|| {
                    (object_type == "station")
                        .then_some(object_id.clone())
                        .flatten()
                })
                .or_else(|| station_id_from_condition_key(&condition_key));
            let key_candidate = station_key_id
                .or_else(|| {
                    (object_type == "station_key")
                        .then_some(object_id)
                        .flatten()
                })
                .or_else(|| station_key_id_from_condition_key(&condition_key));

            let station_orphaned = match station_candidate {
                Some(candidate) => {
                    sqlx::query_scalar::<_, i64>(
                        "SELECT EXISTS(SELECT 1 FROM stations WHERE id = ?1)",
                    )
                    .bind(candidate)
                    .fetch_one(session.connection())
                    .await?
                        == 0
                }
                None => false,
            };
            let key_orphaned = match key_candidate {
                Some(candidate) => {
                    sqlx::query_scalar::<_, i64>(
                        "SELECT EXISTS(SELECT 1 FROM station_keys WHERE id = ?1)",
                    )
                    .bind(candidate)
                    .fetch_one(session.connection())
                    .await?
                        == 0
                }
                None => false,
            };
            let orphaned = station_orphaned || key_orphaned;
            if !orphaned {
                continue;
            }

            let incident_id: String = row.try_get("id")?;
            let episode_number: i64 = row.try_get("episode_number")?;
            sqlx::query(
                "UPDATE notification_deliveries
                 SET status = 'suppressed', suppressed_reason = 'stale_episode',
                     claim_token = NULL, lease_expires_at_ms = NULL, retry_not_before_ms = NULL,
                     updated_at_ms = ?3
                 WHERE incident_id = ?1 AND episode_number = ?2
                   AND status IN ('scheduled', 'claimed', 'outcome_unknown')",
            )
            .bind(&incident_id)
            .bind(episode_number)
            .bind(now_ms)
            .execute(session.connection())
            .await?;
            let affected = sqlx::query(
                "UPDATE change_incidents
                 SET lifecycle_state = 'resolved', resolved_at_ms = ?3,
                     recovering_at_ms = NULL, consecutive_abnormal_count = 0,
                     consecutive_healthy_count = 0, pending_since_ms = NULL,
                     healthy_since_ms = NULL, next_state_evaluation_at_ms = NULL,
                     next_notification_at_ms = NULL, version = version + 1,
                     updated_at_ms = ?3
                 WHERE id = ?1 AND episode_number = ?2
                   AND lifecycle_state IN ('pending', 'open', 'recovering')",
            )
            .bind(&incident_id)
            .bind(episode_number)
            .bind(now_ms)
            .execute(session.connection())
            .await?
            .rows_affected();
            resolved += affected;
        }

        Ok(resolved)
    }

    pub(crate) async fn list_active_page(
        &self,
        session: &mut ReadSession,
        cursor: Option<(i64, String)>,
        limit: u32,
    ) -> Result<Vec<IncidentSnapshot>, PersistenceError> {
        let limit = i64::from(limit.clamp(1, 500));
        let rows = match cursor {
            Some((updated_at_ms, id)) => sqlx::query(
                "SELECT id, condition_key, event_type, lifecycle_state, base_severity, severity,
                        object_type, object_id, station_id, station_key_id, policy_id,
                        policy_revision, lifecycle_policy_fingerprint, episode_number,
                        first_seen_at_ms, last_seen_at_ms, opened_at_ms, recovering_at_ms,
                        resolved_at_ms, occurrence_count, episode_occurrence_count,
                        consecutive_abnormal_count, consecutive_healthy_count, pending_since_ms,
                        healthy_since_ms, last_observation_id, last_observation_summary_json,
                        fact_fresh_until_ms, next_state_evaluation_at_ms, last_notification_at_ms,
                        next_notification_at_ms, version, created_at_ms, updated_at_ms
                 FROM change_incidents
                 WHERE lifecycle_state IN ('pending','open','recovering')
                   AND (updated_at_ms > ?1 OR (updated_at_ms = ?1 AND id > ?2))
                 ORDER BY updated_at_ms ASC, id ASC LIMIT ?3",
            )
            .bind(updated_at_ms)
            .bind(id)
            .bind(limit)
            .fetch_all(session.connection())
            .await?,
            None => sqlx::query(
                "SELECT id, condition_key, event_type, lifecycle_state, base_severity, severity,
                        object_type, object_id, station_id, station_key_id, policy_id,
                        policy_revision, lifecycle_policy_fingerprint, episode_number,
                        first_seen_at_ms, last_seen_at_ms, opened_at_ms, recovering_at_ms,
                        resolved_at_ms, occurrence_count, episode_occurrence_count,
                        consecutive_abnormal_count, consecutive_healthy_count, pending_since_ms,
                        healthy_since_ms, last_observation_id, last_observation_summary_json,
                        fact_fresh_until_ms, next_state_evaluation_at_ms, last_notification_at_ms,
                        next_notification_at_ms, version, created_at_ms, updated_at_ms
                 FROM change_incidents
                 WHERE lifecycle_state IN ('pending','open','recovering')
                 ORDER BY updated_at_ms ASC, id ASC LIMIT ?1",
            )
            .bind(limit)
            .fetch_all(session.connection())
            .await?,
        };
        rows.into_iter().map(row_to_snapshot).collect()
    }

    pub(crate) async fn load_for_write(
        &self,
        session: &mut WriteSession,
        condition_key: &str,
    ) -> Result<Option<IncidentSnapshot>, PersistenceError> {
        let row = sqlx::query(
            "SELECT id, condition_key, event_type, lifecycle_state, base_severity, severity,
                    object_type, object_id, station_id, station_key_id, policy_id,
                    policy_revision, lifecycle_policy_fingerprint, episode_number,
                    first_seen_at_ms, last_seen_at_ms, opened_at_ms, recovering_at_ms,
                    resolved_at_ms, occurrence_count, episode_occurrence_count,
                    consecutive_abnormal_count, consecutive_healthy_count, pending_since_ms,
                    healthy_since_ms, last_observation_id, last_observation_summary_json,
                    fact_fresh_until_ms, next_state_evaluation_at_ms, last_notification_at_ms,
                    next_notification_at_ms, version, created_at_ms, updated_at_ms
             FROM change_incidents WHERE condition_key = ?1",
        )
        .bind(condition_key)
        .fetch_optional(session.connection())
        .await?;
        row.map(row_to_snapshot).transpose()
    }

    pub(crate) async fn insert_snapshot(
        &self,
        session: &mut WriteSession,
        snapshot: &IncidentSnapshot,
    ) -> Result<(), PersistenceError> {
        let incident = &snapshot.incident;
        sqlx::query(
            "INSERT INTO change_incidents (
                id, condition_key, event_type, lifecycle_state, base_severity, severity,
                object_type, object_id, station_id, station_key_id, policy_id, policy_revision,
                lifecycle_policy_fingerprint, episode_number, first_seen_at_ms, last_seen_at_ms,
                opened_at_ms, recovering_at_ms, resolved_at_ms, occurrence_count,
                episode_occurrence_count, consecutive_abnormal_count, consecutive_healthy_count,
                pending_since_ms, healthy_since_ms, last_observation_id,
                last_observation_summary_json, fact_fresh_until_ms, next_state_evaluation_at_ms,
                last_notification_at_ms, next_notification_at_ms, version, created_at_ms,
                updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                       ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27,
                       ?28, ?29, ?30, ?31, ?32, ?33, ?34)",
        )
        .bind(&incident.id)
        .bind(incident.condition_key.as_str())
        .bind(incident.event_type.as_str())
        .bind(incident.lifecycle_state.as_str())
        .bind(incident.base_severity.as_str())
        .bind(incident.severity.as_str())
        .bind(&incident.object_type)
        .bind(&incident.object_id)
        .bind(&incident.station_id)
        .bind(&incident.station_key_id)
        .bind(&incident.policy_id)
        .bind(incident.policy_revision.map(|v| v as i64))
        .bind(&incident.lifecycle_policy_fingerprint)
        .bind(incident.episode_number as i64)
        .bind(incident.first_seen_at_ms)
        .bind(incident.last_seen_at_ms)
        .bind(incident.opened_at_ms)
        .bind(incident.recovering_at_ms)
        .bind(incident.resolved_at_ms)
        .bind(incident.occurrence_count as i64)
        .bind(incident.episode_occurrence_count as i64)
        .bind(incident.consecutive_abnormal_count as i64)
        .bind(incident.consecutive_healthy_count as i64)
        .bind(incident.pending_since_ms)
        .bind(incident.healthy_since_ms)
        .bind(&incident.last_observation_id)
        .bind(&incident.last_observation_summary_json)
        .bind(incident.fact_fresh_until_ms)
        .bind(incident.next_state_evaluation_at_ms)
        .bind(incident.last_notification_at_ms)
        .bind(incident.next_notification_at_ms)
        .bind(incident.version as i64)
        .bind(incident.created_at_ms)
        .bind(incident.updated_at_ms)
        .execute(session.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn update_snapshot_cas(
        &self,
        session: &mut WriteSession,
        snapshot: &IncidentSnapshot,
        expected_version: u64,
    ) -> Result<(), PersistenceError> {
        let incident = &snapshot.incident;
        let affected = sqlx::query(
            "UPDATE change_incidents SET lifecycle_state = ?2, severity = ?3,
                    object_type = ?4, object_id = ?5, station_id = ?6, station_key_id = ?7,
                    policy_id = ?8, policy_revision = ?9, lifecycle_policy_fingerprint = ?10,
                    episode_number = ?11, first_seen_at_ms = ?12, last_seen_at_ms = ?13,
                    opened_at_ms = ?14, recovering_at_ms = ?15, resolved_at_ms = ?16,
                    occurrence_count = ?17, episode_occurrence_count = ?18,
                    consecutive_abnormal_count = ?19, consecutive_healthy_count = ?20,
                    pending_since_ms = ?21, healthy_since_ms = ?22, last_observation_id = ?23,
                    last_observation_summary_json = ?24, fact_fresh_until_ms = ?25,
                    next_state_evaluation_at_ms = ?26, last_notification_at_ms = ?27,
                    next_notification_at_ms = ?28, version = ?29, updated_at_ms = ?30
             WHERE id = ?1 AND version = ?31",
        )
        .bind(&incident.id)
        .bind(incident.lifecycle_state.as_str())
        .bind(incident.severity.as_str())
        .bind(&incident.object_type)
        .bind(&incident.object_id)
        .bind(&incident.station_id)
        .bind(&incident.station_key_id)
        .bind(&incident.policy_id)
        .bind(incident.policy_revision.map(|v| v as i64))
        .bind(&incident.lifecycle_policy_fingerprint)
        .bind(incident.episode_number as i64)
        .bind(incident.first_seen_at_ms)
        .bind(incident.last_seen_at_ms)
        .bind(incident.opened_at_ms)
        .bind(incident.recovering_at_ms)
        .bind(incident.resolved_at_ms)
        .bind(incident.occurrence_count as i64)
        .bind(incident.episode_occurrence_count as i64)
        .bind(incident.consecutive_abnormal_count as i64)
        .bind(incident.consecutive_healthy_count as i64)
        .bind(incident.pending_since_ms)
        .bind(incident.healthy_since_ms)
        .bind(&incident.last_observation_id)
        .bind(&incident.last_observation_summary_json)
        .bind(incident.fact_fresh_until_ms)
        .bind(incident.next_state_evaluation_at_ms)
        .bind(incident.last_notification_at_ms)
        .bind(incident.next_notification_at_ms)
        .bind(incident.version as i64)
        .bind(incident.updated_at_ms)
        .bind(expected_version as i64)
        .execute(session.connection())
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::RevisionConflict(incident.id.clone()));
        }
        Ok(())
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.incident-lookup; owner=persistence/alerting; remove_when=diagnostic and migration reads are retired"
    )]
    pub(crate) async fn get_by_condition_key(
        &self,
        session: &mut ReadSession,
        condition_key: &str,
    ) -> Result<Option<IncidentRecord>, PersistenceError> {
        let row = sqlx::query(
            "SELECT id, condition_key, event_type, lifecycle_state, severity,
                    episode_number, occurrence_count, version, updated_at_ms
             FROM change_incidents WHERE condition_key = ?1",
        )
        .bind(condition_key)
        .fetch_optional(session.connection())
        .await?;
        Ok(row.map(|row| IncidentRecord {
            id: row.get("id"),
            condition_key: row.get("condition_key"),
            event_type: row.get("event_type"),
            lifecycle_state: row.get("lifecycle_state"),
            severity: row.get("severity"),
            episode_number: row.get("episode_number"),
            occurrence_count: row.get("occurrence_count"),
            version: row.get("version"),
            updated_at_ms: row.get("updated_at_ms"),
        }))
    }

    pub(crate) async fn link_occurrence(
        &self,
        session: &mut WriteSession,
        occurrence_id: &str,
        incident_id: &str,
        episode_number: i64,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            "UPDATE change_event_occurrences
             SET incident_id = ?2, episode_number = ?3
             WHERE id = ?1 AND (incident_id IS NULL OR incident_id = ?2)",
        )
        .bind(occurrence_id)
        .bind(incident_id)
        .bind(episode_number)
        .execute(session.connection())
        .await?;
        Ok(())
    }
}

fn station_id_from_condition_key(condition_key: &str) -> Option<String> {
    let rest = ["collector:", "balance:", "station:"]
        .iter()
        .find_map(|prefix| condition_key.strip_prefix(prefix))?;
    let station_id = rest.split(':').next()?.trim();
    (!station_id.is_empty()).then(|| station_id.to_string())
}

fn station_key_id_from_condition_key(condition_key: &str) -> Option<String> {
    let rest = ["station_key:", "key:"]
        .iter()
        .find_map(|prefix| condition_key.strip_prefix(prefix))?;
    let station_key_id = rest.split(':').next()?.trim();
    (!station_key_id.is_empty()).then(|| station_key_id.to_string())
}

fn row_to_snapshot(row: sqlx::sqlite::SqliteRow) -> Result<IncidentSnapshot, PersistenceError> {
    let condition_key = ConditionKey::new(row.get::<String, _>("condition_key"))
        .map_err(PersistenceError::InvariantViolation)?;
    let event_type = AlertEventType::from_str(&row.get::<String, _>("event_type"))
        .ok_or_else(|| PersistenceError::InvariantViolation("unknown alert event type".into()))?;
    let parse_severity = |field: &str| {
        Severity::from_str(&row.get::<String, _>(field))
            .ok_or_else(|| PersistenceError::InvariantViolation("unknown alert severity".into()))
    };
    let parse_lifecycle = match row.get::<String, _>("lifecycle_state").as_str() {
        "pending" => LifecycleState::Pending,
        "open" => LifecycleState::Open,
        "recovering" => LifecycleState::Recovering,
        "resolved" => LifecycleState::Resolved,
        _ => {
            return Err(PersistenceError::InvariantViolation(
                "unknown incident lifecycle".into(),
            ))
        }
    };
    Ok(IncidentSnapshot {
        incident: Incident {
            id: row.get("id"),
            condition_key,
            event_type,
            lifecycle_state: parse_lifecycle,
            base_severity: parse_severity("base_severity")?,
            severity: parse_severity("severity")?,
            object_type: row.get("object_type"),
            object_id: row.get("object_id"),
            station_id: row.get("station_id"),
            station_key_id: row.get("station_key_id"),
            policy_id: row.get("policy_id"),
            policy_revision: row
                .get::<Option<i64>, _>("policy_revision")
                .map(|v| v as u64),
            lifecycle_policy_fingerprint: row.get("lifecycle_policy_fingerprint"),
            episode_number: row.get::<i64, _>("episode_number") as u32,
            first_seen_at_ms: row.get("first_seen_at_ms"),
            last_seen_at_ms: row.get("last_seen_at_ms"),
            opened_at_ms: row.get("opened_at_ms"),
            recovering_at_ms: row.get("recovering_at_ms"),
            resolved_at_ms: row.get("resolved_at_ms"),
            occurrence_count: row.get::<i64, _>("occurrence_count") as u64,
            episode_occurrence_count: row.get::<i64, _>("episode_occurrence_count") as u64,
            consecutive_abnormal_count: row.get::<i64, _>("consecutive_abnormal_count") as u32,
            consecutive_healthy_count: row.get::<i64, _>("consecutive_healthy_count") as u32,
            pending_since_ms: row.get("pending_since_ms"),
            healthy_since_ms: row.get("healthy_since_ms"),
            last_observation_id: row.get("last_observation_id"),
            last_observation_summary_json: row.get("last_observation_summary_json"),
            fact_fresh_until_ms: row.get("fact_fresh_until_ms"),
            next_state_evaluation_at_ms: row.get("next_state_evaluation_at_ms"),
            last_notification_at_ms: row.get("last_notification_at_ms"),
            next_notification_at_ms: row.get("next_notification_at_ms"),
            version: row.get::<i64, _>("version") as u64,
            created_at_ms: row.get("created_at_ms"),
            updated_at_ms: row.get("updated_at_ms"),
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttentionKey<'a> {
    pub incident_id: &'a str,
    pub episode_number: i64,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct AttentionStore;

impl AttentionStore {
    pub(crate) async fn ensure(
        &self,
        session: &mut WriteSession,
        key: AttentionKey<'_>,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            "INSERT INTO incident_attention (incident_id, episode_number, updated_at_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(incident_id, episode_number) DO NOTHING",
        )
        .bind(key.incident_id)
        .bind(key.episode_number)
        .bind(now_ms)
        .execute(session.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_seen(
        &self,
        session: &mut WriteSession,
        key: AttentionKey<'_>,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            "UPDATE incident_attention SET seen_at_ms = COALESCE(seen_at_ms, ?3), updated_at_ms = ?3
             WHERE incident_id = ?1 AND episode_number = ?2",
        )
        .bind(key.incident_id)
        .bind(key.episode_number)
        .bind(now_ms)
        .execute(session.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_all_seen(
        &self,
        session: &mut WriteSession,
        station_id: Option<&str>,
        severity: Option<Severity>,
        now_ms: i64,
    ) -> Result<u64, PersistenceError> {
        let affected = sqlx::query(
            "INSERT INTO incident_attention (incident_id, episode_number, seen_at_ms, updated_at_ms)
             SELECT i.id, i.episode_number, ?3, ?3
             FROM change_incidents i
             LEFT JOIN incident_attention a
               ON a.incident_id = i.id AND a.episode_number = i.episode_number
             WHERE i.lifecycle_state IN ('pending', 'open', 'recovering')
               AND i.severity IN ('warning', 'critical')
               AND a.seen_at_ms IS NULL
               AND (?1 IS NULL OR i.station_id = ?1)
               AND (?2 IS NULL OR i.severity = ?2)
             ON CONFLICT(incident_id, episode_number) DO UPDATE SET
               seen_at_ms = excluded.seen_at_ms,
               updated_at_ms = excluded.updated_at_ms
             WHERE incident_attention.seen_at_ms IS NULL",
        )
        .bind(station_id)
        .bind(severity.map(|value| value.as_str()))
        .bind(now_ms)
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(affected)
    }

    pub(crate) async fn snooze_until(
        &self,
        session: &mut WriteSession,
        key: AttentionKey<'_>,
        until_ms: i64,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            "UPDATE incident_attention
             SET snoozed_until_ms = ?3, updated_at_ms = ?4
             WHERE incident_id = ?1 AND episode_number = ?2",
        )
        .bind(key.incident_id)
        .bind(key.episode_number)
        .bind(until_ms)
        .bind(now_ms)
        .execute(session.connection())
        .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpgradeProgress {
    pub phase: String,
    pub source_high_water_cursor: Option<String>,
    pub last_copied_cursor: Option<String>,
    pub copied_count: i64,
    pub rebuild_version: Option<i64>,
    pub last_error_code: Option<String>,
    pub started_at_ms: Option<i64>,
    pub updated_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct UpgradeProgressStore;

impl UpgradeProgressStore {
    pub(crate) async fn load(
        &self,
        session: &mut ReadSession,
    ) -> Result<Option<UpgradeProgress>, PersistenceError> {
        let row = sqlx::query(
            "SELECT phase, source_high_water_cursor, last_copied_cursor, copied_count,
                    rebuild_version, last_error_code, started_at_ms, updated_at_ms, completed_at_ms
             FROM alerting_upgrade_progress WHERE singleton_key = 1",
        )
        .fetch_optional(session.connection())
        .await?;
        Ok(row.map(|row| UpgradeProgress {
            phase: row.get("phase"),
            source_high_water_cursor: row.get("source_high_water_cursor"),
            last_copied_cursor: row.get("last_copied_cursor"),
            copied_count: row.get("copied_count"),
            rebuild_version: row.get("rebuild_version"),
            last_error_code: row.get("last_error_code"),
            started_at_ms: row.get("started_at_ms"),
            updated_at_ms: row.get("updated_at_ms"),
            completed_at_ms: row.get("completed_at_ms"),
        }))
    }

    pub(crate) async fn set_phase(
        &self,
        session: &mut WriteSession,
        phase: &str,
        now_ms: i64,
        error_code: Option<&str>,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            "INSERT INTO alerting_upgrade_progress (singleton_key, phase, last_error_code, started_at_ms, updated_at_ms)
             VALUES (1, ?1, ?2, ?3, ?3)
             ON CONFLICT(singleton_key) DO UPDATE SET
                phase = excluded.phase,
                last_error_code = excluded.last_error_code,
                started_at_ms = COALESCE(alerting_upgrade_progress.started_at_ms, excluded.started_at_ms),
                updated_at_ms = excluded.updated_at_ms,
                completed_at_ms = CASE WHEN excluded.phase = 'complete' THEN excluded.updated_at_ms ELSE alerting_upgrade_progress.completed_at_ms END",
        )
        .bind(phase)
        .bind(error_code)
        .bind(now_ms)
        .execute(session.connection())
        .await?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PolicyStore;

impl PolicyStore {
    pub(crate) async fn get(
        &self,
        session: &mut ReadSession,
        id: &str,
    ) -> Result<Option<AlertPolicy>, PersistenceError> {
        let row = sqlx::query("SELECT * FROM alert_policies WHERE id = ?1")
            .bind(id)
            .fetch_optional(session.connection())
            .await?;
        row.map(policy_from_row).transpose()
    }

    pub(crate) async fn list(
        &self,
        session: &mut ReadSession,
    ) -> Result<Vec<AlertPolicy>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT * FROM alert_policies
             ORDER BY CASE scope_kind WHEN 'station_key' THEN 3 WHEN 'station' THEN 2
                      WHEN 'event_type' THEN 1 ELSE 0 END DESC,
                      priority ASC, created_at_ms ASC, id ASC",
        )
        .fetch_all(session.connection())
        .await?;
        rows.into_iter().map(policy_from_row).collect()
    }

    pub(crate) async fn list_for_write(
        &self,
        session: &mut WriteSession,
    ) -> Result<Vec<AlertPolicy>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT * FROM alert_policies
             ORDER BY CASE scope_kind WHEN 'station_key' THEN 3 WHEN 'station' THEN 2
                      WHEN 'event_type' THEN 1 ELSE 0 END DESC,
                      priority ASC, created_at_ms ASC, id ASC",
        )
        .fetch_all(session.connection())
        .await?;
        rows.into_iter().map(policy_from_row).collect()
    }

    pub(crate) async fn save(
        &self,
        session: &mut WriteSession,
        policy: &AlertPolicy,
        expected_revision: Option<u64>,
    ) -> Result<(), PersistenceError> {
        policy
            .validate()
            .map_err(PersistenceError::InvariantViolation)?;
        let current =
            sqlx::query_scalar::<_, i64>("SELECT revision FROM alert_policies WHERE id = ?1")
                .bind(&policy.id)
                .fetch_optional(session.connection())
                .await?;
        match (current, expected_revision) {
            (None, None) if policy.revision == 1 => {}
            (Some(current), Some(expected))
                if current == expected as i64 && policy.revision == expected.saturating_add(1) => {}
            _ => return Err(PersistenceError::RevisionConflict(policy.id.clone())),
        }
        let affected = sqlx::query(
            "INSERT INTO alert_policies (
                id, name, enabled, state, scope_kind, event_type, station_id, station_key_id,
                minimum_severity, severity_offset, trigger_mode, trigger_count,
                trigger_duration_seconds, recovery_mode, recovery_count, recovery_duration_seconds,
                in_app_enabled, desktop_enabled, repeat_mode, repeat_interval_seconds,
                cooldown_seconds, recovery_notification_enabled, quiet_hours_policy, priority,
                revision, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                       ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name, enabled = excluded.enabled, state = excluded.state,
                scope_kind = excluded.scope_kind, event_type = excluded.event_type,
                station_id = excluded.station_id, station_key_id = excluded.station_key_id,
                minimum_severity = excluded.minimum_severity, severity_offset = excluded.severity_offset,
                trigger_mode = excluded.trigger_mode, trigger_count = excluded.trigger_count,
                trigger_duration_seconds = excluded.trigger_duration_seconds,
                recovery_mode = excluded.recovery_mode, recovery_count = excluded.recovery_count,
                recovery_duration_seconds = excluded.recovery_duration_seconds,
                in_app_enabled = excluded.in_app_enabled, desktop_enabled = excluded.desktop_enabled,
                repeat_mode = excluded.repeat_mode, repeat_interval_seconds = excluded.repeat_interval_seconds,
                cooldown_seconds = excluded.cooldown_seconds,
                recovery_notification_enabled = excluded.recovery_notification_enabled,
                quiet_hours_policy = excluded.quiet_hours_policy, priority = excluded.priority,
                revision = excluded.revision, updated_at_ms = excluded.updated_at_ms
             WHERE (?28 IS NOT NULL AND alert_policies.revision = ?28)",
        )
        .bind(&policy.id)
        .bind(&policy.name)
        .bind(i64::from(policy.enabled))
        .bind(policy.state.as_str())
        .bind(policy.scope_kind.as_str())
        .bind(policy.event_type.map(AlertEventType::as_str))
        .bind(&policy.station_id)
        .bind(&policy.station_key_id)
        .bind(policy.minimum_severity.map(Severity::as_str))
        .bind(i64::from(policy.severity_offset))
        .bind(policy.trigger_mode.as_str())
        .bind(policy.trigger_count.map(i64::from))
        .bind(policy.trigger_duration_seconds.map(|v| v as i64))
        .bind(policy.recovery_mode.as_str())
        .bind(policy.recovery_count.map(i64::from))
        .bind(policy.recovery_duration_seconds.map(|v| v as i64))
        .bind(i64::from(policy.in_app_enabled))
        .bind(i64::from(policy.desktop_enabled))
        .bind(policy.repeat_mode.as_str())
        .bind(policy.repeat_interval_seconds.map(|v| v as i64))
        .bind(policy.cooldown_seconds as i64)
        .bind(i64::from(policy.recovery_notification_enabled))
        .bind(policy.quiet_hours_policy.as_str())
        .bind(i64::from(policy.priority))
        .bind(policy.revision as i64)
        .bind(policy.created_at_ms)
        .bind(policy.updated_at_ms)
        .bind(expected_revision.map(|v| v as i64))
        .execute(session.connection())
        .await?
        .rows_affected();
        if affected == 0 {
            return Err(PersistenceError::RevisionConflict(policy.id.clone()));
        }
        Ok(())
    }

    pub(crate) async fn mark_state(
        &self,
        session: &mut WriteSession,
        id: &str,
        state: PolicyState,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let affected = sqlx::query(
            "UPDATE alert_policies SET state = ?2,
                    enabled = CASE WHEN ?2 = 'active' THEN 1 ELSE 0 END,
                    revision = revision + 1, updated_at_ms = ?3
             WHERE id = ?1 AND revision = ?4",
        )
        .bind(id)
        .bind(state.as_str())
        .bind(now_ms)
        .bind(expected_revision as i64)
        .execute(session.connection())
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::RevisionConflict(id.to_string()));
        }
        Ok(())
    }
}

fn policy_from_row(row: sqlx::sqlite::SqliteRow) -> Result<AlertPolicy, PersistenceError> {
    let parse = |value: &str, field: &str| {
        value.parse::<u64>().map_err(|_| {
            PersistenceError::InvariantViolation(format!("invalid persisted policy {field}"))
        })
    };
    let parse_optional = |value: Option<i64>, field: &str| {
        value
            .map(|v| {
                u64::try_from(v).map_err(|_| {
                    PersistenceError::InvariantViolation(format!(
                        "invalid persisted policy {field}"
                    ))
                })
            })
            .transpose()
    };
    let scope = ScopeKind::from_str(&row.get::<String, _>("scope_kind"))
        .ok_or_else(|| PersistenceError::InvariantViolation("invalid policy scope".into()))?;
    let state = PolicyState::from_str(&row.get::<String, _>("state"))
        .ok_or_else(|| PersistenceError::InvariantViolation("invalid policy state".into()))?;
    let trigger_mode = TriggerMode::from_str(&row.get::<String, _>("trigger_mode"))
        .ok_or_else(|| PersistenceError::InvariantViolation("invalid trigger mode".into()))?;
    let recovery_mode = RecoveryMode::from_str(&row.get::<String, _>("recovery_mode"))
        .ok_or_else(|| PersistenceError::InvariantViolation("invalid recovery mode".into()))?;
    let repeat_mode = RepeatMode::from_str(&row.get::<String, _>("repeat_mode"))
        .ok_or_else(|| PersistenceError::InvariantViolation("invalid repeat mode".into()))?;
    let quiet_hours_policy = QuietHoursPolicy::from_str(
        &row.get::<String, _>("quiet_hours_policy"),
    )
    .ok_or_else(|| PersistenceError::InvariantViolation("invalid quiet-hours policy".into()))?;
    let event_type = row
        .get::<Option<String>, _>("event_type")
        .map(|v| {
            AlertEventType::from_str(&v).ok_or_else(|| {
                PersistenceError::InvariantViolation("invalid policy event type".into())
            })
        })
        .transpose()?;
    let minimum_severity = row
        .get::<Option<String>, _>("minimum_severity")
        .map(|v| {
            Severity::from_str(&v).ok_or_else(|| {
                PersistenceError::InvariantViolation("invalid policy severity".into())
            })
        })
        .transpose()?;
    let severity_offset = i8::try_from(row.get::<i64, _>("severity_offset"))
        .map_err(|_| PersistenceError::InvariantViolation("invalid severity offset".into()))?;
    let revision = parse(&row.get::<i64, _>("revision").to_string(), "revision")?;
    let priority = u32::try_from(row.get::<i64, _>("priority"))
        .map_err(|_| PersistenceError::InvariantViolation("invalid policy priority".into()))?;
    let trigger_count = row
        .get::<Option<i64>, _>("trigger_count")
        .map(|v| {
            u32::try_from(v)
                .map_err(|_| PersistenceError::InvariantViolation("invalid trigger count".into()))
        })
        .transpose()?;
    let recovery_count = row
        .get::<Option<i64>, _>("recovery_count")
        .map(|v| {
            u32::try_from(v)
                .map_err(|_| PersistenceError::InvariantViolation("invalid recovery count".into()))
        })
        .transpose()?;
    Ok(AlertPolicy {
        id: row.get("id"),
        name: row.get("name"),
        enabled: row.get::<i64, _>("enabled") != 0,
        state,
        scope_kind: scope,
        event_type,
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        minimum_severity,
        severity_offset,
        trigger_mode,
        trigger_count,
        trigger_duration_seconds: parse_optional(
            row.get("trigger_duration_seconds"),
            "trigger_duration_seconds",
        )?,
        recovery_mode,
        recovery_count,
        recovery_duration_seconds: parse_optional(
            row.get("recovery_duration_seconds"),
            "recovery_duration_seconds",
        )?,
        in_app_enabled: row.get::<i64, _>("in_app_enabled") != 0,
        desktop_enabled: row.get::<i64, _>("desktop_enabled") != 0,
        repeat_mode,
        repeat_interval_seconds: parse_optional(
            row.get("repeat_interval_seconds"),
            "repeat_interval_seconds",
        )?,
        cooldown_seconds: parse(
            &row.get::<i64, _>("cooldown_seconds").to_string(),
            "cooldown_seconds",
        )?,
        recovery_notification_enabled: row.get::<i64, _>("recovery_notification_enabled") != 0,
        quiet_hours_policy,
        priority,
        revision,
        created_at_ms: row.get("created_at_ms"),
        updated_at_ms: row.get("updated_at_ms"),
    })
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DeliveryStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryMetadata {
    pub id: String,
    pub incident_id: String,
    pub episode_number: u32,
    pub channel: NotificationChannel,
    pub delivery_kind: DeliveryKind,
    pub policy_snapshot_json: String,
    pub attempt_count: u32,
    pub object_type: Option<String>,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
}

impl DeliveryStore {
    pub(crate) async fn schedule(
        &self,
        session: &mut WriteSession,
        delivery: &NotificationDelivery,
    ) -> Result<InsertResult, PersistenceError> {
        let affected = sqlx::query(
            "INSERT INTO notification_deliveries (
                id, delivery_key, incident_id, episode_number, delivery_sequence,
                policy_id, policy_revision, policy_snapshot_json, channel, delivery_kind,
                status, scheduled_at_ms, attempt_count, created_at_ms, updated_at_ms,
                suppressed_reason
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0, ?12, ?12, ?13)
             ON CONFLICT(delivery_key) DO NOTHING",
        )
        .bind(&delivery.id)
        .bind(&delivery.delivery_key)
        .bind(&delivery.incident_id)
        .bind(i64::from(delivery.episode_number))
        .bind(delivery.delivery_sequence as i64)
        .bind(&delivery.policy_id)
        .bind(delivery.policy_revision.map(|value| value as i64))
        .bind(&delivery.policy_snapshot_json)
        .bind(delivery.channel.as_str())
        .bind(delivery.delivery_kind.as_str())
        .bind(delivery.status.as_str())
        .bind(delivery.scheduled_at_ms)
        .bind(delivery.suppressed_reason.map(SuppressionReason::as_str))
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(InsertResult {
            inserted: affected == 1,
        })
    }

    pub(crate) async fn claim_due(
        &self,
        session: &mut WriteSession,
        id: &str,
        token: &str,
        now_ms: i64,
        lease_ms: i64,
    ) -> Result<bool, PersistenceError> {
        if lease_ms <= 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let affected = sqlx::query(
            "UPDATE notification_deliveries
             SET status = 'claimed', claim_token = ?2, claimed_at_ms = ?3,
                 lease_expires_at_ms = ?3 + ?4, attempt_count = attempt_count + 1,
                 attempted_at_ms = ?3, retry_not_before_ms = NULL, updated_at_ms = ?3
             WHERE id = ?1 AND status IN ('scheduled', 'outcome_unknown')
               AND scheduled_at_ms <= ?3
               AND (retry_not_before_ms IS NULL OR retry_not_before_ms <= ?3)",
        )
        .bind(id)
        .bind(token)
        .bind(now_ms)
        .bind(lease_ms)
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(affected == 1)
    }

    pub(crate) async fn due_ids(
        &self,
        session: &mut ReadSession,
        now_ms: i64,
        limit: u32,
    ) -> Result<Vec<String>, PersistenceError> {
        let limit = i64::from(limit.clamp(1, 500));
        let rows = sqlx::query(
            "SELECT id FROM notification_deliveries
             WHERE status IN ('scheduled', 'outcome_unknown')
               AND scheduled_at_ms <= ?1
               AND (retry_not_before_ms IS NULL OR retry_not_before_ms <= ?1)
             ORDER BY scheduled_at_ms ASC, id ASC LIMIT ?2",
        )
        .bind(now_ms)
        .bind(limit)
        .fetch_all(session.connection())
        .await?;
        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    pub(crate) async fn metadata_for_claim(
        &self,
        session: &mut ReadSession,
        id: &str,
        token: &str,
    ) -> Result<Option<DeliveryMetadata>, PersistenceError> {
        let row = sqlx::query(
            "SELECT d.id, d.incident_id, d.episode_number, d.channel, d.delivery_kind,
                    d.policy_snapshot_json, d.attempt_count,
                    i.object_type, i.object_id, i.station_id, i.station_key_id
             FROM notification_deliveries d
             LEFT JOIN change_incidents i ON i.id = d.incident_id
             WHERE d.id = ?1 AND d.status = 'claimed' AND d.claim_token = ?2",
        )
        .bind(id)
        .bind(token)
        .fetch_optional(session.connection())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let channel =
            NotificationChannel::from_str(&row.get::<String, _>("channel")).ok_or_else(|| {
                PersistenceError::InvariantViolation("invalid delivery channel".into())
            })?;
        let delivery_kind = DeliveryKind::from_str(&row.get::<String, _>("delivery_kind"))
            .ok_or_else(|| PersistenceError::InvariantViolation("invalid delivery kind".into()))?;
        let episode_number = u32::try_from(row.get::<i64, _>("episode_number"))
            .map_err(|_| PersistenceError::InvariantViolation("invalid delivery episode".into()))?;
        let attempt_count = u32::try_from(row.get::<i64, _>("attempt_count"))
            .map_err(|_| PersistenceError::InvariantViolation("invalid delivery attempt".into()))?;
        Ok(Some(DeliveryMetadata {
            id: row.get("id"),
            incident_id: row.get("incident_id"),
            episode_number,
            channel,
            delivery_kind,
            policy_snapshot_json: row.get("policy_snapshot_json"),
            attempt_count,
            object_type: row.get("object_type"),
            object_id: row.get("object_id"),
            station_id: row.get("station_id"),
            station_key_id: row.get("station_key_id"),
        }))
    }

    pub(crate) async fn mark_delivered(
        &self,
        session: &mut WriteSession,
        id: &str,
        token: &str,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let affected = sqlx::query(
            "UPDATE notification_deliveries SET status = 'delivered', delivered_at_ms = ?3,
                    claim_token = NULL, lease_expires_at_ms = NULL, updated_at_ms = ?3
             WHERE id = ?1 AND status = 'claimed' AND claim_token = ?2",
        )
        .bind(id)
        .bind(token)
        .bind(now_ms)
        .execute(session.connection())
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::RevisionConflict(id.to_string()));
        }
        Ok(())
    }

    pub(crate) async fn mark_failed(
        &self,
        session: &mut WriteSession,
        id: &str,
        token: &str,
        error_code: &str,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let affected = sqlx::query(
            "UPDATE notification_deliveries SET status = 'failed', error_code = ?3,
                    claim_token = NULL, lease_expires_at_ms = NULL, updated_at_ms = ?4
             WHERE id = ?1 AND status = 'claimed' AND claim_token = ?2",
        )
        .bind(id)
        .bind(token)
        .bind(error_code)
        .bind(now_ms)
        .execute(session.connection())
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::RevisionConflict(id.to_string()));
        }
        Ok(())
    }

    pub(crate) async fn release_for_retry(
        &self,
        session: &mut WriteSession,
        id: &str,
        token: &str,
        error_code: &str,
        retry_not_before_ms: i64,
        now_ms: i64,
        max_attempts: u32,
    ) -> Result<(), PersistenceError> {
        let affected = sqlx::query(
            "UPDATE notification_deliveries SET
                status = CASE WHEN attempt_count >= ?6 THEN 'failed' ELSE 'outcome_unknown' END,
                error_code = CASE WHEN attempt_count >= ?6 THEN ?3 ELSE error_code END,
                outcome_unknown_at_ms = CASE WHEN attempt_count >= ?6 THEN outcome_unknown_at_ms ELSE ?4 END,
                retry_not_before_ms = CASE WHEN attempt_count >= ?6 THEN NULL ELSE ?4 END,
                claim_token = NULL, lease_expires_at_ms = NULL, updated_at_ms = ?5
             WHERE id = ?1 AND status = 'claimed' AND claim_token = ?2",
        )
        .bind(id)
        .bind(token)
        .bind(error_code)
        .bind(retry_not_before_ms)
        .bind(now_ms)
        .bind(i64::from(max_attempts.max(1)))
        .execute(session.connection())
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::RevisionConflict(id.to_string()));
        }
        Ok(())
    }

    pub(crate) async fn suppress_scheduled_for_incident(
        &self,
        session: &mut WriteSession,
        incident_id: &str,
        episode_number: u32,
        reason: SuppressionReason,
        now_ms: i64,
        limit: u32,
    ) -> Result<u64, PersistenceError> {
        let affected = sqlx::query(
            "UPDATE notification_deliveries SET status = 'suppressed', suppressed_reason = ?4,
                    updated_at_ms = ?5
             WHERE id IN (
                 SELECT id FROM notification_deliveries
                 WHERE incident_id = ?1 AND episode_number = ?2 AND status = 'scheduled'
                 ORDER BY scheduled_at_ms ASC, id ASC LIMIT ?3
             )",
        )
        .bind(incident_id)
        .bind(i64::from(episode_number))
        .bind(i64::from(limit.clamp(1, 500)))
        .bind(reason.as_str())
        .bind(now_ms)
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(affected)
    }

    pub(crate) async fn expire_claims(
        &self,
        session: &mut WriteSession,
        now_ms: i64,
        retry_not_before_ms: i64,
        max_attempts: u32,
        limit: u32,
    ) -> Result<u64, PersistenceError> {
        let affected = sqlx::query(
            "UPDATE notification_deliveries SET
                status = CASE WHEN attempt_count >= ?3 THEN 'failed' ELSE 'outcome_unknown' END,
                error_code = CASE WHEN attempt_count >= ?3 THEN 'retry_exhausted' ELSE error_code END,
                outcome_unknown_at_ms = CASE WHEN attempt_count >= ?3 THEN outcome_unknown_at_ms ELSE ?2 END,
                retry_not_before_ms = CASE WHEN attempt_count >= ?3 THEN NULL ELSE ?4 END,
                claim_token = NULL, lease_expires_at_ms = NULL, updated_at_ms = ?2
             WHERE id IN (
                 SELECT id FROM notification_deliveries
                 WHERE status = 'claimed' AND lease_expires_at_ms IS NOT NULL
                   AND lease_expires_at_ms <= ?2
                 ORDER BY lease_expires_at_ms ASC, id ASC LIMIT ?5
             )",
        )
        .bind(now_ms)
        .bind(now_ms)
        .bind(i64::from(max_attempts.max(1)))
        .bind(retry_not_before_ms)
        .bind(i64::from(limit.clamp(1, 500)))
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(affected)
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.delivery-sequence; owner=persistence/alerting; remove_when=repeat delivery scheduling is retired"
    )]
    pub(crate) async fn next_sequence(
        &self,
        session: &mut ReadSession,
        incident_id: &str,
        episode_number: u32,
        channel: NotificationChannel,
        kind: DeliveryKind,
    ) -> Result<u64, PersistenceError> {
        let value = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(delivery_sequence) FROM notification_deliveries
             WHERE incident_id = ?1 AND episode_number = ?2 AND channel = ?3 AND delivery_kind = ?4",
        )
        .bind(incident_id)
        .bind(i64::from(episode_number))
        .bind(channel.as_str())
        .bind(kind.as_str())
        .fetch_one(session.connection())
        .await?
        .unwrap_or(0);
        u64::try_from(value.saturating_add(1))
            .map_err(|_| PersistenceError::InvariantViolation("delivery sequence overflow".into()))
    }

    pub(crate) async fn delete_terminal_before(
        &self,
        session: &mut WriteSession,
        cutoff_ms: i64,
        limit: u32,
    ) -> Result<u64, PersistenceError> {
        let affected = sqlx::query(
            "DELETE FROM notification_deliveries
             WHERE id IN (
                 SELECT id FROM notification_deliveries
                 WHERE created_at_ms < ?1
                   AND status IN ('delivered','suppressed','failed','outcome_unknown')
                 ORDER BY created_at_ms ASC, id ASC LIMIT ?2
             )",
        )
        .bind(cutoff_ms)
        .bind(i64::from(limit.clamp(1, 1_000)))
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(affected)
    }
}

fn observation_kind_str(kind: ObservationKind) -> &'static str {
    match kind {
        ObservationKind::Abnormal => "abnormal",
        ObservationKind::Healthy => "healthy",
        ObservationKind::Change => "change",
    }
}
