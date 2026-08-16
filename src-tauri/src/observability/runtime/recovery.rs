//! Bounded salvage of abandoned partial segments.

use std::{
    fs,
    io::{self, BufRead, BufReader},
    path::Path,
};

use super::{event::RuntimeEvent, lease::RuntimeLogLease, sink::RuntimeLogWriter, Catalog};

#[derive(Debug, Clone, Copy)]
pub struct RecoveryConfig {
    pub max_files: usize,
    pub max_bytes: u64,
    pub max_segment_bytes: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_files: 8,
            max_bytes: 8 * 1024 * 1024,
            max_segment_bytes: super::sink::DEFAULT_MAX_SEGMENT_BYTES,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryReport {
    pub examined: usize,
    pub recovered: usize,
    pub skipped: usize,
    pub bytes_examined: u64,
}

pub fn recover_partials(
    root: &Path,
    lease: &RuntimeLogLease,
    config: RecoveryConfig,
) -> RecoveryReport {
    let mut report = RecoveryReport::default();
    let Ok(entries) = fs::read_dir(root) else {
        return report;
    };
    let current_prefix = format!("runtime-{}-", lease.identity());
    for entry in entries.flatten() {
        if report.examined >= config.max_files || report.bytes_examined >= config.max_bytes {
            break;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("runtime-")
            || !name.ends_with(".jsonl.partial")
            || name.starts_with(&current_prefix)
        {
            continue;
        }
        report.examined += 1;
        let Ok(file_size) = fs::metadata(&path).map(|metadata| metadata.len()) else {
            report.skipped += 1;
            continue;
        };
        if file_size > config.max_bytes.saturating_sub(report.bytes_examined) {
            report.skipped += 1;
            continue;
        }
        report.bytes_examined = report.bytes_examined.saturating_add(file_size);
        let Ok(lines) = read_complete_lines(&path) else {
            report.skipped += 1;
            continue;
        };
        if lines.is_empty() {
            report.skipped += 1;
            continue;
        }
        let mut writer = RuntimeLogWriter::open(lease, config.max_segment_bytes);
        let mut valid = true;
        for line in lines {
            if !is_runtime_event(&line) || writer.append_json_line(&line).is_err() {
                valid = false;
                break;
            }
        }
        if !valid || writer.flush_and_publish().is_err() {
            report.skipped += 1;
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            report.recovered += 1;
        } else {
            report.skipped += 1;
        }
    }
    report
}

fn read_complete_lines(path: &Path) -> io::Result<Vec<Vec<u8>>> {
    let bytes = fs::read(path)?;
    if bytes.is_empty() || !bytes.ends_with(b"\n") {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "partial line"));
    }
    BufReader::new(bytes.as_slice())
        .lines()
        .map(|line| line.map(|value| value.into_bytes()))
        .collect()
}

fn is_runtime_event(line: &[u8]) -> bool {
    serde_json::from_slice::<RuntimeEvent>(line)
        .ok()
        .is_some_and(|event| Catalog::accepts_event(&event))
}

#[cfg(test)]
mod tests {
    use super::super::{
        event::{Component, EventLevel, EventOutcome, RuntimeDetail, RuntimeEvent},
        lease::RuntimeLogLease,
        subject::StableEventCode,
    };
    use super::{recover_partials, RecoveryConfig};

    #[test]
    fn recovers_only_complete_valid_bounded_partials() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let valid_line = RuntimeEvent::new(
            1,
            1,
            EventLevel::Warn,
            StableEventCode::new("runtime.log_event.dropped").expect("event code"),
            Component::Runtime,
            EventOutcome::Ok,
            super::super::subject::SessionId::new(),
            None,
            None,
            None,
            None,
            None,
            None,
            RuntimeDetail::None,
        )
        .expect("valid event")
        .to_json_line()
        .expect("serialized event");
        std::fs::write(
            root.path().join("runtime-old-session-0.jsonl.partial"),
            valid_line.as_bytes(),
        )
        .expect("partial");
        std::fs::write(
            root.path().join("runtime-bad-session-0.jsonl.partial"),
            b"{\"eventCode\":\"bad\"}",
        )
        .expect("bad partial");
        let report = recover_partials(
            root.path(),
            &lease,
            RecoveryConfig {
                max_files: 8,
                max_bytes: 4096,
                max_segment_bytes: 4096,
            },
        );
        assert_eq!(report.recovered, 1);
        assert_eq!(report.skipped, 1);
        assert!(!root
            .path()
            .join("runtime-old-session-0.jsonl.partial")
            .exists());
        assert!(root
            .path()
            .join("runtime-bad-session-0.jsonl.partial")
            .exists());
    }

    #[test]
    fn rejects_unknown_schema_and_sensitive_json_during_recovery() {
        let root = tempfile::tempdir().expect("runtime root");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        std::fs::write(
            root.path().join("runtime-secret-session-0.jsonl.partial"),
            b"{\"schemaVersion\":1,\"eventCode\":\"runtime.log_event.dropped\",\"password\":\"sk-secret\"}\n",
        )
        .expect("sensitive partial");
        std::fs::write(
            root.path().join("runtime-unknown-session-0.jsonl.partial"),
            b"{\"schemaVersion\":99,\"eventCode\":\"runtime.log_event.dropped\"}\n",
        )
        .expect("unknown schema partial");

        let report = recover_partials(root.path(), &lease, RecoveryConfig::default());
        assert_eq!(report.recovered, 0);
        assert_eq!(report.skipped, 2);
        assert!(root
            .path()
            .join("runtime-secret-session-0.jsonl.partial")
            .exists());
        assert!(root
            .path()
            .join("runtime-unknown-session-0.jsonl.partial")
            .exists());
        assert!(!std::fs::read_dir(root.path())
            .expect("runtime directory")
            .flatten()
            .any(|entry| entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "jsonl")));
    }

    #[test]
    fn rejects_unknown_fields_on_an_otherwise_valid_event() {
        let root = tempfile::tempdir().expect("runtime root");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let valid = RuntimeEvent::new(
            1,
            1,
            EventLevel::Warn,
            StableEventCode::new("runtime.log_event.dropped").expect("event code"),
            Component::Runtime,
            EventOutcome::Ok,
            super::super::subject::SessionId::new(),
            None,
            None,
            None,
            None,
            None,
            None,
            RuntimeDetail::None,
        )
        .expect("valid event")
        .to_json_line()
        .expect("serialized event");
        let mut tampered = valid.trim_end().to_owned();
        tampered.push_str(",\"password\":\"sk-secret\"}\n");
        std::fs::write(
            root.path().join("runtime-secret-session-0.jsonl.partial"),
            tampered,
        )
        .expect("sensitive partial");

        let report = recover_partials(root.path(), &lease, RecoveryConfig::default());
        assert_eq!(report.recovered, 0);
        assert_eq!(report.skipped, 1);
        assert!(root
            .path()
            .join("runtime-secret-session-0.jsonl.partial")
            .exists());
    }
}
