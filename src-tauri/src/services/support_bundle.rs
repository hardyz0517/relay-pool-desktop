//! Local, opt-in support bundle generation.
//!
//! The service consumes only the validated runtime reader. It writes a small
//! directory bundle through a sibling temporary directory and publishes it by
//! one rename, so cancellation or a failed canary never leaves a partial
//! bundle that looks complete.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    application::runtime_diagnostics::RuntimeDiagnosticsService,
    ipc::dto::runtime_diagnostics::{RuntimeDiagnosticsPageDto, RuntimeDiagnosticsQueryDto},
    observability::runtime::RuntimeLogService,
};

const MAX_EVENT_BYTES: usize = 10 * 1024 * 1024;
const MAX_EVENT_COUNT: usize = 10_000;

#[derive(Debug, thiserror::Error)]
pub(crate) enum SupportBundleError {
    #[error("support bundle destination is invalid")]
    InvalidDestination,
    #[error("support bundle already exists")]
    AlreadyExists,
    #[error("support bundle I/O failed")]
    Io(#[from] io::Error),
    #[error("support bundle contains unsafe data")]
    UnsafeData,
    #[error("support bundle serialization failed")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SupportBundleReport {
    pub event_count: u32,
    pub issue_count: u32,
}

pub(crate) struct SupportBundleService;

impl SupportBundleService {
    pub(crate) fn export(
        runtime_log: &RuntimeLogService,
        destination: impl AsRef<Path>,
    ) -> Result<SupportBundleReport, SupportBundleError> {
        // Export is a diagnostic snapshot. Drain the bounded writer first so
        // events accepted by the producer queue are not silently omitted.
        runtime_log.flush();
        let destination = validate_destination(destination.as_ref())?;
        if destination.exists() {
            return Err(SupportBundleError::AlreadyExists);
        }
        let parent = destination
            .parent()
            .ok_or(SupportBundleError::InvalidDestination)?;
        fs::create_dir_all(parent)?;
        let temp = parent.join(format!(
            ".{}.partial",
            destination
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or(SupportBundleError::InvalidDestination)?
        ));
        if temp.exists() {
            return Err(SupportBundleError::AlreadyExists);
        }

        let result = Self::write_temp(runtime_log, &temp);
        match result {
            Ok(report) => {
                match fs::rename(&temp, &destination) {
                    Ok(()) => Ok(report),
                    Err(error) => {
                        // A failed publish must not leave a directory that
                        // looks like a complete support bundle. Best-effort
                        // cleanup is safe because `temp` is owned by this
                        // export attempt and is never a caller path.
                        let _ = fs::remove_dir_all(&temp);
                        Err(SupportBundleError::Io(error))
                    }
                }
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&temp);
                Err(error)
            }
        }
    }

    fn write_temp(
        runtime_log: &RuntimeLogService,
        temp: &Path,
    ) -> Result<SupportBundleReport, SupportBundleError> {
        fs::create_dir(temp)?;
        let diagnostics = RuntimeDiagnosticsService::new(std::sync::Arc::new(runtime_log.clone()));
        let mut query = RuntimeDiagnosticsQueryDto::default();
        let mut events = Vec::new();
        let mut issue_count = 0u32;
        let mut guard = 0usize;
        loop {
            let page = diagnostics
                .read_page_with_limits(query.clone(), MAX_EVENT_COUNT, MAX_EVENT_BYTES)
                .map_err(|_| SupportBundleError::UnsafeData)?;
            append_page(&mut events, &page)?;
            issue_count = issue_count.saturating_add(page.issue_count);
            guard = guard.saturating_add(1);
            let Some(next_segment) = page.next_segment_index else {
                break;
            };
            let Some(next_line) = page.next_line_index else {
                // A next segment without a line cursor is not a valid
                // resumable page and could cause replay or omission.
                return Err(SupportBundleError::UnsafeData);
            };
            if (next_segment, next_line) <= (query.segment_index, query.line_index) {
                // Refuse to publish a bundle when a cursor cannot advance,
                // rather than silently duplicating or omitting events.
                return Err(SupportBundleError::UnsafeData);
            }
            if guard > 1024 {
                return Err(SupportBundleError::UnsafeData);
            }
            query.segment_index = next_segment;
            query.line_index = next_line;
        }

        let runtime_events = events.join("\n") + if events.is_empty() { "" } else { "\n" };
        if runtime_events.len() > MAX_EVENT_BYTES || events.len() > MAX_EVENT_COUNT {
            return Err(SupportBundleError::UnsafeData);
        }
        canary_scan(runtime_events.as_bytes())?;
        fs::write(temp.join("runtime-events.jsonl"), runtime_events.as_bytes())?;
        let snapshot = runtime_log.snapshot();
        let summary = serde_json::json!({
            "formatVersion": 1,
            "eventCount": events.len(),
            "issueCount": issue_count,
            "sinkDegraded": snapshot.state == crate::observability::runtime::RuntimeLogState::Degraded,
            "droppedCount": snapshot.dropped_count,
            "rejectedCount": snapshot.rejected_count,
            "lastSinkErrorCode": snapshot.last_sink_error_code,
            "clockStable": snapshot.clock_stable,
            "recovery": {
                "examined": snapshot.recovery.examined,
                "recovered": snapshot.recovery.recovered,
                "skipped": snapshot.recovery.skipped,
                "bytesExamined": snapshot.recovery.bytes_examined,
            },
            "retention": {
                "considered": snapshot.retention.considered,
                "deleted": snapshot.retention.deleted,
                "bytesDeleted": snapshot.retention.bytes_deleted,
                "skippedActive": snapshot.retention.skipped_active,
                "skippedUnknown": snapshot.retention.skipped_unknown,
                "deleteFailures": snapshot.retention.delete_failures,
                "ageDeletionPaused": snapshot.retention.age_deletion_paused,
            },
        });
        let summary_bytes = serde_json::to_vec_pretty(&summary)?;
        canary_scan(&summary_bytes)?;
        fs::write(temp.join("runtime-summary.json"), summary_bytes)?;
        let manifest = serde_json::json!({
            "formatVersion": 1,
            "contents": ["manifest.json", "runtime-summary.json", "runtime-events.jsonl"],
        });
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        canary_scan(&manifest_bytes)?;
        fs::write(temp.join("manifest.json"), manifest_bytes)?;
        Ok(SupportBundleReport {
            event_count: events.len().min(u32::MAX as usize) as u32,
            issue_count,
        })
    }
}

fn append_page(
    events: &mut Vec<String>,
    page: &RuntimeDiagnosticsPageDto,
) -> Result<(), SupportBundleError> {
    for event in &page.events {
        if events.len() >= MAX_EVENT_COUNT {
            return Err(SupportBundleError::UnsafeData);
        }
        let bytes = serde_json::to_vec(event)?;
        canary_scan(&bytes)?;
        events.push(String::from_utf8(bytes).map_err(|_| SupportBundleError::UnsafeData)?);
    }
    Ok(())
}

fn validate_destination(path: &Path) -> Result<PathBuf, SupportBundleError> {
    // The save dialog normally returns an absolute path, but the service is
    // also exercised directly by tests and future non-UI callers. Reject
    // parent/current components so a caller cannot redirect the atomic rename
    // outside the explicitly selected directory.
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(SupportBundleError::InvalidDestination);
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(SupportBundleError::InvalidDestination)?;
    if name.is_empty()
        || name.len() > 96
        || !name.is_ascii()
        || name == "."
        || name == ".."
        || name.contains(['\\', '/', ':'])
    {
        return Err(SupportBundleError::InvalidDestination);
    }
    Ok(path.to_path_buf())
}

fn canary_scan(bytes: &[u8]) -> Result<(), SupportBundleError> {
    let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    for marker in [
        "sk-",
        "authorization",
        "cookie",
        "password",
        "token=",
        "begin private key",
        "sqlite",
        "\\\\",
    ] {
        if text.contains(marker) {
            return Err(SupportBundleError::UnsafeData);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::runtime::{Component, EventLevel, EventOutcome, RuntimeDetail};

    #[test]
    fn publishes_only_allowlisted_bundle_files() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = RuntimeLogService::open(root.path());
        service.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        let destination = root.path().join("bundle");
        let report = SupportBundleService::export(&service, &destination).expect("bundle");
        assert_eq!(report.event_count, 1);
        let mut names = fs::read_dir(&destination)
            .expect("bundle directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            vec![
                "manifest.json",
                "runtime-events.jsonl",
                "runtime-summary.json"
            ]
        );
    }

    #[test]
    fn rejects_existing_destination_and_traversal_name() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = RuntimeLogService::open(root.path());
        let existing = root.path().join("existing");
        fs::create_dir(&existing).expect("existing destination");
        assert!(matches!(
            SupportBundleService::export(&service, &existing),
            Err(SupportBundleError::AlreadyExists)
        ));
        assert!(matches!(
            SupportBundleService::export(&service, root.path().join("..")),
            Err(SupportBundleError::InvalidDestination)
        ));
        assert!(matches!(
            SupportBundleService::export(
                &service,
                root.path().join("nested").join("..").join("bundle")
            ),
            Err(SupportBundleError::InvalidDestination)
        ));
    }

    #[test]
    fn canary_rejects_sensitive_serialized_content() {
        // The canary is intentionally unit-tested at the serialization
        // boundary; RuntimeEvent itself cannot contain secret-looking free
        // text by construction.
        assert!(matches!(
            canary_scan(br#"{"detail":"authorization: bearer"}"#),
            Err(SupportBundleError::UnsafeData)
        ));
    }

    #[test]
    fn export_cleans_temporary_directory_when_event_limit_is_exceeded() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = RuntimeLogService::open(root.path());
        for index in 0..=MAX_EVENT_COUNT {
            service.record(
                "runtime.log_event.dropped",
                Component::Runtime,
                EventLevel::Warn,
                EventOutcome::Ok,
                RuntimeDetail::None,
            );
            // The production writer deliberately uses a bounded queue. Flush
            // in batches so this fixture exercises the bundle event limit,
            // rather than queue overflow dropping most of its input.
            if index % 128 == 127 {
                service.flush();
            }
        }
        service.flush();
        let destination = root.path().join("too-many-events");
        assert!(matches!(
            SupportBundleService::export(&service, &destination),
            Err(SupportBundleError::UnsafeData)
        ));
        assert!(!destination.exists());
        assert!(!root.path().join(".too-many-events.partial").exists());
    }
}
