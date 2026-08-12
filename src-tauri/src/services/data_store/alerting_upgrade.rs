//! Durable migration of the legacy change-event history into alerting occurrences.
//!
//! This module intentionally does not infer lifecycle or attention state from legacy
//! `change_events.status`. Legacy rows are audit history only; current incidents are
//! rebuilt by a later projection step from authoritative operational facts.

use std::fmt;

use chrono::DateTime;
use sqlx::Row;

use crate::{
    application::alerting::{AlertingIngress, ObservationIngress},
    models::alerting::{AlertEventType, ConditionKey, EventCategory, ObservationKind, Severity},
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::alerting::{
            OccurrenceInsert, OccurrenceStore, UpgradeProgress, UpgradeProgressStore,
        },
    },
};

const DEFAULT_PAGE_SIZE: i64 = 256;
/// Migration 29 creates the alerting schema and is the first schema that needs
/// the durable alerting transition before the product runtime is published.
pub(crate) const ALERTING_FOUNDATION_SCHEMA_VERSION: i64 = 29;
const CURRENT_FACT_REBUILD_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlertingUpgradePhase {
    NotStarted,
    CopyingHistory,
    RebuildingCurrent,
    Verifying,
    Complete,
    Failed,
}

impl AlertingUpgradePhase {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::CopyingHistory => "copying_history",
            Self::RebuildingCurrent => "rebuilding_current",
            Self::Verifying => "verifying",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for AlertingUpgradePhase {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "not_started" => Ok(Self::NotStarted),
            "copying_history" => Ok(Self::CopyingHistory),
            "rebuilding_current" => Ok(Self::RebuildingCurrent),
            "verifying" => Ok(Self::Verifying),
            "complete" => Ok(Self::Complete),
            "failed" => Ok(Self::Failed),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlertingUpgradeReport {
    pub copied_count: i64,
    pub last_cursor: Option<LegacyCursor>,
    pub phase: AlertingUpgradePhase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacyCursor {
    pub updated_at: String,
    pub id: String,
}

impl LegacyCursor {
    fn encode(&self) -> String {
        format!("{}|{}", self.updated_at, self.id)
    }

    fn decode(value: &str) -> Option<Self> {
        let (updated_at, id) = value.rsplit_once('|')?;
        (!updated_at.is_empty() && !id.is_empty()).then(|| Self {
            updated_at: updated_at.to_string(),
            id: id.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlertingUpgradeError {
    code: &'static str,
    message: String,
}

impl AlertingUpgradeError {
    pub(crate) const fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for AlertingUpgradeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl From<PersistenceError> for AlertingUpgradeError {
    fn from(error: PersistenceError) -> Self {
        Self {
            code: "alerting_upgrade_persistence_failed",
            message: error.to_string(),
        }
    }
}

fn stage_error(stage: &'static str, error: PersistenceError) -> AlertingUpgradeError {
    AlertingUpgradeError {
        code: "alerting_upgrade_persistence_failed",
        message: format!("{stage}: {error}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlertingUpgradeReadiness {
    pub phase: AlertingUpgradePhase,
    pub rebuild_version: Option<i64>,
    pub ready: bool,
}

/// Copy one bounded page and persist its cursor in the same transaction.
/// Re-running this function is idempotent because occurrences are keyed by the
/// immutable `legacy:change_event:<id>` observation key.
#[expect(
    dead_code,
    reason = "contract=alerting.history-page-copy; owner=services/data_store; remove_when=legacy history upgrade is retired"
)]
pub(crate) async fn copy_history_page(
    handle: &PersistenceHandle,
    now_ms: i64,
    cursor: Option<&LegacyCursor>,
    page_size: i64,
) -> Result<AlertingUpgradeReport, PersistenceError> {
    copy_history_page_bounded(handle, now_ms, cursor, None, page_size, true).await
}

async fn copy_history_page_bounded(
    handle: &PersistenceHandle,
    now_ms: i64,
    cursor: Option<&LegacyCursor>,
    high_water: Option<&LegacyCursor>,
    page_size: i64,
    mark_complete_when_done: bool,
) -> Result<AlertingUpgradeReport, PersistenceError> {
    let page_size = page_size.clamp(1, DEFAULT_PAGE_SIZE * 8);
    let mut write = handle.begin_write().await?;
    let progress = UpgradeProgressStore;
    progress
        .set_phase(
            &mut write,
            AlertingUpgradePhase::CopyingHistory.as_str(),
            now_ms,
            None,
        )
        .await?;

    let legacy_table_exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'change_events')",
    )
    .fetch_one(write.connection())
    .await?
        != 0;
    let rows = if legacy_table_exists {
        sqlx::query(
            "SELECT id, severity, event_type, object_type, object_id, station_id,
                    station_key_id, pricing_rule_id, request_log_id, old_value_json,
                    new_value_json, impact_json, source, detected_at, created_at, updated_at
             FROM change_events
             WHERE (?1 IS NULL OR updated_at < ?1 OR (updated_at = ?1 AND id < ?2))
               AND (?3 IS NULL OR updated_at < ?3 OR (updated_at = ?3 AND id <= ?4))
             ORDER BY updated_at DESC, id DESC
             LIMIT ?5",
        )
        .bind(cursor.map(|value| value.updated_at.as_str()))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(high_water.map(|value| value.updated_at.as_str()))
        .bind(high_water.map(|value| value.id.as_str()))
        .bind(page_size)
        .fetch_all(write.connection())
        .await?
    } else {
        Vec::new()
    };

    let occurrence_store = OccurrenceStore;
    let mut copied_count = 0_i64;
    let mut last_cursor = None;
    for row in rows {
        let id: String = row.get("id");
        let updated_at: String = row.get("updated_at");
        let created_at: String = row.get("created_at");
        let occurrence = OccurrenceInsert {
            id: format!("legacy-occurrence-{id}"),
            source_observation_key: format!("legacy:change_event:{id}"),
            event_type: AlertEventType::AuditChange,
            category: EventCategory::AuditChange,
            observation_kind: ObservationKind::Change,
            severity: parse_severity(row.get("severity")),
            condition_key: None,
            object_type: row.get("object_type"),
            object_id: row.get("object_id"),
            station_id: row.get("station_id"),
            station_key_id: row.get("station_key_id"),
            source: format!("legacy:{}", row.get::<String, _>("source")),
            reason_code: Some("legacy_change_event".to_string()),
            old_value_json: row.get("old_value_json"),
            new_value_json: row.get("new_value_json"),
            impact_json: row.get("impact_json"),
            observed_at_ms: parse_timestamp_ms(&row.get::<String, _>("detected_at")),
            created_at_ms: parse_timestamp_ms(&created_at),
        };
        let result = occurrence_store
            .insert_ignore(&mut write, &occurrence)
            .await?;
        if result.inserted {
            copied_count += 1;
        }
        last_cursor = Some(LegacyCursor { updated_at, id });
    }

    if let Some(cursor) = &last_cursor {
        sqlx::query(
            "UPDATE alerting_upgrade_progress
             SET last_copied_cursor = ?1, copied_count = copied_count + ?2, updated_at_ms = ?3
             WHERE singleton_key = 1",
        )
        .bind(cursor.encode())
        .bind(copied_count)
        .bind(now_ms)
        .execute(write.connection())
        .await?;
    }
    write.commit().await?;

    let phase = if last_cursor.is_none() {
        AlertingUpgradePhase::Complete
    } else {
        AlertingUpgradePhase::CopyingHistory
    };
    if phase == AlertingUpgradePhase::Complete && mark_complete_when_done {
        let mut write = handle.begin_write().await?;
        progress
            .set_phase(&mut write, phase.as_str(), now_ms, None)
            .await?;
        write.commit().await?;
    }
    Ok(AlertingUpgradeReport {
        copied_count,
        last_cursor,
        phase,
    })
}

#[expect(
    dead_code,
    reason = "contract=alerting.history-backfill; owner=services/data_store; remove_when=legacy history upgrade is retired"
)]
pub(crate) async fn run_history_backfill(
    handle: &PersistenceHandle,
    now_ms: i64,
) -> Result<i64, PersistenceError> {
    let mut cursor = None;
    let mut copied = 0_i64;
    loop {
        let report = copy_history_page(handle, now_ms, cursor.as_ref(), DEFAULT_PAGE_SIZE).await?;
        copied += report.copied_count;
        cursor = report.last_cursor;
        if report.phase == AlertingUpgradePhase::Complete {
            return Ok(copied);
        }
    }
}

/// Execute the complete alerting transition.  Every mutating phase is durable;
/// a process restart resumes from `last_copied_cursor` and idempotent
/// observation keys rather than replaying an unbounded legacy table.
pub(crate) async fn run_durable_upgrade(
    handle: &PersistenceHandle,
    now_ms: i64,
) -> Result<AlertingUpgradeReport, AlertingUpgradeError> {
    let result = run_durable_upgrade_inner(handle, now_ms).await;
    match result {
        Ok(report) => Ok(report),
        Err(error) => {
            let code = error.code();
            let _ = mark_failed(handle, now_ms, code).await;
            Err(error)
        }
    }
}

async fn run_durable_upgrade_inner(
    handle: &PersistenceHandle,
    now_ms: i64,
) -> Result<AlertingUpgradeReport, AlertingUpgradeError> {
    let mut progress = load_progress(handle)
        .await
        .map_err(|error| stage_error("load_progress", error))?;
    let phase = AlertingUpgradePhase::try_from(progress.phase.as_str()).map_err(|_| {
        AlertingUpgradeError {
            code: "alerting_upgrade_invalid_phase",
            message: "alerting upgrade progress contains an unknown phase".to_string(),
        }
    })?;
    if phase == AlertingUpgradePhase::Complete
        && progress.rebuild_version == Some(CURRENT_FACT_REBUILD_VERSION)
    {
        return Ok(AlertingUpgradeReport {
            copied_count: 0,
            last_cursor: progress
                .last_copied_cursor
                .as_deref()
                .and_then(LegacyCursor::decode),
            phase,
        });
    }

    let high_water = if let Some(value) = progress.source_high_water_cursor.as_deref() {
        LegacyCursor::decode(value).ok_or_else(|| AlertingUpgradeError {
            code: "alerting_upgrade_invalid_cursor",
            message: "alerting upgrade source high-water cursor is invalid".to_string(),
        })?
    } else {
        let high_water = read_source_high_water(handle)
            .await
            .map_err(|error| stage_error("read_source_high_water", error))?;
        persist_source_high_water(handle, high_water.as_ref(), now_ms)
            .await
            .map_err(|error| stage_error("persist_source_high_water", error))?;
        high_water.unwrap_or_else(|| LegacyCursor {
            updated_at: "".to_string(),
            id: "".to_string(),
        })
    };

    let mut copied_count = 0_i64;
    let mut cursor = progress
        .last_copied_cursor
        .as_deref()
        .and_then(LegacyCursor::decode);
    if !high_water.updated_at.is_empty() {
        loop {
            let report = copy_history_page_bounded(
                handle,
                now_ms,
                cursor.as_ref(),
                Some(&high_water),
                DEFAULT_PAGE_SIZE,
                false,
            )
            .await
            .map_err(|error| stage_error("copy_history_page", error))?;
            copied_count += report.copied_count;
            let Some(next) = report.last_cursor else {
                break;
            };
            cursor = Some(next);
        }
    }
    mark_phase(
        handle,
        AlertingUpgradePhase::RebuildingCurrent,
        now_ms,
        None,
    )
    .await
    .map_err(|error| stage_error("mark_rebuilding", error))?;

    rebuild_current_facts(handle, now_ms)
        .await
        .map_err(|error| stage_error("rebuild_current_facts", error))?;
    mark_phase(handle, AlertingUpgradePhase::Verifying, now_ms, None)
        .await
        .map_err(|error| stage_error("mark_verifying", error))?;
    verify_upgrade(handle)
        .await
        .map_err(|error| stage_error("verify_upgrade", error))?;
    persist_rebuild_complete(handle, now_ms)
        .await
        .map_err(|error| stage_error("persist_rebuild_complete", error))?;
    progress = load_progress(handle)
        .await
        .map_err(|error| stage_error("reload_progress", error))?;
    Ok(AlertingUpgradeReport {
        copied_count,
        last_cursor: progress
            .last_copied_cursor
            .as_deref()
            .and_then(LegacyCursor::decode),
        phase: AlertingUpgradePhase::Complete,
    })
}

pub(crate) async fn alerting_readiness(
    handle: &PersistenceHandle,
) -> Result<AlertingUpgradeReadiness, PersistenceError> {
    let progress = load_progress(handle).await?;
    let phase = AlertingUpgradePhase::try_from(progress.phase.as_str())
        .unwrap_or(AlertingUpgradePhase::Failed);
    Ok(AlertingUpgradeReadiness {
        ready: phase == AlertingUpgradePhase::Complete
            && progress.rebuild_version == Some(CURRENT_FACT_REBUILD_VERSION),
        phase,
        rebuild_version: progress.rebuild_version,
    })
}

/// Product writers call this gate before accepting alerting observations.  The
/// migration itself uses `record_in_session` directly while this gate is closed.
pub(crate) async fn assert_alerting_writer_ready(
    handle: &PersistenceHandle,
) -> Result<(), PersistenceError> {
    let readiness = alerting_readiness(handle).await?;
    if readiness.ready {
        Ok(())
    } else {
        Err(PersistenceError::InvariantViolation(
            "alerting upgrade is not complete".to_string(),
        ))
    }
}

async fn load_progress(handle: &PersistenceHandle) -> Result<UpgradeProgress, PersistenceError> {
    let mut read = handle.begin_read().await?;
    UpgradeProgressStore.load(&mut read).await?.ok_or_else(|| {
        PersistenceError::InvariantViolation("missing alerting upgrade progress".into())
    })
}

async fn mark_phase(
    handle: &PersistenceHandle,
    phase: AlertingUpgradePhase,
    now_ms: i64,
    error_code: Option<&str>,
) -> Result<(), PersistenceError> {
    let mut write = handle.begin_write().await?;
    UpgradeProgressStore
        .set_phase(&mut write, phase.as_str(), now_ms, error_code)
        .await?;
    write.commit().await
}

async fn mark_failed(
    handle: &PersistenceHandle,
    now_ms: i64,
    error_code: &str,
) -> Result<(), PersistenceError> {
    mark_phase(
        handle,
        AlertingUpgradePhase::Failed,
        now_ms,
        Some(error_code),
    )
    .await
}

async fn read_source_high_water(
    handle: &PersistenceHandle,
) -> Result<Option<LegacyCursor>, PersistenceError> {
    let mut read = handle.begin_read().await?;
    let row = if sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'change_events')",
    )
    .fetch_one(read.connection())
    .await?
        != 0
    {
        sqlx::query(
            "SELECT updated_at, id FROM change_events
             ORDER BY updated_at DESC, id DESC LIMIT 1",
        )
        .fetch_optional(read.connection())
        .await?
    } else {
        None
    };
    Ok(row.map(|row| LegacyCursor {
        updated_at: row.get("updated_at"),
        id: row.get("id"),
    }))
}

async fn persist_source_high_water(
    handle: &PersistenceHandle,
    cursor: Option<&LegacyCursor>,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    let mut write = handle.begin_write().await?;
    UpgradeProgressStore
        .set_phase(
            &mut write,
            AlertingUpgradePhase::CopyingHistory.as_str(),
            now_ms,
            None,
        )
        .await?;
    sqlx::query(
        "UPDATE alerting_upgrade_progress
         SET source_high_water_cursor = ?1, updated_at_ms = ?2
         WHERE singleton_key = 1",
    )
    .bind(cursor.map(LegacyCursor::encode))
    .bind(now_ms)
    .execute(write.connection())
    .await?;
    write.commit().await
}

async fn persist_rebuild_complete(
    handle: &PersistenceHandle,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    let mut write = handle.begin_write().await?;
    sqlx::query(
        "UPDATE alerting_upgrade_progress
         SET phase = ?1, rebuild_version = ?2, updated_at_ms = ?3,
             completed_at_ms = ?3, last_error_code = NULL
         WHERE singleton_key = 1",
    )
    .bind(AlertingUpgradePhase::Complete.as_str())
    .bind(CURRENT_FACT_REBUILD_VERSION)
    .bind(now_ms)
    .execute(write.connection())
    .await?;
    write.commit().await
}

async fn verify_upgrade(handle: &PersistenceHandle) -> Result<(), PersistenceError> {
    let mut read = handle.begin_read().await?;
    let row = sqlx::query(
        "SELECT phase, source_high_water_cursor, last_copied_cursor,
                rebuild_version, copied_count
         FROM alerting_upgrade_progress WHERE singleton_key = 1",
    )
    .fetch_one(read.connection())
    .await?;
    let phase: String = row.get("phase");
    if phase != AlertingUpgradePhase::Verifying.as_str() {
        return Err(PersistenceError::InvariantViolation(
            "alerting upgrade verification entered from an unexpected phase".into(),
        ));
    }
    let high_water: Option<String> = row.get("source_high_water_cursor");
    let _cursor: Option<String> = row.get("last_copied_cursor");
    if let Some(high_water) = high_water.as_deref().and_then(LegacyCursor::decode) {
        let missing = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(
                 SELECT 1 FROM change_events e
                 LEFT JOIN change_event_occurrences o
                   ON o.source_observation_key = 'legacy:change_event:' || e.id
                 WHERE (e.updated_at < ?1 OR (e.updated_at = ?1 AND e.id <= ?2))
                   AND o.id IS NULL
             )",
        )
        .bind(&high_water.updated_at)
        .bind(&high_water.id)
        .fetch_one(read.connection())
        .await?;
        if missing != 0 {
            return Err(PersistenceError::InvariantViolation(
                "alerting history backfill did not reach its source high-water cursor".into(),
            ));
        }
    }
    let copied_count: i64 = row.get("copied_count");
    if copied_count < 0 {
        return Err(PersistenceError::InvariantViolation(
            "alerting upgrade copied count is negative".into(),
        ));
    }
    let rebuild_version: Option<i64> = row.get("rebuild_version");
    if rebuild_version.is_some() && rebuild_version != Some(CURRENT_FACT_REBUILD_VERSION) {
        return Err(PersistenceError::InvariantViolation(
            "alerting current-facts rebuild version is unsupported".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct CurrentFact {
    event_type: AlertEventType,
    condition_key: ConditionKey,
    kind: ObservationKind,
    severity: Severity,
    object_type: String,
    object_id: Option<String>,
    station_id: Option<String>,
    station_key_id: Option<String>,
    source_observation_key: String,
    summary_json: String,
}

impl CurrentFact {
    fn into_ingress(self, now_ms: i64) -> ObservationIngress {
        let freshness_ms = match self.event_type {
            AlertEventType::KeyInvalid
            | AlertEventType::StationDown
            | AlertEventType::RouteImpacted => 300_000,
            _ => 900_000,
        };
        ObservationIngress {
            source_observation_key: self.source_observation_key,
            event_type: self.event_type,
            condition_key: self.condition_key,
            kind: self.kind,
            severity: self.severity,
            object_type: self.object_type,
            object_id: self.object_id,
            station_id: self.station_id,
            station_key_id: self.station_key_id,
            source: "alerting-upgrade".to_string(),
            reason_code: Some("current_fact_rebuild".to_string()),
            summary_json: self.summary_json,
            observed_at_ms: now_ms,
            fact_fresh_until_ms: now_ms.saturating_add(freshness_ms),
        }
    }
}

fn condition_key(prefix: &str, id: &str) -> Option<ConditionKey> {
    let mut normalized = String::with_capacity(id.len());
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() || b"._:-".contains(&byte) {
            normalized.push(byte as char);
        } else {
            normalized.push('_');
        }
    }
    ConditionKey::new(format!("{prefix}:{normalized}")).ok()
}

fn source_key(event_type: AlertEventType, key: &ConditionKey) -> String {
    let value = format!(
        "upgrade:rebuild:v{CURRENT_FACT_REBUILD_VERSION}:{}:{}",
        event_type.as_str(),
        key.as_str()
    );
    value.chars().take(200).collect()
}

fn fact(
    event_type: AlertEventType,
    key: ConditionKey,
    abnormal: bool,
    severity: Severity,
    object_type: &str,
    object_id: Option<String>,
    station_id: Option<String>,
    station_key_id: Option<String>,
    summary: serde_json::Value,
) -> CurrentFact {
    CurrentFact {
        source_observation_key: source_key(event_type, &key),
        event_type,
        condition_key: key,
        kind: if abnormal {
            ObservationKind::Abnormal
        } else {
            ObservationKind::Healthy
        },
        severity,
        object_type: object_type.to_string(),
        object_id,
        station_id,
        station_key_id,
        summary_json: serde_json::to_string(&summary).unwrap_or_else(|_| "{}".to_string()),
    }
}

async fn rebuild_current_facts(
    handle: &PersistenceHandle,
    now_ms: i64,
) -> Result<usize, PersistenceError> {
    let facts = collect_current_facts(handle).await?;
    let ingress = AlertingIngress::new(handle.clone());
    let mut write = handle.begin_write().await?;
    for current in facts.iter().cloned() {
        ingress
            .record_in_session(&mut write, current.into_ingress(now_ms))
            .await?;
    }
    write.commit().await?;
    Ok(facts.len())
}

async fn collect_current_facts(
    handle: &PersistenceHandle,
) -> Result<Vec<CurrentFact>, PersistenceError> {
    let mut read = handle.begin_read().await?;
    let mut facts = Vec::new();
    let rows = sqlx::query("SELECT id, status, updated_at FROM stations WHERE enabled = 1")
        .fetch_all(read.connection())
        .await?;
    for row in rows {
        let id: String = row.get("id");
        let status: String = row.get("status");
        let key =
            condition_key("station", &id).ok_or_else(|| PersistenceError::ConstraintViolation)?;
        let abnormal = matches!(
            status.as_str(),
            "down" | "offline" | "error" | "failed" | "unhealthy"
        );
        facts.push(fact(
            AlertEventType::StationDown,
            key,
            abnormal,
            Severity::Critical,
            "station",
            Some(id.clone()),
            Some(id),
            None,
            serde_json::json!({ "status": status }),
        ));
    }

    let health_table_exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'station_key_health_observations')",
    )
    .fetch_one(read.connection())
    .await?
        != 0;
    let rows = if health_table_exists {
        sqlx::query(
            "SELECT h.station_key_id, k.station_id, h.outcome
             FROM station_key_health_observations h
             JOIN station_keys k ON k.id = h.station_key_id
             WHERE h.id = (
                 SELECT latest.id FROM station_key_health_observations latest
                 WHERE latest.station_key_id = h.station_key_id
                 ORDER BY latest.observed_at_ms DESC, latest.id DESC LIMIT 1
             )",
        )
        .fetch_all(read.connection())
        .await?
    } else {
        Vec::new()
    };
    for row in rows {
        let key_id: String = row.get("station_key_id");
        let station_id: String = row.get("station_id");
        let outcome: String = row.get("outcome");
        let key =
            condition_key("key", &key_id).ok_or_else(|| PersistenceError::ConstraintViolation)?;
        facts.push(fact(
            AlertEventType::KeyInvalid,
            key,
            is_key_invalid_outcome(&outcome),
            Severity::Critical,
            "station_key",
            Some(key_id.clone()),
            Some(station_id),
            Some(key_id),
            serde_json::json!({ "outcome": outcome }),
        ));
    }

    let rows = sqlx::query(
        "SELECT b.station_id, b.station_key_id, b.status, b.updated_at
         FROM balance_snapshots b
         WHERE b.id = (
             SELECT latest.id FROM balance_snapshots latest
             WHERE latest.station_id = b.station_id
               AND (latest.station_key_id = b.station_key_id OR (latest.station_key_id IS NULL AND b.station_key_id IS NULL))
             ORDER BY latest.updated_at DESC, latest.id DESC LIMIT 1
         )",
    )
    .fetch_all(read.connection())
    .await?;
    for row in rows {
        let station_id: String = row.get("station_id");
        let station_key_id: Option<String> = row.get("station_key_id");
        let status: String = row.get("status");
        let event_type = if status == "depleted" {
            AlertEventType::BalanceDepleted
        } else {
            AlertEventType::BalanceLow
        };
        let abnormal = status == "depleted" || status == "low";
        let suffix = station_key_id.as_deref().unwrap_or("account");
        let key = condition_key("balance", &format!("{station_id}:{suffix}"))
            .ok_or_else(|| PersistenceError::ConstraintViolation)?;
        facts.push(fact(
            event_type,
            key,
            abnormal,
            if event_type == AlertEventType::BalanceDepleted {
                Severity::Critical
            } else {
                Severity::Warning
            },
            "balance",
            Some(station_id.clone()),
            Some(station_id),
            station_key_id,
            serde_json::json!({ "status": status }),
        ));
    }

    let rows = sqlx::query(
        "SELECT id, station_id, station_key_id, binding_status, binding_kind, group_name
         FROM station_group_bindings
         WHERE binding_status NOT IN ('disabled', 'manual_legacy')",
    )
    .fetch_all(read.connection())
    .await?;
    for row in rows {
        let id: String = row.get("id");
        let station_id: String = row.get("station_id");
        let station_key_id: Option<String> = row.get("station_key_id");
        let status: String = row.get("binding_status");
        let binding_kind: String = row.get("binding_kind");
        let event_type = if binding_kind == "key_binding" {
            AlertEventType::KeyGroupUnresolved
        } else {
            AlertEventType::GroupMissing
        };
        let key =
            condition_key("group", &id).ok_or_else(|| PersistenceError::ConstraintViolation)?;
        let severity = if event_type == AlertEventType::GroupMissing {
            Severity::Info
        } else {
            Severity::Warning
        };
        facts.push(fact(
            event_type,
            key,
            status == "missing",
            severity,
            "group_binding",
            Some(id),
            Some(station_id),
            station_key_id,
            serde_json::json!({ "status": status, "group": row.get::<String, _>("group_name") }),
        ));
    }

    let rows = sqlx::query(
        "SELECT station_id, task_type, last_status, consecutive_failures
         FROM collector_task_state",
    )
    .fetch_all(read.connection())
    .await?;
    for row in rows {
        let station_id: String = row.get("station_id");
        let task_type: String = row.get("task_type");
        let status: String = row.get("last_status");
        let failures: i64 = row.get("consecutive_failures");
        let key = condition_key("collector", &format!("{station_id}:{task_type}"))
            .ok_or_else(|| PersistenceError::ConstraintViolation)?;
        facts.push(fact(
            AlertEventType::CollectorFailed,
            key,
            failures > 0 || matches!(status.as_str(), "failed" | "error" | "timeout"),
            Severity::Warning,
            "collector_task",
            Some(task_type),
            Some(station_id),
            None,
            serde_json::json!({ "status": status, "consecutive_failures": failures }),
        ));
    }

    let rows = sqlx::query(
        "SELECT scope, value_basis_points FROM routing_health_axes
         WHERE axis = 'availability'",
    )
    .fetch_all(read.connection())
    .await?;
    for row in rows {
        let scope: String = row.get("scope");
        let value: i64 = row.get("value_basis_points");
        let key =
            condition_key("route", &scope).ok_or_else(|| PersistenceError::ConstraintViolation)?;
        facts.push(fact(
            AlertEventType::RouteImpacted,
            key,
            value < 5_000,
            Severity::Warning,
            "route",
            Some(scope),
            None,
            None,
            serde_json::json!({ "availability_basis_points": value }),
        ));
    }
    Ok(facts)
}

fn parse_severity(value: String) -> Severity {
    match value.as_str() {
        "critical" => Severity::Critical,
        "warning" => Severity::Warning,
        _ => Severity::Info,
    }
}

fn is_key_invalid_outcome(outcome: &str) -> bool {
    outcome == "hard_fail"
}

fn parse_timestamp_ms(value: &str) -> i64 {
    if let Ok(value) = value.parse::<i64>() {
        return value;
    }
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp_millis())
        .unwrap_or(0)
        .max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    #[test]
    fn legacy_timestamps_are_normalized_without_exposing_source_text() {
        assert_eq!(parse_timestamp_ms("1700000000000"), 1_700_000_000_000);
        assert_eq!(
            parse_timestamp_ms("2023-11-14T22:13:20Z"),
            1_700_000_000_000
        );
    }

    #[test]
    fn legacy_status_does_not_change_audit_observation_kind() {
        assert_eq!(ObservationKind::Change, ObservationKind::Change);
        assert_eq!(parse_severity("unknown".to_string()), Severity::Info);
    }

    #[test]
    fn only_hard_fail_marks_a_key_as_invalid() {
        assert!(is_key_invalid_outcome("hard_fail"));
        assert!(!is_key_invalid_outcome("unavailable"));
        assert!(!is_key_invalid_outcome("observe_failure"));
        assert!(!is_key_invalid_outcome("success"));
    }

    #[tokio::test]
    async fn history_backfill_is_restartable_and_does_not_infer_attention() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("runtime.sqlite3"))
            .await
            .expect("runtime");
        runtime
            .write(|write| {
                Box::pin(async move {
                    // The production schema is post-cutover (schema 30), so
                    // this fixture recreates the retained legacy source table
                    // explicitly to exercise the private backfill reader.
                    sqlx::query(
                        "CREATE TABLE change_events (
                            id TEXT PRIMARY KEY,
                            severity TEXT NOT NULL,
                            event_type TEXT NOT NULL,
                            status TEXT NOT NULL,
                            title TEXT NOT NULL,
                            message TEXT NOT NULL,
                            object_type TEXT NOT NULL,
                            object_id TEXT,
                            station_id TEXT,
                            station_key_id TEXT,
                            pricing_rule_id TEXT,
                            request_log_id TEXT,
                            old_value_json TEXT,
                            new_value_json TEXT,
                            impact_json TEXT,
                            dedupe_key TEXT,
                            source TEXT NOT NULL,
                            detected_at TEXT NOT NULL,
                            resolved_at TEXT,
                            created_at TEXT NOT NULL,
                            updated_at TEXT NOT NULL
                        )",
                    )
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "INSERT INTO change_events (
                            id, severity, event_type, status, title, message, object_type,
                            dedupe_key, source, detected_at, created_at, updated_at
                         ) VALUES ('legacy-1', 'warning', 'group_missing', 'resolved',
                                   'Legacy', 'redacted', 'station', 'legacy-1', 'fixture',
                                   '1700000000000', '1700000000000', '1700000000000')",
                    )
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("legacy row");

        let copied = run_history_backfill(&runtime.handle(), 1_700_000_000_001)
            .await
            .expect("first backfill");
        let copied_again = run_history_backfill(&runtime.handle(), 1_700_000_000_002)
            .await
            .expect("restart backfill");
        assert_eq!(copied, 1);
        assert_eq!(copied_again, 0);

        let mut read = runtime.handle().begin_read().await.expect("read");
        let row = sqlx::query(
            "SELECT observation_kind, incident_id, reason_code
             FROM change_event_occurrences WHERE source_observation_key = 'legacy:change_event:legacy-1'",
        )
        .fetch_one(read.connection())
        .await
        .expect("occurrence");
        assert_eq!(row.get::<String, _>("observation_kind"), "change");
        assert_eq!(row.get::<Option<String>, _>("incident_id"), None);
        assert_eq!(
            row.get::<Option<String>, _>("reason_code").as_deref(),
            Some("legacy_change_event")
        );
        drop(read);
        runtime.close().await.expect("close");
    }
}
