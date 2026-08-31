use std::fmt;

use sha2::{Digest, Sha256};

use crate::{
    application::{
        credentials::{ExecutionCredentialResolver, SecretBytes, SecretRef},
        routing_engine::capacity::CapacityLease,
    },
    models::{
        proxy::UpstreamApiFormat,
        station_endpoints::{normalize_api_base_url, sanitized_api_base_url_for_trace},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionTargetRef {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) station_type: String,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) endpoint_revision: i64,
    pub(crate) credential_revision: i64,
    pub(crate) account_revision: i64,
    pub(crate) group_revision: Option<i64>,
    pub(crate) api_base_url: String,
    pub(crate) upstream_api_format: UpstreamApiFormat,
    pub(crate) collector_proxy_mode: String,
    pub(crate) collector_proxy_url: Option<String>,
    pub(crate) enabled: bool,
    pub(crate) api_key_secret_ref: Option<SecretRef>,
    pub(crate) inline_api_key_present: bool,
    pub(crate) station_account_max_concurrency: u32,
    pub(crate) station_key_max_concurrency: u32,
}

#[derive(Debug)]
pub(crate) struct LeasedSelectedTarget {
    pub(crate) station_key_id: String,
    pub(crate) expected_endpoint_revision: i64,
    pub(crate) expected_secret_ref_id: String,
    pub(crate) expected_credential_revision: i64,
    pub(crate) expected_account_revision: i64,
    pub(crate) expected_group_binding_id: Option<String>,
    pub(crate) expected_group_revision: Option<i64>,
    pub(crate) resolved_upstream_model: Option<String>,
    pub(crate) model_alias_revision: i64,
    pub(crate) policy_revision: u64,
    pub(crate) request_body_identity: RequestBodyIdentity,
    pub(crate) protocol_profile: TargetProtocolProfile,
    pub(crate) lease: CapacityLease,
}

pub(crate) const TARGET_EXECUTION_COMMITMENT_VERSION: &str = "target-execution-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestBodyIdentity {
    pub(crate) byte_len: usize,
    pub(crate) sha256_hex: String,
}

impl RequestBodyIdentity {
    pub(crate) fn from_bytes(body: &[u8]) -> Self {
        Self {
            byte_len: body.len(),
            sha256_hex: encode_hex(&Sha256::digest(body)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetProtocolProfile {
    pub(crate) upstream_api_format: UpstreamApiFormat,
    pub(crate) stream: bool,
    pub(crate) uses_tools: bool,
    pub(crate) uses_vision: bool,
    pub(crate) uses_reasoning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetExecutionCommitment {
    pub(crate) version: &'static str,
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    /// Provider semantics select the error-rule profile. Retain the identity
    /// in the retry fence so an in-flight request cannot change classifier
    /// behavior after a station configuration update.
    pub(crate) station_type: String,
    pub(crate) credential_revision: i64,
    pub(crate) endpoint_revision: i64,
    pub(crate) account_revision: i64,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) group_revision: Option<i64>,
    pub(crate) resolved_upstream_model: Option<String>,
    pub(crate) model_alias_revision: i64,
    pub(crate) policy_revision: u64,
    pub(crate) request_body_identity: RequestBodyIdentity,
    pub(crate) protocol_profile: TargetProtocolProfile,
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
        let Some(secret_ref) = current.api_key_secret_ref.clone() else {
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
            .await
            .map_err(|error| ExecutionTargetError::SecretUnavailable {
                station_key_id: error.station_key_id,
            })?;
        let commitment = Self::commitment(&selected, &current)?;
        Ok(ExecutionTargetHandle {
            station_key_id: current.station_key_id,
            station_id: current.station_id,
            station_type: current.station_type,
            group_binding_id: current.group_binding_id,
            endpoint_revision: current.endpoint_revision,
            api_base_url: normalized_api_base_url,
            upstream_api_format: current.upstream_api_format,
            collector_proxy_mode: current.collector_proxy_mode,
            collector_proxy_url: current.collector_proxy_url,
            api_key,
            commitment,
            lease: selected.lease,
        })
    }

    /// The resolver is the only constructor and verifier for execution
    /// commitments. Retry code receives this opaque complete value and must
    /// not rebuild a weaker revision list.
    pub(crate) fn commitment(
        selected: &LeasedSelectedTarget,
        current: &ExecutionTargetRef,
    ) -> Result<TargetExecutionCommitment, ExecutionTargetError> {
        if selected.model_alias_revision <= 0
            || selected.policy_revision == 0
            || selected.expected_credential_revision <= 0
            || selected.expected_account_revision <= 0
            || selected
                .expected_group_revision
                .is_some_and(|revision| revision <= 0)
            || selected.expected_group_binding_id.is_some()
                != selected.expected_group_revision.is_some()
        {
            return Err(ExecutionTargetError::InvalidCommitment {
                station_key_id: selected.station_key_id.clone(),
            });
        }
        if selected.expected_credential_revision != current.credential_revision
            || selected.expected_account_revision != current.account_revision
            || selected.expected_group_revision != current.group_revision
        {
            return Err(ExecutionTargetError::CommitmentChanged {
                station_key_id: current.station_key_id.clone(),
            });
        }
        Ok(TargetExecutionCommitment {
            version: TARGET_EXECUTION_COMMITMENT_VERSION,
            station_key_id: current.station_key_id.clone(),
            station_id: current.station_id.clone(),
            station_type: current.station_type.clone(),
            credential_revision: current.credential_revision,
            endpoint_revision: current.endpoint_revision,
            account_revision: current.account_revision,
            group_binding_id: selected.expected_group_binding_id.clone(),
            group_revision: current.group_revision,
            resolved_upstream_model: selected.resolved_upstream_model.clone(),
            model_alias_revision: selected.model_alias_revision,
            policy_revision: selected.policy_revision,
            request_body_identity: selected.request_body_identity.clone(),
            protocol_profile: selected.protocol_profile.clone(),
        })
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=execution-target-revalidation; owner=application/operational_facts; remove_when=all retry callers use the v3 admission CAS"
        )
    )]
    pub(crate) fn revalidate_commitment(
        expected: &TargetExecutionCommitment,
        current: &TargetExecutionCommitment,
    ) -> Result<(), ExecutionTargetError> {
        if expected == current {
            Ok(())
        } else {
            Err(ExecutionTargetError::CommitmentChanged {
                station_key_id: current.station_key_id.clone(),
            })
        }
    }
}

pub(crate) struct ExecutionTargetHandle {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) station_type: String,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) endpoint_revision: i64,
    pub(crate) api_base_url: String,
    pub(crate) upstream_api_format: UpstreamApiFormat,
    pub(crate) collector_proxy_mode: String,
    pub(crate) collector_proxy_url: Option<String>,
    pub(crate) api_key: SecretBytes,
    pub(crate) commitment: TargetExecutionCommitment,
    pub(crate) lease: CapacityLease,
}

impl ExecutionTargetHandle {
    pub(crate) fn into_capacity_lease(self) -> CapacityLease {
        self.lease
    }
}

impl fmt::Debug for ExecutionTargetHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionTargetHandle")
            .field("station_key_id", &self.station_key_id)
            .field("station_id", &self.station_id)
            .field("endpoint_revision", &self.endpoint_revision)
            .field("upstream_api_format", &self.upstream_api_format)
            .field("commitment_version", &self.commitment.version)
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
    InvalidCommitment {
        station_key_id: String,
    },
    CommitmentChanged {
        station_key_id: String,
    },
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}
