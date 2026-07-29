use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{Read, Write},
};

use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Serialize,
};
use serde_json::{Number, Value};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::limits::PortableMigrationLimitsV1;

pub(crate) const PORTABLE_MIGRATION_MAGIC: &[u8; 8] = b"RPDMOVE1";
pub(crate) const PORTABLE_MIGRATION_FORMAT: &str = "relay-pool-portable-migration";
pub(crate) const PORTABLE_MIGRATION_FORMAT_VERSION: u64 = 1;
pub(crate) const PORTABLE_MIGRATION_DATABASE_GENERATION: u64 = 2;
pub(crate) const PORTABLE_MIGRATION_MIN_SCHEMA_VERSION: u64 = 10;
pub(crate) const PORTABLE_MIGRATION_SCHEMA_PROFILE: &str = "encrypted-secrets-v1";
pub(crate) const PORTABLE_MIGRATION_ENCRYPTION_VERSION: u64 = 1;
pub(crate) const PORTABLE_MIGRATION_EXPORT_POLICY_VERSION: u64 = 1;

const MAX_SHORT_STRING_BYTES: usize = 256;

const TOP_LEVEL_FIELDS: [&str; 20] = [
    "format",
    "formatVersion",
    "exportId",
    "createdAt",
    "sourceAppVersion",
    "sourcePlatform",
    "databaseGeneration",
    "databaseSchemaVersion",
    "portableSchemaProfile",
    "minimumImporterVersion",
    "transportKeyId",
    "encryptionVersion",
    "exportPolicyVersion",
    "requiredFeatures",
    "extensions",
    "includedCategories",
    "excludedCategories",
    "recordCounts",
    "sqliteSizeBytes",
    "sqliteSha256",
];

const CATEGORY_CORE_DATA: &str = "core_data";
const CATEGORY_HISTORY: &str = "history";
const CATEGORY_SESSION_CREDENTIALS: &str = "session_credentials";
const CATEGORY_LOCAL_PROXY_ACCESS_KEY: &str = "local_proxy_access_key";
const CATEGORY_DEVICE_RUNTIME_STATE: &str = "device_runtime_state";
const CATEGORY_PROVIDER_DRAFTS: &str = "provider_drafts";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PortableMigrationManifest {
    pub(crate) format: String,
    pub(crate) format_version: u64,
    pub(crate) export_id: String,
    pub(crate) created_at: String,
    pub(crate) source_app_version: String,
    pub(crate) source_platform: String,
    pub(crate) database_generation: u64,
    pub(crate) database_schema_version: u64,
    pub(crate) portable_schema_profile: String,
    pub(crate) minimum_importer_version: String,
    pub(crate) transport_key_id: String,
    pub(crate) encryption_version: u64,
    pub(crate) export_policy_version: u64,
    pub(crate) required_features: Vec<String>,
    pub(crate) extensions: Value,
    pub(crate) included_categories: Vec<String>,
    pub(crate) excluded_categories: Vec<String>,
    pub(crate) record_counts: BTreeMap<String, u64>,
    pub(crate) sqlite_size_bytes: u64,
    pub(crate) sqlite_sha256: String,
}

impl PortableMigrationManifest {
    pub(crate) fn to_canonical_json(&self) -> Result<Vec<u8>, PortableFormatError> {
        serde_json::to_vec(self).map_err(|_| PortableFormatError::ManifestInvalid)
    }
}

pub(crate) struct TransportKeyMaterial(Zeroizing<[u8; 32]>);

impl TransportKeyMaterial {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub(crate) fn with_bytes<R>(&self, action: impl FnOnce(&[u8; 32]) -> R) -> R {
        action(&self.0)
    }
}

impl fmt::Debug for TransportKeyMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TransportKeyMaterial { redacted: true }")
    }
}

#[derive(Debug)]
pub(crate) struct ParsedPortablePayload {
    pub(crate) manifest: PortableMigrationManifest,
    pub(crate) transport_key: TransportKeyMaterial,
    pub(crate) sqlite_bytes: Vec<u8>,
    pub(crate) sqlite_sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PortableFormatError {
    #[error("portable migration framing is unsupported")]
    UnsupportedFraming,
    #[error("portable migration frame is truncated")]
    Truncated,
    #[error("portable migration frame has trailing bytes")]
    TrailingBytes,
    #[error("portable migration length exceeds v1 limit")]
    LengthLimitExceeded,
    #[error("portable migration frame length overflowed")]
    LengthOverflow,
    #[error("portable migration manifest JSON is invalid")]
    ManifestInvalid,
    #[error("portable migration manifest contains duplicate JSON keys")]
    DuplicateJsonKey,
    #[error("portable migration manifest contains unknown top-level fields")]
    UnknownTopLevelField,
    #[error("portable migration manifest has an unsupported version")]
    UnsupportedManifestVersion,
    #[error("portable migration manifest has an invalid identifier")]
    InvalidIdentifier,
    #[error("portable migration manifest has an invalid timestamp")]
    InvalidTimestamp,
    #[error("portable migration manifest has an invalid SemVer")]
    InvalidSemver,
    #[error("portable migration manifest has invalid feature names")]
    InvalidFeatureName,
    #[error("portable migration manifest has invalid categories")]
    InvalidCategories,
    #[error("portable migration manifest has invalid record counts")]
    InvalidRecordCounts,
    #[error("portable migration manifest digest is invalid")]
    InvalidDigest,
    #[error("portable migration SQLite digest does not match manifest")]
    DigestMismatch,
}

pub(crate) fn parse_manifest(
    bytes: &[u8],
    expected_record_count_keys: &[&str],
    limits: PortableMigrationLimitsV1,
) -> Result<PortableMigrationManifest, PortableFormatError> {
    if bytes.len() > limits.max_manifest_bytes {
        return Err(PortableFormatError::LengthLimitExceeded);
    }
    let value = parse_json_value_rejecting_duplicates(bytes)?;
    validate_json_depth(&value, limits.max_json_depth)?;
    manifest_from_value(value, expected_record_count_keys, limits)
}

pub(crate) fn read_framed_payload<R: Read>(
    mut reader: R,
    expected_record_count_keys: &[&str],
    limits: PortableMigrationLimitsV1,
) -> Result<ParsedPortablePayload, PortableFormatError> {
    let mut magic = [0_u8; 8];
    read_exact_or_truncated(&mut reader, &mut magic)?;
    if &magic != PORTABLE_MIGRATION_MAGIC {
        return Err(PortableFormatError::UnsupportedFraming);
    }

    let manifest_len = read_u32_be(&mut reader)? as usize;
    if manifest_len > limits.max_manifest_bytes {
        return Err(PortableFormatError::LengthLimitExceeded);
    }
    let mut manifest_bytes = vec![0_u8; manifest_len];
    read_exact_or_truncated(&mut reader, &mut manifest_bytes)?;
    let manifest = parse_manifest(&manifest_bytes, expected_record_count_keys, limits)?;

    let mut transport_key = [0_u8; 32];
    read_exact_or_truncated(&mut reader, &mut transport_key)?;
    let transport_key = TransportKeyMaterial::from_bytes(transport_key);

    let sqlite_len = read_u64_be(&mut reader)?;
    if sqlite_len > limits.max_sqlite_bytes || manifest.sqlite_size_bytes != sqlite_len {
        return Err(PortableFormatError::LengthLimitExceeded);
    }
    if limits
        .decrypted_payload_upper_bound(manifest_len, sqlite_len)
        .ok_or(PortableFormatError::LengthOverflow)?
        > limits.max_age_file_bytes
    {
        return Err(PortableFormatError::LengthLimitExceeded);
    }
    let sqlite_len_usize =
        usize::try_from(sqlite_len).map_err(|_| PortableFormatError::LengthOverflow)?;
    let mut sqlite_bytes = vec![0_u8; sqlite_len_usize];
    read_exact_or_truncated(&mut reader, &mut sqlite_bytes)?;

    let mut tail_digest = [0_u8; 32];
    read_exact_or_truncated(&mut reader, &mut tail_digest)?;

    let actual_digest = Sha256::digest(&sqlite_bytes);
    let actual_digest: [u8; 32] = actual_digest.into();
    let manifest_digest = decode_sha256_digest(&manifest.sqlite_sha256)?;
    if tail_digest != actual_digest || manifest_digest != actual_digest {
        return Err(PortableFormatError::DigestMismatch);
    }

    let mut trailing = [0_u8; 1];
    match reader.read(&mut trailing) {
        Ok(0) => {}
        Ok(_) => return Err(PortableFormatError::TrailingBytes),
        Err(_) => return Err(PortableFormatError::Truncated),
    }

    Ok(ParsedPortablePayload {
        manifest,
        transport_key,
        sqlite_bytes,
        sqlite_sha256: actual_digest,
    })
}

pub(crate) fn write_framed_payload<W: Write, R: Read>(
    mut writer: W,
    manifest: &PortableMigrationManifest,
    transport_key: &TransportKeyMaterial,
    mut sqlite_reader: R,
    expected_record_count_keys: &[&str],
    limits: PortableMigrationLimitsV1,
) -> Result<[u8; 32], PortableFormatError> {
    let manifest_bytes = manifest.to_canonical_json()?;
    let reparsed = parse_manifest(&manifest_bytes, expected_record_count_keys, limits)?;
    if reparsed != *manifest {
        return Err(PortableFormatError::ManifestInvalid);
    }
    let manifest_len = u32::try_from(manifest_bytes.len())
        .map_err(|_| PortableFormatError::LengthLimitExceeded)?;
    if manifest.sqlite_size_bytes > limits.max_sqlite_bytes {
        return Err(PortableFormatError::LengthLimitExceeded);
    }
    if limits
        .decrypted_payload_upper_bound(manifest_bytes.len(), manifest.sqlite_size_bytes)
        .ok_or(PortableFormatError::LengthOverflow)?
        > limits.max_age_file_bytes
    {
        return Err(PortableFormatError::LengthLimitExceeded);
    }

    write_all(&mut writer, PORTABLE_MIGRATION_MAGIC)?;
    write_all(&mut writer, &manifest_len.to_be_bytes())?;
    write_all(&mut writer, &manifest_bytes)?;
    transport_key.with_bytes(|bytes| write_all(&mut writer, bytes))?;
    write_all(&mut writer, &manifest.sqlite_size_bytes.to_be_bytes())?;

    let mut hasher = Sha256::new();
    let mut remaining = manifest.sqlite_size_bytes;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining > 0 {
        let chunk_len = buffer
            .len()
            .min(usize::try_from(remaining).unwrap_or(buffer.len()));
        let read = sqlite_reader
            .read(&mut buffer[..chunk_len])
            .map_err(|_| PortableFormatError::Truncated)?;
        if read == 0 {
            return Err(PortableFormatError::Truncated);
        }
        hasher.update(&buffer[..read]);
        write_all(&mut writer, &buffer[..read])?;
        remaining -= read as u64;
    }
    let mut trailing = [0_u8; 1];
    if sqlite_reader
        .read(&mut trailing)
        .map_err(|_| PortableFormatError::Truncated)?
        != 0
    {
        return Err(PortableFormatError::TrailingBytes);
    }

    let digest: [u8; 32] = hasher.finalize().into();
    if general_purpose::STANDARD.encode(digest) != manifest.sqlite_sha256 {
        return Err(PortableFormatError::DigestMismatch);
    }
    write_all(&mut writer, &digest)?;
    Ok(digest)
}

fn manifest_from_value(
    value: Value,
    expected_record_count_keys: &[&str],
    limits: PortableMigrationLimitsV1,
) -> Result<PortableMigrationManifest, PortableFormatError> {
    let Value::Object(mut object) = value else {
        return Err(PortableFormatError::ManifestInvalid);
    };
    let allowed = BTreeSet::from(TOP_LEVEL_FIELDS);
    if object.keys().any(|key| !allowed.contains(key.as_str())) {
        return Err(PortableFormatError::UnknownTopLevelField);
    }

    let manifest = PortableMigrationManifest {
        format: take_string(&mut object, "format")?,
        format_version: take_u64(&mut object, "formatVersion")?,
        export_id: take_string(&mut object, "exportId")?,
        created_at: take_string(&mut object, "createdAt")?,
        source_app_version: take_string(&mut object, "sourceAppVersion")?,
        source_platform: take_string(&mut object, "sourcePlatform")?,
        database_generation: take_u64(&mut object, "databaseGeneration")?,
        database_schema_version: take_u64(&mut object, "databaseSchemaVersion")?,
        portable_schema_profile: take_string(&mut object, "portableSchemaProfile")?,
        minimum_importer_version: take_string(&mut object, "minimumImporterVersion")?,
        transport_key_id: take_string(&mut object, "transportKeyId")?,
        encryption_version: take_u64(&mut object, "encryptionVersion")?,
        export_policy_version: take_u64(&mut object, "exportPolicyVersion")?,
        required_features: take_string_vec(&mut object, "requiredFeatures")?,
        extensions: take_required(&mut object, "extensions")?,
        included_categories: take_string_vec(&mut object, "includedCategories")?,
        excluded_categories: take_string_vec(&mut object, "excludedCategories")?,
        record_counts: take_record_counts(&mut object, "recordCounts")?,
        sqlite_size_bytes: take_u64(&mut object, "sqliteSizeBytes")?,
        sqlite_sha256: take_string(&mut object, "sqliteSha256")?,
    };
    if !object.is_empty() {
        return Err(PortableFormatError::ManifestInvalid);
    }
    validate_manifest(&manifest, expected_record_count_keys, limits)?;
    Ok(manifest)
}

fn validate_manifest(
    manifest: &PortableMigrationManifest,
    expected_record_count_keys: &[&str],
    limits: PortableMigrationLimitsV1,
) -> Result<(), PortableFormatError> {
    if manifest.format != PORTABLE_MIGRATION_FORMAT
        || manifest.format_version != PORTABLE_MIGRATION_FORMAT_VERSION
        || manifest.database_generation != PORTABLE_MIGRATION_DATABASE_GENERATION
        || manifest.database_schema_version < PORTABLE_MIGRATION_MIN_SCHEMA_VERSION
        || manifest.portable_schema_profile != PORTABLE_MIGRATION_SCHEMA_PROFILE
        || manifest.encryption_version != PORTABLE_MIGRATION_ENCRYPTION_VERSION
        || manifest.export_policy_version != PORTABLE_MIGRATION_EXPORT_POLICY_VERSION
    {
        return Err(PortableFormatError::UnsupportedManifestVersion);
    }
    validate_short_string(&manifest.source_platform)?;
    validate_semver(&manifest.source_app_version)?;
    validate_semver(&manifest.minimum_importer_version)?;
    validate_uuid_v7(&manifest.export_id)?;
    let transport_uuid = manifest
        .transport_key_id
        .strip_prefix("transport:")
        .ok_or(PortableFormatError::InvalidIdentifier)?;
    validate_uuid_v7(transport_uuid)?;
    validate_utc_rfc3339(&manifest.created_at)?;
    decode_sha256_digest(&manifest.sqlite_sha256)?;
    validate_features(&manifest.required_features, limits)?;
    validate_extensions(&manifest.extensions, limits)?;
    validate_categories(&manifest.included_categories, &manifest.excluded_categories)?;
    validate_record_counts(&manifest.record_counts, expected_record_count_keys, limits)?;
    Ok(())
}

fn take_required(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<Value, PortableFormatError> {
    object
        .remove(key)
        .ok_or(PortableFormatError::ManifestInvalid)
}

fn take_string(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, PortableFormatError> {
    let Value::String(value) = take_required(object, key)? else {
        return Err(PortableFormatError::ManifestInvalid);
    };
    validate_short_string(&value)?;
    Ok(value)
}

fn take_u64(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<u64, PortableFormatError> {
    take_required(object, key)?
        .as_u64()
        .ok_or(PortableFormatError::ManifestInvalid)
}

fn take_string_vec(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, PortableFormatError> {
    let Value::Array(values) = take_required(object, key)? else {
        return Err(PortableFormatError::ManifestInvalid);
    };
    values
        .into_iter()
        .map(|value| match value {
            Value::String(value) => {
                validate_short_string(&value)?;
                Ok(value)
            }
            _ => Err(PortableFormatError::ManifestInvalid),
        })
        .collect()
}

fn take_record_counts(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<BTreeMap<String, u64>, PortableFormatError> {
    let Value::Object(values) = take_required(object, key)? else {
        return Err(PortableFormatError::ManifestInvalid);
    };
    values
        .into_iter()
        .map(|(key, value)| {
            validate_short_string(&key)?;
            let count = value
                .as_u64()
                .ok_or(PortableFormatError::InvalidRecordCounts)?;
            Ok((key, count))
        })
        .collect()
}

fn validate_short_string(value: &str) -> Result<(), PortableFormatError> {
    if value.is_empty() || value.len() > MAX_SHORT_STRING_BYTES {
        return Err(PortableFormatError::ManifestInvalid);
    }
    Ok(())
}

fn validate_semver(value: &str) -> Result<(), PortableFormatError> {
    let version = semver::Version::parse(value).map_err(|_| PortableFormatError::InvalidSemver)?;
    if version.to_string() != value {
        return Err(PortableFormatError::InvalidSemver);
    }
    Ok(())
}

fn validate_uuid_v7(value: &str) -> Result<(), PortableFormatError> {
    if value.len() != 36
        || !value.chars().enumerate().all(|(index, ch)| match index {
            8 | 13 | 18 | 23 => ch == '-',
            _ => ch.is_ascii_digit() || ('a'..='f').contains(&ch),
        })
        || value.as_bytes()[14] != b'7'
        || !matches!(value.as_bytes()[19], b'8' | b'9' | b'a' | b'b')
    {
        return Err(PortableFormatError::InvalidIdentifier);
    }
    uuid::Uuid::parse_str(value).map_err(|_| PortableFormatError::InvalidIdentifier)?;
    Ok(())
}

fn validate_utc_rfc3339(value: &str) -> Result<(), PortableFormatError> {
    let parsed =
        DateTime::parse_from_rfc3339(value).map_err(|_| PortableFormatError::InvalidTimestamp)?;
    if parsed.offset().local_minus_utc() != 0
        || !value.ends_with('Z')
        || parsed
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true)
            != value
    {
        return Err(PortableFormatError::InvalidTimestamp);
    }
    Ok(())
}

fn decode_sha256_digest(value: &str) -> Result<[u8; 32], PortableFormatError> {
    if value.len() != 44 || !value.ends_with('=') {
        return Err(PortableFormatError::InvalidDigest);
    }
    let decoded = general_purpose::STANDARD
        .decode(value)
        .map_err(|_| PortableFormatError::InvalidDigest)?;
    let digest: [u8; 32] = decoded
        .try_into()
        .map_err(|_| PortableFormatError::InvalidDigest)?;
    if general_purpose::STANDARD.encode(digest) != value {
        return Err(PortableFormatError::InvalidDigest);
    }
    Ok(digest)
}

fn validate_feature_name(value: &str, limits: PortableMigrationLimitsV1) -> bool {
    !value.is_empty()
        && value.len() <= limits.max_required_feature_bytes
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
        })
}

fn validate_features(
    features: &[String],
    limits: PortableMigrationLimitsV1,
) -> Result<(), PortableFormatError> {
    if features.len() > limits.max_required_features {
        return Err(PortableFormatError::LengthLimitExceeded);
    }
    let mut seen = BTreeSet::new();
    for feature in features {
        if !validate_feature_name(feature, limits) || !seen.insert(feature) {
            return Err(PortableFormatError::InvalidFeatureName);
        }
    }
    Ok(())
}

fn validate_extensions(
    extensions: &Value,
    limits: PortableMigrationLimitsV1,
) -> Result<(), PortableFormatError> {
    let Value::Object(object) = extensions else {
        return Err(PortableFormatError::ManifestInvalid);
    };
    if object.len() > limits.max_required_features {
        return Err(PortableFormatError::LengthLimitExceeded);
    }
    let encoded =
        serde_json::to_vec(extensions).map_err(|_| PortableFormatError::ManifestInvalid)?;
    if encoded.len() > limits.max_extensions_bytes {
        return Err(PortableFormatError::LengthLimitExceeded);
    }
    let top_level = BTreeSet::from(TOP_LEVEL_FIELDS);
    for key in object.keys() {
        if !validate_feature_name(key, limits) || top_level.contains(key.as_str()) {
            return Err(PortableFormatError::InvalidFeatureName);
        }
    }
    Ok(())
}

fn validate_categories(
    included: &[String],
    excluded: &[String],
) -> Result<(), PortableFormatError> {
    let allowed = BTreeSet::from([
        CATEGORY_CORE_DATA,
        CATEGORY_HISTORY,
        CATEGORY_SESSION_CREDENTIALS,
        CATEGORY_LOCAL_PROXY_ACCESS_KEY,
        CATEGORY_DEVICE_RUNTIME_STATE,
        CATEGORY_PROVIDER_DRAFTS,
    ]);
    let included_set = string_set(included)?;
    let excluded_set = string_set(excluded)?;
    if included_set
        .iter()
        .any(|category| !allowed.contains(category.as_str()))
        || excluded_set
            .iter()
            .any(|category| !allowed.contains(category.as_str()))
        || included_set
            .iter()
            .any(|category| excluded_set.contains(category))
        || !included_set.contains(CATEGORY_CORE_DATA)
        || (included_set.contains(CATEGORY_HISTORY) == excluded_set.contains(CATEGORY_HISTORY))
    {
        return Err(PortableFormatError::InvalidCategories);
    }
    for category in [
        CATEGORY_SESSION_CREDENTIALS,
        CATEGORY_LOCAL_PROXY_ACCESS_KEY,
        CATEGORY_DEVICE_RUNTIME_STATE,
        CATEGORY_PROVIDER_DRAFTS,
    ] {
        if !excluded_set.contains(category) || included_set.contains(category) {
            return Err(PortableFormatError::InvalidCategories);
        }
    }
    Ok(())
}

fn string_set(values: &[String]) -> Result<BTreeSet<String>, PortableFormatError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value.clone()) {
            return Err(PortableFormatError::InvalidCategories);
        }
    }
    Ok(seen)
}

fn validate_record_counts(
    counts: &BTreeMap<String, u64>,
    expected_keys: &[&str],
    limits: PortableMigrationLimitsV1,
) -> Result<(), PortableFormatError> {
    if counts.len() > limits.max_record_counts || counts.len() != expected_keys.len() {
        return Err(PortableFormatError::InvalidRecordCounts);
    }
    let expected = expected_keys.iter().copied().collect::<BTreeSet<_>>();
    if counts.keys().any(|key| !expected.contains(key.as_str()))
        || expected.iter().any(|key| !counts.contains_key(*key))
        || counts
            .values()
            .any(|count| *count > limits.max_rows_per_table)
    {
        return Err(PortableFormatError::InvalidRecordCounts);
    }
    let total = counts
        .values()
        .try_fold(0_u64, |total, count| total.checked_add(*count))
        .ok_or(PortableFormatError::LengthOverflow)?;
    if total > limits.max_total_user_table_rows {
        return Err(PortableFormatError::InvalidRecordCounts);
    }
    Ok(())
}

fn validate_json_depth(value: &Value, max_depth: usize) -> Result<(), PortableFormatError> {
    fn walk(value: &Value, depth: usize, max_depth: usize) -> Result<(), PortableFormatError> {
        if depth > max_depth {
            return Err(PortableFormatError::LengthLimitExceeded);
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    walk(value, depth + 1, max_depth)?;
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    walk(value, depth + 1, max_depth)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(value, 0, max_depth)
}

fn parse_json_value_rejecting_duplicates(bytes: &[u8]) -> Result<Value, PortableFormatError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = JsonValueSeed
        .deserialize(&mut deserializer)
        .map_err(|error| {
            if error.to_string().contains("duplicate JSON key") {
                PortableFormatError::DuplicateJsonKey
            } else {
                PortableFormatError::ManifestInvalid
            }
        })?;
    deserializer
        .end()
        .map_err(|_| PortableFormatError::ManifestInvalid)?;
    Ok(value)
}

struct JsonValueSeed;

impl<'de> DeserializeSeed<'de> for JsonValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(JsonValueVisitor)
    }
}

struct JsonValueVisitor;

impl<'de> Visitor<'de> for JsonValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("valid JSON without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("invalid JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        JsonValueSeed.deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(JsonValueSeed)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = BTreeSet::new();
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            let value = map.next_value_seed(JsonValueSeed)?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

fn read_u32_be(reader: &mut impl Read) -> Result<u32, PortableFormatError> {
    let mut bytes = [0_u8; 4];
    read_exact_or_truncated(reader, &mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_u64_be(reader: &mut impl Read) -> Result<u64, PortableFormatError> {
    let mut bytes = [0_u8; 8];
    read_exact_or_truncated(reader, &mut bytes)?;
    Ok(u64::from_be_bytes(bytes))
}

fn read_exact_or_truncated(
    reader: &mut impl Read,
    buffer: &mut [u8],
) -> Result<(), PortableFormatError> {
    reader
        .read_exact(buffer)
        .map_err(|_| PortableFormatError::Truncated)
}

fn write_all(writer: &mut impl Write, bytes: &[u8]) -> Result<(), PortableFormatError> {
    writer
        .write_all(bytes)
        .map_err(|_| PortableFormatError::Truncated)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use base64::{engine::general_purpose, Engine as _};
    use serde::Serialize;
    use sha2::{Digest, Sha256};
    use static_assertions::assert_not_impl_any;

    use super::{
        parse_manifest, read_framed_payload, write_framed_payload, PortableFormatError,
        PortableMigrationManifest, TransportKeyMaterial,
    };
    use crate::services::portable_migration::limits::PortableMigrationLimitsV1;

    const FIXTURE_KEYS: [&str; 2] = ["station_keys", "stations"];

    assert_not_impl_any!(TransportKeyMaterial: Clone, Copy, Serialize);

    #[test]
    fn valid_manifest_fixture_parses_with_exact_record_count_keys() {
        let manifest = parse_manifest(
            include_bytes!("../../../tests/fixtures/portable-migration/v1/manifest-valid.json"),
            &FIXTURE_KEYS,
            PortableMigrationLimitsV1::CURRENT,
        )
        .expect("valid manifest");

        assert_eq!(manifest.format_version, 1);
        assert_eq!(
            manifest.transport_key_id,
            "transport:018f7f9a-1111-7000-8000-000000000002"
        );
    }

    #[test]
    fn manifest_rejects_duplicate_unknown_invalid_and_inexact_fields() {
        assert_eq!(
            parse_manifest(
                include_bytes!(
                    "../../../tests/fixtures/portable-migration/v1/manifest-duplicate-key.json"
                ),
                &FIXTURE_KEYS,
                PortableMigrationLimitsV1::CURRENT,
            )
            .unwrap_err(),
            PortableFormatError::DuplicateJsonKey
        );
        assert_eq!(
            parse_manifest(
                include_bytes!(
                    "../../../tests/fixtures/portable-migration/v1/manifest-unknown-field.json"
                ),
                &FIXTURE_KEYS,
                PortableMigrationLimitsV1::CURRENT,
            )
            .unwrap_err(),
            PortableFormatError::UnknownTopLevelField
        );

        let valid = valid_manifest();
        let cases = [
            (
                "exportId",
                serde_json::json!("018f7f9a-1111-6000-8000-000000000001"),
                PortableFormatError::InvalidIdentifier,
            ),
            (
                "createdAt",
                serde_json::json!("2026-07-29T08:00:00+08:00"),
                PortableFormatError::InvalidTimestamp,
            ),
            (
                "sourceAppVersion",
                serde_json::json!("v0.3.3"),
                PortableFormatError::InvalidSemver,
            ),
            (
                "sqliteSha256",
                serde_json::json!(general_purpose::STANDARD_NO_PAD.encode([0_u8; 32])),
                PortableFormatError::InvalidDigest,
            ),
            (
                "requiredFeatures",
                serde_json::json!(["BadFeature"]),
                PortableFormatError::InvalidFeatureName,
            ),
        ];
        for (field, value, expected) in cases {
            let mut manifest = valid.clone();
            manifest[field] = value;
            let bytes = serde_json::to_vec(&manifest).expect("case json");
            assert_eq!(
                parse_manifest(&bytes, &FIXTURE_KEYS, PortableMigrationLimitsV1::CURRENT)
                    .unwrap_err(),
                expected,
                "{field} should fail"
            );
        }
    }

    #[test]
    fn manifest_rejects_category_and_record_count_drift() {
        let mut category_drift = valid_manifest();
        category_drift["includedCategories"] =
            serde_json::json!(["core_data", "history", "session_credentials"]);
        let category_bytes = serde_json::to_vec(&category_drift).expect("category json");
        assert_eq!(
            parse_manifest(
                &category_bytes,
                &FIXTURE_KEYS,
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap_err(),
            PortableFormatError::InvalidCategories
        );

        let mut missing_count = valid_manifest();
        missing_count["recordCounts"] = serde_json::json!({"stations": 0});
        let missing_bytes = serde_json::to_vec(&missing_count).expect("record json");
        assert_eq!(
            parse_manifest(
                &missing_bytes,
                &FIXTURE_KEYS,
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap_err(),
            PortableFormatError::InvalidRecordCounts
        );

        let mut too_many = valid_manifest();
        let counts = (0..=PortableMigrationLimitsV1::CURRENT.max_record_counts)
            .map(|index| (format!("table_{index}"), serde_json::json!(0)))
            .collect::<serde_json::Map<_, _>>();
        too_many["recordCounts"] = serde_json::Value::Object(counts);
        let expected = (0..=PortableMigrationLimitsV1::CURRENT.max_record_counts)
            .map(|index| format!("table_{index}"))
            .collect::<Vec<_>>();
        let expected = expected.iter().map(String::as_str).collect::<Vec<_>>();
        let too_many_bytes = serde_json::to_vec(&too_many).expect("too many json");
        assert_eq!(
            parse_manifest(
                &too_many_bytes,
                &expected,
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap_err(),
            PortableFormatError::InvalidRecordCounts
        );
    }

    #[test]
    fn framing_round_trip_uses_big_endian_lengths_raw_transport_key_and_sha_tail() {
        let sqlite = b"SQLite bytes";
        let digest = Sha256::digest(sqlite);
        let mut manifest = manifest_struct();
        manifest.sqlite_size_bytes = sqlite.len() as u64;
        manifest.sqlite_sha256 = general_purpose::STANDARD.encode(digest);
        let key = TransportKeyMaterial::from_bytes([7; 32]);
        let mut framed = Vec::new();

        let written_digest = write_framed_payload(
            &mut framed,
            &manifest,
            &key,
            &sqlite[..],
            &FIXTURE_KEYS,
            PortableMigrationLimitsV1::CURRENT,
        )
        .expect("write frame");

        assert_eq!(&framed[..8], b"RPDMOVE1");
        let manifest_len = u32::from_be_bytes(framed[8..12].try_into().unwrap()) as usize;
        assert!(manifest_len > 0);
        assert_eq!(&framed[12 + manifest_len..12 + manifest_len + 32], &[7; 32]);
        assert_eq!(written_digest, digest.as_slice());

        let parsed = read_framed_payload(
            &framed[..],
            &FIXTURE_KEYS,
            PortableMigrationLimitsV1::CURRENT,
        )
        .expect("read frame");
        assert_eq!(parsed.manifest, manifest);
        assert_eq!(parsed.sqlite_bytes, sqlite);
        parsed
            .transport_key
            .with_bytes(|bytes| assert_eq!(bytes, &[7; 32]));
    }

    #[test]
    fn framing_rejects_bad_magic_truncation_trailing_bytes_and_digest_mismatch() {
        let frame = valid_empty_frame();
        let mut bad_magic = frame.clone();
        bad_magic[0] = b'X';
        assert_eq!(
            read_framed_payload(
                &bad_magic[..],
                &FIXTURE_KEYS,
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap_err(),
            PortableFormatError::UnsupportedFraming
        );
        assert_eq!(
            read_framed_payload(
                &frame[..frame.len() - 1],
                &FIXTURE_KEYS,
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap_err(),
            PortableFormatError::Truncated
        );
        let mut trailing = frame.clone();
        trailing.push(1);
        assert_eq!(
            read_framed_payload(
                &trailing[..],
                &FIXTURE_KEYS,
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap_err(),
            PortableFormatError::TrailingBytes
        );
        let mut bad_digest = frame;
        let last = bad_digest.len() - 1;
        bad_digest[last] ^= 1;
        assert_eq!(
            read_framed_payload(
                &bad_digest[..],
                &FIXTURE_KEYS,
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap_err(),
            PortableFormatError::DigestMismatch
        );
    }

    #[test]
    fn malformed_framing_fixtures_match_expected_failures() {
        let cases = [
            (
                include_str!("../../../tests/fixtures/portable-migration/v1/framing-bad-magic.hex"),
                PortableFormatError::UnsupportedFraming,
            ),
            (
                include_str!("../../../tests/fixtures/portable-migration/v1/framing-truncated.hex"),
                PortableFormatError::Truncated,
            ),
        ];

        for (fixture_hex, expected) in cases {
            let bytes = decode_hex_fixture(fixture_hex);
            assert_eq!(
                read_framed_payload(
                    &bytes[..],
                    &FIXTURE_KEYS,
                    PortableMigrationLimitsV1::CURRENT
                )
                .unwrap_err(),
                expected
            );
        }
    }

    #[test]
    fn transport_key_debug_is_redacted() {
        let key = TransportKeyMaterial::from_bytes([9; 32]);

        assert_eq!(
            format!("{key:?}"),
            "TransportKeyMaterial { redacted: true }"
        );
        assert!(!format!("{key:?}").contains('9'));
    }

    fn valid_empty_frame() -> Vec<u8> {
        let key = TransportKeyMaterial::from_bytes([7; 32]);
        let manifest = manifest_struct();
        let mut framed = Vec::new();
        write_framed_payload(
            &mut framed,
            &manifest,
            &key,
            &[][..],
            &FIXTURE_KEYS,
            PortableMigrationLimitsV1::CURRENT,
        )
        .expect("valid frame");
        framed
    }

    fn manifest_struct() -> PortableMigrationManifest {
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
            database_schema_version: 10,
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
            sqlite_size_bytes: 0,
            sqlite_sha256: general_purpose::STANDARD.encode(Sha256::digest([])),
        }
    }

    fn valid_manifest() -> serde_json::Value {
        serde_json::to_value(manifest_struct()).expect("manifest value")
    }

    fn decode_hex_fixture(hex: &str) -> Vec<u8> {
        let hex = hex.trim();
        assert_eq!(hex.len() % 2, 0, "hex fixture must contain full bytes");
        hex.as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).expect("high nibble");
                let low = (pair[1] as char).to_digit(16).expect("low nibble");
                ((high << 4) | low) as u8
            })
            .collect()
    }
}
