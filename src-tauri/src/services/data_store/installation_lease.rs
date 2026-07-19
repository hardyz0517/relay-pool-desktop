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
    _file: File,
    acquired_at: Instant,
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
            _file: file,
            acquired_at,
        })
    }
}

impl Drop for InstallationLease {
    fn drop(&mut self) {
        let elapsed_ms = self.acquired_at.elapsed().as_millis();
        log_installation_lease_event("installation_lease_released", "ok", elapsed_ms);
    }
}

fn log_installation_lease_event(event: &str, outcome: &str, elapsed_ms: u128) {
    println!("{event} outcome={outcome} elapsed_ms={elapsed_ms}");
}
