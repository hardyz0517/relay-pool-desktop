use sqlx::Row;

use crate::{
    models::station_capacity_domains::StationCapacityDomain,
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StationCapacityDomainStore;

impl StationCapacityDomainStore {
    pub(crate) async fn get(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<Option<StationCapacityDomain>, PersistenceError> {
        let row = sqlx::query("SELECT station_id, provider_family, deployment_identity, region_identity, revision, updated_at FROM station_capacity_domains WHERE station_id = ?1")
            .bind(station_id)
            .fetch_optional(read.connection())
            .await?;
        row.map(row_to_domain).transpose()
    }

    pub(crate) async fn upsert(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        expected_revision: i64,
        provider_family: &str,
        deployment_identity: Option<&str>,
        region_identity: Option<&str>,
        now: &str,
    ) -> Result<StationCapacityDomain, PersistenceError> {
        validate(
            station_id,
            provider_family,
            deployment_identity,
            region_identity,
            expected_revision,
        )?;
        station_exists(write, station_id).await?;
        let current_revision: Option<i64> = sqlx::query_scalar(
            "SELECT revision FROM station_capacity_domains WHERE station_id = ?1",
        )
        .bind(station_id)
        .fetch_optional(write.connection())
        .await?;
        if current_revision.unwrap_or(0) != expected_revision {
            return Err(PersistenceError::RevisionConflict(
                "station capacity domain".into(),
            ));
        }
        let changed = sqlx::query("INSERT INTO station_capacity_domains (station_id, provider_family, deployment_identity, region_identity, revision, updated_at) VALUES (?1, ?2, ?3, ?4, 1, ?5) ON CONFLICT(station_id) DO UPDATE SET provider_family = excluded.provider_family, deployment_identity = excluded.deployment_identity, region_identity = excluded.region_identity WHERE station_capacity_domains.revision = ?6")
            .bind(station_id).bind(provider_family).bind(deployment_identity).bind(region_identity).bind(now)
            .bind(expected_revision).execute(write.connection()).await?.rows_affected();
        if changed == 0 {
            return Err(PersistenceError::RevisionConflict(
                "station capacity domain".into(),
            ));
        }
        get_on_write(write, station_id).await
    }

    pub(crate) async fn clear(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        expected_revision: i64,
    ) -> Result<(), PersistenceError> {
        if station_id.trim().is_empty() || expected_revision <= 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        station_exists(write, station_id).await?;
        let changed = sqlx::query(
            "DELETE FROM station_capacity_domains WHERE station_id = ?1 AND revision = ?2",
        )
        .bind(station_id)
        .bind(expected_revision)
        .execute(write.connection())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(PersistenceError::RevisionConflict(
                "station capacity domain".into(),
            ));
        }
        Ok(())
    }
}

async fn station_exists(
    write: &mut WriteSession,
    station_id: &str,
) -> Result<(), PersistenceError> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM stations WHERE id = ?1")
        .bind(station_id)
        .fetch_optional(write.connection())
        .await?;
    if exists.is_none() {
        Err(PersistenceError::NotFound)
    } else {
        Ok(())
    }
}

async fn get_on_write(
    write: &mut WriteSession,
    station_id: &str,
) -> Result<StationCapacityDomain, PersistenceError> {
    sqlx::query("SELECT station_id, provider_family, deployment_identity, region_identity, revision, updated_at FROM station_capacity_domains WHERE station_id = ?1")
        .bind(station_id).fetch_one(write.connection()).await.map_err(Into::into).and_then(row_to_domain)
}

fn row_to_domain(row: sqlx::sqlite::SqliteRow) -> Result<StationCapacityDomain, PersistenceError> {
    Ok(StationCapacityDomain {
        station_id: row.get("station_id"),
        provider_family: row.get("provider_family"),
        deployment_identity: row.get("deployment_identity"),
        region_identity: row.get("region_identity"),
        revision: row.get("revision"),
        updated_at: row.get("updated_at"),
    })
}

fn validate(
    station_id: &str,
    provider_family: &str,
    deployment: Option<&str>,
    region: Option<&str>,
    expected_revision: i64,
) -> Result<(), PersistenceError> {
    if station_id.trim().is_empty()
        || provider_family.trim().is_empty()
        || provider_family.len() > 128
        || expected_revision < 0
        || deployment.is_some_and(|value| value.trim().is_empty() || value.len() > 256)
        || region.is_some_and(|value| value.trim().is_empty() || value.len() > 128)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}
