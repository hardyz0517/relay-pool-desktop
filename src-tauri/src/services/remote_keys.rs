use std::collections::{BTreeMap, BTreeSet};

use tokio_util::sync::CancellationToken;

use crate::{
    application::error::ApplicationError,
    models::{
        group_facts::{GroupRateRecord, StationGroupBinding},
        remote_keys::{
            api_key_fingerprint, CreateLocalStationKeyFromRemoteResult,
            CreateRemoteStationKeyInput, CreateRemoteStationKeyResult,
            DeleteRemoteStationKeyResult, RemoteKeyCapability, RemoteKeyScanResult,
            RemoteStationKey,
        },
        station_keys::{StationKey, UpdateStationKeyInput},
    },
    observability::correlation,
    outbound::{AsyncOutboundClient, ManualProxy, ProxyPolicy, RequestBudget},
    services::collectors::{
        contract::{
            CollectorContext, CreateRemoteKeyRequest, CredentialScope, CredentialSecret,
            CredentialSecretPurpose, DeleteRemoteKeyRequest, DriverSecretAccessor,
            OpaqueCredentialHandle, ProviderAuthContext, ProviderEndpoints, ProviderKind,
            RemoteKeyRequest, RevealRemoteKeyRequest, StationIdentity, Sub2ApiLoginCredential,
        },
        failure::{DriverFailure, DriverFailureKind},
        orchestration::ProviderRegistry,
        CollectorSourcePort,
    },
};

#[derive(Debug)]
pub(crate) enum RemoteKeyOperationError {
    Application(ApplicationError),
    Unsupported,
    UnsupportedWithDetail(String),
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

pub(crate) trait RemoteKeyPersistencePort: Send + Sync {
    fn list_remote_station_keys<'a>(
        &'a self,
        station_id: String,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<RemoteStationKey>, ApplicationError>>;

    fn replace_remote_station_keys_and_metadata<'a>(
        &'a self,
        station_id: String,
        expected_endpoint_revision: i64,
        remote_keys: Vec<RemoteStationKey>,
        station_key_updates: Vec<UpdateStationKeyInput>,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<RemoteStationKey>, ApplicationError>>;

    fn save_remote_station_key_with_local<'a>(
        &'a self,
        remote_key: RemoteStationKey,
        expected_endpoint_revision: i64,
        matched_station_key_update: Option<UpdateStationKeyInput>,
        new_group_binding_id: Option<String>,
        full_key: String,
    ) -> futures_util::future::BoxFuture<'a, Result<(RemoteStationKey, StationKey), ApplicationError>>;
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

pub(crate) struct PreparedRemoteKeyDelete {
    station_id: String,
    expected_endpoint_revision: i64,
    remote_key_id: String,
    matched_station_key_id: Option<String>,
    already_absent: bool,
    keys: Vec<RemoteStationKey>,
    station_key_updates: Vec<UpdateStationKeyInput>,
}

struct PreparedRemoteKeyLocalState {
    group_bindings: Vec<StationGroupBinding>,
    local_key_candidates: Vec<LocalStationKeyCandidate>,
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
    local_state: PreparedRemoteKeyLocalState,
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
    user_agent: Option<String>,
    secret_accessor: RemoteKeySecretAccessor,
    proxy: ProxyPolicy,
    local_state: PreparedRemoteKeyLocalState,
}

pub(crate) fn prepare_newapi_remote_key_driver_context_v2(
    source: &dyn CollectorSourcePort,
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
        can_delete_remote_keys: true,
        can_read_groups: true,
        requires_manual_session: true,
        unsupported_reason: None,
    };
    let session = source
        .resolve_station_session(
            station.id.clone(),
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
    let local_state = prepare_remote_key_local_state(source, &station.id)
        .map_err(|_| RemoteKeyOperationError::Internal)?;
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
        local_state,
    }))
}

pub(crate) fn prepare_sub2api_remote_key_driver_context_v2(
    source: &dyn CollectorSourcePort,
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
        can_delete_remote_keys: true,
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
        .resolve_station_session(
            station.id.clone(),
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
                purpose: CredentialSecretPurpose::AuthorizationHeader,
                secret: token,
            });
            login_session_handle.clone()
        });
    let session_cookie = session
        .cookie
        .clone()
        .filter(|value| !value.trim().is_empty())
        .map(|cookie| {
            records.push(RemoteKeySecretRecord {
                handle: login_session_handle.clone(),
                purpose: CredentialSecretPurpose::SessionCookie,
                secret: cookie,
            });
            login_session_handle.clone()
        });
    let access_token = access_token.or_else(|| session_cookie.clone());

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
                .get_station_login_password(station.id.clone())
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
    let local_state = prepare_remote_key_local_state(source, &station.id)
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
            session_cookie,
            login,
            credit_per_cny: station.credit_per_cny,
        },
        user_agent: credentials.session_user_agent,
        secret_accessor: RemoteKeySecretAccessor { records },
        proxy,
        local_state,
    }))
}

pub(crate) async fn prepare_newapi_remote_key_scan_v2(
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
    let (keys, station_key_updates) = enrich_remote_key_discoveries_from_parts(
        &prepared.local_state.group_bindings,
        &prepared.local_state.local_key_candidates,
        output.keys,
    );
    Ok(PreparedRemoteKeyScan::Discovered {
        station_id: prepared.station_id,
        expected_endpoint_revision: prepared.expected_endpoint_revision,
        capability: prepared.capability,
        keys,
        station_key_updates,
    })
}

pub(crate) async fn prepare_newapi_remote_key_creation_v2(
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
    prepare_remote_key_save(
        &prepared.local_state,
        output.remote_key,
        output.full_key_once.into_plaintext(),
        "NewAPI remote key created.".to_string(),
        false,
        prepared.expected_endpoint_revision,
    )
}

pub(crate) async fn prepare_newapi_local_key_from_remote_v2(
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
    prepare_remote_key_save(
        &prepared.local_state,
        output.remote_key,
        output.full_key.into_plaintext(),
        "NewAPI remote key synchronized locally.".to_string(),
        false,
        prepared.expected_endpoint_revision,
    )
}

pub(crate) async fn prepare_newapi_remote_key_deletion_v2(
    registry: &ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedNewApiRemoteKeyDriverContext,
    remote_key_id: String,
    matched_station_key_id: Option<String>,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedRemoteKeyDelete, RemoteKeyOperationError> {
    let driver = registry
        .remote_key(ProviderKind::NewApi)
        .map_err(remote_key_error_from_driver)?;
    let context = newapi_remote_key_context(&prepared, outbound, cancellation, correlation_id);
    let output = driver
        .delete_remote_key(
            &context,
            DeleteRemoteKeyRequest {
                station: prepared.station.clone(),
                endpoints: prepared.endpoints.clone(),
                credential: prepared.credential_handle.clone(),
                remote_key_id: remote_key_id.clone(),
            },
        )
        .await
        .map_err(remote_key_error_from_driver)?;
    let (keys, station_key_updates) = enrich_remote_key_discoveries_from_parts(
        &prepared.local_state.group_bindings,
        &prepared.local_state.local_key_candidates,
        output.keys,
    );
    Ok(PreparedRemoteKeyDelete {
        station_id: prepared.station_id,
        expected_endpoint_revision: prepared.expected_endpoint_revision,
        remote_key_id,
        matched_station_key_id,
        already_absent: output.already_absent,
        keys,
        station_key_updates,
    })
}

pub(crate) async fn prepare_sub2api_remote_key_scan_v2(
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
    let (keys, station_key_updates) = enrich_remote_key_discoveries_from_parts(
        &prepared.local_state.group_bindings,
        &prepared.local_state.local_key_candidates,
        output.keys,
    );
    Ok(PreparedRemoteKeyScan::Discovered {
        station_id: prepared.station_id,
        expected_endpoint_revision: prepared.expected_endpoint_revision,
        capability: prepared.capability,
        keys,
        station_key_updates,
    })
}

pub(crate) async fn prepare_sub2api_remote_key_creation_v2(
    registry: &ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedSub2ApiRemoteKeyDriverContext,
    input: CreateRemoteStationKeyInput,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError> {
    let provider_group_id =
        remote_group_id_for_create(&prepared.local_state.group_bindings, &input)
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
    prepare_remote_key_save(
        &prepared.local_state,
        output.remote_key,
        output.full_key_once.into_plaintext(),
        "Sub2API remote key created.".to_string(),
        true,
        prepared.expected_endpoint_revision,
    )
}

pub(crate) async fn prepare_sub2api_local_key_from_remote_v2(
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
    prepare_remote_key_save(
        &prepared.local_state,
        output.remote_key,
        output.full_key.into_plaintext(),
        "Sub2API remote key synchronized locally.".to_string(),
        false,
        prepared.expected_endpoint_revision,
    )
}

pub(crate) async fn prepare_sub2api_remote_key_deletion_v2(
    registry: &ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedSub2ApiRemoteKeyDriverContext,
    remote_key_id: String,
    matched_station_key_id: Option<String>,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedRemoteKeyDelete, RemoteKeyOperationError> {
    let driver = registry
        .remote_key(ProviderKind::Sub2Api)
        .map_err(remote_key_error_from_driver)?;
    let context = sub2api_remote_key_context(&prepared, outbound, cancellation, correlation_id);
    let output = driver
        .delete_remote_key(
            &context,
            DeleteRemoteKeyRequest {
                station: prepared.station.clone(),
                endpoints: prepared.endpoints.clone(),
                credential: prepared.credential_handle.clone(),
                remote_key_id: remote_key_id.clone(),
            },
        )
        .await
        .map_err(remote_key_error_from_driver)?;
    let (keys, station_key_updates) = enrich_remote_key_discoveries_from_parts(
        &prepared.local_state.group_bindings,
        &prepared.local_state.local_key_candidates,
        output.keys,
    );
    Ok(PreparedRemoteKeyDelete {
        station_id: prepared.station_id,
        expected_endpoint_revision: prepared.expected_endpoint_revision,
        remote_key_id,
        matched_station_key_id,
        already_absent: output.already_absent,
        keys,
        station_key_updates,
    })
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
        user_agent: None,
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
        user_agent: prepared.user_agent.clone(),
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
        DriverFailureKind::Unsupported | DriverFailureKind::InvalidRequest => error
            .sanitized_detail
            .filter(|detail| !detail.trim().is_empty())
            .map(RemoteKeyOperationError::UnsupportedWithDetail)
            .unwrap_or(RemoteKeyOperationError::Unsupported),
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

pub(crate) fn prepare_unsupported_remote_key_scan_v2(
    source: &dyn CollectorSourcePort,
    station_id: String,
) -> Result<PreparedRemoteKeyScan, RemoteKeyOperationError> {
    let (capability, expected_endpoint_revision) =
        remote_key_capability_from_source(source, station_id.clone())?;
    if capability.can_list_remote_keys {
        return Err(RemoteKeyOperationError::Unsupported);
    }
    ensure_source_endpoint_revision(source, &station_id, expected_endpoint_revision)?;
    Ok(PreparedRemoteKeyScan::Unsupported {
        station_id,
        capability,
    })
}

pub(crate) async fn finish_remote_key_scan_v2(
    credentials: &dyn RemoteKeyPersistencePort,
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
            let verified_count = keys
                .iter()
                .filter(|key| key.api_key_fingerprint.is_some())
                .count();
            Ok(RemoteKeyScanResult {
                station_id,
                capability,
                message: format!(
                    "远端 Key 扫描完成：校验 {verified_count}/{} 条，识别 {} 个本地匹配。",
                    keys.len(),
                    synced_station_key_ids.len()
                ),
                keys,
                synced_station_key_ids,
            })
        }
    }
}

pub(crate) fn preview_remote_key_scan_v2(
    prepared: PreparedRemoteKeyScan,
) -> Result<RemoteKeyScanResult, RemoteKeyOperationError> {
    match prepared {
        PreparedRemoteKeyScan::Unsupported {
            station_id,
            capability,
        } => Ok(RemoteKeyScanResult {
            station_id,
            capability: capability.clone(),
            keys: Vec::new(),
            synced_station_key_ids: Vec::new(),
            message: capability.unsupported_reason.clone().unwrap_or_else(|| {
                "This provider does not support remote key scanning.".to_string()
            }),
        }),
        PreparedRemoteKeyScan::Discovered {
            station_id,
            capability,
            keys,
            ..
        } => Ok(RemoteKeyScanResult {
            station_id,
            capability,
            message: format!(
                "Remote key scan completed with {} read-only results.",
                keys.len()
            ),
            keys,
            synced_station_key_ids: Vec::new(),
        }),
    }
}

pub(crate) async fn finish_remote_key_creation_v2(
    credentials: &dyn RemoteKeyPersistencePort,
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
    credentials: &dyn RemoteKeyPersistencePort,
    prepared: PreparedRemoteKeySave,
) -> Result<CreateLocalStationKeyFromRemoteResult, RemoteKeyOperationError> {
    let result = finish_remote_key_creation_v2(credentials, prepared).await?;
    Ok(CreateLocalStationKeyFromRemoteResult {
        remote_key: result.remote_key,
        station_key: result.station_key,
        message: "远端 Key 已保存为本地 Station Key。".to_string(),
    })
}

pub(crate) async fn finish_remote_key_deletion_v2(
    credentials: &dyn RemoteKeyPersistencePort,
    prepared: PreparedRemoteKeyDelete,
) -> Result<DeleteRemoteStationKeyResult, RemoteKeyOperationError> {
    let keys = credentials
        .replace_remote_station_keys_and_metadata(
            prepared.station_id.clone(),
            prepared.expected_endpoint_revision,
            prepared.keys,
            prepared.station_key_updates,
        )
        .await
        .map_err(RemoteKeyOperationError::Application)?;
    let message = if prepared.already_absent {
        "远端 Key 已不存在，本地发现记录已完成对账。"
    } else if prepared.matched_station_key_id.is_some() {
        "远端 Key 已删除，关联的本地 Station Key 已保留。"
    } else {
        "远端 Key 已删除。"
    };
    Ok(DeleteRemoteStationKeyResult {
        station_id: prepared.station_id,
        remote_key_id: prepared.remote_key_id,
        already_absent: prepared.already_absent,
        matched_station_key_id: prepared.matched_station_key_id,
        keys,
        message: message.to_string(),
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
        "sub2api" => Ok::<RemoteKeyCapability, String>(RemoteKeyCapability {
            station_id: station.id.clone(),
            station_type: station.station_type.trim().to_string(),
            can_list_remote_keys: true,
            can_create_remote_key: true,
            can_delete_remote_keys: true,
            can_read_groups: true,
            requires_manual_session: true,
            unsupported_reason: None,
        }),
        "newapi" => Ok(RemoteKeyCapability {
            station_id,
            station_type: station_type.clone(),
            can_list_remote_keys: true,
            can_create_remote_key: true,
            can_delete_remote_keys: true,
            can_read_groups: true,
            requires_manual_session: true,
            unsupported_reason: None,
        }),
        _ => Ok(RemoteKeyCapability {
            station_id,
            station_type: station_type.clone(),
            can_list_remote_keys: false,
            can_create_remote_key: false,
            can_delete_remote_keys: false,
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

fn prepare_remote_key_save(
    local_state: &PreparedRemoteKeyLocalState,
    remote_key: RemoteStationKey,
    full_key: String,
    adapter_message: String,
    expose_full_key_once: bool,
    expected_endpoint_revision: i64,
) -> Result<PreparedRemoteKeySave, RemoteKeyOperationError> {
    if full_key.trim().is_empty() {
        return Err(RemoteKeyOperationError::ExternalUnavailable);
    }
    let (mut remote_keys, mut station_key_updates) = enrich_remote_key_discoveries_from_parts(
        &local_state.group_bindings,
        &local_state.local_key_candidates,
        vec![remote_key],
    );
    let remote_key = remote_keys
        .pop()
        .ok_or(RemoteKeyOperationError::ExternalUnavailable)?;
    let matched_station_key_update = station_key_updates.pop();
    let matched_existing = matched_station_key_update.is_some();
    let new_group_binding_id = (!matched_existing)
        .then(|| matching_group_binding(&remote_key, &local_state.group_bindings))
        .flatten()
        .map(|binding| binding.id.clone())
        .filter(|id| !id.trim().is_empty());
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

fn prepare_remote_key_local_state(
    source: &dyn CollectorSourcePort,
    station_id: &str,
) -> Result<PreparedRemoteKeyLocalState, String> {
    Ok(PreparedRemoteKeyLocalState {
        group_bindings: source.list_station_group_bindings(station_id.to_string())?,
        local_key_candidates: local_station_key_candidates_from_source(source, station_id)?,
    })
}

fn enrich_remote_key_discoveries_from_parts(
    bindings: &[StationGroupBinding],
    local_candidates: &[LocalStationKeyCandidate],
    keys: Vec<RemoteStationKey>,
) -> (Vec<RemoteStationKey>, Vec<UpdateStationKeyInput>) {
    let mut updates = BTreeMap::<String, (f64, UpdateStationKeyInput)>::new();
    let mut enriched = keys;
    for key in &mut enriched {
        apply_group_metadata(key, bindings, &[]);
        key.matched_station_key_id = None;
        key.match_confidence = 0.0;
        key.match_status = crate::models::remote_keys::RemoteKeyMatchStatus::Unbound;
    }

    let mut candidates = enriched
        .iter()
        .enumerate()
        .flat_map(|(remote_index, remote_key)| {
            local_candidates
                .iter()
                .enumerate()
                .filter_map(move |(local_index, local_key)| {
                    let confidence = local_key_match_confidence(remote_key, local_key);
                    (confidence >= 0.8).then_some((remote_index, local_index, confidence))
                })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .2
            .partial_cmp(&left.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| enriched[left.0].id.cmp(&enriched[right.0].id))
            .then_with(|| {
                local_candidates[left.1]
                    .key
                    .id
                    .cmp(&local_candidates[right.1].key.id)
            })
    });

    let mut assigned_remote_keys = BTreeSet::new();
    let mut assigned_local_keys = BTreeSet::new();
    for (remote_index, local_index, confidence) in candidates {
        if !assigned_remote_keys.insert(remote_index) || !assigned_local_keys.insert(local_index) {
            continue;
        }
        let remote_key = &mut enriched[remote_index];
        let local_key = &local_candidates[local_index];
        remote_key.matched_station_key_id = Some(local_key.key.id.clone());
        remote_key.match_confidence = confidence;
        remote_key.match_status = if confidence >= 0.9 {
            crate::models::remote_keys::RemoteKeyMatchStatus::Matched
        } else {
            crate::models::remote_keys::RemoteKeyMatchStatus::Possible
        };
        updates.insert(
            local_key.key.id.clone(),
            (
                confidence,
                station_key_metadata_update(&local_key.key, remote_key, bindings),
            ),
        );
    }
    (
        enriched,
        updates.into_values().map(|(_, update)| update).collect(),
    )
}

fn local_station_key_candidates_from_source(
    source: &dyn CollectorSourcePort,
    station_id: &str,
) -> Result<Vec<LocalStationKeyCandidate>, String> {
    let keys = source.list_station_keys(station_id.to_string())?;
    Ok(keys
        .into_iter()
        .map(|key| {
            let fingerprint = if key.api_key_present {
                source
                    .resolve_station_key_secret(&key.id)
                    .ok()
                    .as_deref()
                    .and_then(api_key_fingerprint)
            } else {
                None
            };
            LocalStationKeyCandidate { key, fingerprint }
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
    fingerprint: Option<String>,
}

fn local_key_match_confidence(
    remote_key: &RemoteStationKey,
    candidate: &LocalStationKeyCandidate,
) -> f64 {
    secret_fingerprint_match_confidence(
        remote_key.api_key_fingerprint.as_deref(),
        candidate.fingerprint.as_deref(),
    )
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
    bindings: &[StationGroupBinding],
    input: &CreateRemoteStationKeyInput,
) -> Option<String> {
    let Some(group_binding_id) = input
        .group_binding_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return None;
    };

    bindings
        .iter()
        .find(|binding| {
            binding.id == group_binding_id
                && binding.binding_kind == "station_group"
                && binding.binding_status != "disabled"
        })
        .and_then(|binding| binding.group_id_hash.clone())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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

fn secret_fingerprint_match_confidence(
    remote_fingerprint: Option<&str>,
    local_fingerprint: Option<&str>,
) -> f64 {
    match (remote_fingerprint, local_fingerprint) {
        (Some(remote), Some(local)) if remote == local => 1.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_driver_failure_preserves_its_sanitized_detail() {
        let error = remote_key_error_from_driver(DriverFailure::unsupported(
            "Sub2API remote-key request endpoint revision mismatch",
        ));

        assert!(matches!(
            error,
            RemoteKeyOperationError::UnsupportedWithDetail(detail)
                if detail == "Sub2API remote-key request endpoint revision mismatch"
        ));
    }

    #[test]
    fn fingerprints_are_stable_without_exposing_secrets() {
        assert_eq!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-a"));
        assert_ne!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-b"));
        assert_eq!(api_key_fingerprint("   "), None);
    }

    #[test]
    fn matching_requires_equal_secret_fingerprints() {
        assert_eq!(secret_fingerprint_match_confidence(None, None), 0.0);
        let fingerprint = api_key_fingerprint("sk-live-123-cdef");
        assert_eq!(
            secret_fingerprint_match_confidence(fingerprint.as_deref(), fingerprint.as_deref()),
            1.0,
        );
        assert_eq!(
            secret_fingerprint_match_confidence(
                fingerprint.as_deref(),
                api_key_fingerprint("sk-other").as_deref(),
            ),
            0.0,
        );
    }

    #[test]
    fn matching_rejects_identical_masks_and_names_without_a_remote_fingerprint() {
        let full_key = "sk-shared-123-tail".to_string();
        let local = LocalStationKeyCandidate {
            key: station_key_fixture("local-1"),
            fingerprint: api_key_fingerprint(&full_key),
        };
        let remote = remote_key_fixture("remote-1", "sk-shared****tail");

        let (keys, updates) = enrich_remote_key_discoveries_from_parts(&[], &[local], vec![remote]);

        assert_eq!(
            keys[0].match_status,
            crate::models::remote_keys::RemoteKeyMatchStatus::Unbound
        );
        assert!(keys[0].matched_station_key_id.is_none());
        assert!(updates.is_empty());
    }

    #[test]
    fn discovery_assigns_each_local_key_to_at_most_one_remote_key() {
        let full_key = "sk-shared-123-tail".to_string();
        let local = LocalStationKeyCandidate {
            key: station_key_fixture("local-1"),
            fingerprint: api_key_fingerprint(&full_key),
        };
        let mut remote_a = remote_key_fixture("remote-a", "sk-shared****tail");
        remote_a.api_key_fingerprint = api_key_fingerprint(&full_key);
        let mut remote_b = remote_key_fixture("remote-b", "sk-shared****tail");
        remote_b.api_key_fingerprint = api_key_fingerprint(&full_key);

        let (keys, updates) =
            enrich_remote_key_discoveries_from_parts(&[], &[local], vec![remote_b, remote_a]);
        let matched = keys
            .iter()
            .filter(|key| key.matched_station_key_id.as_deref() == Some("local-1"))
            .collect::<Vec<_>>();

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].id, "remote-a");
        assert_eq!(updates.len(), 1);
        assert!(keys.iter().any(|key| {
            key.id == "remote-b"
                && key.match_status == crate::models::remote_keys::RemoteKeyMatchStatus::Unbound
                && key.matched_station_key_id.is_none()
        }));
    }

    fn station_key_fixture(id: &str) -> StationKey {
        StationKey {
            id: id.to_string(),
            station_id: "station-1".to_string(),
            name: "Shared".to_string(),
            api_key_masked: "sk-***".to_string(),
            api_key_present: true,
            enabled: true,
            priority: 0,
            max_concurrency: 1,
            load_factor: None,
            schedulable: true,
            group_name: None,
            tier_label: None,
            group_binding_id: None,
            group_id_hash: None,
            rate_multiplier: None,
            manual_rate_multiplier: None,
            manual_rate_updated_at: None,
            rate_source: None,
            rate_collected_at: None,
            balance_scope: None,
            status: "unchecked".to_string(),
            last_checked_at: None,
            last_used_at: None,
            note: None,
            created_at: "1".to_string(),
            updated_at: "1".to_string(),
        }
    }

    fn remote_key_fixture(id: &str, masked: &str) -> RemoteStationKey {
        RemoteStationKey {
            id: id.to_string(),
            station_id: "station-1".to_string(),
            remote_key_id_hash: Some(id.to_string()),
            remote_key_name: Some("Shared".to_string()),
            api_key_masked: Some(masked.to_string()),
            api_key_fingerprint: None,
            group_id_hash: None,
            group_name: None,
            tier_label: None,
            rate_multiplier: None,
            rate_source: None,
            created_at: None,
            last_used_at: None,
            raw_source: "fixture".to_string(),
            match_status: crate::models::remote_keys::RemoteKeyMatchStatus::Unbound,
            matched_station_key_id: None,
            match_confidence: 0.0,
            collected_at: "1".to_string(),
        }
    }
}
