use std::sync::Arc;

use crate::{
    ipc::dto::runtime_diagnostics::{
        RuntimeDiagnosticsPageDto, RuntimeDiagnosticsQueryDto, RuntimeEventDto,
    },
    observability::runtime::{
        CorrelationIdRef, InteractionId, RuntimeEvent, RuntimeLogReader, RuntimeLogService,
        StableEventCode,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeDiagnosticsError {
    InvalidFilter,
}

pub(crate) struct RuntimeDiagnosticsService {
    runtime_log: Arc<RuntimeLogService>,
}

impl RuntimeDiagnosticsService {
    pub(crate) fn new(runtime_log: Arc<RuntimeLogService>) -> Self {
        Self { runtime_log }
    }

    pub(crate) fn read_page(
        &self,
        query: RuntimeDiagnosticsQueryDto,
    ) -> Result<RuntimeDiagnosticsPageDto, RuntimeDiagnosticsError> {
        self.read_page_with_limits(
            query,
            crate::observability::runtime::reader::DEFAULT_PAGE_LINES,
            crate::observability::runtime::reader::DEFAULT_PAGE_BYTES,
        )
    }

    pub(crate) fn read_page_with_limits(
        &self,
        query: RuntimeDiagnosticsQueryDto,
        max_lines: usize,
        max_bytes: usize,
    ) -> Result<RuntimeDiagnosticsPageDto, RuntimeDiagnosticsError> {
        let event_code = query
            .event_code
            .as_deref()
            .map(StableEventCode::new)
            .transpose()
            .map_err(|_| RuntimeDiagnosticsError::InvalidFilter)?;
        let correlation_id = query
            .correlation_id
            .as_deref()
            .map(CorrelationIdRef::from_public)
            .transpose()
            .map_err(|_| RuntimeDiagnosticsError::InvalidFilter)?;
        let interaction_id = query
            .interaction_id
            .as_deref()
            .map(InteractionId::from_public)
            .transpose()
            .map_err(|_| RuntimeDiagnosticsError::InvalidFilter)?;

        let reader = RuntimeLogReader::new(self.runtime_log.root());
        let page = reader.read_page_with_cursor(
            query.segment_index,
            query.line_index,
            max_lines,
            max_bytes,
        );
        let mut issue_count = page.issues.len() as u32;
        let mut events = Vec::with_capacity(page.lines.len());
        for line in page.lines {
            let event = match serde_json::from_slice::<RuntimeEvent>(line.as_bytes()) {
                Ok(event) => event,
                Err(_) => {
                    issue_count = issue_count.saturating_add(1);
                    continue;
                }
            };
            let Some(compatibility) =
                crate::observability::runtime::catalog::Catalog::compatibility_for_event(
                    self.runtime_log.root(),
                    line.manifest_id(),
                    &event,
                )
            else {
                issue_count = issue_count.saturating_add(1);
                continue;
            };
            if event_code
                .as_ref()
                .is_some_and(|code| event.event_code != *code)
                || correlation_id
                    .as_ref()
                    .is_some_and(|id| event.correlation_id.as_ref() != Some(id))
                || interaction_id
                    .as_ref()
                    .is_some_and(|id| event.interaction_id.as_ref() != Some(id))
                || query.level.is_some_and(|level| event.level != level)
                || query
                    .component
                    .is_some_and(|component| event.component != component)
            {
                continue;
            }
            events.push(RuntimeEventDto::from_event(event, compatibility));
        }
        let (dropped_count, rejected_count) = self.runtime_log.queue_counters();
        let snapshot = self.runtime_log.snapshot();
        Ok(RuntimeDiagnosticsPageDto {
            events,
            next_segment_index: page.next_segment_index,
            next_line_index: page.next_line_index,
            issue_count,
            sink_degraded: self.runtime_log.state()
                == crate::observability::runtime::RuntimeLogState::Degraded,
            dropped_count,
            rejected_count,
            last_sink_error_code: snapshot.last_sink_error_code.map(str::to_owned),
            clock_stable: snapshot.clock_stable,
            recovery_examined: snapshot.recovery.examined,
            recovery_recovered: snapshot.recovery.recovered,
            recovery_skipped: snapshot.recovery.skipped,
            retention_considered: snapshot.retention.considered,
            retention_deleted: snapshot.retention.deleted,
            retention_skipped_unknown: snapshot.retention.skipped_unknown,
            retention_delete_failures: snapshot.retention.delete_failures,
        })
    }
}

impl RuntimeEventDto {
    fn from_event(
        event: RuntimeEvent,
        compatibility: crate::observability::runtime::catalog::EventCompatibility,
    ) -> Self {
        Self {
            schema_version: event.schema_version,
            at_ms: event.at_ms,
            sequence: event.sequence,
            level: event.level,
            event_code: event.event_code.as_str().to_owned(),
            component: event.component,
            outcome: event.outcome,
            session_id: event.session_id.as_str().to_owned(),
            correlation_id: event
                .correlation_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            interaction_id: event
                .interaction_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            operation_id: event.operation_id.as_ref().map(|id| id.as_str().to_owned()),
            duration_ms: event.duration_ms,
            detail: event.detail,
            error_code: event.error.map(|error| error.code.as_str().to_owned()),
            message_key: compatibility.message_key,
            deprecated_replaced_by: compatibility.replaced_by,
            manifest_source: compatibility.manifest_source,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use sha2::Digest;

    use super::*;
    use crate::observability::runtime::{
        catalog::{Catalog, ManifestLifecycle, OWNER_EVENT_DESCRIPTOR_SLICES},
        event::{EventLevel, EventOutcome, RuntimeDetail},
        sink::SegmentMetadata,
        subject::{SessionId, StableEventCode},
        RuntimeEvent,
    };

    #[test]
    fn rejects_non_canonical_exact_filters_before_reading_disk() {
        let root = tempfile::tempdir().expect("runtime root");
        let service =
            RuntimeDiagnosticsService::new(Arc::new(RuntimeLogService::open(root.path())));
        let invalid = [
            serde_json::json!({ "eventCode": "https://example.test" }),
            serde_json::json!({ "correlationId": "request-id" }),
            serde_json::json!({ "interactionId": "int_invalid" }),
        ];
        for value in invalid {
            let query: RuntimeDiagnosticsQueryDto =
                serde_json::from_value(value).expect("shape is valid; semantic filter must reject");
            assert_eq!(
                service.read_page(query),
                Err(RuntimeDiagnosticsError::InvalidFilter)
            );
        }
    }

    #[test]
    fn unknown_reader_files_are_reported_without_returning_raw_lines() {
        let root = tempfile::tempdir().expect("runtime root");
        std::fs::write(
            root.path().join("runtime-orphan.jsonl"),
            b"secret=not-an-event\n",
        )
        .expect("orphan segment");
        let service =
            RuntimeDiagnosticsService::new(Arc::new(RuntimeLogService::open(root.path())));
        let page = service
            .read_page(RuntimeDiagnosticsQueryDto::default())
            .expect("reader page");
        assert!(page.events.is_empty());
        assert!(page.issue_count >= 1);
    }

    #[test]
    fn maps_deprecated_event_from_previous_manifest_to_replacement() {
        let root = tempfile::tempdir().expect("runtime root");
        let baseline = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("catalog");
        let mut current = baseline.events[0].clone();
        current.code = "runtime.compat.current".to_owned();
        current.message_key = current.code.clone();
        current.lifecycle = ManifestLifecycle::Active;
        let mut deprecated = current.clone();
        deprecated.code = "runtime.compat.previous".to_owned();
        deprecated.message_key = deprecated.code.clone();
        deprecated.lifecycle = ManifestLifecycle::Deprecated {
            replaced_by: current.code.clone(),
            sunset_version: 2,
        };
        let events = vec![current.clone(), deprecated.clone()];
        let unsigned = serde_json::to_vec(&serde_json::json!({
            "manifestVersion": 1,
            "events": events,
        }))
        .expect("unsigned manifest");
        let manifest_id = format!("{:x}", sha2::Sha256::digest(unsigned));
        let manifest = serde_json::json!({
            "manifestVersion": 1,
            "manifestId": manifest_id,
            "events": events,
        });
        fs::write(
            root.path().join("manifest.previous.json"),
            serde_json::to_vec(&manifest).expect("manifest"),
        )
        .expect("previous manifest");

        let event = RuntimeEvent::new(
            1,
            1,
            EventLevel::Info,
            StableEventCode::new("runtime.compat.previous").expect("event code"),
            current.component,
            EventOutcome::Ok,
            SessionId::new(),
            None,
            None,
            None,
            None,
            None,
            None,
            RuntimeDetail::None,
        )
        .expect("event")
        .to_json_line()
        .expect("event json");
        let identity = "legacy";
        fs::write(root.path().join("runtime-legacy-0.jsonl"), event.as_bytes()).expect("segment");
        let metadata = SegmentMetadata {
            schema_version: 1,
            manifest_id: manifest_id.clone(),
            identity: identity.to_owned(),
            generation: 0,
            byte_length: event.len() as u64,
            first_at_ms: 1,
            last_at_ms: 1,
            closed_at_ms: 1,
        };
        fs::write(
            root.path().join("runtime-legacy-0.meta.json"),
            serde_json::to_vec(&metadata).expect("metadata"),
        )
        .expect("metadata file");

        let service =
            RuntimeDiagnosticsService::new(Arc::new(RuntimeLogService::open(root.path())));
        let page = service
            .read_page(RuntimeDiagnosticsQueryDto::default())
            .expect("diagnostics page");
        assert_eq!(page.issue_count, 0);
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].event_code, "runtime.compat.previous");
        assert_eq!(page.events[0].message_key, "runtime.compat.previous");
        assert_eq!(
            page.events[0].deprecated_replaced_by.as_deref(),
            Some("runtime.compat.current")
        );
        assert_eq!(
            page.events[0].manifest_source,
            crate::observability::runtime::catalog::ManifestSource::Previous
        );
    }

    #[test]
    fn excludes_event_code_not_declared_by_the_segment_manifest() {
        let root = tempfile::tempdir().expect("runtime root");
        let service =
            RuntimeDiagnosticsService::new(Arc::new(RuntimeLogService::open(root.path())));
        let event = RuntimeEvent::new(
            1,
            1,
            EventLevel::Warn,
            StableEventCode::new("runtime.unlisted").expect("event code"),
            crate::observability::runtime::Component::Runtime,
            EventOutcome::Ok,
            SessionId::new(),
            None,
            None,
            None,
            None,
            None,
            None,
            RuntimeDetail::None,
        )
        .expect("event")
        .to_json_line()
        .expect("event json");
        fs::write(
            root.path().join("runtime-current-0.jsonl"),
            event.as_bytes(),
        )
        .expect("segment");
        fs::write(
            root.path().join("runtime-current-0.meta.json"),
            serde_json::json!({
                "schemaVersion": 1,
                "manifestId": Catalog::core_manifest_id(),
                "identity": "current",
                "generation": 0,
                "byteLength": event.len(),
                "firstAtMs": 1,
                "lastAtMs": 1,
                "closedAtMs": 1
            })
            .to_string(),
        )
        .expect("metadata");

        let page = service
            .read_page(RuntimeDiagnosticsQueryDto::default())
            .expect("diagnostics page");
        assert!(page.events.is_empty());
        assert_eq!(page.issue_count, 1);
    }
}
