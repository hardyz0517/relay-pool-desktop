mod detect;
mod import;
mod profiles;
mod validate;

use std::fmt;

pub(crate) use detect::{detect_profile, LegacyReadSession};
pub(crate) use import::{
    import_profile, import_profile_with_secrets, ImportedEncryptedSecret, LegacySecretMaterial,
    LegacySecretTransformer,
};
pub(crate) use profiles::{DetectedLegacyProfile, LegacyProfileDescriptor};
pub(crate) use validate::{validate_import, ExpectedImportManifest};

#[derive(Debug, thiserror::Error)]
pub(crate) enum UpgradeError {
    #[error("legacy database is missing")]
    MissingLegacyDatabase,
    #[error("legacy database schema is unsupported")]
    UnsupportedLegacySchema,
    #[error("legacy database failed read-only integrity validation")]
    LegacyIntegrityFailed,
    #[error("legacy database changed while the read-only snapshot was captured")]
    LegacySourceChanged,
    #[error("legacy import contains secrets but no secret transformer was provided")]
    SecretTransformerRequired,
    #[error("legacy secret transformation failed")]
    SecretTransformationFailed,
    #[error("legacy import validation failed")]
    ValidationFailed,
    #[error("legacy import SQL failed")]
    Sqlx(#[from] sqlx::Error),
    #[error("legacy import persistence failed")]
    Persistence(#[from] crate::persistence::error::PersistenceError),
}

pub(crate) struct LegacySecretBytes(zeroize::Zeroizing<Vec<u8>>);

impl LegacySecretBytes {
    pub(crate) fn new(value: Vec<u8>) -> Self {
        Self(zeroize::Zeroizing::new(value))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl fmt::Debug for LegacySecretBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LegacySecretBytes")
            .field("len", &self.0.len())
            .finish_non_exhaustive()
    }
}
