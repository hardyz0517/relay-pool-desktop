//! Independent crash marker used by the panic hook.
//!
//! This handle is opened before the asynchronous runtime logger and is never
//! routed through its queue or mutex.  Panic recording is one non-blocking lock
//! attempt plus a fixed payload; failure falls back to a fixed stderr line.

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviousSession {
    None,
    Panic,
    Unknown,
}

#[derive(Debug, thiserror::Error)]
pub enum CrashMarkerError {
    #[error("crash marker I/O failed")]
    Io(#[from] io::Error),
}

#[derive(Debug)]
pub struct CrashMarker {
    path: PathBuf,
    file: Option<File>,
    recursion_guard: AtomicBool,
}

impl CrashMarker {
    pub fn open(root: impl AsRef<Path>) -> Result<(Self, PreviousSession), CrashMarkerError> {
        fs::create_dir_all(root.as_ref())?;
        let path = root.as_ref().join("runtime-crash.marker");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        let previous = read_previous(&mut file)?;
        file.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => io::Error::new(io::ErrorKind::WouldBlock, "marker locked"),
            TryLockError::Error(error) => error,
        })?;
        write_payload(&file, b"active\n")?;
        File::unlock(&file)?;
        Ok((
            Self {
                path,
                file: Some(file),
                recursion_guard: AtomicBool::new(false),
            },
            previous,
        ))
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Best-effort panic write. Never formats a panic payload, stack, path, or
    /// environment value.
    pub fn record_panic(&self) {
        if self.recursion_guard.swap(true, Ordering::AcqRel) {
            eprintln!("runtime.crash_marker.recursive");
            return;
        }
        let Some(file) = self.file.as_ref() else {
            eprintln!("runtime.crash_marker.unavailable");
            return;
        };
        match file.try_lock() {
            Ok(()) => {
                let result = write_payload(file, b"panic\n");
                let _ = File::unlock(file);
                if result.is_err() {
                    eprintln!("runtime.crash_marker.write_failed");
                }
            }
            Err(TryLockError::WouldBlock) => eprintln!("runtime.crash_marker.locked"),
            Err(TryLockError::Error(_)) => eprintln!("runtime.crash_marker.unavailable"),
        }
    }

    /// Consume the marker only after the caller has drained the runtime queue.
    pub fn clean_shutdown(&self) -> Result<(), CrashMarkerError> {
        if let Some(file) = self.file.as_ref() {
            let _ = File::unlock(&file);
        }
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(CrashMarkerError::Io(error)),
        }
    }
}

impl Drop for CrashMarker {
    fn drop(&mut self) {
        // An unclean drop intentionally leaves the marker in place. The next
        // session can classify it as an unclean exit.
        if let Some(file) = self.file.take() {
            let _ = File::unlock(&file);
        }
    }
}

fn read_previous(file: &mut File) -> io::Result<PreviousSession> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = [0u8; 32];
    let count = file.read(&mut bytes)?;
    Ok(match &bytes[..count] {
        b"panic\n" => PreviousSession::Panic,
        [] => PreviousSession::None,
        _ => PreviousSession::Unknown,
    })
}

fn write_payload(file: &File, payload: &[u8]) -> io::Result<()> {
    file.set_len(0)?;
    (&*file).seek(SeekFrom::Start(0))?;
    (&*file).write_all(payload)?;
    (&*file).flush()?;
    file.sync_data()
}

#[cfg(test)]
mod tests {
    use super::{CrashMarker, PreviousSession};

    #[test]
    fn marker_classifies_unclean_panic_and_is_removed_on_clean_shutdown() {
        let root = tempfile::tempdir().expect("tempdir");
        let (marker, previous) = CrashMarker::open(root.path()).expect("open");
        assert_eq!(previous, PreviousSession::None);
        marker.record_panic();
        drop(marker);
        let (_, previous) = CrashMarker::open(root.path()).expect("reopen");
        assert_eq!(previous, PreviousSession::Panic);
        let (marker, _) = CrashMarker::open(root.path()).expect("open active");
        marker.clean_shutdown().expect("clean");
        assert!(!root.path().join("runtime-crash.marker").exists());
    }

    #[test]
    fn marker_does_not_include_payload_or_environment() {
        let root = tempfile::tempdir().expect("tempdir");
        let (marker, _) = CrashMarker::open(root.path()).expect("open");
        marker.record_panic();
        let contents = std::fs::read_to_string(marker.path()).expect("read");
        assert_eq!(contents, "panic\n");
        assert!(!contents.contains("secret"));
    }
}
