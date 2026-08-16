//! Defensive reader for published runtime-log segments.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use serde::de::IgnoredAny;

use super::sink::SegmentMetadata;

pub const DEFAULT_PAGE_LINES: usize = 200;
pub const DEFAULT_PAGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedSegment {
    pub path: PathBuf,
    pub metadata_path: PathBuf,
    pub metadata: SegmentMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadIssue {
    UnknownSegment,
    MetadataInvalid,
    UnknownManifest,
    CorruptLine,
    LineTooLarge,
}

#[derive(Debug, Default)]
pub struct RuntimeLogReader {
    root: PathBuf,
}

#[derive(Debug, Default)]
pub struct ReadPage {
    /// Syntax-validated event lines. The typed RuntimeEvent adapter owns
    /// deserialization and compatibility mapping above this storage layer.
    pub lines: Vec<ValidatedJsonLine>,
    pub issues: Vec<ReadIssue>,
    pub next_segment_index: Option<usize>,
    /// Zero-based line offset within `next_segment_index`. Keeping the line
    /// cursor separate from the segment cursor lets a bounded page resume
    /// inside a large published segment without replaying its first lines.
    pub next_line_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedJsonLine {
    bytes: Vec<u8>,
    manifest_id: String,
}

impl ValidatedJsonLine {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn manifest_id(&self) -> &str {
        &self.manifest_id
    }
}

impl RuntimeLogReader {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    #[cfg(test)]
    pub fn list_published_segments(&self) -> Vec<PublishedSegment> {
        list_published_segments(&self.root)
    }

    #[cfg(test)]
    pub fn read_page(&self, segment_index: usize, max_lines: usize, max_bytes: usize) -> ReadPage {
        self.read_page_with_cursor(
            segment_index,
            0,
            max_lines.min(DEFAULT_PAGE_LINES),
            max_bytes.min(DEFAULT_PAGE_BYTES),
        )
    }

    /// Read a bounded page from a stable `(segment, line)` cursor. The public
    /// diagnostics page remains capped at 200 lines/1 MiB, while support
    /// bundles use a larger but explicitly bounded batch through this same
    /// reader. The line offset counts every physical line, including corrupt
    /// lines, so callers cannot replay or skip data when a malformed line is
    /// isolated.
    pub(crate) fn read_page_with_cursor(
        &self,
        segment_index: usize,
        line_index: usize,
        max_lines: usize,
        max_bytes: usize,
    ) -> ReadPage {
        let (segments, scan_issues) = scan_published_segments(&self.root);
        let line_limit = max_lines;
        let byte_limit = max_bytes;
        let mut page = ReadPage {
            issues: scan_issues,
            ..ReadPage::default()
        };
        let mut consumed_bytes = 0usize;
        for (index, segment) in segments.iter().enumerate().skip(segment_index) {
            let file = match File::open(&segment.path) {
                Ok(file) => file,
                Err(_) => {
                    page.issues.push(ReadIssue::UnknownSegment);
                    continue;
                }
            };
            let start_line = if index == segment_index {
                line_index
            } else {
                0
            };
            let mut current_line = 0usize;
            for line in BufReader::new(file).lines() {
                if current_line < start_line {
                    current_line = current_line.saturating_add(1);
                    continue;
                }
                let physical_line_index = current_line;
                current_line = current_line.saturating_add(1);
                let line = match line {
                    Ok(line) => line,
                    Err(_) => {
                        page.issues.push(ReadIssue::CorruptLine);
                        continue;
                    }
                };
                let line_bytes = line.len() + 1;
                if line_bytes > super::sink::MAX_EVENT_LINE_BYTES {
                    page.issues.push(ReadIssue::LineTooLarge);
                    continue;
                }
                if consumed_bytes + line_bytes > byte_limit || page.lines.len() >= line_limit {
                    page.next_segment_index = Some(index);
                    page.next_line_index = Some(physical_line_index);
                    return page;
                }
                let is_object = line.trim_start().starts_with('{');
                match (is_object, serde_json::from_str::<IgnoredAny>(&line)) {
                    (true, Ok(_)) => {
                        page.lines.push(ValidatedJsonLine {
                            bytes: line.into_bytes(),
                            manifest_id: segment.metadata.manifest_id.clone(),
                        });
                        consumed_bytes += line_bytes;
                    }
                    _ => page.issues.push(ReadIssue::CorruptLine),
                }
            }
            // Once the requested segment is exhausted, subsequent segments
            // start at line zero. The next iteration computes that cursor
            // explicitly; no extra state is needed here.
        }
        page
    }
}

pub(crate) fn list_published_segments(root: &Path) -> Vec<PublishedSegment> {
    scan_published_segments(root).0
}

pub(crate) fn scan_issues(root: &Path) -> Vec<ReadIssue> {
    scan_published_segments(root).1
}

/// Enumerate the published segment set and classify files that look like
/// runtime segments but fail the publication contract. Invalid files are
/// intentionally never returned as readable segments, while the issue is
/// retained for diagnostics/support-bundle summaries.
fn scan_published_segments(root: &Path) -> (Vec<PublishedSegment>, Vec<ReadIssue>) {
    let Ok(entries) = fs::read_dir(root) else {
        return (Vec::new(), Vec::new());
    };
    let mut segments = Vec::new();
    let mut issues = Vec::new();
    let mut metadata_stems = std::collections::HashSet::new();
    let mut segment_stems = std::collections::HashSet::new();
    let accepted_manifest_ids = accepted_manifest_ids(root);
    for entry in entries.flatten() {
        let metadata_path = entry.path();
        let Some(name) = metadata_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(stem) = name.strip_suffix(".meta.json") {
            if !stem.starts_with("runtime-") || stem.ends_with(".partial") {
                continue;
            }
            metadata_stems.insert(stem.to_owned());
            let segment_path = root.join(format!("{stem}.jsonl"));
            let Ok(file_metadata) = fs::metadata(&segment_path) else {
                issues.push(ReadIssue::UnknownSegment);
                continue;
            };
            let Ok(meta_bytes) = fs::read(&metadata_path) else {
                issues.push(ReadIssue::MetadataInvalid);
                continue;
            };
            let Ok(metadata) = serde_json::from_slice::<SegmentMetadata>(&meta_bytes) else {
                issues.push(ReadIssue::MetadataInvalid);
                continue;
            };
            let expected_prefix = format!("runtime-{}-", metadata.identity);
            if !stem.starts_with(&expected_prefix)
                || !stem
                    .strip_prefix(&expected_prefix)
                    .is_some_and(|generation| generation == metadata.generation.to_string())
            {
                issues.push(ReadIssue::MetadataInvalid);
                continue;
            }
            if !accepted_manifest_ids.contains(&metadata.manifest_id) {
                issues.push(ReadIssue::UnknownManifest);
                continue;
            }
            if !metadata.validate_for(&metadata.identity, file_metadata.len()) {
                issues.push(ReadIssue::MetadataInvalid);
                continue;
            }
            segments.push(PublishedSegment {
                path: segment_path,
                metadata_path,
                metadata,
            });
        } else if let Some(stem) = name.strip_suffix(".jsonl") {
            if stem.starts_with("runtime-") && !stem.ends_with(".partial") {
                segment_stems.insert(stem.to_owned());
            }
        }
    }
    for _orphan in segment_stems.difference(&metadata_stems) {
        issues.push(ReadIssue::UnknownSegment);
    }
    segments.sort_by_key(|segment| segment.metadata.generation);
    (segments, issues)
}

fn accepted_manifest_ids(root: &Path) -> HashSet<String> {
    let mut ids = HashSet::from([current_manifest_id()]);
    // A previous snapshot is accepted only when its content-addressed id is
    // valid. This preserves one-version compatibility without trusting an
    // arbitrary file dropped into the runtime-log directory.
    for name in ["manifest.json", "manifest.previous.json"] {
        let path = root.join(name);
        if let Ok(bytes) = fs::read(path) {
            if let Some(id) = super::catalog::Catalog::validate_snapshot(&bytes) {
                ids.insert(id);
            }
        }
    }
    ids
}

fn current_manifest_id() -> String {
    #[cfg(test)]
    {
        return "runtime-test-manifest-v1".to_owned();
    }
    #[cfg(not(test))]
    {
        super::catalog::Catalog::core_manifest_id()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        catalog::{Catalog, OWNER_EVENT_DESCRIPTOR_SLICES},
        lease::RuntimeLogLease,
        sink::RuntimeLogWriter,
    };
    use super::{ReadIssue, RuntimeLogReader};

    #[test]
    fn reader_ignores_partial_unknown_and_size_mismatched_segments() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 128);
        writer.append_json_line(br#"{"ok":true}"#).expect("write");
        writer.flush_and_publish().expect("publish");
        std::fs::write(root.path().join("runtime-unknown.jsonl"), b"{}\n").expect("unknown");
        std::fs::write(root.path().join("runtime-unknown.meta.json"), b"{}\n")
            .expect("unknown metadata");
        let published = RuntimeLogReader::new(root.path()).list_published_segments();
        assert_eq!(published.len(), 1);
        std::fs::write(&published[0].path, b"not-json\n").expect("corrupt");
        let page = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024);
        assert!(page.lines.is_empty());
        assert!(page.issues.contains(&ReadIssue::MetadataInvalid));
    }

    #[test]
    fn bounded_page_resumes_inside_large_segment_without_replaying_lines() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 4096);
        for index in 0..4 {
            writer
                .append_json_line(format!(r#"{{"line":{index}}}"#).as_bytes())
                .expect("write");
        }
        writer.flush_and_publish().expect("publish");

        let reader = RuntimeLogReader::new(root.path());
        let first = reader.read_page(0, 2, 1024);
        assert_eq!(first.lines.len(), 2);
        assert_eq!(first.next_segment_index, Some(0));
        assert_eq!(first.next_line_index, Some(2));
        assert!(String::from_utf8_lossy(first.lines[0].as_bytes()).contains("\"line\":0"));

        let second = reader.read_page_with_cursor(0, 2, 2, 1024);
        assert_eq!(second.lines.len(), 2);
        assert_eq!(second.next_segment_index, None);
        assert_eq!(second.next_line_index, None);
        assert!(String::from_utf8_lossy(second.lines[0].as_bytes()).contains("\"line\":2"));
    }

    #[test]
    fn reader_reports_unknown_and_invalid_published_candidates_but_never_reads_them() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::write(root.path().join("runtime-orphan.jsonl"), b"{}\n").expect("orphan segment");
        std::fs::write(
            root.path().join("runtime-bad.meta.json"),
            br#"{"schemaVersion":1,"manifestId":"runtime-test-manifest-v1","identity":"bad","generation":0,"byteLength":2,"firstAtMs":1,"lastAtMs":1,"closedAtMs":1}"#,
        )
        .expect("bad metadata");
        std::fs::write(root.path().join("runtime-bad.jsonl"), b"{}\n").expect("bad segment");

        let page = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024);
        assert!(page.lines.is_empty());
        assert!(page.issues.contains(&ReadIssue::UnknownSegment));
        assert!(page.issues.contains(&ReadIssue::MetadataInvalid));
    }

    #[test]
    fn reader_accepts_one_valid_previous_manifest_snapshot() {
        let root = tempfile::tempdir().expect("runtime root");
        let previous = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("manifest");
        std::fs::write(
            root.path().join("manifest.previous.json"),
            serde_json::to_vec(&previous).expect("manifest json"),
        )
        .expect("previous manifest");
        let data = root.path().join("runtime-previous-0.jsonl");
        std::fs::write(&data, b"{}\n").expect("segment");
        let metadata = serde_json::json!({
            "schemaVersion": 1,
            "manifestId": previous.manifest_id,
            "identity": "previous",
            "generation": 0,
            "byteLength": 3,
            "firstAtMs": 1,
            "lastAtMs": 1,
            "closedAtMs": 1
        });
        std::fs::write(
            root.path().join("runtime-previous-0.meta.json"),
            serde_json::to_vec(&metadata).expect("metadata json"),
        )
        .expect("metadata");

        let page = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024);
        assert_eq!(page.lines.len(), 1);
        assert!(page.issues.is_empty());
    }

    #[test]
    fn reader_rejects_tampered_previous_manifest_snapshot() {
        let root = tempfile::tempdir().expect("runtime root");
        let mut previous = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("manifest");
        previous.manifest_id = "not-a-content-hash".to_owned();
        std::fs::write(
            root.path().join("manifest.previous.json"),
            serde_json::to_vec(&previous).expect("manifest json"),
        )
        .expect("previous manifest");
        std::fs::write(root.path().join("runtime-previous-0.jsonl"), b"{}\n").expect("segment");
        std::fs::write(
            root.path().join("runtime-previous-0.meta.json"),
            serde_json::json!({
                "schemaVersion": 1,
                "manifestId": "not-a-content-hash",
                "identity": "previous",
                "generation": 0,
                "byteLength": 3,
                "firstAtMs": 1,
                "lastAtMs": 1,
                "closedAtMs": 1
            })
            .to_string(),
        )
        .expect("metadata");

        let page = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024);
        assert!(page.lines.is_empty());
        assert!(page.issues.contains(&ReadIssue::UnknownManifest));
    }
}
