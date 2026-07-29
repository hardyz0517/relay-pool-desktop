use crate::{
    application::data_maintenance::DataMaintenanceError,
    services::{
        portable_migration::{
            snapshot::PortableSnapshotError, validate::PortableMigrationValidationError,
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
    #[error("data migration target already exists")]
    TargetExists,
    #[error("data migration cleanup failed")]
    CleanupFailed,
}

pub(crate) type DataMigrationResult<T> = Result<T, DataMigrationError>;
