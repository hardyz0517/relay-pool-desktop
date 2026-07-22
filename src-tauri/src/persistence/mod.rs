mod backup;
pub(crate) use backup::{
    create_verified_backup_from_path, temporary_backup_path, validate_read_only_sqlite,
};
pub(crate) mod error;
mod health_check;
mod inspection;
pub(crate) mod legacy_import;
mod migrations;
pub(crate) use migrations::current_schema_version;
mod read_session;
pub(crate) use read_session::ReadSession;
pub(crate) mod runtime;
mod runtime_lifecycle;
mod schema_compatibility;
pub(crate) mod stores;
pub(crate) mod upgrade_fault;
pub(crate) mod upgrade_journal;
pub(crate) mod upgrade_recovery_executor;
pub(crate) mod upgrade_recovery_plan;
mod write_coordinator;
mod write_session;

#[cfg(test)]
mod performance_tests;

#[cfg(test)]
mod differential_tests;

pub(crate) use inspection::{
    inspect_relay_pool_database, read_encrypted_secrets, ReadOnlyDatabaseHealth,
    ReadOnlyDatabaseInspection, StoredEncryptedSecret,
};
