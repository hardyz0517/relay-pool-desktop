use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
    time::Instant,
};

#[derive(Debug, thiserror::Error)]
pub enum LeaseError {
    #[error("installation already running")]
    AlreadyRunning,
    #[error("I/O failed")]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
pub struct InstallationLease {
    file: Option<File>,
    acquired_at: Instant,
    #[cfg(test)]
    release_fault: Option<std::io::ErrorKind>,
}

impl InstallationLease {
    pub fn try_acquire(config_dir: &Path) -> Result<Self, LeaseError> {
        let started = Instant::now();
        std::fs::create_dir_all(config_dir).map_err(LeaseError::Io)?;
        let path = config_dir.join("relay-pool-installation.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(LeaseError::Io)?;
        if let Err(error) = file.try_lock() {
            let elapsed_ms = started.elapsed().as_millis();
            match error {
                TryLockError::WouldBlock => {
                    log_installation_lease_event(
                        "installation_lease_contended",
                        "already_running",
                        elapsed_ms,
                    );
                    return Err(LeaseError::AlreadyRunning);
                }
                TryLockError::Error(error) => {
                    log_installation_lease_event(
                        "installation_lease_acquired",
                        "io_error",
                        elapsed_ms,
                    );
                    return Err(LeaseError::Io(error));
                }
            }
        }
        let acquired_at = Instant::now();
        log_installation_lease_event(
            "installation_lease_acquired",
            "ok",
            started.elapsed().as_millis(),
        );
        Ok(Self {
            file: Some(file),
            acquired_at,
            #[cfg(test)]
            release_fault: None,
        })
    }

    pub fn release(mut self) -> Result<(), LeaseError> {
        if let Err(error) = self.release_inner() {
            log_installation_lease_event(
                "installation_lease_released",
                "io_error",
                self.acquired_at.elapsed().as_millis(),
            );
            return Err(error);
        }
        log_installation_lease_event(
            "installation_lease_released",
            "ok",
            self.acquired_at.elapsed().as_millis(),
        );
        Ok(())
    }

    fn release_inner(&mut self) -> Result<(), LeaseError> {
        #[cfg(test)]
        if let Some(kind) = self.release_fault.take() {
            return Err(LeaseError::Io(std::io::Error::from(kind)));
        }

        let Some(file) = self.file.as_ref() else {
            return Ok(());
        };
        File::unlock(file).map_err(LeaseError::Io)?;
        self.file.take();
        Ok(())
    }

    #[cfg(test)]
    fn fail_next_release(mut self, kind: std::io::ErrorKind) -> Self {
        self.release_fault = Some(kind);
        self
    }
}

impl Drop for InstallationLease {
    fn drop(&mut self) {
        if self.file.is_some() {
            let outcome = if self.release_inner().is_ok() {
                "drop_ok"
            } else {
                // Closing the file handle remains the final OS-backed release path.
                "drop_io_error"
            };
            log_installation_lease_event(
                "installation_lease_released",
                outcome,
                self.acquired_at.elapsed().as_millis(),
            );
        }
    }
}

fn log_installation_lease_event(event: &str, outcome: &str, elapsed_ms: u128) {
    println!("{event} outcome={outcome} elapsed_ms={elapsed_ms}");
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use super::{InstallationLease, LeaseError};

    #[test]
    fn explicit_release_reports_failure_but_drop_still_releases_os_lock() {
        let root = tempfile::tempdir().expect("temp directory");
        let config_dir = root.path().join("config");
        let lease = InstallationLease::try_acquire(&config_dir)
            .expect("acquire lease")
            .fail_next_release(ErrorKind::Other);

        let error = lease.release().expect_err("injected release failure");

        assert!(matches!(error, LeaseError::Io(error) if error.kind() == ErrorKind::Other));
        InstallationLease::try_acquire(&config_dir)
            .expect("drop fallback released the file lock")
            .release()
            .expect("explicit release succeeds");
    }
}
