use keyring::{Entry, Error as KeyringError};
use std::sync::OnceLock;

use super::device_key_store::{CredentialBackend, CredentialBackendError, DeviceKeyErrorKind};

const PRIMARY_APP_IDENTIFIER: &str = "dev.relaypool.desktop";
const LEGACY_SERVICE: &str = "relay-pool-desktop";
static CREDENTIAL_SERVICE: OnceLock<String> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
pub(crate) struct SystemCredentialBackend;

pub(crate) fn configure_for_app_identifier(identifier: &str) -> Result<(), &'static str> {
    let service = service_for_app_identifier(identifier);
    match CREDENTIAL_SERVICE.set(service.clone()) {
        Ok(()) => Ok(()),
        Err(_) if configured_service() == service => Ok(()),
        Err(_) => Err("credential service was already configured for another app identifier"),
    }
}

fn configured_service() -> String {
    CREDENTIAL_SERVICE
        .get()
        .cloned()
        .unwrap_or_else(|| LEGACY_SERVICE.to_string())
}

fn service_for_app_identifier(identifier: &str) -> String {
    if identifier == PRIMARY_APP_IDENTIFIER {
        LEGACY_SERVICE.to_string()
    } else {
        format!("{LEGACY_SERVICE}.{identifier}")
    }
}

impl CredentialBackend for SystemCredentialBackend {
    fn get_password(&self, username: &str) -> Result<String, CredentialBackendError> {
        Entry::new(&configured_service(), username)
            .map_err(map_keyring_error)?
            .get_password()
            .map_err(map_keyring_error)
    }

    fn set_password(&self, username: &str, password: &str) -> Result<(), CredentialBackendError> {
        Entry::new(&configured_service(), username)
            .map_err(map_keyring_error)?
            .set_password(password)
            .map_err(map_keyring_error)
    }
}

#[cfg(test)]
mod tests {
    use super::{service_for_app_identifier, LEGACY_SERVICE, PRIMARY_APP_IDENTIFIER};

    #[test]
    fn primary_identifier_preserves_the_released_credential_service() {
        assert_eq!(
            service_for_app_identifier(PRIMARY_APP_IDENTIFIER),
            LEGACY_SERVICE
        );
    }

    #[test]
    fn alternate_identifiers_use_isolated_credential_services() {
        assert_eq!(
            service_for_app_identifier("dev.relaypool.desktop.guided-tour-test"),
            "relay-pool-desktop.dev.relaypool.desktop.guided-tour-test"
        );
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
