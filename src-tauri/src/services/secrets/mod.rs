pub mod crypto;
pub(crate) mod device_key_journal;
pub(crate) mod device_key_store;
pub mod keychain;
pub mod mask;
pub mod validation;
pub(crate) mod vault;

use crate::background_tasks::{BlockingExecutor, BlockingExecutorError};

use device_key_store::{DeviceKeyError, DeviceKeyStore};

#[derive(Clone)]
pub struct SecretManager {
    data_key: [u8; 32],
    active_key_id: Option<String>,
}

impl SecretManager {
    pub async fn load_existing(blocking: BlockingExecutor) -> Result<Self, DeviceKeyError> {
        let key = blocking
            .submit("device_key_load_existing", None, None, None, |_| {
                let store = DeviceKeyStore::new(keychain::SystemCredentialBackend);
                store
                    .load_active_or_legacy()
                    .map_err(|error| BlockingExecutorError::JobFailed {
                        code: error.kind().stable_code().to_string(),
                    })
            })
            .map_err(|_| DeviceKeyError::new(device_key_store::DeviceKeyErrorKind::Unavailable))?
            .result()
            .await
            .map_err(blocking_error_to_device_key_error)?;
        Ok(Self {
            data_key: key.material,
            active_key_id: key.id,
        })
    }

    pub async fn create_pending_for_first_run(
        blocking: BlockingExecutor,
        key_id: String,
    ) -> Result<Self, DeviceKeyError> {
        let pending = blocking
            .submit("device_key_create_pending", None, None, None, move |_| {
                let store = DeviceKeyStore::new(keychain::SystemCredentialBackend);
                store
                    .create_pending(&key_id)
                    .map_err(|error| BlockingExecutorError::JobFailed {
                        code: error.kind().stable_code().to_string(),
                    })
            })
            .map_err(|_| DeviceKeyError::new(device_key_store::DeviceKeyErrorKind::Unavailable))?
            .result()
            .await
            .map_err(blocking_error_to_device_key_error)?;
        Ok(Self {
            data_key: pending.material,
            active_key_id: Some(pending.id),
        })
    }

    pub async fn commit_active(&self, blocking: BlockingExecutor) -> Result<(), DeviceKeyError> {
        let Some(key_id) = self.active_key_id.clone() else {
            return Ok(());
        };
        blocking
            .submit("device_key_commit_active", None, None, None, move |_| {
                let store = DeviceKeyStore::new(keychain::SystemCredentialBackend);
                store
                    .commit_active(&key_id)
                    .map_err(|error| BlockingExecutorError::JobFailed {
                        code: error.kind().stable_code().to_string(),
                    })
            })
            .map_err(|_| DeviceKeyError::new(device_key_store::DeviceKeyErrorKind::Unavailable))?
            .result()
            .await
            .map_err(blocking_error_to_device_key_error)
    }

    pub fn data_key(&self) -> &[u8; 32] {
        &self.data_key
    }
}

fn blocking_error_to_device_key_error(error: BlockingExecutorError) -> DeviceKeyError {
    let BlockingExecutorError::JobFailed { code } = error else {
        return DeviceKeyError::new(device_key_store::DeviceKeyErrorKind::Unavailable);
    };
    DeviceKeyError::new(match code.as_str() {
        "not_found" => device_key_store::DeviceKeyErrorKind::NotFound,
        "unavailable" => device_key_store::DeviceKeyErrorKind::Unavailable,
        "permission_denied" => device_key_store::DeviceKeyErrorKind::PermissionDenied,
        "corrupt" => device_key_store::DeviceKeyErrorKind::Corrupt,
        "unsupported" => device_key_store::DeviceKeyErrorKind::Unsupported,
        "internal" => device_key_store::DeviceKeyErrorKind::Internal,
        _ => device_key_store::DeviceKeyErrorKind::Internal,
    })
}

impl device_key_store::DeviceKeyErrorKind {
    pub(crate) const fn stable_code(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Unavailable => "unavailable",
            Self::PermissionDenied => "permission_denied",
            Self::Corrupt => "corrupt",
            Self::Unsupported => "unsupported",
            Self::Internal => "internal",
        }
    }
}
