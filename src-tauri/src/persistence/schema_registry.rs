use std::collections::BTreeSet;

use semver::Version;

use crate::persistence::{error::PersistenceError, schema_compatibility::BinaryCompatibility};

pub(crate) const MINIMUM_AUTOMATIC_SCHEMA_BASELINE: i64 = 15;
pub(crate) const PRE_SECRET_BASELINE_SCHEMA: i64 = 16;
pub(crate) const ENCRYPTED_SECRET_BASELINE_SCHEMA: i64 = 17;
pub(crate) const CURRENT_SECRET_FORMAT_VERSION: i64 = 1;

pub(crate) fn migrator() -> &'static sqlx::migrate::Migrator {
    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./src/persistence/migrations");
    &MIGRATOR
}

pub(crate) fn latest_schema() -> i64 {
    migrator()
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or_default()
}

pub(crate) fn current_binary_compatibility() -> BinaryCompatibility {
    let latest = latest_schema();
    BinaryCompatibility {
        app_version: Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("package version must be valid semver"),
        database_generation: 2,
        readable_schema: 1..=latest,
        writable_schema: BTreeSet::from([latest]),
    }
}

pub(crate) fn validate_migration_registry() -> Result<(), PersistenceError> {
    let mut versions = Vec::new();
    for migration in migrator().iter() {
        if migration.version <= 0 {
            return Err(PersistenceError::InvariantViolation(format!(
                "migration version {} must be positive",
                migration.version
            )));
        }
        versions.push(migration.version);
    }
    if versions.is_empty() {
        return Err(PersistenceError::InvariantViolation(
            "migration registry is empty".to_string(),
        ));
    }
    versions.sort_unstable();
    for window in versions.windows(2) {
        let [previous, current] = window else {
            continue;
        };
        if previous == current {
            return Err(PersistenceError::InvariantViolation(format!(
                "migration version {current} is duplicated"
            )));
        }
        if *current != *previous + 1 {
            return Err(PersistenceError::InvariantViolation(format!(
                "migration registry has a gap between {previous} and {current}"
            )));
        }
    }
    Ok(())
}
