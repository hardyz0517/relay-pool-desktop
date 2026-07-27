use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::{
    application::{credentials::CredentialService, error::ApplicationError},
    models::{
        group_facts::{GroupRateRecord, StationGroupBinding},
        remote_keys::{
            CreateLocalStationKeyFromRemoteResult, CreateRemoteStationKeyInput,
            CreateRemoteStationKeyResult, RemoteKeyCapability, RemoteKeyScanResult,
            RemoteStationKey,
        },
        station_keys::{StationKey, UpdateStationKeyInput},
    },
    observability::correlation,
    outbound::{AsyncOutboundClient, ManualProxy, ProxyPolicy, RequestBudget},
    services::collectors::{
        adapters,
        contract::{
            CollectorContext, CreateRemoteKeyRequest, CredentialScope, CredentialSecret,
            CredentialSecretPurpose, DriverSecretAccessor, OpaqueCredentialHandle,
            ProviderAuthContext, ProviderEndpoints, ProviderKind, RemoteKeyRequest,
            RevealRemoteKeyRequest, StationIdentity, Sub2ApiLoginCredential,
        },
        failure::{DriverFailure, DriverFailureKind},
        orchestration::ProviderRegistry,
        output, CollectorSourcePort,
    },
};

// V2CollectorSourceAdapter resolves secrets through CredentialService; provider
// adapters retain this argument only for the temporary legacy port implementation.
const V2_UNUSED_DATA_KEY: [u8; 32] = [0; 32];

#[derive(Debug)]
pub(crate) enum RemoteKeyOperationError {
    Application(ApplicationError),
    Unsupported,
    ExternalUnavailable,
    ResultUnknown,
    Conflict,
    Internal,
}

impl From<ApplicationError> for RemoteKeyOperationError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

pub(crate) enum PreparedRemoteKeyScan {
    Unsupported {
        station_id: String,
        capability: RemoteKeyCapability,
    },
    Discovered {
        station_id: String,
        expected_endpoint_revision: i64,
        capability: RemoteKeyCapability,
        keys: Vec<RemoteStationKey>,
        station_key_updates: Vec<UpdateStationKeyInput>,
    },
}
pub(crate) struct PreparedRemoteKeySave {
    remote_key: RemoteStationKey,
    expected_endpoint_revision: i64,
    matched_station_key_update: Option<UpdateStationKeyInput>,
    new_group_binding_id: Option<String>,
    full_key: String,
    adapter_message: String,
    expose_full_key_once: bool,
    matched_existing: bool,
}

pub(crate) struct PreparedNewApiRemoteKeyDriverContext {
    station_id: String,
    expected_endpoint_revision: i64,
    capability: RemoteKeyCapability,
    station: StationIdentity,
    endpoints: ProviderEndpoints,
    credential_handle: OpaqueCredentialHandle,
    auth_context: ProviderAuthContext,
    secret_accessor: RemoteKeySecretAccessor,
    proxy: ProxyPolicy,
}

struct RemoteKeySecretAccessor {
    records: Vec<RemoteKeySecretRecord>,
}

struct RemoteKeySecretRecord {
    handle: OpaqueCredentialHandle,
    purpose: CredentialSecretPurpose,
    secret: String,
}

impl DriverSecretAccessor for RemoteKeySecretAccessor {
    fn resolve_secret<'a>(
        &'a self,
        handle: &'a OpaqueCredentialHandle,
        purpose: CredentialSecretPurpose,
    ) -> futures_util::future::BoxFuture<'a, Result<CredentialSecret, DriverFailure>> {
        Box::pin(async move {
            let Some(record) = self
                .records
                .iter()
                .find(|record| record.purpose == purpose && &record.handle == handle)
            else {
                return Err(DriverFailure::unsupported(
                    "credential handle is not available to this remote-key driver context",
                ));
            };
            Ok(CredentialSecret::new(record.secret.clone()))
        })
    }
}

pub(crate) struct PreparedSub2ApiRemoteKeyDriverContext {
    station_id: String,
    expected_endpoint_revision: i64,
    capability: RemoteKeyCapability,
    station: StationIdentity,
    endpoints: ProviderEndpoints,
    credential_handle: OpaqueCredentialHandle,
    auth_context: ProviderAuthContext,
    secret_accessor: RemoteKeySecretAccessor,
    proxy: ProxyPolicy,
}

pub(crate) fn prepare_newapi_remote_key_driver_context_v2(
    source: &dyn CollectorSourcePort,
    data_key: &[u8; 32],
    station_id: String,
) -> Result<Option<PreparedNewApiRemoteKeyDriverContext>, RemoteKeyOperationError> {
    let station = source
        .station_for_collector(&station_id)
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    if station.station_type.trim() != "newapi" {
        return Ok(None);
    }
    let capability = RemoteKeyCapability {
        station_id: station.id.clone(),
        station_type: "newapi".to_string(),
        can_list_remote_keys: true,
        can_create_remote_key: true,
        can_read_groups: true,
        requires_manual_session: true,
        unsupported_reason: None,
    };
    let session = source
        .resolve_station_session_with_data_key(
            station.id.clone(),
            data_key,
            crate::services::time::now_millis_for_services() as i64,
        )
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    let user_id = session
        .newapi_user_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or(RemoteKeyOperationError::ExternalUnavailable)?;
    let (secret_purpose, secret) = if let Some(access_token) = session
        .access_token
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        (CredentialSecretPurpose::AuthorizationHeader, access_token)
    } else if let Some(cookie) = session
        .cookie
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        (CredentialSecretPurpose::SessionCookie, cookie)
    } else {
        return Err(RemoteKeyOperationError::ExternalUnavailable);
    };
    let settings = source
        .get_settings()
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    let proxy = crate::services::outbound::resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let proxy = proxy_policy_from_remote_key_config(proxy)
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    let credential_handle = OpaqueCredentialHandle {
        station_id: station.id.clone(),
        credential_revision: station.endpoint_revision,
        scope: CredentialScope::LoginSession,
    };
    Ok(Some(PreparedNewApiRemoteKeyDriverContext {
        station_id: station.id.clone(),
        expected_endpoint_revision: station.endpoint_revision,
        capability,
        station: StationIdentity {
            station_id: station.id.clone(),
            endpoint_revision: station.endpoint_revision,
            provider: ProviderKind::NewApi,
        },
        endpoints: ProviderEndpoints {
            api_base_url: (!station.api_base_url.trim().is_empty())
                .then_some(station.api_base_url.clone()),
            website_url: Some(station.website_url.clone()),
        },
        credential_handle: credential_handle.clone(),
        auth_context: ProviderAuthContext::NewApi {
            user_id,
            secret_purpose,
        },
        secret_accessor: RemoteKeySecretAccessor {
            records: vec![RemoteKeySecretRecord {
                handle: credential_handle,
                purpose: secret_purpose,
                secret,
            }],
        },
        proxy,
    }))
}

pub(crate) fn prepare_sub2api_remote_key_driver_context_v2(
    source: &dyn CollectorSourcePort,
    data_key: &[u8; 32],
    station_id: String,
) -> Result<Option<PreparedSub2ApiRemoteKeyDriverContext>, RemoteKeyOperationError> {
    let station = source
        .station_for_collector(&station_id)
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    if station.station_type.trim() != "sub2api" {
        return Ok(None);
    }
    let capability = RemoteKeyCapability {
        station_id: station.id.clone(),
        station_type: "sub2api".to_string(),
        can_list_remote_keys: true,
        can_create_remote_key: true,
        can_read_groups: true,
        requires_manual_session: true,
        unsupported_reason: None,
    };
    let mut records = Vec::new();
    let login_session_handle = OpaqueCredentialHandle {
        station_id: station.id.clone(),
        credential_revision: station.endpoint_revision,
        scope: CredentialScope::LoginSession,
    };
    let session = source
        .resolve_station_session_with_data_key(
            station.id.clone(),
            data_key,
            crate::services::time::now_millis_for_services() as i64,
        )
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    let access_token = session
        .access_token
        .clone()
        .filter(|value| !value.trim().is_empty())
        .map(|token| {
            records.push(RemoteKeySecretRecord {
                handle: login_session_handle.clone(),
                purpose: CredentialSecretPurpose::SessionCookie,
                secret: token,
            });
            login_session_handle.clone()
        });

    let credentials = source
        .get_station_credentials(station.id.clone())
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    let login = credentials
        .login_username
        .as_deref()
        .map(str::trim)
        .filter(|username| !username.is_empty())
        .and_then(|username| {
            if !credentials.password_present {
                return None;
            }
            let password = source
                .get_station_login_password_with_data_key(station.id.clone(), data_key)
                .ok()
                .flatten()?;
            if password.trim().is_empty() {
                return None;
            }
            let handle = OpaqueCredentialHandle {
                station_id: station.id.clone(),
                credential_revision: station.endpoint_revision,
                scope: CredentialScope::LoginPassword,
            };
            records.push(RemoteKeySecretRecord {
                handle: handle.clone(),
                purpose: CredentialSecretPurpose::LoginPassword,
                secret: password,
            });
            Some(Sub2ApiLoginCredential {
                username: username.to_string(),
                password: handle,
            })
        });
    if access_token.is_none() && login.is_none() {
        return Err(RemoteKeyOperationError::ExternalUnavailable);
    }

    let settings = source
        .get_settings()
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    let proxy = crate::services::outbound::resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let proxy = proxy_policy_from_remote_key_config(proxy)
        .map_err(|_| RemoteKeyOperationError::Internal)?;

    Ok(Some(PreparedSub2ApiRemoteKeyDriverContext {
        station_id: station.id.clone(),
        expected_endpoint_revision: station.endpoint_revision,
        capability,
        station: StationIdentity {
            station_id: station.id.clone(),
            endpoint_revision: station.endpoint_revision,
            provider: ProviderKind::Sub2Api,
        },
        endpoints: ProviderEndpoints {
            api_base_url: Some(station.api_base_url.clone()),
            website_url: Some(station.website_url.clone()),
        },
        credential_handle: login_session_handle,
        auth_context: ProviderAuthContext::Sub2Api {
            station_keys: Vec::new(),
            access_token,
            login,
            credit_per_cny: station.credit_per_cny,
        },
        secret_accessor: RemoteKeySecretAccessor { records },
        proxy,
    }))
}

pub(crate) async fn prepare_newapi_remote_key_scan_v2(
    source: &dyn CollectorSourcePort,
    registry: &ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedNewApiRemoteKeyDriverContext,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedRemoteKeyScan, RemoteKeyOperationError> {
    let driver = registry
        .remote_key(ProviderKind::NewApi)
        .map_err(remote_key_error_from_driver)?;
    let context = newapi_remote_key_context(&prepared, outbound, cancellation, correlation_id);
    let output = driver
        .list_remote_keys(
            &context,
            RemoteKeyRequest {
                station: prepared.station.clone(),
                endpoints: prepared.endpoints.clone(),
                credential: prepared.credential_handle.clone(),
            },
        )
        .await
        .map_err(remote_key_error_from_driver)?;
    let (keys, station_key_updates) =
        enrich_remote_key_discoveries_from_source(source, &prepared.station_id, output.keys)
            .map_err(|_| RemoteKeyOperationError::Internal)?;
    ensure_source_endpoint_revision(
        source,
        &prepared.station_id,
        prepared.expected_endpoint_revision,
    )?;
    Ok(PreparedRemoteKeyScan::Discovered {
        station_id: prepared.station_id,
        expected_endpoint_revision: prepared.expected_endpoint_revision,
        capability: prepared.capability,
        keys,
        station_key_updates,
    })
}

pub(crate) async fn prepare_newapi_remote_key_creation_v2(
    source: &dyn CollectorSourcePort,
    registry: &ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedNewApiRemoteKeyDriverContext,
    input: CreateRemoteStationKeyInput,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError> {
    let driver = registry
        .remote_key(ProviderKind::NewApi)
        .map_err(remote_key_error_from_driver)?;
    let context = newapi_remote_key_context(&prepared, outbound, cancellation, correlation_id);
    let output = driver
        .create_remote_key(
            &context,
            CreateRemoteKeyRequest {
                station: prepared.station.clone(),
                endpoints: prepared.endpoints.clone(),
                credential: prepared.credential_handle.clone(),
                name: input.name,
                provider_group_id: None,
                group_name: input.group_name,
                idempotency_key: None,
            },
        )
        .await
        .map_err(remote_key_error_from_driver)?;
    prepare_remote_key_save_from_source(
        source,
        output.remote_key,
        output.full_key_once.into_plaintext(),
        "NewAPI remote key created.".to_string(),
        false,
        prepared.expected_endpoint_revision,
    )
}

pub(crate) async fn prepare_newapi_local_key_from_remote_v2(
    source: &dyn CollectorSourcePort,
    registry: &ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedNewApiRemoteKeyDriverContext,
    remote_key_id: String,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError> {
    let driver = registry
        .remote_key(ProviderKind::NewApi)
        .map_err(remote_key_error_from_driver)?;
    let context = newapi_remote_key_context(&prepared, outbound, cancellation, correlation_id);
    let output = driver
        .reveal_remote_key(
            &context,
            RevealRemoteKeyRequest {
                station: prepared.station.clone(),
                endpoints: prepared.endpoints.clone(),
                credential: prepared.credential_handle.clone(),
                remote_key_id,
            },
        )
        .await
        .map_err(remote_key_error_from_driver)?;
    prepare_remote_key_save_from_source(
        source,
        output.remote_key,
        output.full_key.into_plaintext(),
        "NewAPI remote key synchronized locally.".to_string(),
        false,
        prepared.expected_endpoint_revision,
    )
}

pub(crate) async fn prepare_sub2api_remote_key_scan_v2(
    source: &dyn CollectorSourcePort,
    registry: &ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedSub2ApiRemoteKeyDriverContext,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedRemoteKeyScan, RemoteKeyOperationError> {
    let driver = registry
        .remote_key(ProviderKind::Sub2Api)
        .map_err(remote_key_error_from_driver)?;
    let context = sub2api_remote_key_context(&prepared, outbound, cancellation, correlation_id);
    let output = driver
        .list_remote_keys(
            &context,
            RemoteKeyRequest {
                station: prepared.station.clone(),
                endpoints: prepared.endpoints.clone(),
                credential: prepared.credential_handle.clone(),
            },
        )
        .await
        .map_err(remote_key_error_from_driver)?;
    let (keys, station_key_updates) =
        enrich_remote_key_discoveries_from_source(source, &prepared.station_id, output.keys)
            .map_err(|_| RemoteKeyOperationError::Internal)?;
    ensure_source_endpoint_revision(
        source,
        &prepared.station_id,
        prepared.expected_endpoint_revision,
    )?;
    Ok(PreparedRemoteKeyScan::Discovered {
        station_id: prepared.station_id,
        expected_endpoint_revision: prepared.expected_endpoint_revision,
        capability: prepared.capability,
        keys,
        station_key_updates,
    })
}

pub(crate) async fn prepare_sub2api_remote_key_creation_v2(
    source: &dyn CollectorSourcePort,
    registry: &ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedSub2ApiRemoteKeyDriverContext,
    input: CreateRemoteStationKeyInput,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError> {
    let provider_group_id = remote_group_id_for_create(source, &input)
        .map_err(|_| RemoteKeyOperationError::Internal)?
        .or_else(|| input.group_id_hash.clone());
    let driver = registry
        .remote_key(ProviderKind::Sub2Api)
        .map_err(remote_key_error_from_driver)?;
    let context = sub2api_remote_key_context(&prepared, outbound, cancellation, correlation_id);
    let output = driver
        .create_remote_key(
            &context,
            CreateRemoteKeyRequest {
                station: prepared.station.clone(),
                endpoints: prepared.endpoints.clone(),
                credential: prepared.credential_handle.clone(),
                name: input.name,
                provider_group_id,
                group_name: input.group_name,
                idempotency_key: None,
            },
        )
        .await
        .map_err(remote_key_error_from_driver)?;
    prepare_remote_key_save_from_source(
        source,
        output.remote_key,
        output.full_key_once.into_plaintext(),
        "Sub2API remote key created.".to_string(),
        true,
        prepared.expected_endpoint_revision,
    )
}

pub(crate) async fn prepare_sub2api_local_key_from_remote_v2(
    source: &dyn CollectorSourcePort,
    registry: &ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedSub2ApiRemoteKeyDriverContext,
    remote_key_id: String,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError> {
    let driver = registry
        .remote_key(ProviderKind::Sub2Api)
        .map_err(remote_key_error_from_driver)?;
    let context = sub2api_remote_key_context(&prepared, outbound, cancellation, correlation_id);
    let output = driver
        .reveal_remote_key(
            &context,
            RevealRemoteKeyRequest {
                station: prepared.station.clone(),
                endpoints: prepared.endpoints.clone(),
                credential: prepared.credential_handle.clone(),
                remote_key_id,
            },
        )
        .await
        .map_err(remote_key_error_from_driver)?;
    prepare_remote_key_save_from_source(
        source,
        output.remote_key,
        output.full_key.into_plaintext(),
        "Sub2API remote key synchronized locally.".to_string(),
        false,
        prepared.expected_endpoint_revision,
    )
}

fn newapi_remote_key_context<'a>(
    prepared: &'a PreparedNewApiRemoteKeyDriverContext,
    outbound: &'a AsyncOutboundClient,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> CollectorContext<'a> {
    CollectorContext {
        station: prepared.station.clone(),
        endpoints: prepared.endpoints.clone(),
        credential: prepared.credential_handle.clone(),
        auth: Some(prepared.auth_context.clone()),
        secrets: &prepared.secret_accessor,
        outbound,
        proxy: prepared.proxy.clone(),
        budget: RequestBudget::from_now(std::time::Duration::from_secs(30)),
        cancellation,
        correlation_id: correlation_id
            .or_else(|| correlation::current().map(|id| id.as_str().to_string()))
            .unwrap_or_else(|| "remote-key:newapi".to_string()),
    }
}

fn sub2api_remote_key_context<'a>(
    prepared: &'a PreparedSub2ApiRemoteKeyDriverContext,
    outbound: &'a AsyncOutboundClient,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> CollectorContext<'a> {
    CollectorContext {
        station: prepared.station.clone(),
        endpoints: prepared.endpoints.clone(),
        credential: prepared.credential_handle.clone(),
        auth: Some(prepared.auth_context.clone()),
        secrets: &prepared.secret_accessor,
        outbound,
        proxy: prepared.proxy.clone(),
        budget: RequestBudget::from_now(std::time::Duration::from_secs(30)),
        cancellation,
        correlation_id: correlation_id
            .or_else(|| correlation::current().map(|id| id.as_str().to_string()))
            .unwrap_or_else(|| "remote-key:sub2api".to_string()),
    }
}

fn remote_key_error_from_driver(error: DriverFailure) -> RemoteKeyOperationError {
    match error.kind {
        DriverFailureKind::Unsupported | DriverFailureKind::InvalidRequest => {
            RemoteKeyOperationError::Unsupported
        }
        DriverFailureKind::ResultUnknown => RemoteKeyOperationError::ResultUnknown,
        DriverFailureKind::AuthRejected
        | DriverFailureKind::RateLimited
        | DriverFailureKind::Timeout
        | DriverFailureKind::BudgetExhausted
        | DriverFailureKind::Cancelled
        | DriverFailureKind::Transport
        | DriverFailureKind::MalformedPayload
        | DriverFailureKind::ProviderUnavailable => RemoteKeyOperationError::ExternalUnavailable,
        DriverFailureKind::Internal => RemoteKeyOperationError::Internal,
    }
}

pub(crate) fn prepare_remote_key_scan_v2(
    source: &dyn CollectorSourcePort,
    station_id: String,
) -> Result<PreparedRemoteKeyScan, RemoteKeyOperationError> {
    let (capability, expected_endpoint_revision) =
        remote_key_capability_from_source(source, station_id.clone())?;
    if !capability.can_list_remote_keys {
        return Ok(PreparedRemoteKeyScan::Unsupported {
            station_id,
            capability,
        });
    }
    let discovered = scan_remote_keys_with_source(source, &station_id, &capability.station_type)
        .map_err(|_| RemoteKeyOperationError::ExternalUnavailable)?;
    let (keys, station_key_updates) =
        enrich_remote_key_discoveries_from_source(source, &station_id, discovered)
            .map_err(|_| RemoteKeyOperationError::Internal)?;
    ensure_source_endpoint_revision(source, &station_id, expected_endpoint_revision)?;
    Ok(PreparedRemoteKeyScan::Discovered {
        station_id,
        expected_endpoint_revision,
        capability,
        keys,
        station_key_updates,
    })
}

pub(crate) async fn finish_remote_key_scan_v2(
    credentials: &CredentialService,
    prepared: PreparedRemoteKeyScan,
) -> Result<RemoteKeyScanResult, RemoteKeyOperationError> {
    match prepared {
        PreparedRemoteKeyScan::Unsupported {
            station_id,
            capability,
        } => {
            let keys = credentials
                .list_remote_station_keys(station_id.clone())
                .await
                .map_err(RemoteKeyOperationError::Application)?;
            Ok(RemoteKeyScanResult {
                station_id,
                capability: capability.clone(),
                keys,
                synced_station_key_ids: Vec::new(),
                message: capability
                    .unsupported_reason
                    .clone()
                    .unwrap_or_else(|| "该中转站暂不支持远端 Key 扫描。".to_string()),
            })
        }
        PreparedRemoteKeyScan::Discovered {
            station_id,
            expected_endpoint_revision,
            capability,
            keys,
            station_key_updates,
        } => {
            let keys = credentials
                .replace_remote_station_keys_and_metadata(
                    station_id.clone(),
                    expected_endpoint_revision,
                    keys,
                    station_key_updates,
                )
                .await
                .map_err(RemoteKeyOperationError::Application)?;
            let synced_station_key_ids = keys
                .iter()
                .filter_map(|key| key.matched_station_key_id.clone())
                .collect::<Vec<_>>();
            Ok(RemoteKeyScanResult {
                station_id,
                capability,
                message: format!("远端 Key 扫描完成，已同步 {} 条发现。", keys.len()),
                keys,
                synced_station_key_ids,
            })
        }
    }
}

pub(crate) fn prepare_remote_key_creation_v2(
    source: &dyn CollectorSourcePort,
    input: CreateRemoteStationKeyInput,
) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError> {
    let (capability, expected_endpoint_revision) =
        remote_key_capability_from_source(source, input.station_id.clone())?;
    if !capability.can_create_remote_key {
        return Err(RemoteKeyOperationError::Unsupported);
    }
    let output::CreatedRemoteKey {
        remote_key,
        full_key_once,
        message,
    } = create_remote_key_with_source(source, input, &capability.station_type)
        .map_err(|_| RemoteKeyOperationError::ExternalUnavailable)?;
    let full_key = full_key_once
        .filter(|value| !value.trim().is_empty())
        .ok_or(RemoteKeyOperationError::ExternalUnavailable)?;
    prepare_remote_key_save_from_source(
        source,
        remote_key,
        full_key,
        message,
        capability.station_type != "newapi",
        expected_endpoint_revision,
    )
}

pub(crate) fn prepare_local_key_from_remote_v2(
    source: &dyn CollectorSourcePort,
    station_id: String,
    remote_key_id: String,
) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError> {
    let (capability, expected_endpoint_revision) =
        remote_key_capability_from_source(source, station_id.clone())?;
    if !capability.can_list_remote_keys {
        return Err(RemoteKeyOperationError::Unsupported);
    }
    let (remote_key, full_key) = remote_key_full_secret_with_source(
        source,
        &station_id,
        &remote_key_id,
        &capability.station_type,
    )
    .map_err(|_| RemoteKeyOperationError::ExternalUnavailable)?;
    prepare_remote_key_save_from_source(
        source,
        remote_key,
        full_key,
        "远端 Key 已同步为本地 Station Key。".to_string(),
        false,
        expected_endpoint_revision,
    )
}

pub(crate) async fn finish_remote_key_creation_v2(
    credentials: &CredentialService,
    prepared: PreparedRemoteKeySave,
) -> Result<CreateRemoteStationKeyResult, RemoteKeyOperationError> {
    let PreparedRemoteKeySave {
        remote_key,
        expected_endpoint_revision,
        matched_station_key_update,
        new_group_binding_id,
        full_key,
        adapter_message,
        expose_full_key_once,
        matched_existing,
    } = prepared;
    let response_key = expose_full_key_once.then(|| full_key.clone());
    let (remote_key, station_key) = credentials
        .save_remote_station_key_with_local(
            remote_key,
            expected_endpoint_revision,
            matched_station_key_update,
            new_group_binding_id,
            full_key,
        )
        .await
        .map_err(RemoteKeyOperationError::Application)?;
    Ok(CreateRemoteStationKeyResult {
        remote_key,
        station_key,
        full_key_once: response_key,
        message: remote_key_save_message(&adapter_message, matched_existing),
    })
}

pub(crate) async fn finish_local_key_from_remote_v2(
    credentials: &CredentialService,
    prepared: PreparedRemoteKeySave,
) -> Result<CreateLocalStationKeyFromRemoteResult, RemoteKeyOperationError> {
    let result = finish_remote_key_creation_v2(credentials, prepared).await?;
    Ok(CreateLocalStationKeyFromRemoteResult {
        remote_key: result.remote_key,
        station_key: result.station_key,
        message: "远端 Key 已保存为本地 Station Key。".to_string(),
    })
}

fn remote_key_capability_from_source(
    source: &dyn CollectorSourcePort,
    station_id: String,
) -> Result<(RemoteKeyCapability, i64), RemoteKeyOperationError> {
    let station = source
        .station_for_collector(&station_id)
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    let endpoint_revision = station.endpoint_revision;
    let station_type = station.station_type.trim().to_string();
    let capability = match station_type.as_str() {
        "sub2api" => adapters::sub2api::remote_key_capability(&station),
        "newapi" => Ok(RemoteKeyCapability {
            station_id,
            station_type: station_type.clone(),
            can_list_remote_keys: true,
            can_create_remote_key: true,
            can_read_groups: true,
            requires_manual_session: true,
            unsupported_reason: None,
        }),
        _ => Ok(RemoteKeyCapability {
            station_id,
            station_type: station_type.clone(),
            can_list_remote_keys: false,
            can_create_remote_key: false,
            can_read_groups: false,
            requires_manual_session: false,
            unsupported_reason: Some(format!(
                "暂不支持 {station_type} 类型中转站的远端 Key 管理。"
            )),
        }),
    }
    .map_err(|_| RemoteKeyOperationError::Internal)?;
    Ok((capability, endpoint_revision))
}

fn ensure_source_endpoint_revision(
    source: &dyn CollectorSourcePort,
    station_id: &str,
    expected_endpoint_revision: i64,
) -> Result<(), RemoteKeyOperationError> {
    let current = source
        .station_for_collector(station_id)
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    if current.endpoint_revision != expected_endpoint_revision {
        return Err(RemoteKeyOperationError::Conflict);
    }
    Ok(())
}

fn proxy_policy_from_remote_key_config(
    proxy: crate::services::outbound::ProxyConfig,
) -> Result<ProxyPolicy, String> {
    match proxy.mode.as_str() {
        "direct" => Ok(ProxyPolicy::Direct),
        "system" => Ok(ProxyPolicy::System),
        "manual" => {
            let Some(url) = proxy.url.as_deref() else {
                return Err("manual collector proxy URL is required".to_string());
            };
            ManualProxy::parse(url)
                .map(ProxyPolicy::Manual)
                .map_err(|error| crate::services::secrets::mask::redact_text(&error.to_string()))
        }
        _ => Err("unsupported collector proxy mode".to_string()),
    }
}

fn scan_remote_keys_with_source(
    _source: &dyn CollectorSourcePort,
    _station_id: &str,
    station_type: &str,
) -> Result<Vec<RemoteStationKey>, String> {
    match station_type {
        "sub2api" => {
            Err("Sub2API remote-key scan must use the async capability driver".to_string())
        }
        "newapi" => Err("NewAPI remote-key scan must use the async capability driver".to_string()),
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站的远端 Key 扫描。"
        )),
    }
}

fn create_remote_key_with_source(
    _source: &dyn CollectorSourcePort,
    input: CreateRemoteStationKeyInput,
    station_type: &str,
) -> Result<output::CreatedRemoteKey, String> {
    match station_type {
        "sub2api" => {
            let _ = input;
            Err("Sub2API remote-key create must use the async capability driver".to_string())
        }
        "newapi" => {
            let _ = input;
            Err("NewAPI remote-key create must use the async capability driver".to_string())
        }
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站的远端 Key 创建。"
        )),
    }
}

fn remote_key_full_secret_with_source(
    _source: &dyn CollectorSourcePort,
    station_id: &str,
    remote_key_id: &str,
    station_type: &str,
) -> Result<(RemoteStationKey, String), String> {
    match station_type {
        "sub2api" => {
            let _ = (station_id, remote_key_id);
            Err("Sub2API remote-key reveal must use the async capability driver".to_string())
        }
        "newapi" => {
            Err("NewAPI remote-key reveal must use the async capability driver".to_string())
        }
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站从远端发现同步本地 Key。"
        )),
    }
}

fn prepare_remote_key_save_from_source(
    source: &dyn CollectorSourcePort,
    remote_key: RemoteStationKey,
    full_key: String,
    adapter_message: String,
    expose_full_key_once: bool,
    expected_endpoint_revision: i64,
) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError> {
    if full_key.trim().is_empty() {
        return Err(RemoteKeyOperationError::ExternalUnavailable);
    }
    let station_id = remote_key.station_id.clone();
    let bindings = source
        .list_station_group_bindings(station_id.clone())
        .map_err(|_| RemoteKeyOperationError::Internal)?;
    let (mut remote_keys, mut station_key_updates) =
        enrich_remote_key_discoveries_from_parts(source, &station_id, &bindings, vec![remote_key])
            .map_err(|_| RemoteKeyOperationError::Internal)?;
    let remote_key = remote_keys
        .pop()
        .ok_or(RemoteKeyOperationError::ExternalUnavailable)?;
    let matched_station_key_update = station_key_updates.pop();
    let matched_existing = matched_station_key_update.is_some();
    let new_group_binding_id = (!matched_existing)
        .then(|| matching_group_binding(&remote_key, &bindings))
        .flatten()
        .map(|binding| binding.id.clone())
        .filter(|id| !id.trim().is_empty());
    ensure_source_endpoint_revision(source, &station_id, expected_endpoint_revision)?;
    Ok(PreparedRemoteKeySave {
        remote_key,
        expected_endpoint_revision,
        matched_station_key_update,
        new_group_binding_id,
        full_key,
        adapter_message,
        expose_full_key_once,
        matched_existing,
    })
}

fn enrich_remote_key_discoveries_from_source(
    source: &dyn CollectorSourcePort,
    station_id: &str,
    keys: Vec<RemoteStationKey>,
) -> Result<(Vec<RemoteStationKey>, Vec<UpdateStationKeyInput>), String> {
    let bindings = source.list_station_group_bindings(station_id.to_string())?;
    enrich_remote_key_discoveries_from_parts(source, station_id, &bindings, keys)
}

fn enrich_remote_key_discoveries_from_parts(
    source: &dyn CollectorSourcePort,
    station_id: &str,
    bindings: &[StationGroupBinding],
    keys: Vec<RemoteStationKey>,
) -> Result<(Vec<RemoteStationKey>, Vec<UpdateStationKeyInput>), String> {
    let local_candidates = local_station_key_candidates_from_source(source, station_id)?;
    let mut updates = BTreeMap::<String, (f64, UpdateStationKeyInput)>::new();
    let mut enriched = Vec::with_capacity(keys.len());
    for mut key in keys {
        apply_group_metadata(&mut key, bindings, &[]);
        if let Some((local_key, confidence)) = best_local_key_match(&key, &local_candidates) {
            if confidence >= 0.8 {
                key.matched_station_key_id = Some(local_key.key.id.clone());
                key.match_confidence = confidence;
                key.match_status = if confidence >= 0.9 {
                    crate::models::remote_keys::RemoteKeyMatchStatus::Matched
                } else {
                    crate::models::remote_keys::RemoteKeyMatchStatus::Possible
                };
                let update = station_key_metadata_update(&local_key.key, &key, bindings);
                let replace = updates
                    .get(&local_key.key.id)
                    .map(|(current, _)| confidence > *current)
                    .unwrap_or(true);
                if replace {
                    updates.insert(local_key.key.id.clone(), (confidence, update));
                }
            }
        }
        enriched.push(key);
    }
    Ok((
        enriched,
        updates.into_values().map(|(_, update)| update).collect(),
    ))
}

fn local_station_key_candidates_from_source(
    source: &dyn CollectorSourcePort,
    station_id: &str,
) -> Result<Vec<LocalStationKeyCandidate>, String> {
    let keys = source.list_station_keys(station_id.to_string())?;
    Ok(keys
        .into_iter()
        .map(|key| {
            let full_key = if key.api_key_present {
                source
                    .resolve_station_key_secret_with_data_key(&V2_UNUSED_DATA_KEY, &key.id)
                    .ok()
            } else {
                None
            };
            let fingerprint = full_key.as_deref().and_then(api_key_fingerprint);
            LocalStationKeyCandidate {
                key,
                full_key,
                fingerprint,
            }
        })
        .collect())
}

fn station_key_metadata_update(
    local_key: &StationKey,
    remote_key: &RemoteStationKey,
    bindings: &[StationGroupBinding],
) -> UpdateStationKeyInput {
    let group_binding_id = matching_group_binding(remote_key, bindings)
        .map(|binding| binding.id.clone())
        .or_else(|| local_key.group_binding_id.clone());
    UpdateStationKeyInput {
        id: local_key.id.clone(),
        station_id: local_key.station_id.clone(),
        name: local_key.name.clone(),
        api_key: None,
        enabled: local_key.enabled,
        priority: local_key.priority,
        max_concurrency: local_key.max_concurrency,
        load_factor: local_key.load_factor,
        schedulable: local_key.schedulable,
        group_name: remote_key
            .group_name
            .clone()
            .or_else(|| local_key.group_name.clone()),
        tier_label: remote_key
            .tier_label
            .clone()
            .or_else(|| local_key.tier_label.clone()),
        group_binding_id,
        group_id_hash: remote_key
            .group_id_hash
            .clone()
            .or_else(|| local_key.group_id_hash.clone()),
        rate_multiplier: remote_key.rate_multiplier.or(local_key.rate_multiplier),
        manual_rate_multiplier: None,
        rate_source: remote_key
            .rate_source
            .clone()
            .or_else(|| local_key.rate_source.clone()),
        balance_scope: local_key.balance_scope.clone(),
        status: local_key.status.clone(),
        note: local_key.note.clone(),
    }
}

fn remote_key_save_message(adapter_message: &str, matched_existing: bool) -> String {
    match (adapter_message.trim().is_empty(), matched_existing) {
        (true, true) => "远端 Key 已创建，并已关联到已有本地 Station Key。".to_string(),
        (false, true) => format!("{adapter_message} 已关联到已有本地 Station Key。"),
        (true, false) => "远端 Key 已创建，并已保存为启用的本地 Station Key。".to_string(),
        (false, false) => format!("{adapter_message} 已保存为启用的本地 Station Key。"),
    }
}

#[derive(Debug, Clone)]
struct LocalStationKeyCandidate {
    key: StationKey,
    full_key: Option<String>,
    fingerprint: Option<String>,
}

fn best_local_key_match<'a>(
    remote_key: &RemoteStationKey,
    local_candidates: &'a [LocalStationKeyCandidate],
) -> Option<(&'a LocalStationKeyCandidate, f64)> {
    local_candidates
        .iter()
        .map(|candidate| {
            let same_group = remote_key
                .group_id_hash
                .as_deref()
                .zip(candidate.key.group_id_hash.as_deref())
                .map(|(remote, local)| remote == local)
                .unwrap_or(false)
                || names_match(
                    remote_key.group_name.as_deref(),
                    candidate.key.group_name.as_deref(),
                );
            let same_name = names_match(
                remote_key.remote_key_name.as_deref(),
                Some(candidate.key.name.as_str()),
            );
            let confidence = remote_key_confidence(
                remote_key.api_key_fingerprint.as_deref(),
                candidate.fingerprint.as_deref(),
                remote_key.api_key_masked.as_deref(),
                candidate.full_key.as_deref(),
                same_group,
                same_name,
            );
            (candidate, confidence)
        })
        .max_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|(_, confidence)| *confidence > 0.0)
}

fn apply_group_metadata(
    remote_key: &mut RemoteStationKey,
    group_bindings: &[StationGroupBinding],
    group_rates: &[GroupRateRecord],
) {
    let Some(binding) = matching_group_binding(remote_key, group_bindings) else {
        return;
    };
    let latest_rate = latest_group_rate(binding, group_rates);

    remote_key.group_id_hash = Some(binding.group_key_hash.clone());
    remote_key.group_name = Some(binding.group_name.clone());
    if remote_key.rate_multiplier.is_none() {
        remote_key.rate_multiplier = latest_rate
            .and_then(effective_rate_from_record)
            .or_else(|| effective_rate_from_binding(binding));
    }
    if remote_key.rate_source.as_deref() == Some("sub2api_keys")
        || remote_key
            .rate_source
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        remote_key.rate_source = latest_rate
            .map(|rate| rate.source.clone())
            .or_else(|| binding.rate_source.clone())
            .or_else(|| Some("station_group_binding".to_string()));
    }
}

fn matching_group_binding<'a>(
    remote_key: &RemoteStationKey,
    group_bindings: &'a [StationGroupBinding],
) -> Option<&'a StationGroupBinding> {
    group_bindings
        .iter()
        .filter(|binding| {
            binding.binding_kind == "station_group" && binding.binding_status != "disabled"
        })
        .find(|binding| {
            remote_key
                .group_id_hash
                .as_deref()
                .map(|remote_group| {
                    remote_group == binding.group_key_hash
                        || binding.group_id_hash.as_deref() == Some(remote_group)
                })
                .unwrap_or(false)
                || names_match(
                    remote_key.group_name.as_deref(),
                    Some(binding.group_name.as_str()),
                )
        })
}

fn remote_group_id_for_create(
    source: &dyn CollectorSourcePort,
    input: &CreateRemoteStationKeyInput,
) -> Result<Option<String>, String> {
    let Some(group_binding_id) = input
        .group_binding_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let bindings = source.list_station_group_bindings(input.station_id.clone())?;
    Ok(bindings
        .into_iter()
        .find(|binding| {
            binding.id == group_binding_id
                && binding.binding_kind == "station_group"
                && binding.binding_status != "disabled"
        })
        .and_then(|binding| binding.group_id_hash)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

fn latest_group_rate<'a>(
    binding: &StationGroupBinding,
    group_rates: &'a [GroupRateRecord],
) -> Option<&'a GroupRateRecord> {
    group_rates.iter().find(|rate| {
        rate.binding_kind == "station_group"
            && (rate.group_binding_id.as_deref() == Some(binding.id.as_str())
                || rate.group_key_hash == binding.group_key_hash
                || normalized_text(&rate.group_name) == normalized_text(&binding.group_name))
    })
}

fn effective_rate_from_binding(binding: &StationGroupBinding) -> Option<f64> {
    binding
        .user_rate_multiplier
        .or(binding.effective_rate_multiplier)
        .or(binding.default_rate_multiplier)
}

fn effective_rate_from_record(record: &GroupRateRecord) -> Option<f64> {
    record
        .user_rate_multiplier
        .or(record.effective_rate_multiplier)
        .or(record.default_rate_multiplier)
}

fn names_match(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => normalized_text(left) == normalized_text(right),
        _ => false,
    }
}

fn normalized_text(value: &str) -> String {
    value.trim().to_lowercase()
}

pub fn api_key_fingerprint(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    Some(format!("{:x}", hasher.finalize()))
}

pub fn visible_mask_parts(masked: &str) -> Option<(String, String)> {
    let trimmed = masked.trim();
    let (prefix, suffix) = trimmed
        .split_once("****")
        .or_else(|| trimmed.split_once("..."))?;
    let prefix = prefix.trim().to_string();
    let suffix = suffix.trim().to_string();
    if prefix.len() < 3 || suffix.len() < 3 {
        return None;
    }
    Some((prefix, suffix))
}

pub fn masked_key_matches_full(masked: &str, full_key: &str) -> bool {
    visible_mask_parts(masked)
        .map(|(prefix, suffix)| full_key.starts_with(&prefix) && full_key.ends_with(&suffix))
        .unwrap_or(false)
}

pub fn remote_key_confidence(
    remote_fingerprint: Option<&str>,
    local_fingerprint: Option<&str>,
    remote_masked: Option<&str>,
    local_full_key: Option<&str>,
    same_group: bool,
    same_name: bool,
) -> f64 {
    if let Some(remote_fingerprint) = remote_fingerprint {
        return if Some(remote_fingerprint) == local_fingerprint {
            1.0
        } else {
            0.0
        };
    }
    if let (Some(masked), Some(full_key)) = (remote_masked, local_full_key) {
        if masked_key_matches_full(masked, full_key) {
            return if same_group || same_name { 0.92 } else { 0.82 };
        }
    }
    match (same_group, same_name) {
        (true, true) => 0.72,
        (true, false) | (false, true) => 0.55,
        (false, false) => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_are_stable_without_exposing_secrets() {
        assert_eq!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-a"));
        assert_ne!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-b"));
        assert_eq!(api_key_fingerprint("   "), None);
    }

    #[test]
    fn masked_key_matching_requires_meaningful_visible_parts() {
        assert!(masked_key_matches_full(
            "sk-live****cdef",
            "sk-live-123-cdef",
        ));
        assert!(!masked_key_matches_full("sk****ef", "sk-live-123-cdef"));
        assert!(!masked_key_matches_full("not-masked", "sk-live-123-cdef"));
    }

    #[test]
    fn confidence_never_accepts_name_and_group_only_as_a_secret_match() {
        assert!(remote_key_confidence(None, None, None, None, true, true) < 0.8);
        let fingerprint = api_key_fingerprint("sk-live-123-cdef");
        assert_eq!(
            remote_key_confidence(
                fingerprint.as_deref(),
                fingerprint.as_deref(),
                None,
                None,
                false,
                false,
            ),
            1.0,
        );
    }
}
