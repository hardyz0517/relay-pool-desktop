use keyring::{Entry, Error as KeyringError};

use super::device_key_store::{CredentialBackend, CredentialBackendError, DeviceKeyErrorKind};

const SERVICE: &str = "relay-pool-desktop";

#[derive(Debug, Clone, Copy)]
pub(crate) struct SystemCredentialBackend;

impl CredentialBackend for SystemCredentialBackend {
    fn get_password(&self, username: &str) -> Result<String, CredentialBackendError> {
        Entry::new(SERVICE, username)
            .map_err(map_keyring_error)?
            .get_password()
            .map_err(map_keyring_error)
    }

    fn set_password(&self, username: &str, password: &str) -> Result<(), CredentialBackendError> {
        Entry::new(SERVICE, username)
            .map_err(map_keyring_error)?
            .set_password(password)
            .map_err(map_keyring_error)
    }
}

fn map_keyring_error(error: KeyringError) -> CredentialBackendError {
    CredentialBackendError::new(match error {
        KeyringError::NoEntry => DeviceKeyErrorKind::NotFound,
        KeyringError::NoStorageAccess(_) => DeviceKeyErrorKind::PermissionDenied,
        KeyringError::BadEncoding(_) | KeyringError::Ambiguous(_) => DeviceKeyErrorKind::Corrupt,
        KeyringError::TooLong(_, _) | KeyringError::Invalid(_, _) => {
            DeviceKeyErrorKind::Unsupported
        }
        KeyringError::PlatformFailure(_) => DeviceKeyErrorKind::Unavailable,
        _ => DeviceKeyErrorKind::Internal,
    })
}
