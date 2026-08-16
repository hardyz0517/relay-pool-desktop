//! Bounded JSONL segment writer.
//!
//! The writer only exists while a [`RuntimeLogLease`](super::lease::RuntimeLogLease)
//! is held.  It writes a `*.partial` file and publishes a segment only after
//! the metadata has been validated and both files have been flushed.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

use super::lease::RuntimeLogLease;

pub const DEFAULT_MAX_SEGMENT_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_EVENT_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeSinkError {
    #[error("runtime event line is not a JSON object")]
    InvalidJson,
    #[error("runtime event line exceeds 16 KiB")]
    EventTooLarge,
    #[error("runtime event line contains a newline")]
    EmbeddedNewline,
    #[error("runtime sink I/O failed")]
    Io(#[from] io::Error),
    #[error("metadata serialization failed")]
    Metadata(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SegmentMetadata {
    pub schema_version: u16,
    pub manifest_id: String,
    pub identity: String,
    pub generation: u64,
    pub byte_length: u64,
    pub first_at_ms: u128,
    pub last_at_ms: u128,
    pub closed_at_ms: u128,
}

impl SegmentMetadata {
    pub fn validate_for(&self, identity: &str, actual_bytes: u64) -> bool {
        self.schema_version == 1
            && !self.manifest_id.is_empty()
            && self.identity == identity
            && self.byte_length == actual_bytes
            && self.first_at_ms <= self.last_at_ms
            && self.last_at_ms <= self.closed_at_ms
    }
}

#[derive(Debug)]
pub struct RuntimeLogWriter {
    root: PathBuf,
    identity: String,
    max_segment_bytes: u64,
    next_generation: u64,
    io: Arc<dyn SinkIo>,
    active: Option<ActiveSegment>,
}

#[derive(Debug)]
struct ActiveSegment {
    generation: u64,
    path: PathBuf,
    writer: BufWriter<Box<dyn SinkFile>>,
    bytes: u64,
    logical_day: i64,
    first_at_ms: u128,
    last_at_ms: u128,
}

/// Filesystem boundary for the runtime sink.  Production uses [`StdSinkIo`];
/// tests can inject one bounded, named failure without relying on disk state,
/// permissions, or platform-specific delete/rename behaviour.
trait SinkIo: Send + Sync + std::fmt::Debug {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn SinkFile>>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
}

trait SinkFile: Write + Send + std::fmt::Debug {
    fn sync_all(&mut self) -> io::Result<()>;
}

#[derive(Debug, Default)]
struct StdSinkIo;

#[derive(Debug)]
struct StdSinkFile(File);

impl Write for StdSinkFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl SinkFile for StdSinkFile {
    fn sync_all(&mut self) -> io::Result<()> {
        self.0.sync_all()
    }
}

impl SinkIo for StdSinkIo {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn SinkFile>> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(|file| Box::new(StdSinkFile(file)) as Box<dyn SinkFile>)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
}

impl RuntimeLogWriter {
    pub fn open(lease: &RuntimeLogLease, max_segment_bytes: u64) -> Self {
        Self::open_with_io(lease, max_segment_bytes, Arc::new(StdSinkIo))
    }

    fn open_with_io(lease: &RuntimeLogLease, max_segment_bytes: u64, io: Arc<dyn SinkIo>) -> Self {
        Self {
            root: lease.root().to_path_buf(),
            identity: lease.identity().to_owned(),
            max_segment_bytes: max_segment_bytes.max(1),
            next_generation: 0,
            io,
            active: None,
        }
    }

    pub fn active_path(&self) -> Option<&Path> {
        self.active.as_ref().map(|active| active.path.as_path())
    }

    pub fn append_json_line(&mut self, line: &[u8]) -> Result<(), RuntimeSinkError> {
        self.append_json_line_at(line, unix_ms().min(i64::MAX as u128) as i64, true)
    }

    pub fn append_json_line_at(
        &mut self,
        line: &[u8],
        at_ms: i64,
        allow_day_rotation: bool,
    ) -> Result<(), RuntimeSinkError> {
        if line.contains(&b'\n') || line.contains(&b'\r') {
            return Err(RuntimeSinkError::EmbeddedNewline);
        }
        if line.is_empty() || line.len() + 1 > MAX_EVENT_LINE_BYTES {
            return Err(RuntimeSinkError::EventTooLarge);
        }
        if !line
            .iter()
            .copied()
            .skip_while(|byte| byte.is_ascii_whitespace())
            .next()
            .is_some_and(|byte| byte == b'{')
            || serde_json::from_slice::<IgnoredAny>(line).is_err()
        {
            return Err(RuntimeSinkError::InvalidJson);
        }

        let now = at_ms.max(0) as u128;
        let logical_day = logical_day(now);
        let required = line.len() as u64 + 1;
        if required > self.max_segment_bytes {
            return Err(RuntimeSinkError::EventTooLarge);
        }
        if self.active.as_ref().is_some_and(|active| {
            active.bytes > 0
                && (active.bytes + required > self.max_segment_bytes
                    || (allow_day_rotation && active.logical_day != logical_day))
        }) {
            self.publish_active()?;
        }
        if self.active.is_none() {
            self.active = Some(self.open_segment(logical_day)?);
        }
        let active = self.active.as_mut().expect("active segment exists");
        active.writer.write_all(line)?;
        active.writer.write_all(b"\n")?;
        active.bytes += required;
        if active.bytes == required {
            active.first_at_ms = now;
        }
        active.last_at_ms = now;
        Ok(())
    }

    pub fn flush_and_publish(&mut self) -> Result<(), RuntimeSinkError> {
        self.publish_active()
    }

    fn open_segment(&mut self, logical_day: i64) -> Result<ActiveSegment, RuntimeSinkError> {
        loop {
            let generation = self.next_generation;
            self.next_generation = self.next_generation.saturating_add(1);
            let stem = format!("runtime-{}-{}", self.identity, generation);
            let path = self.root.join(format!("{stem}.jsonl.partial"));
            match self.io.create_new(&path) {
                Ok(file) => {
                    return Ok(ActiveSegment {
                        generation,
                        path,
                        writer: BufWriter::new(file),
                        bytes: 0,
                        logical_day,
                        first_at_ms: 0,
                        last_at_ms: 0,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(RuntimeSinkError::Io(error)),
            }
        }
    }

    fn publish_active(&mut self) -> Result<(), RuntimeSinkError> {
        let Some(mut active) = self.active.take() else {
            return Ok(());
        };
        if active.bytes == 0 {
            let _ = fs::remove_file(&active.path);
            return Ok(());
        }
        active.writer.flush()?;
        active.writer.get_mut().sync_all()?;
        let metadata = SegmentMetadata {
            schema_version: 1,
            manifest_id: current_manifest_id(),
            identity: self.identity.clone(),
            generation: active.generation,
            byte_length: active.bytes,
            first_at_ms: active.first_at_ms,
            last_at_ms: active.last_at_ms,
            closed_at_ms: unix_ms().max(active.last_at_ms),
        };
        if !metadata.validate_for(&self.identity, active.bytes) {
            return Err(RuntimeSinkError::InvalidJson);
        }
        let stem = active
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".jsonl.partial"))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid segment name"))?;
        let metadata_partial = self.root.join(format!("{stem}.meta.json.partial"));
        let metadata_path = self.root.join(format!("{stem}.meta.json"));
        let mut metadata_file = self.io.create_new(&metadata_partial)?;
        serde_json::to_writer(&mut metadata_file, &metadata)?;
        metadata_file.write_all(b"\n")?;
        metadata_file.sync_all()?;
        self.io.rename(&metadata_partial, &metadata_path)?;

        let published_path = self.root.join(format!("{stem}.jsonl"));
        self.io.rename(&active.path, &published_path)?;
        Ok(())
    }
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

impl Drop for RuntimeLogWriter {
    fn drop(&mut self) {
        if let Some(active) = self.active.take() {
            // Preserve an unfinished segment for bounded startup recovery. A
            // normal shutdown publishes it before drop; deleting it here
            // would silently lose events after a flush or metadata failure.
            let mut writer = active.writer;
            let _ = writer.flush();
            let _ = writer.get_mut().sync_all();
        }
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn logical_day(at_ms: u128) -> i64 {
    (at_ms / 86_400_000).min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{self, Write},
        path::Path,
        sync::{Arc, Mutex},
    };

    use super::super::lease::RuntimeLogLease;
    use super::{RuntimeLogWriter, RuntimeSinkError, MAX_EVENT_LINE_BYTES};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum FaultPoint {
        SegmentCreate,
        AppendWrite,
        DiskFull,
        AppendSync,
        MetadataCreate,
        MetadataWrite,
        MetadataSync,
        MetadataRename,
        SegmentRename,
    }

    #[derive(Debug, Clone)]
    struct FaultSinkIo {
        pending: Arc<Mutex<Vec<FaultPoint>>>,
    }

    impl FaultSinkIo {
        fn once(point: FaultPoint) -> Arc<Self> {
            Arc::new(Self {
                pending: Arc::new(Mutex::new(vec![point])),
            })
        }

        fn take(&self, point: FaultPoint) -> bool {
            let mut pending = self.pending.lock().expect("fault queue");
            pending
                .iter()
                .position(|candidate| *candidate == point)
                .map(|index| {
                    pending.remove(index);
                    true
                })
                .unwrap_or(false)
        }
    }

    #[derive(Debug)]
    struct FaultSinkFile {
        file: fs::File,
        io: FaultSinkIo,
        metadata: bool,
    }

    impl Write for FaultSinkFile {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let point = if self.metadata {
                FaultPoint::MetadataWrite
            } else {
                FaultPoint::AppendWrite
            };
            if !self.metadata && self.io.take(FaultPoint::DiskFull) {
                return Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "injected runtime sink disk-full failure",
                ));
            }
            if self.io.take(point) {
                return Err(io::Error::other("injected runtime sink write failure"));
            }
            self.file.write(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }

    impl super::SinkFile for FaultSinkFile {
        fn sync_all(&mut self) -> io::Result<()> {
            let point = if self.metadata {
                FaultPoint::MetadataSync
            } else {
                FaultPoint::AppendSync
            };
            if self.io.take(point) {
                return Err(io::Error::other("injected runtime sink sync failure"));
            }
            self.file.sync_all()
        }
    }

    impl super::SinkIo for FaultSinkIo {
        fn create_new(&self, path: &Path) -> io::Result<Box<dyn super::SinkFile>> {
            let metadata = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".meta.json.partial"));
            let point = if metadata {
                FaultPoint::MetadataCreate
            } else {
                FaultPoint::SegmentCreate
            };
            if self.take(point) {
                return Err(io::Error::other("injected runtime sink create failure"));
            }
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map(|file| {
                    Box::new(FaultSinkFile {
                        file,
                        io: self.clone(),
                        metadata,
                    }) as Box<dyn super::SinkFile>
                })
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            let metadata = from
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".meta.json.partial"));
            let point = if metadata {
                FaultPoint::MetadataRename
            } else {
                FaultPoint::SegmentRename
            };
            if self.take(point) {
                return Err(io::Error::other("injected runtime sink rename failure"));
            }
            fs::rename(from, to)
        }
    }

    #[test]
    fn writes_partial_then_publishes_metadata_and_segment_atomically() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 128);
        writer
            .append_json_line(br#"{"eventCode":"ok"}"#)
            .expect("append");
        let active = writer.active_path().expect("active").to_path_buf();
        assert!(active.extension().is_some_and(|ext| ext == "partial"));
        writer.flush_and_publish().expect("publish");
        assert!(!active.exists());
        let published = root.path().join(
            active
                .file_name()
                .expect("name")
                .to_string_lossy()
                .replace(".partial", ""),
        );
        assert!(published.exists());
        assert!(root.path().read_dir().expect("read dir").any(|entry| entry
            .expect("entry")
            .file_name()
            .to_string_lossy()
            .ends_with(".meta.json")));
    }

    #[test]
    fn rotates_before_crossing_segment_limit_and_uses_create_new() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 10);
        writer.append_json_line(br#"{"a":1}"#).expect("first");
        writer.append_json_line(br#"{"b":2}"#).expect("second");
        writer.flush_and_publish().expect("publish");
        let segments = fs::read_dir(root.path())
            .expect("read")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
            .count();
        assert_eq!(segments, 2);
    }

    #[test]
    fn rejects_dynamic_or_oversized_lines_before_writing() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 128);
        assert!(matches!(
            writer.append_json_line(b"not-json"),
            Err(RuntimeSinkError::InvalidJson)
        ));
        let oversized = vec![b'a'; MAX_EVENT_LINE_BYTES];
        assert!(matches!(
            writer.append_json_line(&oversized),
            Err(RuntimeSinkError::EventTooLarge)
        ));
        assert_eq!(fs::read_dir(root.path()).expect("read").count(), 1);
    }

    #[test]
    fn dropping_an_unpublished_segment_preserves_it_for_recovery() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let active_path = {
            let mut writer = RuntimeLogWriter::open(&lease, 128);
            writer
                .append_json_line(br#"{"recover":true}"#)
                .expect("write");
            writer.active_path().expect("active").to_path_buf()
        };
        assert!(active_path.exists());
    }

    #[test]
    fn stable_utc_day_change_rotates_but_unstable_clock_does_not() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let mut writer = RuntimeLogWriter::open(&lease, 1024);
        writer
            .append_json_line_at(br#"{"day":1}"#, 86_400_000 - 1, true)
            .expect("first day");
        writer
            .append_json_line_at(br#"{"day":2}"#, 86_400_000, true)
            .expect("stable next day");
        writer.flush_and_publish().expect("publish stable rotation");
        let stable_segments = fs::read_dir(root.path())
            .expect("read stable")
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
            .count();
        assert_eq!(stable_segments, 2);

        let second_root = tempfile::tempdir().expect("second root");
        let second_lease = RuntimeLogLease::try_acquire(second_root.path()).expect("lease");
        let mut second_writer = RuntimeLogWriter::open(&second_lease, 1024);
        second_writer
            .append_json_line_at(br#"{"day":1}"#, 86_400_000 - 1, false)
            .expect("first unstable day");
        second_writer
            .append_json_line_at(br#"{"day":2}"#, 86_400_000, false)
            .expect("unstable next day");
        second_writer
            .flush_and_publish()
            .expect("publish unstable stream");
        let unstable_segments = fs::read_dir(second_root.path())
            .expect("read unstable")
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
            .count();
        assert_eq!(unstable_segments, 1);
    }

    fn publish_with_fault(point: FaultPoint) -> (tempfile::TempDir, bool) {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let fault = FaultSinkIo::once(point);
        let mut writer = RuntimeLogWriter::open_with_io(&lease, 1024, fault);
        writer
            .append_json_line(br#"{"eventCode":"runtime.log_event.dropped"}"#)
            .expect("append queues in memory");
        let failed = writer.flush_and_publish().is_err();
        (root, failed)
    }

    #[test]
    fn injected_sink_faults_fail_closed_and_preserve_recovery_material() {
        for point in [
            FaultPoint::AppendWrite,
            FaultPoint::DiskFull,
            FaultPoint::AppendSync,
            FaultPoint::MetadataCreate,
            FaultPoint::MetadataWrite,
            FaultPoint::MetadataSync,
            FaultPoint::MetadataRename,
            FaultPoint::SegmentRename,
        ] {
            let (root, failed) = publish_with_fault(point);
            assert!(failed, "fault point {point:?} must reach publish");
            let published = fs::read_dir(root.path())
                .expect("runtime directory")
                .flatten()
                .filter(|entry| {
                    entry
                        .path()
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".jsonl"))
                })
                .count();
            assert_eq!(published, 0, "fault point {point:?} published partial data");
            assert!(fs::read_dir(root.path())
                .expect("runtime directory")
                .flatten()
                .any(|entry| entry
                    .path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".partial"))));
        }
    }

    #[test]
    fn injected_segment_create_failure_does_not_leave_empty_partial() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        let fault = FaultSinkIo::once(FaultPoint::SegmentCreate);
        let mut writer = RuntimeLogWriter::open_with_io(&lease, 1024, fault);
        assert!(writer
            .append_json_line(br#"{"eventCode":"runtime.log_event.dropped"}"#)
            .is_err());
        assert_eq!(
            fs::read_dir(root.path())
                .expect("runtime directory")
                .count(),
            1
        );
    }
}
