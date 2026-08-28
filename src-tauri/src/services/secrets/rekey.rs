use std::collections::BTreeMap;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::models::secrets::{SecretRecordSelector, VersionedEncryptedSecret};

use super::{
    DeviceKeyId, DeviceKeyResolver, SecretKeyAccessError, SecretKeyMaterial,
    CURRENT_SECRET_ENCRYPTION_VERSION,
};

const AES_GCM_NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretRekeyRowPolicy {
    Include,
    Drop,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRekeyPolicy {
    default: SecretRekeyRowPolicy,
    overrides: BTreeMap<SecretRecordSelector, SecretRekeyRowPolicy>,
}

impl SecretRekeyPolicy {
    pub fn include_all() -> Self {
        Self {
            default: SecretRekeyRowPolicy::Include,
            overrides: BTreeMap::new(),
        }
    }

    pub fn set(mut self, selector: SecretRecordSelector, policy: SecretRekeyRowPolicy) -> Self {
        self.overrides.insert(selector, policy);
        self
    }

    fn policy_for(&self, selector: &SecretRecordSelector) -> SecretRekeyRowPolicy {
        self.overrides
            .get(selector)
            .copied()
            .unwrap_or(self.default)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretRekeyErrorCode {
    Cancelled,
    DestinationExists,
    InputReadOnly,
    InvalidNonce,
    OutputNotActivatable,
    SourceDecryptFailed,
    TargetEncryptFailed,
    UnknownSourceKey,
    UnknownTargetKey,
    UnsupportedEncryptionVersion,
    WriteFailed,
}

impl SecretRekeyErrorCode {
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::DestinationExists => "destination_exists",
            Self::InputReadOnly => "input_read_only",
            Self::InvalidNonce => "invalid_nonce",
            Self::OutputNotActivatable => "output_not_activatable",
            Self::SourceDecryptFailed => "source_decrypt_failed",
            Self::TargetEncryptFailed => "target_encrypt_failed",
            Self::UnknownSourceKey => "unknown_source_key",
            Self::UnknownTargetKey => "unknown_target_key",
            Self::UnsupportedEncryptionVersion => "unsupported_encryption_version",
            Self::WriteFailed => "write_failed",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretRekeyError {
    code: SecretRekeyErrorCode,
    row_index: Option<usize>,
}

impl SecretRekeyError {
    pub fn new(code: SecretRekeyErrorCode) -> Self {
        Self {
            code,
            row_index: None,
        }
    }

    fn at_row(code: SecretRekeyErrorCode, row_index: usize) -> Self {
        Self {
            code,
            row_index: Some(row_index),
        }
    }

    pub fn code(&self) -> SecretRekeyErrorCode {
        self.code
    }

    pub fn stable_code(&self) -> &'static str {
        self.code.stable_code()
    }

    pub fn row_index(&self) -> Option<usize> {
        self.row_index
    }
}

impl std::fmt::Debug for SecretRekeyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretRekeyError")
            .field("code", &self.code.stable_code())
            .field("row_index", &self.row_index)
            .finish()
    }
}

impl std::fmt::Display for SecretRekeyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "secret rekey failed: {}",
            self.code.stable_code()
        )
    }
}

impl std::error::Error for SecretRekeyError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRekeyReport {
    pub from_key_id: String,
    pub to_key_id: String,
    pub included_rows: usize,
    pub dropped_rows: usize,
    pub reset_rows: usize,
    pub code: &'static str,
}

pub trait SecretRekeyWriter {
    fn create_new(&mut self) -> Result<(), SecretRekeyErrorCode>;
    fn write_secret(&mut self, row: VersionedEncryptedSecret) -> Result<(), SecretRekeyErrorCode>;
    fn finish(&mut self) -> Result<(), SecretRekeyErrorCode>;
}

#[derive(Debug, Default)]
pub struct BufferedSecretRekeyWriter {
    rows: Vec<VersionedEncryptedSecret>,
    exists: bool,
    read_only: bool,
    activatable: bool,
    fail_on_write_index: Option<usize>,
}

impl BufferedSecretRekeyWriter {
    pub fn create_new() -> Self {
        Self {
            activatable: true,
            ..Self::default()
        }
    }

    pub fn existing_destination() -> Self {
        Self {
            exists: true,
            activatable: true,
            ..Self::default()
        }
    }

    pub fn read_only_destination() -> Self {
        Self {
            read_only: true,
            activatable: true,
            ..Self::default()
        }
    }

    pub fn not_activatable() -> Self {
        Self {
            activatable: false,
            ..Self::default()
        }
    }

    pub fn fail_on_write_index(index: usize) -> Self {
        Self {
            fail_on_write_index: Some(index),
            activatable: true,
            ..Self::default()
        }
    }

    pub fn rows(&self) -> &[VersionedEncryptedSecret] {
        &self.rows
    }
}

impl SecretRekeyWriter for BufferedSecretRekeyWriter {
    fn create_new(&mut self) -> Result<(), SecretRekeyErrorCode> {
        if self.exists {
            return Err(SecretRekeyErrorCode::DestinationExists);
        }
        if self.read_only {
            return Err(SecretRekeyErrorCode::InputReadOnly);
        }
        Ok(())
    }

    fn write_secret(&mut self, row: VersionedEncryptedSecret) -> Result<(), SecretRekeyErrorCode> {
        if self.fail_on_write_index == Some(self.rows.len()) {
            return Err(SecretRekeyErrorCode::WriteFailed);
        }
        self.rows.push(row);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), SecretRekeyErrorCode> {
        if self.activatable {
            Ok(())
        } else {
            Err(SecretRekeyErrorCode::OutputNotActivatable)
        }
    }
}

pub struct SecretRekeyService {
    source_keys: DeviceKeyResolver,
    target_keys: DeviceKeyResolver,
    target_encryption_version: u16,
}

#[derive(Clone)]
pub struct TransportSecretKey {
    resolver: DeviceKeyResolver,
}

impl TransportSecretKey {
    pub fn generate() -> Self {
        let mut material = [0_u8; 32];
        OsRng.fill_bytes(&mut material);
        Self::from_parts(format!("transport:{}", uuid::Uuid::now_v7()), material)
    }

    pub fn from_parts(key_id: String, material: [u8; 32]) -> Self {
        Self {
            resolver: DeviceKeyResolver::active(
                DeviceKeyId::new(key_id),
                SecretKeyMaterial::from_bytes(material),
                CURRENT_SECRET_ENCRYPTION_VERSION,
            ),
        }
    }

    pub fn key_id(&self) -> &str {
        self.resolver.active_key_id().as_str()
    }

    pub fn resolver(&self) -> DeviceKeyResolver {
        self.resolver.clone()
    }

    pub fn with_key<R>(
        &self,
        action: impl FnOnce(&[u8; 32]) -> R,
    ) -> Result<R, SecretKeyAccessError> {
        self.resolver.with_active_key(action)
    }
}

impl std::fmt::Debug for TransportSecretKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TransportSecretKey")
            .field("key_id", &self.key_id())
            .field("material", &"<redacted>")
            .finish()
    }
}

impl SecretRekeyService {
    pub fn new(
        source_keys: DeviceKeyResolver,
        target_keys: DeviceKeyResolver,
        target_encryption_version: u16,
    ) -> Self {
        Self {
            source_keys,
            target_keys,
            target_encryption_version,
        }
    }

    pub fn rekey<I, W>(
        &self,
        rows: I,
        policy: &SecretRekeyPolicy,
        writer: &mut W,
        cancellation: Option<&CancellationToken>,
    ) -> Result<SecretRekeyReport, SecretRekeyError>
    where
        I: IntoIterator<Item = VersionedEncryptedSecret>,
        W: SecretRekeyWriter,
    {
        writer.create_new().map_err(SecretRekeyError::new)?;
        let mut report = SecretRekeyReport {
            from_key_id: self.source_keys.active_key_id().as_str().to_string(),
            to_key_id: self.target_keys.active_key_id().as_str().to_string(),
            included_rows: 0,
            dropped_rows: 0,
            reset_rows: 0,
            code: "ok",
        };
        for (row_index, row) in rows.into_iter().enumerate() {
            if cancellation.is_some_and(CancellationToken::is_cancelled) {
                return Err(SecretRekeyError::at_row(
                    SecretRekeyErrorCode::Cancelled,
                    row_index,
                ));
            }
            match policy.policy_for(&row.selector) {
                SecretRekeyRowPolicy::Drop => {
                    report.dropped_rows += 1;
                }
                SecretRekeyRowPolicy::Reset => {
                    report.reset_rows += 1;
                }
                SecretRekeyRowPolicy::Include => {
                    let next = self.rekey_row(row, row_index)?;
                    writer
                        .write_secret(next)
                        .map_err(|code| SecretRekeyError::at_row(code, row_index))?;
                    report.included_rows += 1;
                }
            }
        }
        writer.finish().map_err(SecretRekeyError::new)?;
        Ok(report)
    }

    fn rekey_row(
        &self,
        row: VersionedEncryptedSecret,
        row_index: usize,
    ) -> Result<VersionedEncryptedSecret, SecretRekeyError> {
        let plaintext = decrypt_row(&self.source_keys, &row)
            .map_err(|code| SecretRekeyError::at_row(code, row_index))?;
        encrypt_row(
            &self.target_keys,
            &row,
            plaintext,
            self.target_encryption_version,
        )
        .map_err(|code| SecretRekeyError::at_row(code, row_index))
    }
}

fn decrypt_row(
    source_keys: &DeviceKeyResolver,
    row: &VersionedEncryptedSecret,
) -> Result<Zeroizing<Vec<u8>>, SecretRekeyErrorCode> {
    if row.nonce.len() != AES_GCM_NONCE_LEN {
        return Err(SecretRekeyErrorCode::InvalidNonce);
    }
    source_keys
        .with_key(&row.key_id, row.encryption_version, |key| {
            decrypt_bytes(key, &row.ciphertext, &row.nonce, row.aad().as_bytes())
        })
        .map_err(source_key_access_error)?
}

fn encrypt_row(
    target_keys: &DeviceKeyResolver,
    row: &VersionedEncryptedSecret,
    plaintext: Zeroizing<Vec<u8>>,
    target_encryption_version: u16,
) -> Result<VersionedEncryptedSecret, SecretRekeyErrorCode> {
    let mut next = row.clone();
    next.key_id = target_keys.active_key_id().as_str().to_string();
    next.encryption_version = target_encryption_version;
    let aad = next.aad();
    let (ciphertext, nonce) = target_keys
        .with_key(&next.key_id, target_encryption_version, |key| {
            encrypt_bytes(key, plaintext.as_slice(), aad.as_bytes())
        })
        .map_err(target_key_access_error)??;
    next.ciphertext = ciphertext;
    next.nonce = nonce;
    next.value_hash = hash_bytes(plaintext.as_slice());
    Ok(next)
}

fn encrypt_bytes(
    key: &[u8; 32],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), SecretRekeyErrorCode> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| SecretRekeyErrorCode::TargetEncryptFailed)?;
    let mut nonce = vec![0_u8; AES_GCM_NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| SecretRekeyErrorCode::TargetEncryptFailed)?;
    Ok((ciphertext, nonce))
}

fn decrypt_bytes(
    key: &[u8; 32],
    ciphertext: &[u8],
    nonce: &[u8],
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, SecretRekeyErrorCode> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| SecretRekeyErrorCode::SourceDecryptFailed)?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(nonce),
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| SecretRekeyErrorCode::SourceDecryptFailed)?;
    Ok(Zeroizing::new(plaintext))
}

fn source_key_access_error(error: SecretKeyAccessError) -> SecretRekeyErrorCode {
    match error {
        SecretKeyAccessError::UnknownKeyId => SecretRekeyErrorCode::UnknownSourceKey,
        SecretKeyAccessError::UnsupportedEncryptionVersion => {
            SecretRekeyErrorCode::UnsupportedEncryptionVersion
        }
    }
}

fn target_key_access_error(error: SecretKeyAccessError) -> SecretRekeyErrorCode {
    match error {
        SecretKeyAccessError::UnknownKeyId => SecretRekeyErrorCode::UnknownTargetKey,
        SecretKeyAccessError::UnsupportedEncryptionVersion => {
            SecretRekeyErrorCode::UnsupportedEncryptionVersion
        }
    }
}

fn hash_bytes(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    general_purpose::STANDARD.encode(digest)
}
