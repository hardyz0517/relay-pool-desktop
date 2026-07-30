use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(crate) const JOURNAL_VERSION: u32 = 1;
const GENERATION_1: u32 = 1;
const LEGACY_BACKUP_FILE: &str = "relay-pool-desktop.sqlite3";
const V2_FINAL_FILE: &str = "relay-pool-desktop-v2.sqlite3";
const ENCRYPTED_SECRET_BASELINE_KIND: &str = "encryptedSecretBaseline";
const ENCRYPTED_SECRET_BASELINE_SOURCE_SCHEMA: i64 = 15;
const ENCRYPTED_SECRET_BASELINE_TARGET_SCHEMA: i64 = 16;
const ENCRYPTED_SECRET_BASELINE_PROFILE: &str = "encrypted-secrets-v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum UpgradePhase {
    Prepared,
    BackupVerified,
    V2Validated,
    LegacyDeactivated,
    GenerationCommitted,
    V2Reopened,
}

impl UpgradePhase {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 6] = [
        Self::Prepared,
        Self::BackupVerified,
        Self::V2Validated,
        Self::LegacyDeactivated,
        Self::GenerationCommitted,
        Self::V2Reopened,
    ];

    const fn next(self) -> Option<Self> {
        match self {
            Self::Prepared => Some(Self::BackupVerified),
            Self::BackupVerified => Some(Self::V2Validated),
            Self::V2Validated => Some(Self::LegacyDeactivated),
            Self::LegacyDeactivated => Some(Self::GenerationCommitted),
            Self::GenerationCommitted => Some(Self::V2Reopened),
            Self::V2Reopened => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub(crate) struct UpgradeAttemptId(String);

impl UpgradeAttemptId {
    pub(crate) fn parse(value: &str) -> Result<Self, JournalValidationError> {
        let parsed =
            Uuid::parse_str(value).map_err(|_| JournalValidationError::InvalidAttemptId)?;
        let canonical = parsed.hyphenated().to_string();
        if value != canonical {
            return Err(JournalValidationError::InvalidAttemptId);
        }
        Ok(Self(canonical))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(&self) -> Result<(), JournalValidationError> {
        Self::parse(&self.0).map(|_| ())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub(crate) struct ReleasedSchemaProfile(String);

impl ReleasedSchemaProfile {
    pub(crate) fn parse(value: &str) -> Result<Self, JournalValidationError> {
        let valid = !value.is_empty()
            && value.len() <= 64
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            });
        if !valid {
            return Err(JournalValidationError::InvalidSchemaProfile);
        }
        Ok(Self(value.to_owned()))
    }

    fn validate(&self) -> Result<(), JournalValidationError> {
        Self::parse(&self.0).map(|_| ())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub(crate) struct Sha256Digest(String);

impl Sha256Digest {
    pub(crate) fn parse(value: &str) -> Result<Self, JournalValidationError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(JournalValidationError::InvalidSha256);
        }
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(&self) -> Result<(), JournalValidationError> {
        Self::parse(&self.0).map(|_| ())
    }

    fn of_bytes(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub(crate) struct UtcTimestamp(String);

impl UtcTimestamp {
    pub(crate) fn parse(value: &str) -> Result<Self, JournalValidationError> {
        let parsed = DateTime::parse_from_rfc3339(value)
            .map_err(|_| JournalValidationError::InvalidTimestamp)?;
        if parsed.offset().local_minus_utc() != 0 || !value.ends_with('Z') {
            return Err(JournalValidationError::InvalidTimestamp);
        }
        Ok(Self(value.to_owned()))
    }

    fn as_utc(&self) -> Result<DateTime<Utc>, JournalValidationError> {
        DateTime::parse_from_rfc3339(&self.0)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|_| JournalValidationError::InvalidTimestamp)
    }

    fn validate(&self) -> Result<(), JournalValidationError> {
        Self::parse(&self.0).map(|_| ())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct JournalArtifactPaths {
    backup: String,
    v2_temporary: String,
    v2_final: String,
}

impl JournalArtifactPaths {
    pub(crate) fn for_attempt(attempt_id: &UpgradeAttemptId) -> Self {
        Self {
            backup: format!("backups/{}/{LEGACY_BACKUP_FILE}", attempt_id.as_str()),
            v2_temporary: format!("{V2_FINAL_FILE}.upgrade-{}.tmp", attempt_id.as_str()),
            v2_final: V2_FINAL_FILE.to_owned(),
        }
    }

    pub(crate) fn backup(&self) -> &str {
        &self.backup
    }

    pub(crate) fn v2_temporary(&self) -> &str {
        &self.v2_temporary
    }

    pub(crate) fn v2_final(&self) -> &str {
        &self.v2_final
    }

    fn validate(&self, attempt_id: &UpgradeAttemptId) -> Result<(), JournalValidationError> {
        let expected = Self::for_attempt(attempt_id);
        if self != &expected
            || [&self.backup, &self.v2_temporary, &self.v2_final]
                .iter()
                .any(|path| !is_safe_relative_path(path))
        {
            return Err(JournalValidationError::InvalidArtifactPath);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpgradeJournalPayload {
    pub(crate) journal_version: u32,
    pub(crate) attempt_id: UpgradeAttemptId,
    pub(crate) phase: UpgradePhase,
    pub(crate) source_generation: u32,
    pub(crate) released_schema_profile: ReleasedSchemaProfile,
    pub(crate) source_candidate_identity: Sha256Digest,
    pub(crate) verified_backup_sha256: Option<Sha256Digest>,
    pub(crate) paths: JournalArtifactPaths,
    pub(crate) created_at: UtcTimestamp,
    pub(crate) updated_at: UtcTimestamp,
}

impl UpgradeJournalPayload {
    fn validate(&self) -> Result<(), JournalValidationError> {
        if self.journal_version != JOURNAL_VERSION {
            return Err(JournalValidationError::UnsupportedVersion);
        }
        if self.source_generation != GENERATION_1 {
            return Err(JournalValidationError::InvalidSourceGeneration);
        }
        self.attempt_id.validate()?;
        self.released_schema_profile.validate()?;
        self.source_candidate_identity.validate()?;
        self.paths.validate(&self.attempt_id)?;
        self.created_at.validate()?;
        self.updated_at.validate()?;
        if self.updated_at.as_utc()? < self.created_at.as_utc()? {
            return Err(JournalValidationError::InvalidTimestampOrder);
        }
        match (&self.phase, &self.verified_backup_sha256) {
            (UpgradePhase::Prepared, None) => {}
            (UpgradePhase::Prepared, Some(_)) | (_, None) => {
                return Err(JournalValidationError::InvalidPhaseShape)
            }
            (_, Some(hash)) => hash.validate()?,
        }
        Ok(())
    }

    fn checksum(&self) -> Result<Sha256Digest, JournalValidationError> {
        let canonical =
            serde_json::to_vec(self).map_err(|_| JournalValidationError::SerializationFailed)?;
        Ok(Sha256Digest::of_bytes(&canonical))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpgradeJournal {
    payload: UpgradeJournalPayload,
    canonical_payload_checksum: Sha256Digest,
}

impl UpgradeJournal {
    pub(crate) fn prepared(
        attempt_id: UpgradeAttemptId,
        released_schema_profile: ReleasedSchemaProfile,
        source_candidate_identity: Sha256Digest,
        created_at: UtcTimestamp,
    ) -> Result<Self, JournalValidationError> {
        Self::seal(UpgradeJournalPayload {
            journal_version: JOURNAL_VERSION,
            paths: JournalArtifactPaths::for_attempt(&attempt_id),
            attempt_id,
            phase: UpgradePhase::Prepared,
            source_generation: GENERATION_1,
            released_schema_profile,
            source_candidate_identity,
            verified_backup_sha256: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        })
    }

    pub(crate) fn advance(
        &self,
        next_phase: UpgradePhase,
        verified_backup_sha256: Option<Sha256Digest>,
        updated_at: UtcTimestamp,
    ) -> Result<Self, JournalValidationError> {
        if self.payload.phase.next() != Some(next_phase) {
            return Err(JournalValidationError::NonAdjacentPhase);
        }
        let verified_backup_sha256 = match next_phase {
            UpgradePhase::Prepared => return Err(JournalValidationError::NonAdjacentPhase),
            UpgradePhase::BackupVerified => {
                verified_backup_sha256.ok_or(JournalValidationError::InvalidPhaseShape)?
            }
            _ => {
                if verified_backup_sha256.is_some() {
                    return Err(JournalValidationError::InvalidPhaseShape);
                }
                self.payload
                    .verified_backup_sha256
                    .clone()
                    .ok_or(JournalValidationError::InvalidPhaseShape)?
            }
        };
        let mut payload = self.payload.clone();
        payload.phase = next_phase;
        payload.verified_backup_sha256 = Some(verified_backup_sha256);
        payload.updated_at = updated_at;
        Self::seal(payload)
    }

    pub(crate) fn seal(payload: UpgradeJournalPayload) -> Result<Self, JournalValidationError> {
        payload.validate()?;
        let canonical_payload_checksum = payload.checksum()?;
        Ok(Self {
            payload,
            canonical_payload_checksum,
        })
    }

    pub(crate) fn from_json(bytes: &[u8]) -> Result<Self, JournalValidationError> {
        let journal: Self =
            serde_json::from_slice(bytes).map_err(|_| JournalValidationError::MalformedJournal)?;
        journal.validate()?;
        Ok(journal)
    }

    pub(crate) fn to_canonical_json(&self) -> Result<Vec<u8>, JournalValidationError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| JournalValidationError::SerializationFailed)
    }

    pub(crate) fn payload(&self) -> &UpgradeJournalPayload {
        &self.payload
    }

    pub(crate) fn canonical_payload_checksum(&self) -> &Sha256Digest {
        &self.canonical_payload_checksum
    }

    pub(crate) fn validate(&self) -> Result<(), JournalValidationError> {
        self.payload.validate()?;
        self.canonical_payload_checksum.validate()?;
        if self.payload.checksum()? != self.canonical_payload_checksum {
            return Err(JournalValidationError::ChecksumMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BaselineConversionPhase {
    Prepared,
    BackupVerified,
    CandidateBuilt,
    CandidateValidated,
    ActivePublished,
    ActiveValidated,
}

impl BaselineConversionPhase {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 6] = [
        Self::Prepared,
        Self::BackupVerified,
        Self::CandidateBuilt,
        Self::CandidateValidated,
        Self::ActivePublished,
        Self::ActiveValidated,
    ];

    const fn next(self) -> Option<Self> {
        match self {
            Self::Prepared => Some(Self::BackupVerified),
            Self::BackupVerified => Some(Self::CandidateBuilt),
            Self::CandidateBuilt => Some(Self::CandidateValidated),
            Self::CandidateValidated => Some(Self::ActivePublished),
            Self::ActivePublished => Some(Self::ActiveValidated),
            Self::ActiveValidated => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BaselineConversionArtifactPaths {
    backup: String,
    candidate: String,
    active: String,
}

impl BaselineConversionArtifactPaths {
    pub(crate) fn for_attempt(attempt_id: &UpgradeAttemptId, active_file_name: &str) -> Self {
        Self {
            backup: format!(
                "backups/encrypted-secret-baseline/{}/{active_file_name}",
                attempt_id.as_str()
            ),
            candidate: format!(
                "{active_file_name}.encrypted-baseline-{}.tmp",
                attempt_id.as_str()
            ),
            active: active_file_name.to_owned(),
        }
    }

    pub(crate) fn backup(&self) -> &str {
        &self.backup
    }

    pub(crate) fn candidate(&self) -> &str {
        &self.candidate
    }

    pub(crate) fn active(&self) -> &str {
        &self.active
    }

    fn validate(&self, attempt_id: &UpgradeAttemptId) -> Result<(), JournalValidationError> {
        if !self
            .candidate
            .contains(&format!("encrypted-baseline-{}", attempt_id.as_str()))
            || !self.backup.starts_with(&format!(
                "backups/encrypted-secret-baseline/{}/",
                attempt_id.as_str()
            ))
            || [&self.backup, &self.candidate, &self.active]
                .iter()
                .any(|path| !is_safe_relative_path(path))
        {
            return Err(JournalValidationError::InvalidArtifactPath);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BaselineConversionJournalPayload {
    pub(crate) journal_version: u32,
    pub(crate) kind: String,
    pub(crate) attempt_id: UpgradeAttemptId,
    pub(crate) phase: BaselineConversionPhase,
    pub(crate) source_schema_version: i64,
    pub(crate) target_schema_version: i64,
    pub(crate) schema_profile: ReleasedSchemaProfile,
    pub(crate) source_candidate_identity: Sha256Digest,
    pub(crate) verified_backup_sha256: Option<Sha256Digest>,
    pub(crate) candidate_sha256: Option<Sha256Digest>,
    pub(crate) paths: BaselineConversionArtifactPaths,
    pub(crate) created_at: UtcTimestamp,
    pub(crate) updated_at: UtcTimestamp,
}

impl BaselineConversionJournalPayload {
    fn validate(&self) -> Result<(), JournalValidationError> {
        if self.journal_version != JOURNAL_VERSION {
            return Err(JournalValidationError::UnsupportedVersion);
        }
        if self.kind != ENCRYPTED_SECRET_BASELINE_KIND {
            return Err(JournalValidationError::InvalidJournalKind);
        }
        if self.source_schema_version != ENCRYPTED_SECRET_BASELINE_SOURCE_SCHEMA
            || self.target_schema_version != ENCRYPTED_SECRET_BASELINE_TARGET_SCHEMA
        {
            return Err(JournalValidationError::InvalidSchemaVersion);
        }
        self.attempt_id.validate()?;
        self.schema_profile.validate()?;
        self.source_candidate_identity.validate()?;
        self.paths.validate(&self.attempt_id)?;
        self.created_at.validate()?;
        self.updated_at.validate()?;
        if self.updated_at.as_utc()? < self.created_at.as_utc()? {
            return Err(JournalValidationError::InvalidTimestampOrder);
        }
        match self.phase {
            BaselineConversionPhase::Prepared => {
                if self.verified_backup_sha256.is_some() || self.candidate_sha256.is_some() {
                    return Err(JournalValidationError::InvalidPhaseShape);
                }
            }
            BaselineConversionPhase::BackupVerified => {
                self.verified_backup_sha256
                    .as_ref()
                    .ok_or(JournalValidationError::InvalidPhaseShape)?
                    .validate()?;
                if self.candidate_sha256.is_some() {
                    return Err(JournalValidationError::InvalidPhaseShape);
                }
            }
            BaselineConversionPhase::CandidateBuilt
            | BaselineConversionPhase::CandidateValidated
            | BaselineConversionPhase::ActivePublished
            | BaselineConversionPhase::ActiveValidated => {
                self.verified_backup_sha256
                    .as_ref()
                    .ok_or(JournalValidationError::InvalidPhaseShape)?
                    .validate()?;
                self.candidate_sha256
                    .as_ref()
                    .ok_or(JournalValidationError::InvalidPhaseShape)?
                    .validate()?;
            }
        }
        Ok(())
    }

    fn checksum(&self) -> Result<Sha256Digest, JournalValidationError> {
        let canonical =
            serde_json::to_vec(self).map_err(|_| JournalValidationError::SerializationFailed)?;
        Ok(Sha256Digest::of_bytes(&canonical))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BaselineConversionJournal {
    payload: BaselineConversionJournalPayload,
    canonical_payload_checksum: Sha256Digest,
}

impl BaselineConversionJournal {
    pub(crate) fn prepared(
        attempt_id: UpgradeAttemptId,
        source_candidate_identity: Sha256Digest,
        active_file_name: &str,
        created_at: UtcTimestamp,
    ) -> Result<Self, JournalValidationError> {
        Self::seal(BaselineConversionJournalPayload {
            journal_version: JOURNAL_VERSION,
            kind: ENCRYPTED_SECRET_BASELINE_KIND.to_owned(),
            paths: BaselineConversionArtifactPaths::for_attempt(&attempt_id, active_file_name),
            attempt_id,
            phase: BaselineConversionPhase::Prepared,
            source_schema_version: ENCRYPTED_SECRET_BASELINE_SOURCE_SCHEMA,
            target_schema_version: ENCRYPTED_SECRET_BASELINE_TARGET_SCHEMA,
            schema_profile: ReleasedSchemaProfile::parse(ENCRYPTED_SECRET_BASELINE_PROFILE)?,
            source_candidate_identity,
            verified_backup_sha256: None,
            candidate_sha256: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        })
    }

    pub(crate) fn advance(
        &self,
        next_phase: BaselineConversionPhase,
        verified_backup_sha256: Option<Sha256Digest>,
        candidate_sha256: Option<Sha256Digest>,
        updated_at: UtcTimestamp,
    ) -> Result<Self, JournalValidationError> {
        if self.payload.phase.next() != Some(next_phase) {
            return Err(JournalValidationError::NonAdjacentPhase);
        }
        let mut payload = self.payload.clone();
        payload.phase = next_phase;
        payload.updated_at = updated_at;
        match next_phase {
            BaselineConversionPhase::Prepared => {
                return Err(JournalValidationError::NonAdjacentPhase)
            }
            BaselineConversionPhase::BackupVerified => {
                payload.verified_backup_sha256 =
                    Some(verified_backup_sha256.ok_or(JournalValidationError::InvalidPhaseShape)?);
                payload.candidate_sha256 = None;
            }
            BaselineConversionPhase::CandidateBuilt
            | BaselineConversionPhase::CandidateValidated
            | BaselineConversionPhase::ActivePublished
            | BaselineConversionPhase::ActiveValidated => {
                payload.verified_backup_sha256 = Some(match verified_backup_sha256 {
                    Some(hash) => hash,
                    None => self
                        .payload
                        .verified_backup_sha256
                        .clone()
                        .ok_or(JournalValidationError::InvalidPhaseShape)?,
                });
                payload.candidate_sha256 = Some(match candidate_sha256 {
                    Some(hash) => hash,
                    None => self
                        .payload
                        .candidate_sha256
                        .clone()
                        .ok_or(JournalValidationError::InvalidPhaseShape)?,
                });
            }
        }
        Self::seal(payload)
    }

    pub(crate) fn seal(
        payload: BaselineConversionJournalPayload,
    ) -> Result<Self, JournalValidationError> {
        payload.validate()?;
        let canonical_payload_checksum = payload.checksum()?;
        Ok(Self {
            payload,
            canonical_payload_checksum,
        })
    }

    pub(crate) fn from_json(bytes: &[u8]) -> Result<Self, JournalValidationError> {
        let journal: Self =
            serde_json::from_slice(bytes).map_err(|_| JournalValidationError::MalformedJournal)?;
        journal.validate()?;
        Ok(journal)
    }

    pub(crate) fn to_canonical_json(&self) -> Result<Vec<u8>, JournalValidationError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| JournalValidationError::SerializationFailed)
    }

    pub(crate) fn payload(&self) -> &BaselineConversionJournalPayload {
        &self.payload
    }

    pub(crate) fn canonical_payload_checksum(&self) -> &Sha256Digest {
        &self.canonical_payload_checksum
    }

    pub(crate) fn validate(&self) -> Result<(), JournalValidationError> {
        self.payload.validate()?;
        self.canonical_payload_checksum.validate()?;
        if self.payload.checksum()? != self.canonical_payload_checksum {
            return Err(JournalValidationError::ChecksumMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub(crate) enum JournalValidationError {
    #[error("upgrade journal is malformed")]
    MalformedJournal,
    #[error("upgrade journal version is unsupported")]
    UnsupportedVersion,
    #[error("upgrade attempt id is invalid")]
    InvalidAttemptId,
    #[error("released schema profile is invalid")]
    InvalidSchemaProfile,
    #[error("SHA-256 value is invalid")]
    InvalidSha256,
    #[error("upgrade timestamp is invalid")]
    InvalidTimestamp,
    #[error("upgrade timestamp order is invalid")]
    InvalidTimestampOrder,
    #[error("upgrade source generation is invalid")]
    InvalidSourceGeneration,
    #[error("upgrade journal kind is invalid")]
    InvalidJournalKind,
    #[error("upgrade schema version is invalid")]
    InvalidSchemaVersion,
    #[error("upgrade artifact path is invalid")]
    InvalidArtifactPath,
    #[error("upgrade journal phase shape is invalid")]
    InvalidPhaseShape,
    #[error("upgrade journal checksum does not match")]
    ChecksumMismatch,
    #[error("upgrade journal serialization failed")]
    SerializationFailed,
    #[error("upgrade journal phase transition is not adjacent")]
    NonAdjacentPhase,
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains(':')
        && !path.contains('\\')
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'-' | b'_')
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(value: &str) -> UtcTimestamp {
        UtcTimestamp::parse(value).expect("timestamp")
    }

    #[test]
    fn journal_advances_only_through_adjacent_durable_phases() {
        let attempt =
            UpgradeAttemptId::parse("019f7d50-9d44-7000-8000-000000000001").expect("attempt");
        let prepared = UpgradeJournal::prepared(
            attempt,
            ReleasedSchemaProfile::parse("v0.3.1").expect("profile"),
            Sha256Digest::parse(&"a".repeat(64)).expect("source digest"),
            timestamp("2026-07-21T00:00:00Z"),
        )
        .expect("prepared journal");
        let backup = Sha256Digest::parse(&"b".repeat(64)).expect("backup digest");
        let backup_verified = prepared
            .advance(
                UpgradePhase::BackupVerified,
                Some(backup.clone()),
                timestamp("2026-07-21T00:01:00Z"),
            )
            .expect("backup verified");

        assert_eq!(
            prepared.advance(
                UpgradePhase::V2Validated,
                None,
                timestamp("2026-07-21T00:01:00Z")
            ),
            Err(JournalValidationError::NonAdjacentPhase)
        );
        let validated = backup_verified
            .advance(
                UpgradePhase::V2Validated,
                None,
                timestamp("2026-07-21T00:02:00Z"),
            )
            .expect("V2 validated");
        assert_eq!(
            validated.payload().verified_backup_sha256.as_ref(),
            Some(&backup)
        );
    }
}
