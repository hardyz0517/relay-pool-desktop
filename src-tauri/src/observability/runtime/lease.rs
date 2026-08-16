//! Installation-wide ownership for runtime-log writers and maintenance.
//!
//! The lock is deliberately independent from the business data-store lease.  A
//! process must keep the returned file handle alive for as long as it may write,
//! recover, or retain runtime-log segments.

use std::{
    fs::{File, OpenOptions, TryLockError},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeLeaseError {
    #[error("runtime log lease is already held")]
    AlreadyHeld,
    #[error("runtime log lease I/O failed")]
    Io(#[from] std::io::Error),
}

/// A held installation-wide OS file lock.
#[derive(Debug)]
pub struct RuntimeLogLease {
    root: PathBuf,
    lock_file: Option<File>,
    identity: String,
}

impl RuntimeLogLease {
    pub fn try_acquire(root: impl AsRef<Path>) -> Result<Self, RuntimeLeaseError> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        let lock_path = root.join("runtime-log.lease");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(RuntimeLeaseError::AlreadyHeld),
            Err(TryLockError::Error(error)) => return Err(RuntimeLeaseError::Io(error)),
        }

        let identity = format!("{}-{}", std::process::id(), Uuid::now_v7());
        let payload = format!(
            "{{\"pid\":{},\"identity\":\"{}\",\"acquiredAtMs\":{}}}\n",
            std::process::id(),
            identity,
            unix_ms()
        );
        // The lock remains held while the diagnostic identity is refreshed.
        file.set_len(0)?;
        (&file).seek(SeekFrom::Start(0))?;
        (&file).write_all(payload.as_bytes())?;
        (&file).flush()?;
        Ok(Self {
            root,
            lock_file: Some(file),
            identity,
        })
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    #[cfg(test)]
    pub fn lock_path(&self) -> PathBuf {
        self.root.join("runtime-log.lease")
    }

    #[cfg(test)]
    pub fn is_held(&self) -> bool {
        self.lock_file.is_some()
    }
}

impl Drop for RuntimeLogLease {
    fn drop(&mut self) {
        if let Some(file) = self.lock_file.take() {
            // Closing the handle is the final release path. Unlocking first is
            // best effort because process teardown must never panic.
            let _ = File::unlock(&file);
        }
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        process::{Child, Command},
        thread,
        time::Duration,
    };

    use super::{RuntimeLeaseError, RuntimeLogLease};

    const CHILD_ROOT: &str = "RELAY_POOL_RUNTIME_LEASE_CHILD_ROOT";
    const CHILD_READY: &str = "RELAY_POOL_RUNTIME_LEASE_CHILD_READY";
    const CHILD_RELEASE: &str = "RELAY_POOL_RUNTIME_LEASE_CHILD_RELEASE";

    #[test]
    fn only_one_process_can_hold_the_runtime_lease() {
        let root = tempfile::tempdir().expect("tempdir");
        let first = RuntimeLogLease::try_acquire(root.path()).expect("first lease");
        assert!(matches!(
            RuntimeLogLease::try_acquire(root.path()),
            Err(RuntimeLeaseError::AlreadyHeld)
        ));
        assert!(!first.identity().is_empty());
        drop(first);
        RuntimeLogLease::try_acquire(root.path()).expect("lease after drop");
    }

    #[test]
    fn lease_identity_is_not_a_path_or_user_value() {
        let root = tempfile::tempdir().expect("tempdir");
        let lease = RuntimeLogLease::try_acquire(root.path()).expect("lease");
        assert!(!lease.identity().contains(['\\', '/', '?', '=']));
        assert!(!lease.identity().contains("token"));
    }

    #[test]
    fn lease_is_exclusive_across_processes_and_recovers_after_release() {
        if env::var_os(CHILD_ROOT).is_some() {
            return;
        }

        let root = tempfile::tempdir().expect("tempdir");
        let ready = root.path().join("child.ready");
        let release = root.path().join("child.release");
        let mut child = spawn_lease_child(root.path(), &ready, &release);

        wait_for_marker(&ready, &mut child);
        assert!(matches!(
            RuntimeLogLease::try_acquire(root.path()),
            Err(RuntimeLeaseError::AlreadyHeld)
        ));

        fs::write(&release, b"release\n").expect("release child");
        let status = child.wait().expect("wait child");
        assert!(status.success(), "lease child exited with {status}");
        RuntimeLogLease::try_acquire(root.path()).expect("lease after child release");
    }

    #[test]
    fn lease_child_holds() {
        let Some(root) = env::var_os(CHILD_ROOT) else {
            return;
        };
        let ready = env::var_os(CHILD_READY).expect("child ready path");
        let release = env::var_os(CHILD_RELEASE).expect("child release path");
        let _lease = RuntimeLogLease::try_acquire(root).expect("child lease");
        fs::write(ready, b"ready\n").expect("child ready");
        for _ in 0..600 {
            if PathLike::exists(&release) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("parent did not release child lease");
    }

    fn spawn_lease_child(
        root: &std::path::Path,
        ready: &std::path::Path,
        release: &std::path::Path,
    ) -> Child {
        Command::new(env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "observability::runtime::lease::tests::lease_child_holds",
                "--nocapture",
            ])
            .env(CHILD_ROOT, root)
            .env(CHILD_READY, ready)
            .env(CHILD_RELEASE, release)
            .spawn()
            .expect("spawn lease child")
    }

    fn wait_for_marker(marker: &std::path::Path, child: &mut Child) {
        for _ in 0..200 {
            if marker.exists() {
                return;
            }
            if let Some(status) = child.try_wait().expect("poll child") {
                panic!("lease child exited before readiness: {status}");
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        panic!("lease child did not become ready");
    }

    struct PathLike;

    impl PathLike {
        fn exists(path: &std::ffi::OsStr) -> bool {
            std::path::Path::new(path).exists()
        }
    }
}
