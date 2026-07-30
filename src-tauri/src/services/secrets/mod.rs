pub(crate) mod baseline_conversion;
pub mod crypto;
pub(crate) mod device_key_journal;
pub(crate) mod device_key_store;
pub mod keychain;
pub mod mask;
pub(crate) mod material;
pub mod rekey;
pub mod validation;
pub(crate) mod vault;

use crate::background_tasks::{BlockingExecutor, BlockingExecutorError};

use device_key_store::{DeviceKeyError, DeviceKeyStore};
pub(crate) use material::LEGACY_DEVICE_KEY_ID;
pub use material::{
    DeviceKeyId, DeviceKeyResolver, SecretKeyAccessError, SecretKeyMaterial,
    CURRENT_SECRET_ENCRYPTION_VERSION,
};

pub struct SecretManager {
    resolver: DeviceKeyResolver,
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
        Ok(Self::from_loaded_key(
            key.id.unwrap_or_else(|| LEGACY_DEVICE_KEY_ID.to_string()),
            key.material,
        ))
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
        Ok(Self::from_loaded_key(pending.id, pending.material))
    }

    pub async fn load_by_key_id(
        blocking: BlockingExecutor,
        key_id: String,
    ) -> Result<Self, DeviceKeyError> {
        let loaded = blocking
            .submit("device_key_load_by_id", None, None, None, move |_| {
                let store = DeviceKeyStore::new(keychain::SystemCredentialBackend);
                store
                    .load_by_id(&key_id)
                    .map_err(|error| BlockingExecutorError::JobFailed {
                        code: error.kind().stable_code().to_string(),
                    })
            })
            .map_err(|_| DeviceKeyError::new(device_key_store::DeviceKeyErrorKind::Unavailable))?
            .result()
            .await
            .map_err(blocking_error_to_device_key_error)?;
        Ok(Self::from_loaded_key(
            loaded
                .id
                .unwrap_or_else(|| LEGACY_DEVICE_KEY_ID.to_string()),
            loaded.material,
        ))
    }

    pub async fn commit_key_id(
        blocking: BlockingExecutor,
        key_id: String,
    ) -> Result<(), DeviceKeyError> {
        if key_id == LEGACY_DEVICE_KEY_ID {
            return Ok(());
        }
        blocking
            .submit("device_key_commit_key_id", None, None, None, move |_| {
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

    pub async fn commit_active(&self, blocking: BlockingExecutor) -> Result<(), DeviceKeyError> {
        let key_id = self.resolver.active_key_id().as_str().to_string();
        Self::commit_key_id(blocking, key_id).await
    }

    pub(crate) fn resolver(&self) -> DeviceKeyResolver {
        self.resolver.clone()
    }

    pub(crate) fn with_active_key<R>(
        &self,
        action: impl FnOnce(&[u8; 32]) -> R,
    ) -> Result<R, SecretKeyAccessError> {
        self.resolver.with_active_key(action)
    }

    fn from_loaded_key(key_id: String, material: [u8; 32]) -> Self {
        Self {
            resolver: DeviceKeyResolver::active(
                DeviceKeyId::new(key_id),
                SecretKeyMaterial::from_bytes(material),
                CURRENT_SECRET_ENCRYPTION_VERSION,
            ),
        }
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
