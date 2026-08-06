mod application {
    pub(crate) mod credentials {
        use std::fmt;

        use futures_util::future::BoxFuture;
        use zeroize::Zeroizing;

        pub(crate) struct SecretBytes(Zeroizing<Vec<u8>>);

        impl SecretBytes {
            pub(crate) fn as_bytes(&self) -> &[u8] {
                self.0.as_slice()
            }
        }

        impl From<String> for SecretBytes {
            fn from(value: String) -> Self {
                Self(Zeroizing::new(value.into_bytes()))
            }
        }

        impl fmt::Debug for SecretBytes {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct("SecretBytes")
                    .field("len", &self.0.len())
                    .finish_non_exhaustive()
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub(crate) struct SecretRef {
            pub(crate) id: String,
            pub(crate) scope: String,
            pub(crate) owner_id: String,
            pub(crate) kind: String,
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub(crate) struct ExecutionCredentialError {
            pub(crate) station_key_id: String,
        }

        pub(crate) trait ExecutionCredentialResolver: Send + Sync {
            fn resolve_station_key_secret_ref(
                &self,
                station_key_id: String,
                secret_ref: SecretRef,
            ) -> BoxFuture<'static, Result<SecretBytes, ExecutionCredentialError>>;
        }
    }

    pub(crate) mod routing_engine {
        pub(crate) mod capacity {
            use std::{
                collections::BTreeMap,
                sync::{Arc, Mutex},
            };

            #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
            pub(crate) enum CapacityConstraintKey {
                StationKey(String),
            }

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) enum ProviderAccountConstraint {
                NotApplicable,
            }

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct CompositeCapacityRequest {
                pub(crate) station_id: String,
                pub(crate) station_key_id: String,
                pub(crate) half_open_probe_id: Option<String>,
                pub(crate) global_max_concurrency: u32,
                pub(crate) station_account_max_concurrency: u32,
                pub(crate) station_key_max_concurrency: u32,
                pub(crate) provider_account_constraint: ProviderAccountConstraint,
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
            pub(crate) struct CapacityGauge {
                pub(crate) active: u32,
            }

            #[derive(Debug, Default, Clone)]
            pub(crate) struct CompositeCapacityRegistry {
                active_by_constraint: Arc<Mutex<BTreeMap<CapacityConstraintKey, u32>>>,
            }

            impl CompositeCapacityRegistry {
                pub(crate) fn try_acquire(
                    &self,
                    request: CompositeCapacityRequest,
                ) -> Result<CapacityLease, &'static str> {
                    let _fixture_contract = (
                        &request.station_id,
                        &request.half_open_probe_id,
                        request.global_max_concurrency,
                        request.station_account_max_concurrency,
                        request.station_key_max_concurrency,
                        &request.provider_account_constraint,
                    );
                    let constraint = CapacityConstraintKey::StationKey(request.station_key_id);
                    let mut active_by_constraint = self
                        .active_by_constraint
                        .lock()
                        .expect("fixture capacity registry poisoned");
                    *active_by_constraint.entry(constraint.clone()).or_default() += 1;
                    Ok(CapacityLease {
                        active_by_constraint: Some(Arc::clone(&self.active_by_constraint)),
                        constraint,
                        released: false,
                    })
                }

                pub(crate) fn gauge(&self, constraint: &CapacityConstraintKey) -> CapacityGauge {
                    let active_by_constraint = self
                        .active_by_constraint
                        .lock()
                        .expect("fixture capacity registry poisoned");
                    CapacityGauge {
                        active: active_by_constraint
                            .get(constraint)
                            .copied()
                            .unwrap_or_default(),
                    }
                }
            }

            #[derive(Debug)]
            pub(crate) struct CapacityLease {
                active_by_constraint: Option<Arc<Mutex<BTreeMap<CapacityConstraintKey, u32>>>>,
                constraint: CapacityConstraintKey,
                released: bool,
            }

            impl CapacityLease {
                pub(crate) fn release(&mut self) {
                    if self.released {
                        return;
                    }
                    self.released = true;
                    let Some(active_by_constraint) = &self.active_by_constraint else {
                        return;
                    };
                    let mut active_by_constraint = active_by_constraint
                        .lock()
                        .expect("fixture capacity registry poisoned");
                    if let Some(active) = active_by_constraint.get_mut(&self.constraint) {
                        *active = active.saturating_sub(1);
                    }
                }
            }

            impl Drop for CapacityLease {
                fn drop(&mut self) {
                    self.release();
                }
            }

            #[derive(Debug)]
            pub(crate) struct RetryPermit;
        }
    }
}

mod models {
    pub(crate) mod proxy {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum UpstreamApiFormat {
            Auto,
        }
    }

    pub(crate) mod station_endpoints {
        pub(crate) use crate::station_endpoints::*;
    }
}

#[path = "../src/models/station_endpoints.rs"]
mod station_endpoints;
#[path = "../src/application/operational_facts/target_resolver.rs"]
mod target_resolver;

use futures_util::future::BoxFuture;

use application::credentials::{
    ExecutionCredentialError, ExecutionCredentialResolver, SecretBytes, SecretRef,
};
use application::routing_engine::capacity::{
    CapacityConstraintKey, CompositeCapacityRegistry, CompositeCapacityRequest,
};
use models::{proxy::UpstreamApiFormat, station_endpoints::sanitized_api_base_url_for_trace};
use target_resolver::{
    ExecutionTargetError, ExecutionTargetRef, ExecutionTargetResolver, LeasedSelectedTarget,
};

#[derive(Clone)]
struct FakeCredentialResolver {
    secret: String,
}

impl ExecutionCredentialResolver for FakeCredentialResolver {
    fn resolve_station_key_secret_ref(
        &self,
        station_key_id: String,
        secret_ref: SecretRef,
    ) -> BoxFuture<'static, Result<SecretBytes, ExecutionCredentialError>> {
        let secret = self.secret.clone();
        Box::pin(async move {
            if secret_ref.owner_id != station_key_id {
                return Err(ExecutionCredentialError { station_key_id });
            }
            Ok(SecretBytes::from(secret))
        })
    }
}

fn secret_ref(id: &str, station_key_id: &str) -> SecretRef {
    SecretRef {
        id: id.to_string(),
        scope: "station_key".to_string(),
        owner_id: station_key_id.to_string(),
        kind: "api_key".to_string(),
    }
}

fn target_ref(
    station_key_id: &str,
    endpoint_revision: i64,
    api_key_secret_ref: Option<SecretRef>,
) -> ExecutionTargetRef {
    ExecutionTargetRef {
        station_key_id: station_key_id.to_string(),
        station_id: format!("station-{station_key_id}"),
        endpoint_revision,
        api_base_url: "https://relay.example/proxy/v1".to_string(),
        upstream_api_format: UpstreamApiFormat::Auto,
        collector_proxy_mode: "direct".to_string(),
        collector_proxy_url: None,
        enabled: true,
        api_key_secret_ref,
        inline_api_key_present: false,
        station_account_max_concurrency: 0,
        station_key_max_concurrency: 1,
    }
}

fn leased_selected(
    registry: &CompositeCapacityRegistry,
    station_key_id: &str,
    expected_endpoint_revision: i64,
    expected_secret_ref_id: &str,
) -> LeasedSelectedTarget {
    let lease = registry
        .try_acquire(CompositeCapacityRequest {
            station_id: format!("station-{station_key_id}"),
            station_key_id: station_key_id.to_string(),
            half_open_probe_id: None,
            global_max_concurrency: 4,
            station_account_max_concurrency: 4,
            station_key_max_concurrency: 1,
            provider_account_constraint:
                application::routing_engine::capacity::ProviderAccountConstraint::NotApplicable,
        })
        .expect("selected route capacity lease");
    LeasedSelectedTarget {
        station_key_id: station_key_id.to_string(),
        expected_endpoint_revision,
        expected_secret_ref_id: expected_secret_ref_id.to_string(),
        lease,
        retry_permit: None,
    }
}

#[tokio::test]
async fn resolver_returns_non_clone_handle_and_debug_redacts_plaintext_secret() {
    let registry = CompositeCapacityRegistry::default();
    let selected = leased_selected(&registry, "key-a", 3, "secret-a");
    let credentials = FakeCredentialResolver {
        secret: "sk-task17-secret-canary".to_string(),
    };

    let handle = ExecutionTargetResolver::resolve(
        selected,
        target_ref("key-a", 3, Some(secret_ref("secret-a", "key-a"))),
        &credentials,
    )
    .await
    .expect("resolved target");

    assert_eq!(handle.api_key.as_bytes(), b"sk-task17-secret-canary");
    assert_eq!(handle.station_id, "station-key-a");
    assert_eq!(handle.endpoint_revision, 3);
    assert_eq!(handle.api_base_url, "https://relay.example/proxy/v1");
    assert_eq!(handle.upstream_api_format, UpstreamApiFormat::Auto);
    assert_eq!(handle.collector_proxy_mode, "direct");
    assert_eq!(handle.collector_proxy_url, None);
    assert!(format!("{:?}", &handle.lease).contains("CapacityLease"));
    let debug = format!("{handle:?}");
    assert!(debug.contains("key-a"));
    assert!(!debug.contains("relay.example"));
    assert!(!debug.contains("sk-task17-secret-canary"));
    assert!(!debug.contains("Bearer"));
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::StationKey("key-a".to_string()))
            .active,
        1,
        "handle owns the capacity lease until request send/build scope ends"
    );
    drop(handle);
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::StationKey("key-a".to_string()))
            .active,
        0
    );
}

#[tokio::test]
async fn stale_endpoint_or_credential_ref_returns_typed_error_and_releases_lease() {
    let registry = CompositeCapacityRegistry::default();
    let credentials = FakeCredentialResolver {
        secret: "sk-task17-secret-canary".to_string(),
    };
    let stale_endpoint = ExecutionTargetResolver::resolve(
        leased_selected(&registry, "key-a", 3, "secret-a"),
        target_ref("key-a", 4, Some(secret_ref("secret-a", "key-a"))),
        &credentials,
    )
    .await
    .expect_err("stale endpoint");
    assert!(matches!(
        stale_endpoint,
        ExecutionTargetError::StaleTarget {
            expected_endpoint_revision: 3,
            actual_endpoint_revision: 4,
            ..
        }
    ));
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::StationKey("key-a".to_string()))
            .active,
        0
    );

    let stale_secret = ExecutionTargetResolver::resolve(
        leased_selected(&registry, "key-a", 3, "secret-a"),
        target_ref("key-a", 3, Some(secret_ref("secret-b", "key-a"))),
        &credentials,
    )
    .await
    .expect_err("stale secret");
    assert!(matches!(
        stale_secret,
        ExecutionTargetError::StaleCredentialRef {
            expected_secret_ref_id,
            actual_secret_ref_id,
            ..
        } if expected_secret_ref_id == "secret-a" && actual_secret_ref_id == "secret-b"
    ));
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::StationKey("key-a".to_string()))
            .active,
        0
    );
}

#[tokio::test]
async fn missing_opaque_secret_ref_does_not_fall_back_to_inline_legacy_plaintext() {
    let registry = CompositeCapacityRegistry::default();
    let credentials = FakeCredentialResolver {
        secret: "sk-task17-secret-canary".to_string(),
    };
    let mut current = target_ref("key-a", 3, None);
    current.inline_api_key_present = true;

    let failure = ExecutionTargetResolver::resolve(
        leased_selected(&registry, "key-a", 3, "secret-a"),
        current,
        &credentials,
    )
    .await
    .expect_err("missing secret ref");

    assert!(matches!(
        failure,
        ExecutionTargetError::MissingCredentialRef {
            inline_api_key_present: true,
            ..
        }
    ));
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::StationKey("key-a".to_string()))
            .active,
        0
    );
}

#[test]
fn url_trace_sanitizer_redacts_unsafe_or_invalid_api_bases() {
    for value in [
        "https://user:secret@relay.example/v1",
        "https://relay.example/v1?token=secret",
        "https://relay.example/v1#secret",
        "file:///tmp/secret",
        "%",
    ] {
        assert_eq!(
            sanitized_api_base_url_for_trace(value),
            "[redacted-invalid-url]"
        );
    }
    assert_eq!(
        sanitized_api_base_url_for_trace("https://relay.example/proxy/v1/"),
        "https://relay.example/proxy/v1"
    );
}
