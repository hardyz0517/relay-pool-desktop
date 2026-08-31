use std::collections::BTreeSet;

use semver::Version;

use crate::persistence::{error::PersistenceError, schema_compatibility::BinaryCompatibility};

pub(crate) const MINIMUM_AUTOMATIC_SCHEMA_BASELINE: i64 = 15;
pub(crate) const PRE_SECRET_BASELINE_SCHEMA: i64 = 16;
pub(crate) const ENCRYPTED_SECRET_BASELINE_SCHEMA: i64 = 17;
pub(crate) const CURRENT_SECRET_FORMAT_VERSION: i64 = 1;

/// The routing v3 schema is a coordinated set of migrations. Keep the
/// versions/descriptions explicit so a future migration cannot accidentally
/// leave one generation component out of the registry.
pub(crate) const ROUTING_V3_MIGRATIONS: &[(i64, &str)] = &[
    (60, "routing policy v3"),
    (61, "routing observation v3"),
    (62, "routing key circuit v3"),
    (63, "routing runtime generation"),
    (64, "routing generation qualification reports"),
    (65, "routing raw event retention"),
    (66, "routing observation contract hardening"),
    (67, "routing generation resume and qualification"),
    (68, "routing circuit persistence gate"),
    (69, "routing generation qualification v2"),
    (70, "routing circuit applied event"),
    (71, "repair routing lifecycle projection"),
];

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

    for (version, description) in ROUTING_V3_MIGRATIONS {
        let Some(migration) = migrator()
            .iter()
            .find(|migration| migration.version == *version)
        else {
            return Err(PersistenceError::InvariantViolation(format!(
                "routing v3 migration {version} is missing from registry"
            )));
        };
        if migration.description.as_ref() != *description {
            return Err(PersistenceError::InvariantViolation(format!(
                "routing v3 migration {version} has description '{}' but expected '{description}'",
                migration.description
            )));
        }
    }
    Ok(())
}
