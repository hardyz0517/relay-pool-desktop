use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(crate) const JOURNAL_VERSION: u32 = 1;
const ENCRYPTED_SECRET_BASELINE_KIND: &str = "encryptedSecretBaseline";
const ENCRYPTED_SECRET_BASELINE_SOURCE_SCHEMA: i64 = 16;
const ENCRYPTED_SECRET_BASELINE_TARGET_SCHEMA: i64 = 17;
const ENCRYPTED_SECRET_BASELINE_PROFILE: &str = "encrypted-secrets-v1";

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
