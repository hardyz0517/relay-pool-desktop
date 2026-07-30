use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::crypto::generate_data_key;

pub(crate) const LEGACY_DATA_KEY_USERNAME: &str = "local-data-key-v1";
const ACTIVE_POINTER_USERNAME: &str = "device-data-key-active-v1";
const DEVICE_KEY_USERNAME_PREFIX: &str = "device-data-key:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DeviceKeyErrorKind {
    NotFound,
    Unavailable,
    PermissionDenied,
    Corrupt,
    Unsupported,
    Internal,
}

#[derive(Debug, thiserror::Error)]
#[error("device key store failed: {kind:?}")]
pub(crate) struct DeviceKeyError {
    kind: DeviceKeyErrorKind,
}

impl DeviceKeyError {
    pub(crate) const fn new(kind: DeviceKeyErrorKind) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind(&self) -> DeviceKeyErrorKind {
        self.kind
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CredentialBackendError {
    kind: DeviceKeyErrorKind,
}

impl CredentialBackendError {
    pub(crate) const fn new(kind: DeviceKeyErrorKind) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind(&self) -> DeviceKeyErrorKind {
        self.kind
    }
}

pub(crate) trait CredentialBackend {
    fn get_password(&self, username: &str) -> Result<String, CredentialBackendError>;
    fn set_password(&self, username: &str, password: &str) -> Result<(), CredentialBackendError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceKey {
    pub(crate) id: Option<String>,
    pub(crate) material: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingDeviceKey {
    pub(crate) id: String,
    pub(crate) material: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivePointer {
    version: u32,
    active_key_id: String,
}

pub(crate) struct DeviceKeyStore<B> {
    backend: B,
}

impl<B> DeviceKeyStore<B>
where
    B: CredentialBackend,
{
    pub(crate) const fn new(backend: B) -> Self {
        Self { backend }
    }

    pub(crate) fn generate_key_id() -> String {
        Uuid::now_v7().hyphenated().to_string()
    }

    pub(crate) fn load_active_or_legacy(&self) -> Result<DeviceKey, DeviceKeyError> {
        match self.backend.get_password(ACTIVE_POINTER_USERNAME) {
            Ok(raw_pointer) => {
                let pointer: ActivePointer = serde_json::from_str(&raw_pointer)
                    .map_err(|_| DeviceKeyError::new(DeviceKeyErrorKind::Corrupt))?;
                if pointer.version != 1 || !valid_key_id(&pointer.active_key_id) {
                    return Err(DeviceKeyError::new(DeviceKeyErrorKind::Corrupt));
                }
                let encoded = self
                    .backend
                    .get_password(&device_key_username(&pointer.active_key_id))
                    .map_err(device_key_error)?;
                Ok(DeviceKey {
                    id: Some(pointer.active_key_id),
                    material: decode_key(&encoded)?,
                })
            }
            Err(error) if error.kind() == DeviceKeyErrorKind::NotFound => {
                let encoded = self
                    .backend
                    .get_password(LEGACY_DATA_KEY_USERNAME)
                    .map_err(device_key_error)?;
                Ok(DeviceKey {
                    id: None,
                    material: decode_key(&encoded)?,
                })
            }
            Err(error) => Err(device_key_error(error)),
        }
    }

    pub(crate) fn load_by_id(&self, id: &str) -> Result<DeviceKey, DeviceKeyError> {
        if !valid_key_id(id) {
            return Err(DeviceKeyError::new(DeviceKeyErrorKind::Unsupported));
        }
        let encoded = self
            .backend
            .get_password(&device_key_username(id))
            .map_err(device_key_error)?;
        Ok(DeviceKey {
            id: Some(id.to_string()),
            material: decode_key(&encoded)?,
        })
    }

    pub(crate) fn create_pending(&self, id: &str) -> Result<PendingDeviceKey, DeviceKeyError> {
        if !valid_key_id(id) {
            return Err(DeviceKeyError::new(DeviceKeyErrorKind::Unsupported));
        }
        let username = device_key_username(id);
        match self.backend.get_password(&username) {
            Ok(_) => return Err(DeviceKeyError::new(DeviceKeyErrorKind::Corrupt)),
            Err(error) if error.kind() == DeviceKeyErrorKind::NotFound => {}
            Err(error) => return Err(device_key_error(error)),
        }

        let material = generate_data_key();
        let encoded = general_purpose::STANDARD.encode(material);
        self.backend
            .set_password(&username, &encoded)
            .map_err(device_key_error)?;
        let readback = self
            .backend
            .get_password(&username)
            .map_err(device_key_error)?;
        let decoded = decode_key(&readback)?;
        if decoded.ct_eq(&material).unwrap_u8() != 1 {
            return Err(DeviceKeyError::new(DeviceKeyErrorKind::Corrupt));
        }
        Ok(PendingDeviceKey {
            id: id.to_string(),
            material,
        })
    }

    pub(crate) fn commit_active(&self, id: &str) -> Result<(), DeviceKeyError> {
        if !valid_key_id(id) {
            return Err(DeviceKeyError::new(DeviceKeyErrorKind::Unsupported));
        }
        let encoded = self
            .backend
            .get_password(&device_key_username(id))
            .map_err(device_key_error)?;
        let _ = decode_key(&encoded)?;
        let pointer = serde_json::to_string(&ActivePointer {
            version: 1,
            active_key_id: id.to_string(),
        })
        .map_err(|_| DeviceKeyError::new(DeviceKeyErrorKind::Internal))?;
        self.backend
            .set_password(ACTIVE_POINTER_USERNAME, &pointer)
            .map_err(device_key_error)?;
        let readback = self
            .backend
            .get_password(ACTIVE_POINTER_USERNAME)
            .map_err(device_key_error)?;
        if readback.as_bytes().ct_eq(pointer.as_bytes()).unwrap_u8() != 1 {
            return Err(DeviceKeyError::new(DeviceKeyErrorKind::Corrupt));
        }
        Ok(())
    }
}

fn device_key_username(id: &str) -> String {
    format!("{DEVICE_KEY_USERNAME_PREFIX}{id}")
}

fn valid_key_id(id: &str) -> bool {
    Uuid::parse_str(id).is_ok()
}

fn decode_key(encoded: &str) -> Result<[u8; 32], DeviceKeyError> {
    let bytes = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| DeviceKeyError::new(DeviceKeyErrorKind::Corrupt))?;
    bytes
        .try_into()
        .map_err(|_| DeviceKeyError::new(DeviceKeyErrorKind::Corrupt))
}

fn device_key_error(error: CredentialBackendError) -> DeviceKeyError {
    DeviceKeyError::new(error.kind())
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::BTreeMap};

    use base64::{engine::general_purpose, Engine as _};

    use super::{
        device_key_username, CredentialBackend, CredentialBackendError, DeviceKeyErrorKind,
        DeviceKeyStore, ACTIVE_POINTER_USERNAME, LEGACY_DATA_KEY_USERNAME,
    };

    #[derive(Debug, Default)]
    struct FakeBackend {
        values: RefCell<BTreeMap<String, String>>,
        get_failures: RefCell<BTreeMap<String, DeviceKeyErrorKind>>,
        set_failures: RefCell<BTreeMap<String, DeviceKeyErrorKind>>,
        rewrite_readback: RefCell<Option<String>>,
        set_count: RefCell<usize>,
    }

    impl FakeBackend {
        fn with_value(self, username: &str, value: String) -> Self {
            self.values.borrow_mut().insert(username.to_string(), value);
            self
        }

        fn fail_get(self, username: &str, kind: DeviceKeyErrorKind) -> Self {
            self.get_failures
                .borrow_mut()
                .insert(username.to_string(), kind);
            self
        }

        fn fail_set(self, username: &str, kind: DeviceKeyErrorKind) -> Self {
            self.set_failures
                .borrow_mut()
                .insert(username.to_string(), kind);
            self
        }

        fn rewrite_next_readback(self, value: &str) -> Self {
            *self.rewrite_readback.borrow_mut() = Some(value.to_string());
            self
        }

        fn set_count(&self) -> usize {
            *self.set_count.borrow()
        }
    }

    impl CredentialBackend for FakeBackend {
        fn get_password(&self, username: &str) -> Result<String, CredentialBackendError> {
            if let Some(kind) = self.get_failures.borrow().get(username).copied() {
                return Err(CredentialBackendError::new(kind));
            }
            let value = self
                .values
                .borrow()
                .get(username)
                .cloned()
                .ok_or(CredentialBackendError::new(DeviceKeyErrorKind::NotFound))?;
            if let Some(rewrite) = self.rewrite_readback.borrow_mut().take() {
                return Ok(rewrite);
            }
            Ok(value)
        }

        fn set_password(
            &self,
            username: &str,
            password: &str,
        ) -> Result<(), CredentialBackendError> {
            if let Some(kind) = self.set_failures.borrow().get(username).copied() {
                return Err(CredentialBackendError::new(kind));
            }
            *self.set_count.borrow_mut() += 1;
            self.values
                .borrow_mut()
                .insert(username.to_string(), password.to_string());
            Ok(())
        }
    }

    #[test]
    fn legacy_reader_loads_existing_local_data_key_without_creating_entries() {
        let legacy_key = [7_u8; 32];
        let backend = FakeBackend::default().with_value(
            LEGACY_DATA_KEY_USERNAME,
            general_purpose::STANDARD.encode(legacy_key),
        );
        let store = DeviceKeyStore::new(backend);

        let loaded = store.load_active_or_legacy().expect("legacy key");

        assert_eq!(loaded.id, None);
        assert_eq!(loaded.material, legacy_key);
        assert_eq!(store.backend.set_count(), 0);
    }

    #[test]
    fn active_pointer_loads_versioned_device_key() {
        let id = DeviceKeyStore::<FakeBackend>::generate_key_id();
        let key = [9_u8; 32];
        let backend = FakeBackend::default()
            .with_value(
                ACTIVE_POINTER_USERNAME,
                serde_json::json!({"version":1,"activeKeyId":id}).to_string(),
            )
            .with_value(
                &device_key_username(&id),
                general_purpose::STANDARD.encode(key),
            );
        let store = DeviceKeyStore::new(backend);

        let loaded = store.load_active_or_legacy().expect("active key");

        assert_eq!(loaded.id.as_deref(), Some(id.as_str()));
        assert_eq!(loaded.material, key);
    }

    #[test]
    fn load_by_id_reads_pending_key_without_switching_active_pointer() {
        let id = DeviceKeyStore::<FakeBackend>::generate_key_id();
        let key = [8_u8; 32];
        let backend = FakeBackend::default().with_value(
            &device_key_username(&id),
            general_purpose::STANDARD.encode(key),
        );
        let store = DeviceKeyStore::new(backend);

        let loaded = store.load_by_id(&id).expect("load by id");

        assert_eq!(loaded.id.as_deref(), Some(id.as_str()));
        assert_eq!(loaded.material, key);
        assert!(matches!(
            store.backend.get_password(ACTIVE_POINTER_USERNAME),
            Err(error) if error.kind() == DeviceKeyErrorKind::NotFound
        ));
    }

    #[test]
    fn create_pending_refuses_to_overwrite_existing_key_and_does_not_commit_active_pointer() {
        let id = DeviceKeyStore::<FakeBackend>::generate_key_id();
        let backend = FakeBackend::default().with_value(
            &device_key_username(&id),
            general_purpose::STANDARD.encode([3_u8; 32]),
        );
        let store = DeviceKeyStore::new(backend);

        let error = store.create_pending(&id).expect_err("overwrite rejected");

        assert_eq!(error.kind(), DeviceKeyErrorKind::Corrupt);
        assert!(matches!(
            store.backend.get_password(ACTIVE_POINTER_USERNAME),
            Err(error) if error.kind() == DeviceKeyErrorKind::NotFound
        ));
    }

    #[test]
    fn create_pending_fails_when_readback_differs_and_active_pointer_stays_absent() {
        let id = DeviceKeyStore::<FakeBackend>::generate_key_id();
        let backend = FakeBackend::default()
            .rewrite_next_readback(&general_purpose::STANDARD.encode([1; 32]));
        let store = DeviceKeyStore::new(backend);

        let error = store.create_pending(&id).expect_err("readback mismatch");

        assert_eq!(error.kind(), DeviceKeyErrorKind::Corrupt);
        assert!(matches!(
            store.backend.get_password(ACTIVE_POINTER_USERNAME),
            Err(error) if error.kind() == DeviceKeyErrorKind::NotFound
        ));
    }

    #[test]
    fn commit_active_only_switches_pointer_after_key_exists() {
        let id = DeviceKeyStore::<FakeBackend>::generate_key_id();
        let backend = FakeBackend::default().with_value(
            &device_key_username(&id),
            general_purpose::STANDARD.encode([4_u8; 32]),
        );
        let store = DeviceKeyStore::new(backend);

        store.commit_active(&id).expect("commit active");
        let loaded = store.load_active_or_legacy().expect("load active");

        assert_eq!(loaded.id.as_deref(), Some(id.as_str()));
        assert_eq!(loaded.material, [4_u8; 32]);
    }

    #[test]
    fn credential_backend_error_matrix_is_stable_and_never_creates_on_load() {
        for kind in [
            DeviceKeyErrorKind::NotFound,
            DeviceKeyErrorKind::Unavailable,
            DeviceKeyErrorKind::PermissionDenied,
            DeviceKeyErrorKind::Corrupt,
            DeviceKeyErrorKind::Unsupported,
            DeviceKeyErrorKind::Internal,
        ] {
            let backend = FakeBackend::default().fail_get(LEGACY_DATA_KEY_USERNAME, kind);
            let store = DeviceKeyStore::new(backend);
            let error = store.load_active_or_legacy().expect_err("load error");

            assert_eq!(error.kind(), kind);
            assert_eq!(store.backend.set_count(), 0);
        }
    }

    #[test]
    fn commit_active_preserves_stable_error_category_when_pointer_write_fails() {
        let id = DeviceKeyStore::<FakeBackend>::generate_key_id();
        let backend = FakeBackend::default()
            .with_value(
                &device_key_username(&id),
                general_purpose::STANDARD.encode([5; 32]),
            )
            .fail_set(
                ACTIVE_POINTER_USERNAME,
                DeviceKeyErrorKind::PermissionDenied,
            );
        let store = DeviceKeyStore::new(backend);

        let error = store.commit_active(&id).expect_err("commit error");

        assert_eq!(error.kind(), DeviceKeyErrorKind::PermissionDenied);
    }
}
