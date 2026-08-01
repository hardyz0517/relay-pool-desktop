#![allow(dead_code)]

use std::fmt;

use futures_util::future::BoxFuture;

use crate::{
    application::routing_engine::capacity::CapacityLease,
    models::{
        credentials::{SecretBytes, SecretRef},
        proxy::UpstreamApiFormat,
        station_endpoints::{normalize_api_base_url, sanitized_api_base_url_for_trace},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionTargetRef {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) api_base_url: String,
    pub(crate) upstream_api_format: UpstreamApiFormat,
    pub(crate) collector_proxy_mode: String,
    pub(crate) collector_proxy_url: Option<String>,
    pub(crate) enabled: bool,
    pub(crate) api_key_secret_ref: Option<SecretRef>,
    pub(crate) inline_api_key_present: bool,
}

#[derive(Debug)]
pub(crate) struct LeasedSelectedTarget {
    pub(crate) station_key_id: String,
    pub(crate) expected_endpoint_revision: i64,
    pub(crate) expected_secret_ref_id: String,
    pub(crate) lease: CapacityLease,
}

pub(crate) trait ExecutionCredentialResolver: Send + Sync {
    fn resolve_station_key_secret_ref(
        &self,
        station_key_id: String,
        secret_ref: SecretRef,
    ) -> BoxFuture<'static, Result<SecretBytes, ExecutionTargetError>>;
}

pub(crate) struct ExecutionTargetResolver;

impl ExecutionTargetResolver {
    pub(crate) async fn resolve<C>(
        selected: LeasedSelectedTarget,
        current: ExecutionTargetRef,
        credentials: &C,
    ) -> Result<ExecutionTargetHandle, ExecutionTargetError>
    where
        C: ExecutionCredentialResolver + ?Sized,
    {
        if selected.station_key_id != current.station_key_id
            || selected.expected_endpoint_revision != current.endpoint_revision
        {
            return Err(ExecutionTargetError::StaleTarget {
                station_key_id: selected.station_key_id,
                expected_endpoint_revision: selected.expected_endpoint_revision,
                actual_endpoint_revision: current.endpoint_revision,
            });
        }
        if !current.enabled {
            return Err(ExecutionTargetError::TargetUnavailable {
                station_key_id: current.station_key_id,
                reason: "target_disabled",
            });
        }
        let Some(secret_ref) = current.api_key_secret_ref else {
            return Err(ExecutionTargetError::MissingCredentialRef {
                station_key_id: current.station_key_id,
                inline_api_key_present: current.inline_api_key_present,
            });
        };
        if selected.expected_secret_ref_id != secret_ref.id {
            return Err(ExecutionTargetError::StaleCredentialRef {
                station_key_id: current.station_key_id,
                expected_secret_ref_id: selected.expected_secret_ref_id,
                actual_secret_ref_id: secret_ref.id,
            });
        }
        let normalized_api_base_url =
            normalize_api_base_url(&current.api_base_url).map_err(|_| {
                ExecutionTargetError::InvalidEndpoint {
                    station_key_id: current.station_key_id.clone(),
                    sanitized_api_base_url: sanitized_api_base_url_for_trace(&current.api_base_url),
                }
            })?;
        let api_key = credentials
            .resolve_station_key_secret_ref(current.station_key_id.clone(), secret_ref)
            .await?;
        Ok(ExecutionTargetHandle {
            station_key_id: current.station_key_id,
            station_id: current.station_id,
            endpoint_revision: current.endpoint_revision,
            api_base_url: normalized_api_base_url,
            upstream_api_format: current.upstream_api_format,
            collector_proxy_mode: current.collector_proxy_mode,
            collector_proxy_url: current.collector_proxy_url,
            api_key,
            lease: selected.lease,
        })
    }
}

pub(crate) struct ExecutionTargetHandle {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) api_base_url: String,
    pub(crate) upstream_api_format: UpstreamApiFormat,
    pub(crate) collector_proxy_mode: String,
    pub(crate) collector_proxy_url: Option<String>,
    pub(crate) api_key: SecretBytes,
    pub(crate) lease: CapacityLease,
}

impl fmt::Debug for ExecutionTargetHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionTargetHandle")
            .field("station_key_id", &self.station_key_id)
            .field("station_id", &self.station_id)
            .field("endpoint_revision", &self.endpoint_revision)
            .field("upstream_api_format", &self.upstream_api_format)
            .field("api_key", &"<redacted>")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExecutionTargetError {
    StaleTarget {
        station_key_id: String,
        expected_endpoint_revision: i64,
        actual_endpoint_revision: i64,
    },
    StaleCredentialRef {
        station_key_id: String,
        expected_secret_ref_id: String,
        actual_secret_ref_id: String,
    },
    TargetUnavailable {
        station_key_id: String,
        reason: &'static str,
    },
    MissingCredentialRef {
        station_key_id: String,
        inline_api_key_present: bool,
    },
    InvalidEndpoint {
        station_key_id: String,
        sanitized_api_base_url: String,
    },
    SecretUnavailable {
        station_key_id: String,
    },
}
