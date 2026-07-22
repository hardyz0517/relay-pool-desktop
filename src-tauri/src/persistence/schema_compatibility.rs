use std::{collections::BTreeSet, ops::RangeInclusive};

use semver::Version;
use sqlx::{Executor, Sqlite};

use crate::persistence::error::{CompatibilityDecisionCode, PersistenceError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchemaCompatibility {
    pub(crate) database_generation: i64,
    pub(crate) schema_version: i64,
    pub(crate) min_reader_app_version: Version,
    pub(crate) min_writer_app_version: Version,
    pub(crate) updated_by_migration: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BinaryCompatibility {
    pub(crate) app_version: Version,
    pub(crate) database_generation: i64,
    pub(crate) readable_schema: RangeInclusive<i64>,
    pub(crate) writable_schema: BTreeSet<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenMode {
    Writable,
    InspectionOnly,
}

impl OpenMode {
    pub(crate) fn as_code(self) -> &'static str {
        match self {
            Self::Writable => "writable",
            Self::InspectionOnly => "inspection_only",
        }
    }
}

pub(crate) fn decide_open_mode(
    binary: &BinaryCompatibility,
    database: &SchemaCompatibility,
    sqlx_version: i64,
) -> Result<OpenMode, PersistenceError> {
    if database.database_generation != binary.database_generation {
        return Err(PersistenceError::IncompatibleSchema {
            writable: false,
            code: CompatibilityDecisionCode::GenerationMismatch,
        });
    }
    if database.schema_version != sqlx_version
        || !binary.readable_schema.contains(&database.schema_version)
    {
        return Err(PersistenceError::IncompatibleSchema {
            writable: false,
            code: CompatibilityDecisionCode::MetadataMismatch,
        });
    }
    if binary.app_version < database.min_reader_app_version {
        return Err(PersistenceError::IncompatibleSchema {
            writable: false,
            code: CompatibilityDecisionCode::ReaderTooOld,
        });
    }
    if !binary.writable_schema.contains(&database.schema_version) {
        return Ok(OpenMode::InspectionOnly);
    }
    if binary.app_version < database.min_writer_app_version {
        return Ok(OpenMode::InspectionOnly);
    }
    Ok(OpenMode::Writable)
}

#[cfg(test)]
#[allow(
    dead_code,
    reason = "compatibility decision codes are asserted by runtime and schema integration targets"
)]
pub(crate) fn compatibility_decision_code(
    binary: &BinaryCompatibility,
    database: &SchemaCompatibility,
    sqlx_version: i64,
) -> CompatibilityDecisionCode {
    match decide_open_mode(binary, database, sqlx_version) {
        Ok(OpenMode::Writable) => CompatibilityDecisionCode::Writable,
        Ok(OpenMode::InspectionOnly) if binary.app_version < database.min_writer_app_version => {
            CompatibilityDecisionCode::WriterTooOld
        }
        Ok(OpenMode::InspectionOnly) => CompatibilityDecisionCode::InspectionOnly,
        Err(PersistenceError::IncompatibleSchema { code, .. }) => code,
        Err(_) => CompatibilityDecisionCode::MetadataMismatch,
    }
}

pub(crate) async fn load_schema_compatibility<'e, E>(
    executor: E,
) -> Result<SchemaCompatibility, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query!(
        r#"
        SELECT
            database_generation,
            schema_version,
            min_reader_app_version,
            min_writer_app_version,
            updated_by_migration
        FROM persistence_schema_compatibility
        WHERE singleton_key = 1
        "#,
    )
    .fetch_optional(executor)
    .await?;

    let row = row.ok_or(PersistenceError::MissingCompatibilityMetadata)?;
    Ok(SchemaCompatibility {
        database_generation: row.database_generation,
        schema_version: row.schema_version,
        min_reader_app_version: parse_version(&row.min_reader_app_version)?,
        min_writer_app_version: parse_version(&row.min_writer_app_version)?,
        updated_by_migration: row.updated_by_migration,
    })
}

fn parse_version(value: &str) -> Result<Version, PersistenceError> {
    Version::parse(value).map_err(|_| PersistenceError::InvalidCompatibilityMetadata)
}

#[cfg(test)]
mod tests {
    use super::{
        compatibility_decision_code, decide_open_mode, BinaryCompatibility, OpenMode,
        SchemaCompatibility,
    };
    use crate::persistence::error::CompatibilityDecisionCode;
    use semver::Version;
    use std::collections::BTreeSet;

    #[test]
    fn compatibility_gate_classifies_without_business_callers() {
        let binary = BinaryCompatibility {
            app_version: Version::new(0, 3, 1),
            database_generation: 2,
            readable_schema: 1..=1,
            writable_schema: BTreeSet::from([1]),
        };
        let database = SchemaCompatibility {
            database_generation: 2,
            schema_version: 1,
            min_reader_app_version: Version::new(0, 3, 1),
            min_writer_app_version: Version::new(0, 4, 0),
            updated_by_migration: 1,
        };

        assert_eq!(
            decide_open_mode(&binary, &database, 1).expect("inspection"),
            OpenMode::InspectionOnly
        );
        assert_eq!(
            compatibility_decision_code(&binary, &database, 1),
            CompatibilityDecisionCode::WriterTooOld
        );
    }
}
