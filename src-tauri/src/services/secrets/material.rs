use std::{fmt, sync::Arc};

use zeroize::Zeroizing;

pub const CURRENT_SECRET_ENCRYPTION_VERSION: u16 = 1;
pub(crate) const LEGACY_DEVICE_KEY_ID: &str = "local-data-key-v1";

pub struct SecretKeyMaterial(Zeroizing<[u8; 32]>);

impl SecretKeyMaterial {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub(super) fn expose_secret(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SecretKeyMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretKeyMaterial { redacted: true }")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceKeyId(String);

impl DeviceKeyId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SecretKeyAccessError {
    #[error("unknown device key id")]
    UnknownKeyId,
    #[error("unsupported secret encryption version")]
    UnsupportedEncryptionVersion,
}

#[derive(Clone)]
pub struct DeviceKeyResolver {
    active_key_id: DeviceKeyId,
    active_material: Arc<SecretKeyMaterial>,
    encryption_version: u16,
}

impl DeviceKeyResolver {
    pub fn active(
        active_key_id: DeviceKeyId,
        active_material: SecretKeyMaterial,
        encryption_version: u16,
    ) -> Self {
        Self {
            active_key_id,
            active_material: Arc::new(active_material),
            encryption_version,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(material: [u8; 32]) -> Self {
        Self::active(
            DeviceKeyId::new("test-device-key"),
            SecretKeyMaterial::from_bytes(material),
            CURRENT_SECRET_ENCRYPTION_VERSION,
        )
    }

    pub fn active_key_id(&self) -> &DeviceKeyId {
        &self.active_key_id
    }

    pub fn with_active_key<R>(
        &self,
        action: impl FnOnce(&[u8; 32]) -> R,
    ) -> Result<R, SecretKeyAccessError> {
        self.with_key(self.active_key_id.as_str(), self.encryption_version, action)
    }

    pub fn with_key<R>(
        &self,
        key_id: &str,
        encryption_version: u16,
        action: impl FnOnce(&[u8; 32]) -> R,
    ) -> Result<R, SecretKeyAccessError> {
        if encryption_version != self.encryption_version {
            return Err(SecretKeyAccessError::UnsupportedEncryptionVersion);
        }
        if key_id != self.active_key_id.as_str() {
            return Err(SecretKeyAccessError::UnknownKeyId);
        }
        Ok(action(self.active_material.expose_secret()))
    }
}

impl fmt::Debug for DeviceKeyResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceKeyResolver")
            .field("active_key_id", &self.active_key_id)
            .field("encryption_version", &self.encryption_version)
            .field("active_material", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use static_assertions::{assert_not_impl_all, assert_not_impl_any};

    use super::*;

    assert_not_impl_any!(SecretKeyMaterial: Clone, Copy, Serialize);
    assert_not_impl_all!(SecretKeyMaterial: fmt::Debug, Clone);

    #[test]
    fn secret_key_material_debug_is_redacted() {
        let material = SecretKeyMaterial::from_bytes([9; 32]);

        assert_eq!(
            format!("{material:?}"),
            "SecretKeyMaterial { redacted: true }"
        );
        assert!(!format!("{material:?}").contains('9'));
    }

    #[test]
    fn resolver_rejects_unknown_key_id_and_version() {
        let resolver = DeviceKeyResolver::for_test([3; 32]);

        assert_eq!(
            resolver
                .with_key("other", CURRENT_SECRET_ENCRYPTION_VERSION, |_| ())
                .unwrap_err(),
            SecretKeyAccessError::UnknownKeyId
        );
        assert_eq!(
            resolver
                .with_key(
                    "test-device-key",
                    CURRENT_SECRET_ENCRYPTION_VERSION + 1,
                    |_| ()
                )
                .unwrap_err(),
            SecretKeyAccessError::UnsupportedEncryptionVersion
        );
    }

    #[test]
    fn resolver_borrows_active_key_only_inside_callback() {
        let resolver = DeviceKeyResolver::for_test([5; 32]);

        let first_byte = resolver
            .with_active_key(|key| {
                assert_eq!(key, &[5; 32]);
                key[0]
            })
            .expect("active key");

        assert_eq!(first_byte, 5);
    }
}
