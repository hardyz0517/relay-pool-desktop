use crate::{
    application::data_maintenance::DataMaintenanceError,
    persistence::error::PersistenceError,
    services::{
        data_store::file_identity::FileIdentityError,
        portable_migration::{
            activation_journal::PortableActivationJournalError, age_envelope::AgeEnvelopeError,
            fault::PortableActivationFault, inspection_registry::ImportInspectionError,
            occupancy::RestoreTargetOccupancyError, path_tokens::PathTokenError,
            snapshot::PortableSnapshotError, staging::PortablePackageStagingError,
            validate::PortableMigrationValidationError,
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

#[derive(Debug, thiserror::Error)]
pub(crate) enum DataMigrationImportError {
    #[error("data migration maintenance coordinator rejected inspection")]
    Maintenance(#[from] DataMaintenanceError),
    #[error("data migration import path token failed")]
    PathToken(#[from] PathTokenError),
    #[error("data migration import inspection handle failed")]
    Inspection(#[from] ImportInspectionError),
    #[error("data migration package staging failed")]
    Package(#[from] PortablePackageStagingError),
    #[error("data migration package envelope failed")]
    Envelope(#[from] AgeEnvelopeError),
    #[error("data migration validation failed")]
    Validation(#[from] PortableMigrationValidationError),
    #[error("data migration secret rekey failed")]
    SecretRekey(#[from] SecretRekeyError),
    #[error("data migration import confirmation text is invalid")]
    ConfirmationTextMismatch,
    #[error("data migration restore target is not empty")]
    RestoreTargetNotEmpty,
    #[error("data migration import inspection is temporarily blocked")]
    TemporarilyBlocked,
    #[error("data migration verified backup failed")]
    Backup(String),
    #[error("data migration active database identity changed after backup")]
    ActiveIdentityChanged,
    #[error("data migration file identity failed")]
    FileIdentity(#[from] FileIdentityError),
    #[error("data migration activation journal failed")]
    ActivationJournal(#[from] PortableActivationJournalError),
    #[error("data migration persistence operation failed")]
    Persistence(#[from] PersistenceError),
    #[error("data migration activation fault injected")]
    ActivationFault(#[from] PortableActivationFault),
}

impl From<RestoreTargetOccupancyError> for DataMigrationImportError {
    fn from(error: RestoreTargetOccupancyError) -> Self {
        match error {
            RestoreTargetOccupancyError::NotEmpty => Self::RestoreTargetNotEmpty,
            RestoreTargetOccupancyError::Validation(error) => Self::Validation(error),
        }
    }
}
