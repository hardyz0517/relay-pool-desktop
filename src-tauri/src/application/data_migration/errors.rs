use crate::{
    application::data_maintenance::DataMaintenanceError,
    services::{
        portable_migration::{
            age_envelope::AgeEnvelopeError, snapshot::PortableSnapshotError,
            staging::PortablePackageStagingError, validate::PortableMigrationValidationError,
        },
        secrets::rekey::SecretRekeyError,
    },
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum DataMigrationError {
    #[error("data migration maintenance coordinator rejected the operation")]
    Maintenance(#[from] DataMaintenanceError),
    #[error("data migration snapshot failed")]
    Snapshot(#[from] PortableSnapshotError),
    #[error("data migration validation failed")]
    Validation(#[from] PortableMigrationValidationError),
    #[error("data migration secret rekey failed")]
    SecretRekey(#[from] SecretRekeyError),
    #[error("data migration package envelope failed")]
    Envelope(#[from] AgeEnvelopeError),
    #[error("data migration package staging failed")]
    PackageStaging(#[from] PortablePackageStagingError),
    #[error("data migration package passphrase confirmation does not match")]
    PassphraseConfirmationMismatch,
    #[error("data migration transport key is unavailable")]
    TransportKeyUnavailable,
    #[error("data migration target already exists")]
    TargetExists,
    #[error("data migration cleanup failed")]
    CleanupFailed,
}

pub(crate) type DataMigrationResult<T> = Result<T, DataMigrationError>;
