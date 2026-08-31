use std::sync::Arc;

use crate::{
    application::{clock::Clock, error::ApplicationError},
    models::station_capacity_domains::{
        ClearStationCapacityDomainInput, StationCapacityDomain, UpsertStationCapacityDomainInput,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::station_capacity_domain_store::StationCapacityDomainStore,
    },
};

#[derive(Clone)]
pub(crate) struct StationCapacityDomainService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    store: StationCapacityDomainStore,
}

impl StationCapacityDomainService {
    pub(crate) fn new(runtime: PersistenceHandle, clock: Arc<dyn Clock>) -> Self {
        Self {
            runtime,
            clock,
            store: StationCapacityDomainStore,
        }
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-capacity-domain-service-reference; owner=application/station_capacity_domains; remove_when=capacity-domain reference endpoints are deleted"
        )
    )]
    pub(crate) async fn get(
        &self,
        station_id: String,
    ) -> Result<Option<StationCapacityDomain>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .get(&mut read, &station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn upsert(
        &self,
        input: UpsertStationCapacityDomainInput,
    ) -> Result<StationCapacityDomain, ApplicationError> {
        let store = self.store;
        let now = self.clock.now_utc().timestamp_millis().to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .upsert(
                            write,
                            &input.station_id,
                            input.expected_revision,
                            &input.provider_family,
                            input.deployment_identity.as_deref(),
                            input.region_identity.as_deref(),
                            &now,
                        )
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-capacity-domain-service-reference; owner=application/station_capacity_domains; remove_when=capacity-domain reference endpoints are deleted"
        )
    )]
    pub(crate) async fn clear(
        &self,
        input: ClearStationCapacityDomainInput,
    ) -> Result<(), ApplicationError> {
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .clear(write, &input.station_id, input.expected_revision)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        application::{clock::SystemClock, ids::UuidV7Generator, stations::StationService},
        models::{
            station_capacity_domains::{
                ClearStationCapacityDomainInput, UpsertStationCapacityDomainInput,
            },
            stations::CreateStationInput,
        },
        persistence::runtime::PersistenceRuntime,
    };

    use super::StationCapacityDomainService;

    #[tokio::test]
    async fn upsert_and_clear_are_revision_fenced() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("capacity-domain.sqlite3"))
                .await
                .expect("runtime");
        let clock = Arc::new(SystemClock);
        let stations =
            StationService::new(runtime.handle(), clock.clone(), Arc::new(UuidV7Generator));
        let station = stations
            .create(CreateStationInput {
                name: "Capacity domain fixture".into(),
                station_type: "newapi".into(),
                website_url: "https://capacity-domain.invalid".into(),
                api_base_url: "https://capacity-domain.invalid/v1".into(),
                api_key: String::new(),
                collector_proxy_mode: "inherit".into(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .await
            .expect("station");
        let service = StationCapacityDomainService::new(runtime.handle(), clock);
        let first = service
            .upsert(UpsertStationCapacityDomainInput {
                station_id: station.id.clone(),
                expected_revision: 0,
                provider_family: "openai".into(),
                deployment_identity: Some("prod".into()),
                region_identity: Some("us-east".into()),
            })
            .await
            .expect("initial upsert");
        assert_eq!(first.revision, 1);
        let second = service
            .upsert(UpsertStationCapacityDomainInput {
                station_id: station.id.clone(),
                expected_revision: first.revision,
                provider_family: "openai".into(),
                deployment_identity: Some("prod-2".into()),
                region_identity: Some("us-east".into()),
            })
            .await
            .expect("revision update");
        assert_eq!(second.revision, 2);
        assert!(service
            .clear(ClearStationCapacityDomainInput {
                station_id: station.id.clone(),
                expected_revision: first.revision,
            })
            .await
            .is_err());
        service
            .clear(ClearStationCapacityDomainInput {
                station_id: station.id.clone(),
                expected_revision: second.revision,
            })
            .await
            .expect("clear current revision");
        assert!(service.get(station.id).await.expect("read").is_none());
    }
}
