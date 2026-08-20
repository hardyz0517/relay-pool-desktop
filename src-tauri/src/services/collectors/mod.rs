pub(crate) mod collector_apply;
#[allow(
    dead_code,
    reason = "Stage 19.A freezes provider capability contracts before production driver cutover"
)]
pub mod contract;
pub mod drivers;
#[allow(
    dead_code,
    reason = "Stage 19.A freezes provider evidence contracts before production driver cutover"
)]
pub mod evidence;
pub mod facts;
#[allow(
    dead_code,
    reason = "Stage 19.A freezes provider failure contracts before production driver cutover"
)]
pub mod failure;
mod login_probe;
mod manual_authorization;
#[allow(
    dead_code,
    reason = "Stage 19.A freezes provider registry contracts before production driver cutover"
)]
pub mod orchestration;
pub mod output;

// Preserve the crate-local composition path while the V2 apply boundary is
// owned by the collector consumer rather than a legacy persistence module.
pub(crate) mod apply {
    pub(crate) use super::collector_apply::{CollectorApplyPort, V2CollectorApplyAdapter};
}
use std::{sync::Arc, time::Duration};

use futures_util::{future::BoxFuture, FutureExt};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::{
    application::{
        collectors::{CollectorApplyOutcome, CollectorService},
        credentials::CredentialService,
        error::ApplicationError,
        settings::SettingsService,
    },
    models::{
        collector::{CollectorRunResult, StationLoginTestInput, StationLoginTestResult},
        credentials::{PersistStationSessionInput, ResolvedSession, StationCredentials},
        group_facts::StationGroupBinding,
        settings::AppSettings,
        station_keys::StationKey,
        stations::Station,
    },
    outbound::{AsyncOutboundClient, ManualProxy, ProxyPolicy, RequestBudget},
};

use crate::models::provider_drafts::{ProviderDraftPreview, ProviderDraftPreviewGroup};
use collector_apply::CollectorApplyPort;
use output::{AdapterOutput, CollectorTask};

/// Consumer-owned read/write boundary required by provider collection drivers.
///
/// Production composition supplies this port from catalog, settings, and
/// credential application services.
pub(crate) trait CollectorSourcePort: Send + Sync {
    fn station_for_collector(&self, station_id: &str) -> Result<Station, String>;
    fn get_settings(&self) -> Result<AppSettings, String>;
    fn list_station_keys(&self, station_id: String) -> Result<Vec<StationKey>, String>;
    fn resolve_station_key_secret(&self, station_key_id: &str) -> Result<String, String>;
    fn get_station_credentials(&self, station_id: String) -> Result<StationCredentials, String>;
    fn get_station_login_password(&self, station_id: String) -> Result<Option<String>, String>;
    fn resolve_station_session(
        &self,
        station_id: String,
        now_ms: i64,
    ) -> Result<ResolvedSession, String>;
    fn persist_station_session<'a>(
        &'a self,
        input: PersistStationSessionInput,
        expected_revision: i64,
    ) -> BoxFuture<'a, Result<StationCredentials, String>>;
    fn list_station_group_bindings(
        &self,
        station_id: String,
    ) -> Result<Vec<StationGroupBinding>, String>;
}

#[derive(Clone)]
pub(crate) struct V2CollectorSourceAdapter {
    collectors: Arc<CollectorService>,
    credentials: Arc<CredentialService>,
    settings: Arc<SettingsService>,
}

impl V2CollectorSourceAdapter {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        credentials: Arc<CredentialService>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self {
            collectors,
            credentials,
            settings,
        }
    }
}

/// Resolve a session that can be injected into the temporary recharge WebView.
///
/// The normal collector can authenticate inline while it is making an API
/// request. A browser scan needs the same session before the first document is
/// loaded, so this helper performs the existing password login probe once,
/// persists the redacted session metadata through the credential service, and
/// resolves it again from the canonical store.
pub(crate) async fn resolve_station_session_for_browser(
    source: &V2CollectorSourceAdapter,
    outbound: &AsyncOutboundClient,
    station_id: String,
    cancellation: CancellationToken,
    correlation_id: Option<String>,
) -> Result<ResolvedSession, ApplicationError> {
    let current = source
        .credentials
        .resolve_station_session(
            station_id.clone(),
            crate::services::time::now_millis_for_services() as i64,
        )
        .await
        .map_err(|_| ApplicationError::Internal)?;
    if browser_session_is_usable(&current) {
        return Ok(current);
    }

    let station = source
        .collectors
        .station_for_collection(&station_id)
        .await
        .map_err(|_| ApplicationError::Internal)?;
    let credentials = source
        .credentials
        .get_station_credentials(station_id.clone())
        .await
        .map_err(|_| ApplicationError::Internal)?;
    let Some(username) = credentials
        .login_username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(current);
    };
    let Some(password) = source
        .credentials
        .get_station_login_password(station_id.clone())
        .await
        .map_err(|_| ApplicationError::Internal)?
        .map(|secret| {
            String::from_utf8(secret.as_bytes().to_vec()).map_err(|_| ApplicationError::Internal)
        })
        .transpose()?
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(current);
    };
    let settings = source
        .settings
        .load()
        .await
        .map_err(|_| ApplicationError::Internal)?;
    let proxy_config = crate::services::outbound::resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let proxy =
        proxy_policy_from_collector_config(proxy_config).map_err(|_| ApplicationError::Internal)?;
    let login_base_url = if station.station_type.eq_ignore_ascii_case("sub2api") {
        station.api_base_url.as_str()
    } else {
        station.website_url.as_str()
    };
    let attempt = match login_probe::probe_login(
        outbound,
        &station.station_type,
        login_base_url,
        username,
        &password,
        proxy,
        cancellation,
        correlation_id,
    )
    .await
    {
        Ok(attempt) => attempt,
        Err(error) => {
            return Ok(ResolvedSession::manual_required(
                crate::services::secrets::mask::redact_text(&error),
            ));
        }
    };

    let Some(session) = attempt.session else {
        return Ok(ResolvedSession::manual_required(
            attempt
                .manual_required
                .or(attempt.login_message)
                .unwrap_or_else(|| "stored login did not return a usable session".to_string()),
        ));
    };
    source
        .credentials
        .persist_station_session_if_revision(
            PersistStationSessionInput {
                station_id: station_id.clone(),
                access_token: session.access_token,
                refresh_token: session.refresh_token,
                cookie: session.cookie,
                newapi_user_id: session.newapi_user_id,
                token_expires_at: None,
                session_expires_at: None,
                session_source: "password_login".to_string(),
                session_user_agent: credentials.session_user_agent,
            },
            station.endpoint_revision,
        )
        .await
        .map_err(|_| ApplicationError::Internal)?;
    source
        .credentials
        .resolve_station_session(
            station_id,
            crate::services::time::now_millis_for_services() as i64,
        )
        .await
        .map_err(|_| ApplicationError::Internal)
}

fn browser_session_is_usable(session: &ResolvedSession) -> bool {
    session.message.is_none()
        && session
            .newapi_user_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        && (session
            .cookie
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            || session
                .access_token
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()))
}

impl CollectorSourcePort for V2CollectorSourceAdapter {
    fn station_for_collector(&self, station_id: &str) -> Result<Station, String> {
        tauri::async_runtime::block_on(self.collectors.station_for_collection(station_id))
            .map_err(application_error)
    }

    fn get_settings(&self) -> Result<AppSettings, String> {
        tauri::async_runtime::block_on(self.settings.load()).map_err(application_error)
    }

    fn list_station_keys(&self, station_id: String) -> Result<Vec<StationKey>, String> {
        tauri::async_runtime::block_on(self.credentials.list_station_keys(station_id))
            .map_err(application_error)
    }

    fn resolve_station_key_secret(&self, station_key_id: &str) -> Result<String, String> {
        let secret = tauri::async_runtime::block_on(
            self.credentials
                .resolve_station_key_secret(station_key_id.to_string()),
        )
        .map_err(application_error)?;
        String::from_utf8(secret.as_bytes().to_vec())
            .map_err(|_| "station key secret is not valid UTF-8".to_string())
    }

    fn get_station_credentials(&self, station_id: String) -> Result<StationCredentials, String> {
        tauri::async_runtime::block_on(self.credentials.get_station_credentials(station_id))
            .map_err(application_error)
    }

    fn get_station_login_password(&self, station_id: String) -> Result<Option<String>, String> {
        let secret =
            tauri::async_runtime::block_on(self.credentials.get_station_login_password(station_id))
                .map_err(application_error)?;
        secret
            .map(|secret| {
                String::from_utf8(secret.as_bytes().to_vec())
                    .map_err(|_| "station login password is not valid UTF-8".to_string())
            })
            .transpose()
    }

    fn resolve_station_session(
        &self,
        station_id: String,
        now_ms: i64,
    ) -> Result<ResolvedSession, String> {
        tauri::async_runtime::block_on(self.credentials.resolve_station_session(station_id, now_ms))
            .map_err(application_error)
    }

    fn persist_station_session<'a>(
        &'a self,
        input: PersistStationSessionInput,
        expected_revision: i64,
    ) -> BoxFuture<'a, Result<StationCredentials, String>> {
        let credentials = Arc::clone(&self.credentials);
        async move {
            credentials
                .persist_station_session_if_revision(input, expected_revision)
                .await
                .map_err(application_error)
        }
        .boxed()
    }

    fn list_station_group_bindings(
        &self,
        station_id: String,
    ) -> Result<Vec<StationGroupBinding>, String> {
        tauri::async_runtime::block_on(self.collectors.list_station_group_bindings(&station_id))
            .map_err(application_error)
    }
}

impl drivers::newapi::auth::NewApiAuthSessionSource for dyn CollectorSourcePort + '_ {
    fn resolve_newapi_session(
        &self,
        station_id: &str,
        now_ms: i64,
    ) -> Result<drivers::newapi::auth::NewApiResolvedSession, String> {
        let session = self.resolve_station_session(station_id.to_string(), now_ms)?;
        Ok(drivers::newapi::auth::NewApiResolvedSession {
            access_token: session.access_token,
            cookie: session.cookie,
            newapi_user_id: session.newapi_user_id,
            message: session.message,
        })
    }
}

fn application_error(error: ApplicationError) -> String {
    error.to_string()
}

pub(crate) enum PreparedStationCollectionRoute {
    Sub2Api(PreparedSub2ApiCollection),
    NewApi(PreparedNewApiCollection),
}

pub(crate) enum PreparedStationTaskRoute {
    Sub2Api(PreparedSub2ApiCollection),
    NewApi(PreparedNewApiCollection),
}

pub(crate) enum PreparedNewApiCollection {
    Immediate(PreparedStationCollection),
    Driver(PreparedNewApiDriverCollection),
}

pub(crate) struct PreparedNewApiDriverCollection {
    station_id: String,
    endpoint_revision: i64,
    task: CollectorTask,
    driver_tasks: Vec<CollectorTask>,
    enabled_key_count: usize,
    website_url: String,
    proxy: ProxyPolicy,
    timeout: Duration,
    credential_handle: contract::OpaqueCredentialHandle,
    auth_context: Option<contract::ProviderAuthContext>,
    secret_accessor: StaticSecretAccessor,
    password_login: Option<PreparedNewApiPasswordLogin>,
}

struct PreparedNewApiPasswordLogin {
    username: String,
    password: String,
}

pub(crate) enum PreparedSub2ApiCollection {
    Driver(PreparedSub2ApiDriverCollection),
}

pub(crate) struct PreparedSub2ApiDriverCollection {
    station_id: String,
    endpoint_revision: i64,
    task: CollectorTask,
    driver_tasks: Vec<CollectorTask>,
    enabled_key_count: usize,
    api_base_url: String,
    website_url: String,
    proxy: ProxyPolicy,
    timeout: Duration,
    credential_handle: contract::OpaqueCredentialHandle,
    auth_context: contract::ProviderAuthContext,
    user_agent: Option<String>,
    secret_accessor: MultiSecretAccessor,
}

struct SecretRecord {
    handle: contract::OpaqueCredentialHandle,
    purpose: contract::CredentialSecretPurpose,
    secret: String,
}

struct MultiSecretAccessor {
    records: Vec<SecretRecord>,
}

impl contract::DriverSecretAccessor for MultiSecretAccessor {
    fn resolve_secret<'a>(
        &'a self,
        handle: &'a contract::OpaqueCredentialHandle,
        purpose: contract::CredentialSecretPurpose,
    ) -> BoxFuture<'a, Result<contract::CredentialSecret, failure::DriverFailure>> {
        async move {
            let Some(record) = self
                .records
                .iter()
                .find(|record| record.purpose == purpose && &record.handle == handle)
            else {
                return Err(failure::DriverFailure::unsupported(
                    "credential handle is not available to this driver context",
                ));
            };
            Ok(contract::CredentialSecret::new(record.secret.clone()))
        }
        .boxed()
    }
}

struct StaticSecretAccessor {
    expected: contract::OpaqueCredentialHandle,
    purpose: contract::CredentialSecretPurpose,
    secret: String,
}

impl contract::DriverSecretAccessor for StaticSecretAccessor {
    fn resolve_secret<'a>(
        &'a self,
        handle: &'a contract::OpaqueCredentialHandle,
        purpose: contract::CredentialSecretPurpose,
    ) -> BoxFuture<'a, Result<contract::CredentialSecret, failure::DriverFailure>> {
        async move {
            if purpose != self.purpose || handle != &self.expected {
                return Err(failure::DriverFailure::unsupported(
                    "credential handle is not available to this driver context",
                ));
            }
            Ok(contract::CredentialSecret::new(self.secret.clone()))
        }
        .boxed()
    }
}

pub(crate) fn prepare_station_collection_route_v2(
    source: &dyn CollectorSourcePort,
    station_id: String,
    task: CollectorTask,
) -> Result<PreparedStationCollectionRoute, ApplicationError> {
    let station = source
        .station_for_collector(&station_id)
        .map_err(|_| ApplicationError::Internal)?;
    let provider = provider_kind_for_station_type(&station.station_type)
        .map_err(|_| ApplicationError::ConstraintViolation)?;
    match provider {
        contract::ProviderKind::Sub2Api => prepare_sub2api_collection_v2(source, station, task)
            .map(PreparedStationCollectionRoute::Sub2Api),
        contract::ProviderKind::NewApi => prepare_newapi_collection_v2(source, station, task)
            .map(PreparedStationCollectionRoute::NewApi),
    }
}

pub(crate) fn prepare_station_task_route_v2(
    source: &dyn CollectorSourcePort,
    station_id: String,
    task: CollectorTask,
) -> Result<PreparedStationTaskRoute, ApplicationError> {
    let station = source
        .station_for_collector(&station_id)
        .map_err(|_| ApplicationError::Internal)?;
    let provider = provider_kind_for_station_type(&station.station_type)
        .map_err(|_| ApplicationError::ConstraintViolation)?;
    match provider {
        contract::ProviderKind::Sub2Api => prepare_sub2api_collection_v2(source, station, task)
            .map(PreparedStationTaskRoute::Sub2Api),
        contract::ProviderKind::NewApi => prepare_newapi_collection_v2(source, station, task)
            .map(PreparedStationTaskRoute::NewApi),
    }
}

fn prepare_sub2api_collection_v2(
    source: &dyn CollectorSourcePort,
    station: Station,
    task: CollectorTask,
) -> Result<PreparedSub2ApiCollection, ApplicationError> {
    let station_id = station.id.clone();
    let driver_tasks = if task == CollectorTask::Full {
        full_child_tasks(contract::ProviderKind::Sub2Api)
    } else {
        vec![task]
    };
    if driver_tasks.is_empty() || driver_tasks.len() > 3 {
        return Err(ApplicationError::ConstraintViolation);
    }

    let keys = source
        .list_station_keys(station_id.clone())
        .map_err(|_| ApplicationError::Internal)?;
    let enabled_key_count = keys.iter().filter(|key| key.enabled).count();
    let mut records = Vec::new();
    let mut station_key_credentials = Vec::new();
    for key in keys
        .into_iter()
        .filter(|key| key.enabled && key.api_key_present)
    {
        let handle = contract::OpaqueCredentialHandle {
            station_id: key.id.clone(),
            credential_revision: station.endpoint_revision,
            scope: contract::CredentialScope::StationKey,
        };
        if let Ok(secret) = source.resolve_station_key_secret(&key.id) {
            records.push(SecretRecord {
                handle: handle.clone(),
                purpose: contract::CredentialSecretPurpose::AuthorizationHeader,
                secret,
            });
        }
        station_key_credentials.push(contract::Sub2ApiStationKeyCredential {
            station_key_id: key.id,
            credential: handle,
        });
    }

    let session = source
        .resolve_station_session(
            station_id.clone(),
            crate::services::time::now_millis_for_services() as i64,
        )
        .map_err(|_| ApplicationError::Internal)?;
    let access_token_handle = session
        .access_token
        .filter(|token| !token.trim().is_empty())
        .map(|token| {
            let handle = contract::OpaqueCredentialHandle {
                station_id: station_id.clone(),
                credential_revision: station.endpoint_revision,
                scope: contract::CredentialScope::LoginSession,
            };
            records.push(SecretRecord {
                handle: handle.clone(),
                purpose: contract::CredentialSecretPurpose::AuthorizationHeader,
                secret: token,
            });
            handle
        });

    let refresh_token_handle = session
        .refresh_token
        .filter(|token| !token.trim().is_empty())
        .map(|token| {
            let handle = contract::OpaqueCredentialHandle {
                station_id: station_id.clone(),
                credential_revision: station.endpoint_revision,
                scope: contract::CredentialScope::LoginSession,
            };
            records.push(SecretRecord {
                handle: handle.clone(),
                purpose: contract::CredentialSecretPurpose::RefreshToken,
                secret: token,
            });
            handle
        });

    // Keep the browser cookie separately from the JWT.  Cloudflare-protected
    // deployments may require both headers on the same management request.
    let session_cookie_handle = session
        .cookie
        .filter(|cookie| !cookie.trim().is_empty())
        .map(|cookie| {
            let handle = contract::OpaqueCredentialHandle {
                station_id: station_id.clone(),
                credential_revision: station.endpoint_revision,
                scope: contract::CredentialScope::LoginSession,
            };
            records.push(SecretRecord {
                handle: handle.clone(),
                purpose: contract::CredentialSecretPurpose::SessionCookie,
                secret: cookie,
            });
            handle
        });
    let access_token_handle = access_token_handle.or_else(|| session_cookie_handle.clone());

    let credentials = source
        .get_station_credentials(station_id.clone())
        .map_err(|_| ApplicationError::Internal)?;
    let user_agent = credentials.session_user_agent.clone();
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
                .get_station_login_password(station_id.clone())
                .ok()
                .flatten()?;
            if password.trim().is_empty() {
                return None;
            }
            let handle = contract::OpaqueCredentialHandle {
                station_id: station_id.clone(),
                credential_revision: station.endpoint_revision,
                scope: contract::CredentialScope::LoginPassword,
            };
            records.push(SecretRecord {
                handle: handle.clone(),
                purpose: contract::CredentialSecretPurpose::LoginPassword,
                secret: password,
            });
            Some(contract::Sub2ApiLoginCredential {
                username: username.to_string(),
                password: handle,
            })
        });

    let settings = source
        .get_settings()
        .map_err(|_| ApplicationError::Internal)?;
    let proxy = crate::services::outbound::resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let proxy =
        proxy_policy_from_collector_config(proxy).map_err(|_| ApplicationError::Internal)?;
    let credential_handle =
        access_token_handle
            .clone()
            .unwrap_or_else(|| contract::OpaqueCredentialHandle {
                station_id: station_id.clone(),
                credential_revision: station.endpoint_revision,
                scope: contract::CredentialScope::LoginSession,
            });
    Ok(PreparedSub2ApiCollection::Driver(
        PreparedSub2ApiDriverCollection {
            station_id,
            endpoint_revision: station.endpoint_revision,
            task,
            driver_tasks,
            enabled_key_count,
            api_base_url: station.api_base_url,
            website_url: station.website_url,
            proxy,
            timeout: Duration::from_secs(u64::from(settings.collector_timeout_seconds)),
            credential_handle,
            auth_context: contract::ProviderAuthContext::Sub2Api {
                station_keys: station_key_credentials,
                access_token: access_token_handle,
                refresh_token: refresh_token_handle,
                session_cookie: session_cookie_handle,
                login,
                credit_per_cny: station.credit_per_cny,
            },
            user_agent,
            secret_accessor: MultiSecretAccessor { records },
        },
    ))
}

fn prepare_newapi_collection_v2(
    source: &dyn CollectorSourcePort,
    station: Station,
    task: CollectorTask,
) -> Result<PreparedNewApiCollection, ApplicationError> {
    let station_id = station.id.clone();
    let driver_tasks = if task == CollectorTask::Full {
        full_child_tasks(contract::ProviderKind::NewApi)
    } else {
        vec![task]
    };
    if driver_tasks.is_empty() || driver_tasks.len() > 3 {
        return Err(ApplicationError::ConstraintViolation);
    }
    let enabled_key_count = source
        .list_station_keys(station_id.clone())
        .map_err(|_| ApplicationError::Internal)?
        .into_iter()
        .filter(|key| key.enabled)
        .count();
    let needs_auth = driver_tasks
        .iter()
        .any(|task| *task != CollectorTask::Detect);
    let (auth_context, secret_purpose, secret, password_login) = if needs_auth {
        match drivers::newapi::auth::prepare_collector_auth_context(
            source,
            &station.id,
            crate::services::time::now_millis_for_services() as i64,
        ) {
            Ok(auth) => {
                let secret_purpose = match auth.kind {
                    drivers::newapi::auth::PreparedNewApiAuthKind::AccessToken => {
                        contract::CredentialSecretPurpose::AuthorizationHeader
                    }
                    drivers::newapi::auth::PreparedNewApiAuthKind::Cookie => {
                        contract::CredentialSecretPurpose::SessionCookie
                    }
                };
                (
                    Some(contract::ProviderAuthContext::NewApi {
                        user_id: auth.user_id,
                        secret_purpose,
                    }),
                    secret_purpose,
                    auth.secret,
                    None,
                )
            }
            Err(error) => {
                let credentials = source
                    .get_station_credentials(station_id.clone())
                    .map_err(|_| ApplicationError::Internal)?;
                let password = source
                    .get_station_login_password(station_id.clone())
                    .map_err(|_| ApplicationError::Internal)?;
                if let Some(password_login) = prepare_newapi_password_login(
                    credentials.login_username,
                    credentials.password_present,
                    password,
                ) {
                    (
                        None,
                        contract::CredentialSecretPurpose::SessionCookie,
                        String::new(),
                        Some(password_login),
                    )
                } else {
                    let message = crate::services::secrets::mask::redact_text(&error);
                    let outputs = driver_tasks
                        .into_iter()
                        .map(|child_task| {
                            manual_required_output_for_adapter("newapi", child_task, &message)
                        })
                        .collect();
                    return Ok(PreparedNewApiCollection::Immediate(
                        PreparedStationCollection {
                            station_id,
                            endpoint_revision: station.endpoint_revision,
                            adapter: "newapi".to_string(),
                            task,
                            outputs,
                            enabled_key_count,
                        },
                    ));
                }
            }
        }
    } else {
        (
            None,
            contract::CredentialSecretPurpose::AuthorizationHeader,
            String::new(),
            None,
        )
    };
    let settings = source
        .get_settings()
        .map_err(|_| ApplicationError::Internal)?;
    let proxy = crate::services::outbound::resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let proxy =
        proxy_policy_from_collector_config(proxy).map_err(|_| ApplicationError::Internal)?;
    let credential_handle = contract::OpaqueCredentialHandle {
        station_id: station_id.clone(),
        credential_revision: station.endpoint_revision,
        scope: contract::CredentialScope::LoginSession,
    };
    Ok(PreparedNewApiCollection::Driver(
        PreparedNewApiDriverCollection {
            station_id,
            endpoint_revision: station.endpoint_revision,
            task,
            driver_tasks,
            enabled_key_count,
            website_url: station.website_url,
            proxy,
            timeout: Duration::from_secs(u64::from(settings.collector_timeout_seconds)),
            credential_handle: credential_handle.clone(),
            auth_context,
            secret_accessor: StaticSecretAccessor {
                expected: credential_handle,
                purpose: secret_purpose,
                secret,
            },
            password_login,
        },
    ))
}

fn prepare_newapi_password_login(
    username: Option<String>,
    password_present: bool,
    password: Option<String>,
) -> Option<PreparedNewApiPasswordLogin> {
    if !has_login_credentials(&username, password_present) {
        return None;
    }
    let username = username?.trim().to_string();
    let password = password.filter(|value| !value.trim().is_empty())?;
    Some(PreparedNewApiPasswordLogin { username, password })
}

pub(crate) async fn finish_sub2api_collection_v2(
    registry: &orchestration::ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedSub2ApiCollection,
    cancellation_token: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedStationCollection, ApplicationError> {
    match prepared {
        PreparedSub2ApiCollection::Driver(prepared) => {
            let driver = registry
                .collector(contract::ProviderKind::Sub2Api)
                .map_err(|_| ApplicationError::ConstraintViolation)?;
            let context = contract::CollectorContext {
                station: contract::StationIdentity {
                    station_id: prepared.station_id.clone(),
                    endpoint_revision: prepared.endpoint_revision,
                    provider: contract::ProviderKind::Sub2Api,
                },
                endpoints: contract::ProviderEndpoints {
                    api_base_url: Some(prepared.api_base_url),
                    website_url: Some(prepared.website_url),
                },
                credential: prepared.credential_handle,
                auth: Some(prepared.auth_context),
                user_agent: prepared.user_agent,
                secrets: &prepared.secret_accessor,
                outbound,
                proxy: prepared.proxy,
                budget: RequestBudget::from_now(prepared.timeout),
                cancellation: cancellation_token,
                correlation_id: correlation_id.unwrap_or_else(|| "station-collection".to_string()),
            };
            let outputs = prepared
                .driver_tasks
                .iter()
                .copied()
                .map(|child_task| {
                    let driver_task = collector_task_kind(child_task)
                        .ok_or(ApplicationError::ConstraintViolation)?;
                    Ok((child_task, driver_task))
                })
                .collect::<Result<Vec<_>, ApplicationError>>()?;
            let mut adapter_outputs = Vec::with_capacity(outputs.len());
            for (child_task, driver_task) in outputs {
                let output = driver
                    .collect(&context, driver_task)
                    .await
                    .map(|output| driver_output_to_adapter_output("sub2api", child_task, output))
                    .unwrap_or_else(|failure| {
                        driver_failure_to_adapter_output("sub2api", child_task, failure)
                    });
                adapter_outputs.push(output);
            }
            Ok(PreparedStationCollection {
                station_id: prepared.station_id,
                endpoint_revision: prepared.endpoint_revision,
                adapter: "sub2api".to_string(),
                task: prepared.task,
                outputs: adapter_outputs,
                enabled_key_count: prepared.enabled_key_count,
            })
        }
    }
}

pub(crate) async fn finish_sub2api_task_v2(
    registry: &orchestration::ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedSub2ApiCollection,
    cancellation_token: CancellationToken,
    correlation_id: Option<String>,
) -> Result<(String, i64, AdapterOutput), ApplicationError> {
    let prepared = finish_sub2api_collection_v2(
        registry,
        outbound,
        prepared,
        cancellation_token,
        correlation_id,
    )
    .await?;
    let output = prepared
        .outputs
        .into_iter()
        .next()
        .ok_or(ApplicationError::ConstraintViolation)?;
    Ok((prepared.station_id, prepared.endpoint_revision, output))
}

pub(crate) async fn finish_newapi_collection_v2(
    source: &dyn CollectorSourcePort,
    registry: &orchestration::ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedNewApiCollection,
    cancellation_token: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedStationCollection, ApplicationError> {
    match prepared {
        PreparedNewApiCollection::Immediate(prepared) => Ok(prepared),
        PreparedNewApiCollection::Driver(mut prepared) => {
            if let Some(login) = prepared.password_login.take() {
                let attempt = login_probe::probe_login(
                    outbound,
                    "newapi",
                    &prepared.website_url,
                    &login.username,
                    &login.password,
                    prepared.proxy.clone(),
                    cancellation_token.clone(),
                    correlation_id.clone(),
                )
                .await;
                let session = match attempt {
                    Ok(attempt) => attempt.newapi_session,
                    Err(error) => {
                        return Ok(newapi_manual_required_collection(
                            prepared,
                            &crate::services::secrets::mask::redact_text(&error),
                        ));
                    }
                };
                let Some(session) = session else {
                    return Ok(newapi_manual_required_collection(
                        prepared,
                        "NewAPI password login requires manual authorization",
                    ));
                };
                source
                    .persist_station_session(
                        PersistStationSessionInput {
                            station_id: prepared.station_id.clone(),
                            access_token: None,
                            refresh_token: None,
                            cookie: Some(session.cookie.clone()),
                            newapi_user_id: Some(session.user_id.clone()),
                            token_expires_at: None,
                            session_expires_at: None,
                            session_source: "password_login".to_string(),
                            session_user_agent: None,
                        },
                        prepared.endpoint_revision,
                    )
                    .await
                    .map_err(|_| ApplicationError::Internal)?;
                prepared.auth_context = Some(contract::ProviderAuthContext::NewApi {
                    user_id: session.user_id,
                    secret_purpose: contract::CredentialSecretPurpose::SessionCookie,
                });
                prepared.secret_accessor.purpose = contract::CredentialSecretPurpose::SessionCookie;
                prepared.secret_accessor.secret = session.cookie;
            }
            let driver = registry
                .collector(contract::ProviderKind::NewApi)
                .map_err(|_| ApplicationError::ConstraintViolation)?;
            let context = contract::CollectorContext {
                station: contract::StationIdentity {
                    station_id: prepared.station_id.clone(),
                    endpoint_revision: prepared.endpoint_revision,
                    provider: contract::ProviderKind::NewApi,
                },
                endpoints: contract::ProviderEndpoints {
                    api_base_url: None,
                    website_url: Some(prepared.website_url),
                },
                credential: prepared.credential_handle,
                auth: prepared.auth_context,
                user_agent: None,
                secrets: &prepared.secret_accessor,
                outbound,
                proxy: prepared.proxy,
                budget: RequestBudget::from_now(prepared.timeout),
                cancellation: cancellation_token,
                correlation_id: correlation_id.unwrap_or_else(|| "station-collection".to_string()),
            };
            let outputs = prepared
                .driver_tasks
                .iter()
                .copied()
                .map(|child_task| {
                    let driver_task = collector_task_kind(child_task)
                        .ok_or(ApplicationError::ConstraintViolation)?;
                    Ok((child_task, driver_task))
                })
                .collect::<Result<Vec<_>, ApplicationError>>()?;
            let mut adapter_outputs = Vec::with_capacity(outputs.len());
            for (child_task, driver_task) in outputs {
                let output = driver
                    .collect(&context, driver_task)
                    .await
                    .map(|output| driver_output_to_adapter_output("newapi", child_task, output))
                    .unwrap_or_else(|failure| {
                        driver_failure_to_adapter_output("newapi", child_task, failure)
                    });
                adapter_outputs.push(output);
            }
            Ok(PreparedStationCollection {
                station_id: prepared.station_id,
                endpoint_revision: prepared.endpoint_revision,
                adapter: "newapi".to_string(),
                task: prepared.task,
                outputs: adapter_outputs,
                enabled_key_count: prepared.enabled_key_count,
            })
        }
    }
}

fn newapi_manual_required_collection(
    prepared: PreparedNewApiDriverCollection,
    message: &str,
) -> PreparedStationCollection {
    let message = crate::services::secrets::mask::redact_text(message);
    let outputs = prepared
        .driver_tasks
        .into_iter()
        .map(|child_task| manual_required_output_for_adapter("newapi", child_task, &message))
        .collect();
    PreparedStationCollection {
        station_id: prepared.station_id,
        endpoint_revision: prepared.endpoint_revision,
        adapter: "newapi".to_string(),
        task: prepared.task,
        outputs,
        enabled_key_count: prepared.enabled_key_count,
    }
}

pub(crate) async fn finish_newapi_task_v2(
    source: &dyn CollectorSourcePort,
    registry: &orchestration::ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedNewApiCollection,
    cancellation_token: CancellationToken,
    correlation_id: Option<String>,
) -> Result<(String, i64, AdapterOutput), ApplicationError> {
    let prepared = finish_newapi_collection_v2(
        source,
        registry,
        outbound,
        prepared,
        cancellation_token,
        correlation_id,
    )
    .await?;
    let output = prepared
        .outputs
        .into_iter()
        .next()
        .ok_or(ApplicationError::ConstraintViolation)?;
    Ok((prepared.station_id, prepared.endpoint_revision, output))
}

pub(crate) async fn apply_prepared_station_task_v2(
    port: &dyn CollectorApplyPort,
    station_id: String,
    endpoint_revision: i64,
    output: AdapterOutput,
) -> Result<CollectorApplyOutcome, ApplicationError> {
    collector_apply::apply_station_output_v2(port, station_id, endpoint_revision, None, output)
        .await
}

pub(crate) fn should_refresh_remote_keys_after_collection(
    task: CollectorTask,
    status: &str,
) -> bool {
    matches!(task, CollectorTask::Groups | CollectorTask::Full)
        && matches!(status, "success" | "partial")
}

#[derive(Debug)]
pub(crate) struct PreparedStationCollection {
    station_id: String,
    endpoint_revision: i64,
    adapter: String,
    task: CollectorTask,
    outputs: Vec<AdapterOutput>,
    enabled_key_count: usize,
}

pub(crate) fn provider_draft_preview_from_prepared(
    prepared: PreparedStationCollection,
    runtime_fingerprint: String,
    collected_at: String,
) -> ProviderDraftPreview {
    let mut groups = Vec::<ProviderDraftPreviewGroup>::new();
    let mut balance = None;
    let mut summaries = Vec::new();
    let mut status = "success".to_string();
    for output in prepared.outputs {
        if output.status == "failed" {
            status = "failed".to_string();
        } else if output.status != "success" && status == "success" {
            status = output.status.clone();
        }
        summaries.push(output.summary_json.clone());
        for group in output.facts.groups {
            let rate = output
                .facts
                .rates
                .iter()
                .find(|rate| rate.group_key_hash == group.group_key_hash)
                .and_then(|rate| rate.effective_rate_multiplier);
            if !groups
                .iter()
                .any(|item| item.group_key_hash == group.group_key_hash)
            {
                groups.push(ProviderDraftPreviewGroup {
                    group_key_hash: group.group_key_hash,
                    group_id_hash: group.group_id,
                    group_name: group.group_name,
                    rate_multiplier: rate,
                    inferred_group_category: group.inferred_group_category,
                    source: group.source,
                    confidence: group.confidence,
                });
            }
        }
        if balance.is_none() {
            balance = output.facts.balances.iter().find_map(|item| item.value);
        }
    }
    ProviderDraftPreview {
        draft_id: prepared.station_id,
        kind: prepared.task.as_str().to_string(),
        runtime_fingerprint,
        status,
        groups,
        models: Vec::new(),
        balance,
        summary_json: serde_json::json!({
            "adapter": prepared.adapter,
            "results": summaries,
        }),
        collected_at,
    }
}

/// Applies a prepared task through V2 and returns a complete, bounded read model.
pub(crate) async fn apply_prepared_station_collection_v2(
    service: &CollectorService,
    port: &dyn CollectorApplyPort,
    prepared: PreparedStationCollection,
) -> Result<CollectorRunResult, ApplicationError> {
    if prepared.task != CollectorTask::Full {
        let output = prepared
            .outputs
            .into_iter()
            .next()
            .ok_or(ApplicationError::ConstraintViolation)?;
        let task_type = if output.adapter == "login-state" {
            "login-test".to_string()
        } else {
            output.task.as_str().to_string()
        };
        let outcome = collector_apply::apply_station_output_v2(
            port,
            prepared.station_id,
            prepared.endpoint_revision,
            None,
            output,
        )
        .await?;
        return service.result_for_apply(&outcome, &task_type).await;
    }

    apply_prepared_full_collection_v2(service, port, prepared).await
}

pub(crate) struct PreparedStationLoginProbe {
    station: Station,
    credentials: StationCredentials,
    username: String,
    password: Option<String>,
    proxy: ProxyPolicy,
}

pub(crate) fn prepare_station_login_probe_v2(
    source: &dyn CollectorSourcePort,
    station_id: String,
) -> Result<PreparedStationLoginProbe, ApplicationError> {
    let station = source
        .station_for_collector(&station_id)
        .map_err(|_| ApplicationError::Internal)?;
    let credentials = source
        .get_station_credentials(station_id.clone())
        .map_err(|_| ApplicationError::Internal)?;
    let username = credentials.login_username.clone().unwrap_or_default();
    let password = source
        .get_station_login_password(station_id)
        .map_err(|_| ApplicationError::Internal)?;
    let settings = source
        .get_settings()
        .map_err(|_| ApplicationError::Internal)?;
    let proxy = crate::services::outbound::resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let proxy =
        proxy_policy_from_collector_config(proxy).map_err(|_| ApplicationError::Internal)?;
    Ok(PreparedStationLoginProbe {
        station,
        credentials,
        username,
        password,
        proxy,
    })
}

pub(crate) async fn finish_station_login_probe_v2(
    source: &dyn CollectorSourcePort,
    outbound: &AsyncOutboundClient,
    prepared: PreparedStationLoginProbe,
    cancellation_token: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedStationCollection, ApplicationError> {
    let password_present = prepared
        .password
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let attempt = if !has_login_credentials(
        &prepared.credentials.login_username,
        prepared.credentials.password_present,
    ) || !password_present
    {
        login_probe::LoginProbeAttempt {
            credential_present: false,
            login_message: Some("Saved login credentials are incomplete.".to_string()),
            manual_required: Some(
                "Save both the login username and password before testing login.".to_string(),
            ),
            newapi_session: None,
            session: None,
        }
    } else {
        login_probe::probe_login(
            outbound,
            &prepared.station.station_type,
            &prepared.station.website_url,
            &prepared.username,
            prepared.password.as_deref().unwrap_or_default(),
            prepared.proxy.clone(),
            cancellation_token,
            correlation_id,
        )
        .await
        .map_err(|_| ApplicationError::Internal)?
    };

    if let Some(session) = attempt.newapi_session.clone() {
        source
            .persist_station_session(
                PersistStationSessionInput {
                    station_id: prepared.station.id.clone(),
                    access_token: None,
                    refresh_token: None,
                    cookie: Some(session.cookie),
                    newapi_user_id: Some(session.user_id),
                    token_expires_at: None,
                    session_expires_at: None,
                    session_source: "password_login".to_string(),
                    session_user_agent: None,
                },
                prepared.station.endpoint_revision,
            )
            .await
            .map_err(|_| ApplicationError::Internal)?;
    }

    Ok(station_login_probe_collection(prepared, attempt))
}

fn station_login_probe_collection(
    prepared: PreparedStationLoginProbe,
    attempt: login_probe::LoginProbeAttempt,
) -> PreparedStationCollection {
    let token_present = attempt.credential_present;
    let status = if token_present {
        "success".to_string()
    } else {
        "manual_required".to_string()
    };
    let diagnosis = attempt
        .manual_required
        .unwrap_or_else(|| "The login endpoint returned a usable session credential.".to_string());
    let message = attempt
        .login_message
        .unwrap_or_else(|| "Login test completed.".to_string());
    let station_id = prepared.station.id.clone();
    let output = AdapterOutput {
        adapter: "login-state".to_string(),
        task: CollectorTask::Detect,
        status: status.clone(),
        facts: facts::CollectorFacts::default(),
        summary_json: json!({
            "mode": "login-state",
            "adapter": "Login State Adapter",
            "detectedType": "Login State",
            "conclusion": if token_present { "Login succeeded" } else { "Action required" },
            "message": message,
            "login": {
                "usernamePresent": !prepared.username.trim().is_empty(),
                "passwordPresent": prepared.password.as_deref().is_some_and(|value| !value.trim().is_empty()),
                "status": prepared.credentials.login_status,
            },
            "loginRequired": !token_present,
            "diagnosis": diagnosis,
            "endpointResults": [],
            "recognized": {
                "balanceLabel": Value::Null,
                "groupCount": 0,
                "rateCount": 0,
                "keyCount": 0,
                "matchedFieldCount": 0,
            },
            "stationName": prepared.station.name,
        }),
        normalized_json: json!({
            "stationId": station_id,
            "adapter": "login-state",
            "status": status,
            "balance": Value::Null,
            "groups": [],
            "rateMultipliers": [],
            "keys": [],
            "models": [],
            "matchedFields": [],
            "detectedEndpoints": [],
            "pendingConfirmations": [],
            "confidenceSummary": { "recognizedFieldCount": 0 },
        }),
        raw_json_redacted: Some(json!({
            "stationName": prepared.station.name,
            "loginUsernamePresent": !prepared.username.trim().is_empty(),
            "loginPasswordPresent": prepared.password.as_deref().is_some_and(|value| !value.trim().is_empty()),
        })),
        error_code: (!token_present).then(|| "login_action_required".to_string()),
        error_message: (!token_present).then_some(diagnosis),
    };
    PreparedStationCollection {
        station_id,
        endpoint_revision: prepared.station.endpoint_revision,
        adapter: "login-state".to_string(),
        task: CollectorTask::Detect,
        outputs: vec![output],
        enabled_key_count: 0,
    }
}

async fn apply_prepared_full_collection_v2(
    service: &CollectorService,
    port: &dyn CollectorApplyPort,
    prepared: PreparedStationCollection,
) -> Result<CollectorRunResult, ApplicationError> {
    let full_output = aggregate_full_output_v2(&prepared);
    let parent_outcome = collector_apply::apply_station_output_v2(
        port,
        prepared.station_id.clone(),
        prepared.endpoint_revision,
        None,
        full_output,
    )
    .await?;
    let mut events = Vec::with_capacity(prepared.outputs.len() + 1);
    for output in &prepared.outputs {
        let outcome = collector_apply::apply_station_output_v2(
            port,
            prepared.station_id.clone(),
            prepared.endpoint_revision,
            Some(parent_outcome.run_id.clone()),
            output.clone(),
        )
        .await?;
        let result = service
            .result_for_apply(&outcome, output.task.as_str())
            .await?;
        events.extend(result.events);
    }
    let parent_result = service.result_for_apply(&parent_outcome, "full").await?;
    events.extend(parent_result.events);
    Ok(CollectorRunResult {
        snapshot: parent_result.snapshot,
        events,
    })
}

fn aggregate_full_output_v2(prepared: &PreparedStationCollection) -> AdapterOutput {
    let status = aggregate_full_output_status(&prepared.outputs);
    let endpoint_results = prepared
        .outputs
        .iter()
        .flat_map(|output| {
            output
                .summary_json
                .get("endpointResults")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let child_runs = prepared
        .outputs
        .iter()
        .map(|output| {
            json!({
                "task": output.task.as_str(),
                "status": output.status,
                "errorCode": output.error_code,
            })
        })
        .collect::<Vec<_>>();
    let business =
        full_business_summary_from_outputs(&prepared.outputs, prepared.enabled_key_count);
    let manual_authorization_required = status == "manual_required";
    let error_message = if manual_authorization_required {
        Some(manual_authorization::MESSAGE.to_string())
    } else if status == "failed" {
        Some("all full collector child tasks failed".to_string())
    } else {
        None
    };

    AdapterOutput {
        adapter: prepared.adapter.clone(),
        task: CollectorTask::Full,
        status: status.clone(),
        facts: facts::CollectorFacts::default(),
        summary_json: json!({
            "adapter": prepared.adapter,
            "task": "full",
            "conclusion": conclusion_for_full_status(&status),
            "message": full_summary_message(&business),
            "manualActionRequired": manual_authorization_required,
            "recommendedAction": manual_authorization_required
                .then_some(manual_authorization::RECOMMENDED_ACTION),
            "childRuns": child_runs,
            "endpointResults": endpoint_results,
            "recognized": {
                "balanceLabel": business.balance_label,
                "groupCount": business.groups.len(),
                "rateCount": business.rate_multipliers.len(),
                "keyCount": business.key_count,
                "matchedFieldCount": business.matched_field_count(),
            },
        }),
        normalized_json: json!({
            "balance": business.balance_value,
            "balanceLabel": business.balance_label,
            "groups": business.groups,
            "rateMultipliers": business.rate_multipliers,
            "childRuns": child_runs,
        }),
        raw_json_redacted: None,
        error_code: if manual_authorization_required {
            Some(manual_authorization::ERROR_CODE.to_string())
        } else if status == "failed" {
            Some("all_child_tasks_failed".to_string())
        } else {
            None
        },
        error_message,
    }
}

fn aggregate_full_output_status(outputs: &[AdapterOutput]) -> String {
    // Published status is display-only for core station health, but its child
    // run remains part of the Full operation result shown to the user.
    let core_outputs = outputs
        .iter()
        .filter(|output| output.task != CollectorTask::PublishedStatus)
        .collect::<Vec<_>>();
    if core_outputs.is_empty() {
        return "failed".to_string();
    }
    let success = core_outputs
        .iter()
        .filter(|output| output.status == "success")
        .count();
    let partial = core_outputs
        .iter()
        .filter(|output| output.status == "partial")
        .count();
    let manual = core_outputs
        .iter()
        .filter(|output| output.status == "manual_required")
        .count();
    if success == core_outputs.len() {
        let published_status_failed = outputs.iter().any(|output| {
            output.task == CollectorTask::PublishedStatus && output.status != "success"
        });
        if published_status_failed {
            "partial".to_string()
        } else {
            "success".to_string()
        }
    } else if success > 0 || partial > 0 {
        "partial".to_string()
    } else if manual > 0 {
        "manual_required".to_string()
    } else {
        "failed".to_string()
    }
}

fn full_business_summary_from_outputs(
    outputs: &[AdapterOutput],
    key_count: usize,
) -> FullBusinessSummary {
    let balances = outputs.iter().flat_map(|output| &output.facts.balances);
    let latest_balance = balances
        .filter(|balance| balance.value.is_some())
        .max_by_key(|balance| balance.scope == "station");
    let balance_value = latest_balance
        .and_then(|balance| balance.value)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let balance_label = latest_balance.and_then(|balance| {
        balance
            .value
            .map(|value| format_balance_label(value, &balance.currency))
    });
    let groups = outputs
        .iter()
        .flat_map(|output| &output.facts.groups)
        .map(|group| {
            json!({
                "groupName": group.group_name,
                "status": group.visibility,
                "source": group.source,
            })
        })
        .collect();
    let rate_multipliers = outputs
        .iter()
        .flat_map(|output| &output.facts.rates)
        .filter_map(|rate| {
            rate.effective_rate_multiplier.map(|multiplier| {
                json!({
                    "groupName": rate.group_name,
                    "multiplier": multiplier,
                    "defaultRateMultiplier": rate.default_rate_multiplier,
                    "userRateMultiplier": rate.user_rate_multiplier,
                    "source": rate.source,
                    "checkedAt": rate.checked_at,
                })
            })
        })
        .collect();
    FullBusinessSummary {
        balance_value,
        balance_label,
        groups,
        rate_multipliers,
        key_count,
    }
}

#[derive(Debug, Clone)]
struct FullBusinessSummary {
    balance_value: Value,
    balance_label: Option<String>,
    groups: Vec<Value>,
    rate_multipliers: Vec<Value>,
    key_count: usize,
}

impl FullBusinessSummary {
    fn matched_field_count(&self) -> usize {
        usize::from(self.balance_label.is_some()) + self.groups.len() + self.rate_multipliers.len()
    }
}

fn conclusion_for_full_status(status: &str) -> &'static str {
    match status {
        "success" => "Collected",
        "partial" => "Partially collected",
        "manual_required" => "Manual action required",
        "failed" => "Failed",
        _ => "Checked",
    }
}

fn full_summary_message(summary: &FullBusinessSummary) -> String {
    let mut parts = Vec::new();
    if summary.balance_label.is_some() {
        parts.push("balance");
    }
    if !summary.groups.is_empty() {
        parts.push("groups");
    }
    if !summary.rate_multipliers.is_empty() {
        parts.push("rates");
    }
    if parts.is_empty() {
        "Full collection completed without displayable business facts.".to_string()
    } else {
        format!("Full collection recognized {}.", parts.join(", "))
    }
}
fn format_balance_label(value: f64, currency: &str) -> String {
    let mut amount = format!("{value:.6}");
    while amount.contains('.') && amount.ends_with('0') {
        amount.pop();
    }
    if amount.ends_with('.') {
        amount.pop();
    }
    let currency = currency.trim();
    if currency.is_empty() {
        amount
    } else {
        format!("{amount} {currency}")
    }
}

fn manual_required_output_for_adapter(
    adapter: &str,
    task: CollectorTask,
    message: &str,
) -> AdapterOutput {
    AdapterOutput {
        adapter: adapter.to_string(),
        task,
        status: "manual_required".to_string(),
        facts: facts::CollectorFacts::default(),
        summary_json: json!({
            "adapter": adapter,
            "task": task.as_str(),
            "message": message,
            "manualActionRequired": true,
            "recommendedAction": manual_authorization::RECOMMENDED_ACTION,
        }),
        normalized_json: json!({ "models": [] }),
        raw_json_redacted: None,
        error_code: Some(manual_authorization::ERROR_CODE.to_string()),
        error_message: Some(message.to_string()),
    }
}

fn driver_output_to_adapter_output(
    adapter: &str,
    task: CollectorTask,
    output: contract::DriverOutput,
) -> AdapterOutput {
    let endpoint_results = output
        .evidence
        .iter()
        .map(endpoint_evidence_json)
        .collect::<Vec<_>>();
    let balance_count = output.facts.balances.len();
    let group_count = output.facts.groups.len();
    let rate_count = output.facts.rates.len();
    let published_status_count = output
        .facts
        .published_status
        .as_ref()
        .map(|batch| batch.monitors.len())
        .unwrap_or_default();
    let groups = output
        .facts
        .groups
        .iter()
        .map(|group| {
            json!({
                "groupId": group.group_id,
                "groupIdHash": group.group_key_hash,
                "groupName": group.group_name,
                "status": group.visibility,
                "source": group.source,
            })
        })
        .collect::<Vec<_>>();
    let rate_multipliers = output
        .facts
        .rates
        .iter()
        .map(|rate| {
            json!({
                "groupId": rate.group_id,
                "groupIdHash": rate.group_key_hash,
                "groupName": rate.group_name,
                "effectiveRateMultiplier": rate.effective_rate_multiplier,
                "defaultRateMultiplier": rate.default_rate_multiplier,
                "userRateMultiplier": rate.user_rate_multiplier,
                "source": rate.source,
            })
        })
        .collect::<Vec<_>>();
    AdapterOutput {
        adapter: adapter.to_string(),
        task,
        status: match output.status {
            contract::DriverOutputStatus::Success => "success",
            contract::DriverOutputStatus::Partial => "partial",
            contract::DriverOutputStatus::ManualRequired => "manual_required",
        }
        .to_string(),
        facts: output.facts,
        summary_json: json!({
            "adapter": adapter,
            "task": task.as_str(),
            "endpointResults": endpoint_results,
            "balanceCount": balance_count,
            "groupCount": group_count,
            "rateCount": rate_count,
            "publishedStatusMonitorCount": published_status_count,
            "modelCount": 0,
        }),
        normalized_json: json!({
            "balanceCount": balance_count,
            "groupCount": group_count,
            "rateCount": rate_count,
            "publishedStatusMonitorCount": published_status_count,
            "groups": groups,
            "rateMultipliers": rate_multipliers,
            "models": [],
        }),
        raw_json_redacted: output.diagnostics.raw_json_redacted,
        error_code: None,
        error_message: None,
    }
}

fn driver_failure_to_adapter_output(
    adapter: &str,
    task: CollectorTask,
    failure: failure::DriverFailure,
) -> AdapterOutput {
    let manual_authorization_required = failure.auth_effect == failure::AuthEffect::Reauthorize;
    let diagnostic_message = failure
        .sanitized_detail
        .clone()
        .unwrap_or_else(|| format!("{:?}", failure.kind));
    let message = if manual_authorization_required {
        manual_authorization::MESSAGE.to_string()
    } else {
        diagnostic_message.clone()
    };
    let endpoint_results = failure
        .evidence
        .entries()
        .iter()
        .map(endpoint_evidence_json)
        .collect::<Vec<_>>();
    AdapterOutput {
        adapter: adapter.to_string(),
        task,
        status: if manual_authorization_required {
            "manual_required"
        } else {
            "failed"
        }
        .to_string(),
        facts: facts::CollectorFacts::default(),
        summary_json: json!({
            "adapter": adapter,
            "task": task.as_str(),
            "endpointResults": endpoint_results,
            "message": message,
            "diagnosticMessage": manual_authorization_required.then_some(diagnostic_message),
            "manualActionRequired": manual_authorization_required,
            "recommendedAction": manual_authorization_required
                .then_some(manual_authorization::RECOMMENDED_ACTION),
        }),
        normalized_json: json!({ "models": [] }),
        raw_json_redacted: None,
        error_code: Some(if manual_authorization_required {
            manual_authorization::ERROR_CODE.to_string()
        } else {
            driver_failure_code(failure.kind).to_string()
        }),
        error_message: Some(message),
    }
}

fn endpoint_evidence_json(evidence: &evidence::EndpointEvidence) -> Value {
    json!({
        "role": format!("{:?}", evidence.role),
        "url": evidence.url,
        "status": evidence.status_code,
        "ok": evidence.status_code.is_some_and(|status| (200..400).contains(&status)),
        "method": evidence.method,
        "detail": evidence.detail,
    })
}

fn driver_failure_code(kind: failure::DriverFailureKind) -> &'static str {
    match kind {
        failure::DriverFailureKind::Unsupported => "unsupported_task",
        failure::DriverFailureKind::InvalidRequest => "invalid_request",
        failure::DriverFailureKind::AuthRejected => "auth_rejected",
        failure::DriverFailureKind::BrowserContextRequired => "browser_context_required",
        failure::DriverFailureKind::RateLimited => "rate_limited",
        failure::DriverFailureKind::Timeout => "network_timeout",
        failure::DriverFailureKind::BudgetExhausted => "budget_exhausted",
        failure::DriverFailureKind::Cancelled => "cancelled",
        failure::DriverFailureKind::ResultUnknown => "result_unknown",
        failure::DriverFailureKind::Transport => "network_error",
        failure::DriverFailureKind::MalformedPayload => "malformed_payload",
        failure::DriverFailureKind::ProviderUnavailable => "provider_unavailable",
        failure::DriverFailureKind::Internal => "internal",
    }
}

fn collector_task_kind(task: CollectorTask) -> Option<contract::CollectorTaskKind> {
    match task {
        CollectorTask::Balance => Some(contract::CollectorTaskKind::Balance),
        CollectorTask::Groups => Some(contract::CollectorTaskKind::Groups),
        CollectorTask::PublishedStatus => Some(contract::CollectorTaskKind::PublishedStatus),
        CollectorTask::Detect => Some(contract::CollectorTaskKind::Detect),
        CollectorTask::Full => None,
    }
}

fn proxy_policy_from_collector_config(
    proxy: crate::services::outbound::ProxyConfig,
) -> Result<ProxyPolicy, String> {
    let system_proxy_url = (proxy.mode == "system")
        .then(crate::services::outbound::current_system_proxy_url)
        .flatten();
    proxy_policy_from_collector_config_with_system_proxy(proxy, system_proxy_url.as_deref())
}

fn proxy_policy_from_collector_config_with_system_proxy(
    proxy: crate::services::outbound::ProxyConfig,
    system_proxy_url: Option<&str>,
) -> Result<ProxyPolicy, String> {
    match proxy.mode.as_str() {
        "direct" => Ok(ProxyPolicy::Direct),
        "system" => match system_proxy_url {
            Some(url) => ManualProxy::parse(url)
                .map(ProxyPolicy::Manual)
                .map_err(|error| crate::services::secrets::mask::redact_text(&error.to_string())),
            None => Ok(ProxyPolicy::System),
        },
        "manual" => {
            let Some(url) = proxy.url.as_deref() else {
                return Err("鎵嬪姩閲囬泦浠ｇ悊鍦板潃涓嶈兘涓虹┖".to_string());
            };
            ManualProxy::parse(url)
                .map(ProxyPolicy::Manual)
                .map_err(|error| crate::services::secrets::mask::redact_text(&error.to_string()))
        }
        _ => Ok(ProxyPolicy::Direct),
    }
}

fn provider_kind_for_station_type(station_type: &str) -> Result<contract::ProviderKind, String> {
    match station_type.trim() {
        "sub2api" => Ok(contract::ProviderKind::Sub2Api),
        "newapi" => Ok(contract::ProviderKind::NewApi),
        other => Err(format!("unsupported station type: {other}")),
    }
}

fn full_child_tasks(provider: contract::ProviderKind) -> Vec<CollectorTask> {
    drivers::static_provider_entries()
        .into_iter()
        .find(|entry| entry.descriptor.kind == provider)
        .and_then(|entry| entry.descriptor.capabilities.collector)
        .map(|capability| {
            capability
                .full_tasks
                .iter()
                .copied()
                .map(collector_task_from_kind)
                .collect()
        })
        .unwrap_or_default()
}

fn collector_task_from_kind(task: contract::CollectorTaskKind) -> CollectorTask {
    match task {
        contract::CollectorTaskKind::Balance => CollectorTask::Balance,
        contract::CollectorTaskKind::Groups => CollectorTask::Groups,
        contract::CollectorTaskKind::PublishedStatus => CollectorTask::PublishedStatus,
        contract::CollectorTaskKind::Detect => CollectorTask::Detect,
    }
}
pub(crate) async fn test_station_login_input_async(
    outbound: &AsyncOutboundClient,
    input: StationLoginTestInput,
    cancellation_token: CancellationToken,
    correlation_id: Option<String>,
) -> Result<StationLoginTestResult, String> {
    login_probe::test_station_login_input(outbound, input, cancellation_token, correlation_id).await
}

fn has_login_credentials(username: &Option<String>, password_present: bool) -> bool {
    username
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        && password_present
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_parent_keeps_canonical_facts_empty() {
        let prepared = PreparedStationCollection {
            station_id: "station-1".to_string(),
            endpoint_revision: 7,
            adapter: "newapi".to_string(),
            task: CollectorTask::Full,
            outputs: vec![AdapterOutput {
                adapter: "newapi".to_string(),
                task: CollectorTask::Balance,
                status: "success".to_string(),
                facts: facts::CollectorFacts {
                    balances: vec![facts::CollectedBalanceFact {
                        station_id: "station-1".to_string(),
                        station_key_id: None,
                        scope: "station".to_string(),
                        value: Some(12.0),
                        used_value: None,
                        total_value: None,
                        today_request_count: None,
                        total_request_count: None,
                        today_consumption: None,
                        total_consumption: None,
                        today_base_consumption: None,
                        total_base_consumption: None,
                        today_token_count: None,
                        total_token_count: None,
                        today_input_token_count: None,
                        today_output_token_count: None,
                        total_input_token_count: None,
                        total_output_token_count: None,
                        account_concurrency_limit: None,
                        currency: "USD".to_string(),
                        credit_unit: None,
                        status: "normal".to_string(),
                        source: "test".to_string(),
                        confidence: 1.0,
                        collected_at: None,
                    }],
                    ..facts::CollectorFacts::default()
                },
                summary_json: json!({"endpointResults": []}),
                normalized_json: json!({}),
                raw_json_redacted: None,
                error_code: None,
                error_message: None,
            }],
            enabled_key_count: 1,
        };
        let full = aggregate_full_output_v2(&prepared);

        assert_eq!(full.task, CollectorTask::Full);
        assert!(full.facts.balances.is_empty());
        assert!(full.facts.groups.is_empty());
        assert!(full.facts.rates.is_empty());
    }

    #[test]
    fn reauthorization_effect_maps_to_stable_manual_required_output() {
        let failure = failure::DriverFailure::reauthorization_required(
            failure::FailedEndpoint {
                role: evidence::EndpointRole::Authorization,
                status_code: Some(403),
            },
            "interactive authorization is required",
        );

        let output = driver_failure_to_adapter_output("sub2api", CollectorTask::Groups, failure);

        assert_eq!(output.status, "manual_required");
        assert_eq!(
            output.error_code.as_deref(),
            Some("manual_authorization_required")
        );
        assert_eq!(output.summary_json["manualActionRequired"], json!(true));
        assert_eq!(
            output.summary_json["recommendedAction"],
            json!("reauthorize")
        );
    }

    #[test]
    fn unsupported_capability_is_not_misreported_as_manual_authorization() {
        let output = driver_failure_to_adapter_output(
            "sub2api",
            CollectorTask::Groups,
            failure::DriverFailure::unsupported("groups are unsupported"),
        );

        assert_eq!(output.status, "failed");
        assert_eq!(output.error_code.as_deref(), Some("unsupported_task"));
        assert_eq!(output.summary_json["manualActionRequired"], json!(false));
    }

    #[test]
    fn full_parent_preserves_manual_authorization_when_other_children_fail() {
        let prepared = PreparedStationCollection {
            station_id: "station-1".to_string(),
            endpoint_revision: 7,
            adapter: "sub2api".to_string(),
            task: CollectorTask::Full,
            outputs: vec![
                manual_required_output_for_adapter(
                    "sub2api",
                    CollectorTask::Groups,
                    "interactive authorization detected",
                ),
                AdapterOutput {
                    adapter: "sub2api".to_string(),
                    task: CollectorTask::Balance,
                    status: "failed".to_string(),
                    facts: facts::CollectorFacts::default(),
                    summary_json: json!({"endpointResults": []}),
                    normalized_json: json!({}),
                    raw_json_redacted: None,
                    error_code: Some("provider_unavailable".to_string()),
                    error_message: Some("provider unavailable".to_string()),
                },
            ],
            enabled_key_count: 1,
        };

        let output = aggregate_full_output_v2(&prepared);

        assert_eq!(output.status, "manual_required");
        assert_eq!(
            output.error_code.as_deref(),
            Some(manual_authorization::ERROR_CODE)
        );
        assert_eq!(
            output.summary_json["recommendedAction"],
            json!("reauthorize")
        );
    }

    #[test]
    fn collector_task_kind_rejects_parent_task() {
        assert_eq!(
            collector_task_kind(CollectorTask::Detect),
            Some(contract::CollectorTaskKind::Detect)
        );
        assert_eq!(collector_task_kind(CollectorTask::Full), None);
    }

    #[test]
    fn login_requires_username_and_password() {
        assert!(!has_login_credentials(&None, false));
        assert!(!has_login_credentials(
            &Some("user@example.com".to_string()),
            false,
        ));
        assert!(!has_login_credentials(&None, true));
        assert!(has_login_credentials(
            &Some("user@example.com".to_string()),
            true,
        ));
    }

    #[test]
    fn newapi_collection_can_fall_back_to_saved_password_login() {
        let login = prepare_newapi_password_login(
            Some("  user@example.com  ".to_string()),
            true,
            Some("saved-password".to_string()),
        )
        .expect("saved login credentials should be usable");

        assert_eq!(login.username, "user@example.com");
        assert_eq!(login.password, "saved-password");
        assert!(
            prepare_newapi_password_login(Some("user@example.com".to_string()), true, None,)
                .is_none()
        );
        assert!(prepare_newapi_password_login(
            Some("user@example.com".to_string()),
            false,
            Some("saved-password".to_string()),
        )
        .is_none());
    }

    #[test]
    fn full_tasks_are_bounded_by_provider_capability() {
        assert_eq!(
            full_child_tasks(contract::ProviderKind::NewApi),
            vec![CollectorTask::Balance, CollectorTask::Groups],
        );
        assert_eq!(
            full_child_tasks(contract::ProviderKind::Sub2Api),
            vec![
                CollectorTask::Balance,
                CollectorTask::Groups,
                CollectorTask::PublishedStatus,
            ],
        );
    }

    #[test]
    fn full_parent_surfaces_published_status_child_failures_as_partial() {
        let output = |task: CollectorTask, status: &str| AdapterOutput {
            adapter: "sub2api".to_string(),
            task,
            status: status.to_string(),
            facts: facts::CollectorFacts::default(),
            summary_json: json!({"endpointResults": []}),
            normalized_json: json!({}),
            raw_json_redacted: None,
            error_code: None,
            error_message: None,
        };
        let prepared = PreparedStationCollection {
            station_id: "station-1".to_string(),
            endpoint_revision: 7,
            adapter: "sub2api".to_string(),
            task: CollectorTask::Full,
            outputs: vec![
                output(CollectorTask::Balance, "success"),
                output(CollectorTask::Groups, "success"),
                output(CollectorTask::PublishedStatus, "manual_required"),
            ],
            enabled_key_count: 1,
        };

        let full = aggregate_full_output_v2(&prepared);

        assert_eq!(full.status, "partial");
        assert_eq!(
            full.summary_json["childRuns"],
            json!([
                {"task": "balance", "status": "success", "errorCode": null},
                {"task": "groups", "status": "success", "errorCode": null},
                {"task": "published_status", "status": "manual_required", "errorCode": null},
            ])
        );
    }

    #[test]
    fn old_openai_provider_station_types_are_rejected() {
        assert!(provider_kind_for_station_type("openai-compatible").is_err());
        assert!(provider_kind_for_station_type("openai_compatible").is_err());
        assert!(provider_kind_for_station_type("custom").is_err());
    }

    #[test]
    fn configured_system_proxy_is_materialized_for_collector_transport() {
        let policy = proxy_policy_from_collector_config_with_system_proxy(
            crate::services::outbound::ProxyConfig {
                mode: "system".to_string(),
                url: None,
            },
            Some("http://127.0.0.1:7890"),
        )
        .expect("system proxy should be valid");

        assert!(matches!(
            policy,
            ProxyPolicy::Manual(ManualProxy { endpoint, .. })
                if endpoint == "http://127.0.0.1:7890"
        ));
    }

    #[test]
    fn missing_system_proxy_preserves_system_transport_fallback() {
        let policy = proxy_policy_from_collector_config_with_system_proxy(
            crate::services::outbound::ProxyConfig {
                mode: "system".to_string(),
                url: None,
            },
            None,
        )
        .expect("system fallback should remain valid");

        assert_eq!(policy, ProxyPolicy::System);
    }
}
