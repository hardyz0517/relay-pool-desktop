//! Bounded retention over validated published segments.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use super::reader::{list_published_segments, scan_issues, PublishedSegment, ReadIssue};

#[derive(Debug, Clone, Copy)]
pub struct RetentionConfig {
    pub max_bytes: u64,
    pub max_age_ms: u128,
    pub clock_stable: bool,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            max_bytes: 96 * 1024 * 1024,
            max_age_ms: 14 * 24 * 60 * 60 * 1000,
            clock_stable: true,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RetentionReport {
    pub considered: usize,
    pub deleted: usize,
    pub bytes_deleted: u64,
    pub skipped_active: usize,
    pub skipped_unknown: usize,
    pub delete_failures: usize,
    pub age_deletion_paused: bool,
}

pub fn retain(
    root: &Path,
    active_paths: &[PathBuf],
    now_utc_ms: u128,
    config: RetentionConfig,
) -> RetentionReport {
    retain_with_io(root, active_paths, now_utc_ms, config, &StdRetentionIo)
}

trait RetentionIo {
    fn remove(&self, path: &Path) -> std::io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()>;
}

struct StdRetentionIo;

impl RetentionIo for StdRetentionIo {
    fn remove(&self, path: &Path) -> std::io::Result<()> {
        fs::remove_file(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        fs::rename(from, to)
    }
}

fn retain_with_io(
    root: &Path,
    active_paths: &[PathBuf],
    now_utc_ms: u128,
    config: RetentionConfig,
    io: &dyn RetentionIo,
) -> RetentionReport {
    let mut report = RetentionReport {
        age_deletion_paused: !config.clock_stable,
        ..RetentionReport::default()
    };
    let active: HashSet<PathBuf> = active_paths.iter().cloned().collect();
    let scan_issues = scan_issues(root);
    report.skipped_unknown = scan_issues
        .iter()
        .filter(|issue| {
            matches!(
                issue,
                ReadIssue::UnknownSegment | ReadIssue::MetadataInvalid | ReadIssue::UnknownManifest
            )
        })
        .count();
    let mut segments = list_published_segments(root);
    report.considered = segments.len();
    let mut total_bytes: u64 = segments
        .iter()
        .map(|segment| segment.metadata.byte_length)
        .sum();

    segments.sort_by_key(|segment| segment.metadata.generation);
    let mut candidates = Vec::new();
    for segment in segments {
        if active.contains(&segment.path) {
            report.skipped_active += 1;
            continue;
        }
        let too_old = config.clock_stable
            && now_utc_ms >= segment.metadata.closed_at_ms
            && now_utc_ms - segment.metadata.closed_at_ms > config.max_age_ms;
        if too_old || total_bytes > config.max_bytes {
            candidates.push(segment);
        }
    }
    for segment in candidates {
        if total_bytes <= config.max_bytes
            && (!config.clock_stable
                || now_utc_ms.saturating_sub(segment.metadata.closed_at_ms) <= config.max_age_ms)
        {
            break;
        }
        let bytes = segment.metadata.byte_length;
        match delete_segment(&segment, io) {
            Ok(()) => {
                total_bytes = total_bytes.saturating_sub(bytes);
                report.deleted += 1;
                report.bytes_deleted += bytes;
            }
            Err(_) => report.delete_failures += 1,
        }
    }
    report
}

fn delete_segment(segment: &PublishedSegment, io: &dyn RetentionIo) -> std::io::Result<()> {
    // There is no multi-file unlink primitive. First move both files out of
    // the published namespace, then delete metadata before data. If the data
    // delete fails, restore the metadata bytes and put both files back. This
    // keeps a failed retention pass from leaving a published half-pair.
    let staged_data = deletion_staging_path(&segment.path);
    let staged_metadata = deletion_staging_path(&segment.metadata_path);
    let metadata_bytes = fs::read(&segment.metadata_path)?;

    io.rename(&segment.path, &staged_data)?;
    if let Err(error) = io.rename(&segment.metadata_path, &staged_metadata) {
        let _ = io.rename(&staged_data, &segment.path);
        return Err(error);
    }

    if let Err(error) = io.remove(&staged_metadata) {
        let _ = io.rename(&staged_metadata, &segment.metadata_path);
        let _ = io.rename(&staged_data, &segment.path);
        return Err(error);
    }

    if let Err(error) = io.remove(&staged_data) {
        // A successful metadata unlink precedes the failing data unlink. The
        // saved, validated metadata lets us restore the pair without ever
        // exposing a half-published segment to readers.
        let restore_result = fs::write(&staged_metadata, metadata_bytes)
            .and_then(|()| io.rename(&staged_metadata, &segment.metadata_path))
            .and_then(|()| io.rename(&staged_data, &segment.path));
        return match restore_result {
            Ok(()) => Err(error),
            Err(restore_error) => Err(std::io::Error::other(format!(
                "retention delete failed ({error}); pair restore failed ({restore_error})"
            ))),
        };
    }

    Ok(())
}

fn deletion_staging_path(path: &Path) -> std::path::PathBuf {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".retention-delete");
    std::path::PathBuf::from(staged)
}

#[cfg(test)]
mod tests {
    use std::{io, path::Path};

    use super::super::{lease::RuntimeLogLease, sink::RuntimeLogWriter};
    use super::{retain, retain_with_io, RetentionConfig, RetentionIo};

    struct AlwaysFailDelete;

    impl RetentionIo for AlwaysFailDelete {
        fn remove(&self, _path: &Path) -> io::Result<()> {
            Err(io::Error::other("injected retention delete failure"))
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            std::fs::rename(from, to)
        }
    }

    struct FailOnSecondDelete {
        calls: std::sync::Mutex<usize>,
    }

    impl RetentionIo for FailOnSecondDelete {
        fn remove(&self, path: &Path) -> io::Result<()> {
            let mut calls = self.calls.lock().expect("delete call counter");
            *calls += 1;
            if *calls == 2 {
                return Err(io::Error::other("injected second retention delete failure"));
            }
            std::fs::remove_file(path)
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            std::fs::rename(from, to)
        }
    }

    #[test]
    fn byte_cap_deletes_oldest_validated_segments_but_not_active_partial() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 128);
        writer.append_json_line(br#"{"a":1}"#).expect("first");
        writer.flush_and_publish().expect("first publish");
        writer.append_json_line(br#"{"b":2}"#).expect("second");
        let active = writer.active_path().expect("active").to_path_buf();
        let report = retain(
            root.path(),
            std::slice::from_ref(&active),
            u128::MAX,
            RetentionConfig {
                max_bytes: 1,
                max_age_ms: 0,
                clock_stable: true,
            },
        );
        assert_eq!(report.deleted, 1);
        assert!(active.exists());
    }

    #[test]
    fn age_deletion_is_paused_when_clock_is_unstable() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 128);
        writer.append_json_line(br#"{"a":1}"#).expect("write");
        writer.flush_and_publish().expect("publish");
        let report = retain(
            root.path(),
            &[],
            u128::MAX,
            RetentionConfig {
                max_bytes: u64::MAX,
                max_age_ms: 0,
                clock_stable: false,
            },
        );
        assert!(report.age_deletion_paused);
        assert_eq!(report.deleted, 0);
    }

    #[test]
    fn injected_delete_failure_preserves_segment_pair_and_reports_failure() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 128);
        writer.append_json_line(br#"{"a":1}"#).expect("write");
        writer.flush_and_publish().expect("publish");
        let files: Vec<_> = std::fs::read_dir(root.path())
            .expect("read")
            .flatten()
            .map(|entry| entry.path())
            .collect();
        let report = retain_with_io(
            root.path(),
            &[],
            u128::MAX,
            RetentionConfig {
                max_bytes: 1,
                max_age_ms: 0,
                clock_stable: true,
            },
            &AlwaysFailDelete,
        );
        assert_eq!(report.deleted, 0);
        assert_eq!(report.delete_failures, 1);
        for path in files {
            assert!(path.exists(), "retention fault removed {path:?}");
        }
    }

    #[test]
    fn second_delete_failure_restores_segment_pair_and_reports_failure() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 128);
        writer.append_json_line(br#"{"a":1}"#).expect("write");
        writer.flush_and_publish().expect("publish");
        let segment = super::super::reader::list_published_segments(root.path())
            .into_iter()
            .next()
            .expect("published segment");

        let report = retain_with_io(
            root.path(),
            &[],
            u128::MAX,
            RetentionConfig {
                max_bytes: 1,
                max_age_ms: 0,
                clock_stable: true,
            },
            &FailOnSecondDelete {
                calls: std::sync::Mutex::new(0),
            },
        );

        assert_eq!(report.deleted, 0);
        assert_eq!(report.delete_failures, 1);
        assert!(segment.path.exists(), "data pair member was lost");
        assert!(
            segment.metadata_path.exists(),
            "metadata pair member was lost"
        );
        assert!(super::super::reader::list_published_segments(root.path()).len() == 1);
    }

    #[test]
    fn reports_unknown_published_candidates_without_deleting_them() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("runtime-orphan.jsonl"), b"{}\n").expect("orphan data");
        std::fs::write(root.path().join("runtime-bad.meta.json"), b"{}\n")
            .expect("invalid metadata");
        let report = retain(
            root.path(),
            &[],
            u128::MAX,
            RetentionConfig {
                max_bytes: 1,
                max_age_ms: 0,
                clock_stable: true,
            },
        );
        assert!(report.skipped_unknown >= 2);
        assert!(root.path().join("runtime-orphan.jsonl").exists());
        assert!(root.path().join("runtime-bad.meta.json").exists());
    }
}
