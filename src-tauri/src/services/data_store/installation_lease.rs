use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
};

use crate::observability::runtime::bootstrap::{
    emit_installation_lease_event, InstallationLeaseEvent,
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
    #[cfg(test)]
    release_fault: Option<std::io::ErrorKind>,
}

impl InstallationLease {
    pub fn try_acquire(config_dir: &Path) -> Result<Self, LeaseError> {
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
            match error {
                TryLockError::WouldBlock => {
                    log_installation_lease_event(InstallationLeaseEvent::Contended);
                    return Err(LeaseError::AlreadyRunning);
                }
                TryLockError::Error(error) => {
                    log_installation_lease_event(InstallationLeaseEvent::AcquireFailed);
                    return Err(LeaseError::Io(error));
                }
            }
        }
        log_installation_lease_event(InstallationLeaseEvent::Acquired);
        Ok(Self {
            file: Some(file),
            #[cfg(test)]
            release_fault: None,
        })
    }

    pub fn release(mut self) -> Result<(), LeaseError> {
        if let Err(error) = self.release_inner() {
            log_installation_lease_event(InstallationLeaseEvent::ReleaseFailed);
            return Err(error);
        }
        log_installation_lease_event(InstallationLeaseEvent::Released);
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
            let event = if self.release_inner().is_ok() {
                InstallationLeaseEvent::Released
            } else {
                // Closing the file handle remains the final OS-backed release path.
                InstallationLeaseEvent::ReleaseFailed
            };
            log_installation_lease_event(event);
        }
    }
}

fn log_installation_lease_event(event: InstallationLeaseEvent) {
    emit_installation_lease_event(event);
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::sync::Arc;

    use super::{InstallationLease, LeaseError};
    use crate::observability::runtime::{
        bootstrap, DetailKind, LeaseState, RuntimeEvent, RuntimeLogReader, RuntimeLogService,
    };

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

    #[tokio::test]
    async fn lease_lifecycle_publishes_typed_runtime_events() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = Arc::new(RuntimeLogService::open(root.path().join("logs")));
        let config_dir = root.path().join("config");

        bootstrap::with_test_service(Arc::clone(&service), || async {
            let lease = InstallationLease::try_acquire(&config_dir).expect("first lease");
            assert!(matches!(
                InstallationLease::try_acquire(&config_dir),
                Err(LeaseError::AlreadyRunning)
            ));
            lease.release().expect("release lease");
        })
        .await;
        service.flush();

        let page = RuntimeLogReader::new(root.path().join("logs")).read_page(0, 50, 1024 * 1024);
        assert!(page.issues.is_empty(), "reader issues: {:?}", page.issues);
        let events = page
            .lines
            .iter()
            .filter_map(|line| serde_json::from_slice::<RuntimeEvent>(line.as_bytes()).ok())
            .collect::<Vec<_>>();
        assert!(events.iter().any(|event| {
            event.event_code.as_str() == "persistence.installation_lease.acquired"
                && event.detail.kind() == DetailKind::Lease
                && event.detail
                    == crate::observability::runtime::RuntimeDetail::Lease {
                        state: LeaseState::Acquired,
                    }
        }));
        assert!(events.iter().any(|event| {
            event.event_code.as_str() == "persistence.installation_lease.contended"
                && event.detail
                    == crate::observability::runtime::RuntimeDetail::Lease {
                        state: LeaseState::Unavailable,
                    }
        }));
        assert!(events.iter().any(|event| {
            event.event_code.as_str() == "persistence.installation_lease.released"
                && event.detail
                    == crate::observability::runtime::RuntimeDetail::Lease {
                        state: LeaseState::Released,
                    }
        }));
    }
}
