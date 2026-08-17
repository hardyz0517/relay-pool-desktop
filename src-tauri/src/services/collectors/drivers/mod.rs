pub mod newapi;
pub mod sub2api;

use std::sync::Arc;

use crate::services::collectors::contract::{
    AuthorizationCapabilityDescriptor, CollectorCapabilityDescriptor, CollectorTaskKind,
    DriverCapabilities, ProviderDescriptor, ProviderEntry, ProviderKind,
    RemoteKeyCapabilityDescriptor,
};

pub fn static_provider_entries() -> Vec<ProviderEntry> {
    vec![
        ProviderEntry {
            descriptor: ProviderDescriptor {
                kind: ProviderKind::Sub2Api,
                display_name: "Sub2API",
                station_types: &["sub2api"],
                capabilities: DriverCapabilities {
                    collector: Some(CollectorCapabilityDescriptor {
                        supported_tasks: sub2api::SUPPORTED_COLLECTOR_TASKS,
                        full_tasks: sub2api::FULL_COLLECTOR_TASKS,
                    }),
                    remote_key: Some(RemoteKeyCapabilityDescriptor {
                        supports_list: true,
                        supports_create: true,
                        supports_delete: true,
                        supports_reveal: true,
                        supports_result_unknown_reconciliation: true,
                    }),
                    authorization: None,
                },
            },
            collector: Some(Arc::new(sub2api::Sub2ApiCollectorDriver)),
            remote_key: Some(Arc::new(sub2api::Sub2ApiRemoteKeyDriver)),
            authorization: None,
        },
        ProviderEntry {
            descriptor: ProviderDescriptor {
                kind: ProviderKind::NewApi,
                display_name: "NewAPI",
                station_types: &["newapi"],
                capabilities: DriverCapabilities {
                    collector: Some(CollectorCapabilityDescriptor {
                        supported_tasks: newapi::SUPPORTED_COLLECTOR_TASKS,
                        full_tasks: newapi::FULL_COLLECTOR_TASKS,
                    }),
                    remote_key: Some(RemoteKeyCapabilityDescriptor {
                        supports_list: true,
                        supports_create: true,
                        supports_delete: true,
                        supports_reveal: true,
                        supports_result_unknown_reconciliation: true,
                    }),
                    authorization: Some(AuthorizationCapabilityDescriptor {
                        supports_header_validation: true,
                        supports_session_validation: true,
                    }),
                },
            },
            collector: Some(Arc::new(newapi::NewApiCollectorDriver)),
            remote_key: Some(Arc::new(newapi::NewApiRemoteKeyDriver)),
            authorization: Some(Arc::new(newapi::NewApiAuthorizationDriver)),
        },
    ]
}

pub const REQUIRED_PROVIDER_KINDS: &[ProviderKind] = &[ProviderKind::Sub2Api, ProviderKind::NewApi];

/// Read-model composition has no runtime registry, so it reads the same
/// provider descriptors used to build the registry rather than hard-coding
/// individual station types.
pub fn station_type_supports_collector_task(station_type: &str, task: CollectorTaskKind) -> bool {
    static_provider_entries().into_iter().any(|entry| {
        entry
            .descriptor
            .station_types
            .contains(&station_type.trim())
            && entry
                .descriptor
                .capabilities
                .collector
                .as_ref()
                .is_some_and(|capability| capability.supported_tasks.contains(&task))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_status_support_is_declared_by_provider_capability() {
        assert!(station_type_supports_collector_task(
            "sub2api",
            CollectorTaskKind::PublishedStatus,
        ));
        assert!(!station_type_supports_collector_task(
            "newapi",
            CollectorTaskKind::PublishedStatus,
        ));
    }
}
