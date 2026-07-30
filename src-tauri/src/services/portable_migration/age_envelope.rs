use std::{
    fmt,
    io::{self, BufReader, Read, Write},
};

use age::{
    scrypt, secrecy::SecretString, DecryptError, EncryptError, Encryptor, Identity, Recipient,
};

use super::{
    format::{
        read_framed_payload, write_framed_payload, ParsedPortablePayload,
        ParsedPortablePayloadInfo, PortableFormatError, PortableMigrationManifest,
        TransportKeyMaterial,
    },
    limits::{LimitViolation, PortableMigrationLimitsV1},
};

pub(crate) const DEFAULT_SCRYPT_WORK_FACTOR: u8 = 15;
pub(crate) const DEFAULT_MAX_ACCEPTED_SCRYPT_WORK_FACTOR: u8 = 18;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgeEnvelopeOptions {
    pub(crate) work_factor: u8,
    pub(crate) max_accepted_work_factor: u8,
    pub(crate) limits: PortableMigrationLimitsV1,
}

impl AgeEnvelopeOptions {
    pub(crate) const CURRENT: Self = Self {
        work_factor: DEFAULT_SCRYPT_WORK_FACTOR,
        max_accepted_work_factor: DEFAULT_MAX_ACCEPTED_SCRYPT_WORK_FACTOR,
        limits: PortableMigrationLimitsV1::CURRENT,
    };

    #[cfg(test)]
    pub(crate) const TEST_FAST: Self = Self {
        work_factor: 10,
        max_accepted_work_factor: 12,
        limits: PortableMigrationLimitsV1::CURRENT,
    };
}

impl Default for AgeEnvelopeOptions {
    fn default() -> Self {
        Self::CURRENT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgeEnvelopeErrorCode {
    AuthenticationFailed,
    Cancelled,
    ExcessiveWork,
    InvalidFormat,
    Io,
    LimitExceeded,
}

impl AgeEnvelopeErrorCode {
    pub(crate) const fn stable_code(self) -> &'static str {
        match self {
            Self::AuthenticationFailed => "authentication_failed",
            Self::Cancelled => "cancelled",
            Self::ExcessiveWork => "excessive_work",
            Self::InvalidFormat => "invalid_format",
            Self::Io => "io",
            Self::LimitExceeded => "limit_exceeded",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AgeEnvelopeError {
    code: AgeEnvelopeErrorCode,
}

impl AgeEnvelopeError {
    pub(crate) const fn new(code: AgeEnvelopeErrorCode) -> Self {
        Self { code }
    }

    pub(crate) const fn code(&self) -> AgeEnvelopeErrorCode {
        self.code
    }

    pub(crate) const fn stable_code(&self) -> &'static str {
        self.code.stable_code()
    }
}

impl fmt::Debug for AgeEnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgeEnvelopeError")
            .field("code", &self.code.stable_code())
            .finish()
    }
}

impl fmt::Display for AgeEnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "portable migration age envelope failed: {}",
            self.code.stable_code()
        )
    }
}

impl std::error::Error for AgeEnvelopeError {}

pub(crate) type AgeEnvelopeResult<T> = Result<T, AgeEnvelopeError>;

pub(crate) fn encrypt_framed_payload<W: Write, R: Read>(
    writer: W,
    passphrase: &str,
    manifest: &PortableMigrationManifest,
    transport_key: &TransportKeyMaterial,
    sqlite_reader: R,
    expected_record_count_keys: &[&str],
    options: AgeEnvelopeOptions,
) -> AgeEnvelopeResult<[u8; 32]> {
    options
        .limits
        .validate_passphrase(passphrase)
        .map_err(limit_error)?;
    let mut recipient = scrypt::Recipient::new(secret_string(passphrase));
    recipient.set_work_factor(options.work_factor);
    let encryptor = Encryptor::with_recipients(std::iter::once(&recipient as &dyn Recipient))
        .map_err(encrypt_error)?;
    let mut bounded = BoundedWrite::new(writer, options.limits.max_age_file_bytes);
    let (digest, finish) = {
        let mut age_writer = encryptor.wrap_output(&mut bounded).map_err(io_error)?;
        let digest = write_framed_payload(
            &mut age_writer,
            manifest,
            transport_key,
            sqlite_reader,
            expected_record_count_keys,
            options.limits,
        );
        if digest.is_ok() {
            let finish = age_writer.finish().map(|_| ());
            (digest, Some(finish))
        } else {
            (digest, None)
        }
    };
    if bounded.limit_exceeded() {
        return Err(AgeEnvelopeError::new(AgeEnvelopeErrorCode::LimitExceeded));
    }
    let digest = digest.map_err(format_error)?;
    if let Some(finish) = finish {
        finish.map_err(io_error)?;
    }
    Ok(digest)
}

pub(crate) fn decrypt_framed_payload<R: Read>(
    reader: R,
    passphrase: &str,
    expected_record_count_keys: &[&str],
    options: AgeEnvelopeOptions,
) -> AgeEnvelopeResult<ParsedPortablePayload> {
    options
        .limits
        .validate_passphrase(passphrase)
        .map_err(limit_error)?;
    let bounded = BufReader::new(BoundedRead::new(reader, options.limits.max_age_file_bytes));
    let decryptor = age::Decryptor::new_buffered(bounded).map_err(decrypt_error)?;
    let mut identity = scrypt::Identity::new(secret_string(passphrase));
    identity.set_max_work_factor(options.max_accepted_work_factor);
    let mut plaintext = decryptor
        .decrypt(std::iter::once(&identity as &dyn Identity))
        .map_err(decrypt_error)?;
    read_framed_payload(&mut plaintext, expected_record_count_keys, options.limits)
        .map_err(format_error)
}

pub(crate) fn decrypt_framed_payload_to_writer<R: Read, W: Write>(
    reader: R,
    passphrase: &str,
    expected_record_count_keys: &[&str],
    options: AgeEnvelopeOptions,
    sqlite_writer: W,
) -> AgeEnvelopeResult<ParsedPortablePayloadInfo> {
    options
        .limits
        .validate_passphrase(passphrase)
        .map_err(limit_error)?;
    let bounded = BufReader::new(BoundedRead::new(reader, options.limits.max_age_file_bytes));
    let decryptor = age::Decryptor::new_buffered(bounded).map_err(decrypt_error)?;
    let mut identity = scrypt::Identity::new(secret_string(passphrase));
    identity.set_max_work_factor(options.max_accepted_work_factor);
    let mut plaintext = decryptor
        .decrypt(std::iter::once(&identity as &dyn Identity))
        .map_err(decrypt_error)?;
    super::format::read_framed_payload_to_writer(
        &mut plaintext,
        expected_record_count_keys,
        options.limits,
        sqlite_writer,
    )
    .map_err(format_error)
}

fn secret_string(passphrase: &str) -> SecretString {
    SecretString::from(passphrase.to_owned())
}

fn limit_error(error: LimitViolation) -> AgeEnvelopeError {
    match error {
        LimitViolation::PassphraseTooLarge => {
            AgeEnvelopeError::new(AgeEnvelopeErrorCode::LimitExceeded)
        }
        LimitViolation::RegularFieldTooLarge | LimitViolation::LargeRedactedJsonFieldTooLarge => {
            AgeEnvelopeError::new(AgeEnvelopeErrorCode::LimitExceeded)
        }
    }
}

fn encrypt_error(error: EncryptError) -> AgeEnvelopeError {
    match error {
        EncryptError::Io(_) => AgeEnvelopeError::new(AgeEnvelopeErrorCode::Io),
        #[allow(unreachable_patterns)]
        _ => AgeEnvelopeError::new(AgeEnvelopeErrorCode::InvalidFormat),
    }
}

fn io_error(_: io::Error) -> AgeEnvelopeError {
    AgeEnvelopeError::new(AgeEnvelopeErrorCode::Io)
}

fn decrypt_error(error: DecryptError) -> AgeEnvelopeError {
    match error {
        DecryptError::DecryptionFailed
        | DecryptError::InvalidMac
        | DecryptError::KeyDecryptionFailed
        | DecryptError::NoMatchingKeys => {
            AgeEnvelopeError::new(AgeEnvelopeErrorCode::AuthenticationFailed)
        }
        DecryptError::ExcessiveWork { .. } => {
            AgeEnvelopeError::new(AgeEnvelopeErrorCode::ExcessiveWork)
        }
        DecryptError::InvalidHeader | DecryptError::UnknownFormat => {
            AgeEnvelopeError::new(AgeEnvelopeErrorCode::InvalidFormat)
        }
        DecryptError::Io(_) => AgeEnvelopeError::new(AgeEnvelopeErrorCode::Io),
        #[allow(unreachable_patterns)]
        _ => AgeEnvelopeError::new(AgeEnvelopeErrorCode::InvalidFormat),
    }
}

fn format_error(error: PortableFormatError) -> AgeEnvelopeError {
    match error {
        PortableFormatError::LengthLimitExceeded | PortableFormatError::LengthOverflow => {
            AgeEnvelopeError::new(AgeEnvelopeErrorCode::LimitExceeded)
        }
        PortableFormatError::Truncated | PortableFormatError::DigestMismatch => {
            AgeEnvelopeError::new(AgeEnvelopeErrorCode::AuthenticationFailed)
        }
        PortableFormatError::UnsupportedFraming
        | PortableFormatError::TrailingBytes
        | PortableFormatError::ManifestInvalid
        | PortableFormatError::DuplicateJsonKey
        | PortableFormatError::UnknownTopLevelField
        | PortableFormatError::UnsupportedManifestVersion
        | PortableFormatError::InvalidIdentifier
        | PortableFormatError::InvalidTimestamp
        | PortableFormatError::InvalidSemver
        | PortableFormatError::InvalidFeatureName
        | PortableFormatError::InvalidCategories
        | PortableFormatError::InvalidRecordCounts
        | PortableFormatError::InvalidDigest => {
            AgeEnvelopeError::new(AgeEnvelopeErrorCode::InvalidFormat)
        }
    }
}

struct BoundedRead<R> {
    inner: R,
    remaining: u64,
}

impl<R> BoundedRead<R> {
    const fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<R: Read> Read for BoundedRead<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 && !buffer.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "portable migration age envelope exceeds limit",
            ));
        }
        let allowed = buffer
            .len()
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        let read = self.inner.read(&mut buffer[..allowed])?;
        self.remaining = self.remaining.saturating_sub(read as u64);
        Ok(read)
    }
}

struct BoundedWrite<W> {
    inner: W,
    written: u64,
    limit: u64,
    exceeded: bool,
}

impl<W> BoundedWrite<W> {
    const fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            written: 0,
            limit,
            exceeded: false,
        }
    }

    const fn limit_exceeded(&self) -> bool {
        self.exceeded
    }
}

impl<W: Write> Write for BoundedWrite<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next = self.written.saturating_add(buffer.len() as u64);
        if next > self.limit {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "portable migration age envelope exceeds limit",
            ));
        }
        let written = self.inner.write(buffer)?;
        self.written = self.written.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Cursor};

    use base64::{engine::general_purpose, Engine as _};
    use sha2::{Digest, Sha256};

    use super::{
        decrypt_framed_payload, encrypt_framed_payload, AgeEnvelopeErrorCode, AgeEnvelopeOptions,
    };
    use crate::services::portable_migration::format::{
        PortableMigrationManifest, TransportKeyMaterial, PORTABLE_MIGRATION_MIN_SCHEMA_VERSION,
    };

    const FIXTURE_KEYS: [&str; 2] = ["station_keys", "stations"];

    #[test]
    fn age_envelope_round_trips_framed_payload_to_authenticated_eof() {
        let sqlite = b"SQLite fixture";
        let mut encrypted = Vec::new();
        let manifest = manifest(sqlite);
        let key = TransportKeyMaterial::from_bytes([8; 32]);

        encrypt_framed_payload(
            &mut encrypted,
            "correct horse battery staple",
            &manifest,
            &key,
            Cursor::new(sqlite),
            &FIXTURE_KEYS,
            AgeEnvelopeOptions::TEST_FAST,
        )
        .expect("encrypt");

        let parsed = decrypt_framed_payload(
            Cursor::new(&encrypted),
            "correct horse battery staple",
            &FIXTURE_KEYS,
            AgeEnvelopeOptions::TEST_FAST,
        )
        .expect("decrypt");
        assert_eq!(parsed.manifest, manifest);
        assert_eq!(parsed.sqlite_bytes, sqlite);
        parsed
            .transport_key
            .with_bytes(|bytes| assert_eq!(bytes, &[8; 32]));
    }

    #[test]
    fn age_envelope_wrong_passphrase_and_truncation_share_public_code() {
        let sqlite = b"SQLite fixture";
        let mut encrypted = Vec::new();
        let manifest = manifest(sqlite);
        let key = TransportKeyMaterial::from_bytes([8; 32]);
        encrypt_framed_payload(
            &mut encrypted,
            "passphrase",
            &manifest,
            &key,
            Cursor::new(sqlite),
            &FIXTURE_KEYS,
            AgeEnvelopeOptions::TEST_FAST,
        )
        .expect("encrypt");

        let wrong = decrypt_framed_payload(
            Cursor::new(&encrypted),
            "wrong",
            &FIXTURE_KEYS,
            AgeEnvelopeOptions::TEST_FAST,
        )
        .unwrap_err();
        let mut truncated = encrypted;
        truncated.truncate(truncated.len().saturating_sub(8));
        let truncation = decrypt_framed_payload(
            Cursor::new(&truncated),
            "passphrase",
            &FIXTURE_KEYS,
            AgeEnvelopeOptions::TEST_FAST,
        )
        .unwrap_err();

        assert_eq!(wrong.code(), AgeEnvelopeErrorCode::AuthenticationFailed);
        assert_eq!(
            truncation.code(),
            AgeEnvelopeErrorCode::AuthenticationFailed
        );
    }

    #[test]
    fn age_envelope_rejects_passphrase_utf8_boundary_over_limit() {
        let exact = "界".repeat(PortableMigrationLimitsV1::CURRENT.max_passphrase_utf8_bytes / 3);
        let too_large = format!("{exact}ab");
        let sqlite = b"SQLite fixture";
        let manifest = manifest(sqlite);
        let key = TransportKeyMaterial::from_bytes([8; 32]);
        let mut encrypted = Vec::new();

        encrypt_framed_payload(
            &mut encrypted,
            &too_large,
            &manifest,
            &key,
            Cursor::new(sqlite),
            &FIXTURE_KEYS,
            AgeEnvelopeOptions::TEST_FAST,
        )
        .expect_err("oversized utf-8 passphrase must fail");
    }

    fn manifest(sqlite: &[u8]) -> PortableMigrationManifest {
        let mut record_counts = BTreeMap::new();
        record_counts.insert("station_keys".to_string(), 0);
        record_counts.insert("stations".to_string(), 0);
        PortableMigrationManifest {
            format: "relay-pool-portable-migration".to_string(),
            format_version: 1,
            export_id: "018f7f9a-1111-7000-8000-000000000001".to_string(),
            created_at: "2026-07-29T00:00:00Z".to_string(),
            source_app_version: "0.3.3".to_string(),
            source_platform: "windows".to_string(),
            database_generation: 2,
            database_schema_version: PORTABLE_MIGRATION_MIN_SCHEMA_VERSION,
            portable_schema_profile: "encrypted-secrets-v1".to_string(),
            minimum_importer_version: "0.3.3".to_string(),
            transport_key_id: "transport:018f7f9a-1111-7000-8000-000000000002".to_string(),
            encryption_version: 1,
            export_policy_version: 1,
            required_features: vec![],
            extensions: serde_json::json!({}),
            included_categories: vec!["core_data".to_string(), "history".to_string()],
            excluded_categories: vec![
                "session_credentials".to_string(),
                "local_proxy_access_key".to_string(),
                "device_runtime_state".to_string(),
                "provider_drafts".to_string(),
            ],
            record_counts,
            sqlite_size_bytes: sqlite.len() as u64,
            sqlite_sha256: general_purpose::STANDARD.encode(Sha256::digest(sqlite)),
        }
    }

    use crate::services::portable_migration::limits::PortableMigrationLimitsV1;
}
