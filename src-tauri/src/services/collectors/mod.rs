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
        credentials::{
            PersistStationSessionInput, ResolvedSession, StationCredentials,
            StationSessionCredentialKind, UpdateStationSessionInput,
        },
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

const SUB2API_CHILD_TASK_TIMEOUT: Duration = Duration::from_secs(30);
const NEWAPI_CHILD_TASK_TIMEOUT: Duration = Duration::from_secs(20);

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
    fn update_station_session(
        &self,
        input: UpdateStationSessionInput,
        expected_revision: i64,
    ) -> Result<StationCredentials, String>;
    fn persist_station_session<'a>(
        &'a self,
        input: PersistStationSessionInput,
        expected_revision: i64,
    ) -> BoxFuture<'a, Result<StationCredentials, String>>;
    fn invalidate_station_session_credential(
        &self,
        station_id: &str,
        kind: StationSessionCredentialKind,
    ) -> Result<(), String>;
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

    fn update_station_session(
        &self,
        input: UpdateStationSessionInput,
        expected_revision: i64,
    ) -> Result<StationCredentials, String> {
        tauri::async_runtime::block_on(
            self.credentials
                .update_station_session_if_revision(input, expected_revision),
        )
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

    fn invalidate_station_session_credential(
        &self,
        station_id: &str,
        kind: StationSessionCredentialKind,
    ) -> Result<(), String> {
        tauri::async_runtime::block_on(
            self.credentials
                .invalidate_station_session_credential(station_id.to_string(), kind),
        )
        .map_err(application_error)
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
    OpenAiCompatible(PreparedOpenAiCompatibleCollection),
    NewApi(PreparedNewApiCollection),
}

pub(crate) enum PreparedStationTaskRoute {
    Sub2Api(PreparedSub2ApiCollection),
    OpenAiCompatible(PreparedOpenAiCompatibleCollection),
    NewApi(PreparedNewApiCollection),
}

pub(crate) enum PreparedOpenAiCompatibleCollection {
    Immediate(PreparedStationCollection),
    Driver(PreparedOpenAiCompatibleDriverCollection),
}

pub(crate) struct PreparedOpenAiCompatibleDriverCollection {
    station_id: String,
    endpoint_revision: i64,
    task: CollectorTask,
    output_task: CollectorTask,
    driver_task: contract::CollectorTaskKind,
    enabled_key_count: usize,
    api_base_url: String,
    website_url: Option<String>,
    proxy: ProxyPolicy,
    credential_handle: contract::OpaqueCredentialHandle,
    secret_accessor: StaticSecretAccessor,
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
    credential_handle: contract::OpaqueCredentialHandle,
    auth_context: contract::ProviderAuthContext,
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
        contract::ProviderKind::OpenAiCompatible => {
            prepare_openai_compatible_collection_v2(source, station, task)
                .map(PreparedStationCollectionRoute::OpenAiCompatible)
        }
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
        contract::ProviderKind::OpenAiCompatible => {
            prepare_openai_compatible_collection_v2(source, station, task)
                .map(PreparedStationTaskRoute::OpenAiCompatible)
        }
    }
}

fn prepare_openai_compatible_collection_v2(
    source: &dyn CollectorSourcePort,
    station: Station,
    task: CollectorTask,
) -> Result<PreparedOpenAiCompatibleCollection, ApplicationError> {
    let station_id = station.id.clone();
    let tasks = if task == CollectorTask::Full {
        vec![CollectorTask::Models]
    } else {
        vec![task]
    };
    if tasks.len() != 1 {
        return Err(ApplicationError::ConstraintViolation);
    }
    let child_task = tasks[0];
    if !matches!(child_task, CollectorTask::Detect | CollectorTask::Models) {
        return Ok(PreparedOpenAiCompatibleCollection::Immediate(
            prepared_openai_immediate_collection(
                station_id,
                station.endpoint_revision,
                task,
                child_task,
                manual_required_output(
                    child_task,
                    "unsupported_task",
                    "OpenAI-compatible 站点不支持该采集能力。",
                ),
                0,
            ),
        ));
    }
    let keys = source
        .list_station_keys(station_id.clone())
        .map_err(|_| ApplicationError::Internal)?;
    let enabled_key_count = keys.iter().filter(|key| key.enabled).count();
    let Some(key) = keys
        .into_iter()
        .find(|key| key.enabled && key.api_key_present)
    else {
        return Ok(PreparedOpenAiCompatibleCollection::Immediate(
            prepared_openai_immediate_collection(
                station_id,
                station.endpoint_revision,
                task,
                child_task,
                manual_required_output(
                    child_task,
                    "api_key_required",
                    "模型采集需要可用 API Key。",
                ),
                enabled_key_count,
            ),
        ));
    };
    let api_key = match source.resolve_station_key_secret(&key.id) {
        Ok(api_key) => api_key,
        Err(error) => {
            return Ok(PreparedOpenAiCompatibleCollection::Immediate(
                prepared_openai_immediate_collection(
                    station_id,
                    station.endpoint_revision,
                    task,
                    child_task,
                    manual_required_output(
                        child_task,
                        "api_key_required",
                        &format!(
                            "API Key 不可解密：{}",
                            crate::services::secrets::mask::redact_text(&error)
                        ),
                    ),
                    enabled_key_count,
                ),
            ));
        }
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
        scope: contract::CredentialScope::StationKey,
    };
    let driver_task =
        collector_task_kind(child_task).ok_or(ApplicationError::ConstraintViolation)?;
    Ok(PreparedOpenAiCompatibleCollection::Driver(
        PreparedOpenAiCompatibleDriverCollection {
            station_id: station_id.clone(),
            endpoint_revision: station.endpoint_revision,
            task,
            output_task: child_task,
            driver_task,
            enabled_key_count,
            api_base_url: station.api_base_url,
            website_url: (!station.website_url.trim().is_empty()).then_some(station.website_url),
            proxy,
            credential_handle: credential_handle.clone(),
            secret_accessor: StaticSecretAccessor {
                expected: credential_handle,
                purpose: contract::CredentialSecretPurpose::AuthorizationHeader,
                secret: api_key,
            },
        },
    ))
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
                purpose: contract::CredentialSecretPurpose::SessionCookie,
                secret: token,
            });
            handle
        });

    let credentials = source
        .get_station_credentials(station_id.clone())
        .map_err(|_| ApplicationError::Internal)?;
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
            credential_handle,
            auth_context: contract::ProviderAuthContext::Sub2Api {
                station_keys: station_key_credentials,
                access_token: access_token_handle,
                login,
                credit_per_cny: station.credit_per_cny,
            },
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
                            manual_required_output_for_adapter(
                                "newapi",
                                child_task,
                                "manual_session_required",
                                &message,
                            )
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

pub(crate) async fn finish_openai_compatible_collection_v2(
    registry: &orchestration::ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedOpenAiCompatibleCollection,
    cancellation_token: CancellationToken,
    correlation_id: Option<String>,
) -> Result<PreparedStationCollection, ApplicationError> {
    match prepared {
        PreparedOpenAiCompatibleCollection::Immediate(prepared) => Ok(prepared),
        PreparedOpenAiCompatibleCollection::Driver(prepared) => {
            let driver = registry
                .collector(contract::ProviderKind::OpenAiCompatible)
                .map_err(|_| ApplicationError::ConstraintViolation)?;
            let context = contract::CollectorContext {
                station: contract::StationIdentity {
                    station_id: prepared.station_id.clone(),
                    endpoint_revision: prepared.endpoint_revision,
                    provider: contract::ProviderKind::OpenAiCompatible,
                },
                endpoints: contract::ProviderEndpoints {
                    api_base_url: Some(prepared.api_base_url),
                    website_url: prepared.website_url,
                },
                credential: prepared.credential_handle,
                auth: None,
                secrets: &prepared.secret_accessor,
                outbound,
                proxy: prepared.proxy,
                budget: RequestBudget::from_now(Duration::from_secs(20)),
                cancellation: cancellation_token,
                correlation_id: correlation_id.unwrap_or_else(|| "station-collection".to_string()),
            };
            let output = driver
                .collect(&context, prepared.driver_task)
                .await
                .map(|output| {
                    driver_output_to_adapter_output(
                        "openai-compatible",
                        prepared.output_task,
                        output,
                    )
                })
                .unwrap_or_else(|failure| {
                    driver_failure_to_adapter_output(
                        "openai-compatible",
                        prepared.output_task,
                        failure,
                    )
                });
            Ok(PreparedStationCollection {
                station_id: prepared.station_id,
                endpoint_revision: prepared.endpoint_revision,
                adapter: "openai-compatible".to_string(),
                task: prepared.task,
                outputs: vec![output],
                enabled_key_count: prepared.enabled_key_count,
            })
        }
    }
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
            let mut context = contract::CollectorContext {
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
                secrets: &prepared.secret_accessor,
                outbound,
                proxy: prepared.proxy,
                budget: RequestBudget::from_now(SUB2API_CHILD_TASK_TIMEOUT),
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
                renew_child_task_budget(&mut context.budget, SUB2API_CHILD_TASK_TIMEOUT);
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

pub(crate) async fn finish_openai_compatible_task_v2(
    registry: &orchestration::ProviderRegistry,
    outbound: &AsyncOutboundClient,
    prepared: PreparedOpenAiCompatibleCollection,
    cancellation_token: CancellationToken,
    correlation_id: Option<String>,
) -> Result<(String, i64, AdapterOutput), ApplicationError> {
    let prepared = finish_openai_compatible_collection_v2(
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
            let mut context = contract::CollectorContext {
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
                secrets: &prepared.secret_accessor,
                outbound,
                proxy: prepared.proxy,
                budget: RequestBudget::from_now(NEWAPI_CHILD_TASK_TIMEOUT),
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
                renew_child_task_budget(&mut context.budget, NEWAPI_CHILD_TASK_TIMEOUT);
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
        .map(|child_task| {
            manual_required_output_for_adapter(
                "newapi",
                child_task,
                "manual_session_required",
                &message,
            )
        })
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
    let mut models = Vec::<String>::new();
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
        for model in output.facts.models {
            if model.available && !models.contains(&model.model) {
                models.push(model.model);
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
        models,
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
            })
        })
        .collect::<Vec<_>>();
    let business =
        full_business_summary_from_outputs(&prepared.outputs, prepared.enabled_key_count);
    let models = prepared
        .outputs
        .iter()
        .flat_map(|output| output.facts.models.iter().cloned())
        .collect::<Vec<_>>();
    let error_message =
        (status == "failed").then(|| "all full collector child tasks failed".to_string());

    AdapterOutput {
        adapter: prepared.adapter.clone(),
        task: CollectorTask::Full,
        status: status.clone(),
        facts: facts::CollectorFacts {
            models,
            ..facts::CollectorFacts::default()
        },
        summary_json: json!({
            "adapter": prepared.adapter,
            "task": "full",
            "conclusion": conclusion_for_full_status(&status),
            "message": full_summary_message(&business),
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
            "models": business.models,
            "childRuns": child_runs,
        }),
        raw_json_redacted: None,
        error_code: (status == "failed").then(|| "all_child_tasks_failed".to_string()),
        error_message,
    }
}

fn aggregate_full_output_status(outputs: &[AdapterOutput]) -> String {
    let success = outputs
        .iter()
        .filter(|output| output.status == "success")
        .count();
    let partial = outputs
        .iter()
        .filter(|output| output.status == "partial")
        .count();
    let manual = outputs
        .iter()
        .filter(|output| output.status == "manual_required")
        .count();
    if success == outputs.len() {
        "success".to_string()
    } else if success > 0 || partial > 0 {
        "partial".to_string()
    } else if manual == outputs.len() {
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
    let models = outputs
        .iter()
        .flat_map(|output| &output.facts.models)
        .filter(|model| model.available)
        .map(|model| model.model.clone())
        .collect();
    FullBusinessSummary {
        balance_value,
        balance_label,
        groups,
        rate_multipliers,
        models,
        key_count,
    }
}

#[derive(Debug, Clone)]
struct FullBusinessSummary {
    balance_value: Value,
    balance_label: Option<String>,
    groups: Vec<Value>,
    rate_multipliers: Vec<Value>,
    models: Vec<String>,
    key_count: usize,
}

impl FullBusinessSummary {
    fn matched_field_count(&self) -> usize {
        usize::from(self.balance_label.is_some())
            + self.groups.len()
            + self.rate_multipliers.len()
            + self.models.len()
    }
}

fn conclusion_for_full_status(status: &str) -> &'static str {
    match status {
        "success" => "已采集",
        "partial" => "部分采集",
        "manual_required" => "需要登录",
        "failed" => "失败",
        _ => "已检查",
    }
}

fn full_summary_message(summary: &FullBusinessSummary) -> String {
    let mut parts = Vec::new();
    if summary.balance_label.is_some() {
        parts.push("余额");
    }
    if !summary.groups.is_empty() {
        parts.push("分组");
    }
    if !summary.rate_multipliers.is_empty() {
        parts.push("倍率");
    }
    if !summary.models.is_empty() {
        parts.push("模型");
    }

    if parts.is_empty() {
        "Full 采集已完成，但暂未识别到可展示的业务字段。".to_string()
    } else {
        format!("Full 采集已识别{}。", parts.join("、"))
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

fn prepared_openai_immediate_collection(
    station_id: String,
    endpoint_revision: i64,
    task: CollectorTask,
    child_task: CollectorTask,
    output: AdapterOutput,
    enabled_key_count: usize,
) -> PreparedStationCollection {
    PreparedStationCollection {
        station_id,
        endpoint_revision,
        adapter: "openai-compatible".to_string(),
        task,
        outputs: vec![AdapterOutput {
            task: child_task,
            ..output
        }],
        enabled_key_count,
    }
}

fn manual_required_output(task: CollectorTask, code: &str, message: &str) -> AdapterOutput {
    manual_required_output_for_adapter("openai-compatible", task, code, message)
}

fn manual_required_output_for_adapter(
    adapter: &str,
    task: CollectorTask,
    code: &str,
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
        }),
        normalized_json: json!({ "models": [] }),
        raw_json_redacted: None,
        error_code: Some(code.to_string()),
        error_message: Some(message.to_string()),
    }
}

fn driver_output_to_adapter_output(
    adapter: &str,
    task: CollectorTask,
    output: contract::DriverOutput,
) -> AdapterOutput {
    let model_names = output
        .facts
        .models
        .iter()
        .map(|model| model.model.clone())
        .collect::<Vec<_>>();
    let endpoint_results = output
        .evidence
        .iter()
        .map(endpoint_evidence_json)
        .collect::<Vec<_>>();
    let balance_count = output.facts.balances.len();
    let group_count = output.facts.groups.len();
    let rate_count = output.facts.rates.len();
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
            "modelCount": model_names.len(),
        }),
        normalized_json: json!({
            "balanceCount": balance_count,
            "groupCount": group_count,
            "rateCount": rate_count,
            "groups": groups,
            "rateMultipliers": rate_multipliers,
            "models": model_names,
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
    let message = failure
        .sanitized_detail
        .clone()
        .unwrap_or_else(|| format!("{:?}", failure.kind));
    let endpoint_results = failure
        .evidence
        .entries()
        .iter()
        .map(endpoint_evidence_json)
        .collect::<Vec<_>>();
    AdapterOutput {
        adapter: adapter.to_string(),
        task,
        status: match failure.kind {
            failure::DriverFailureKind::Unsupported => "manual_required",
            _ => "failed",
        }
        .to_string(),
        facts: facts::CollectorFacts::default(),
        summary_json: json!({
            "adapter": adapter,
            "task": task.as_str(),
            "endpointResults": endpoint_results,
            "message": message,
        }),
        normalized_json: json!({ "models": [] }),
        raw_json_redacted: None,
        error_code: Some(driver_failure_code(failure.kind).to_string()),
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
        CollectorTask::Models => Some(contract::CollectorTaskKind::Models),
        CollectorTask::Full => Some(contract::CollectorTaskKind::Full),
        CollectorTask::Detect => Some(contract::CollectorTaskKind::Detect),
    }
}

fn proxy_policy_from_collector_config(
    proxy: crate::services::outbound::ProxyConfig,
) -> Result<ProxyPolicy, String> {
    match proxy.mode.as_str() {
        "direct" => Ok(ProxyPolicy::Direct),
        "system" => Ok(ProxyPolicy::System),
        "manual" => {
            let Some(url) = proxy.url.as_deref() else {
                return Err("手动采集代理地址不能为空".to_string());
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
        "openai-compatible" | "openai_compatible" | "custom" => {
            Ok(contract::ProviderKind::OpenAiCompatible)
        }
        other => Err(format!("不支持的站点类型: {other}")),
    }
}

fn full_child_tasks(provider: contract::ProviderKind) -> Vec<CollectorTask> {
    match provider {
        contract::ProviderKind::NewApi => vec![
            CollectorTask::Balance,
            CollectorTask::Groups,
            CollectorTask::Models,
        ],
        contract::ProviderKind::Sub2Api => vec![CollectorTask::Balance, CollectorTask::Groups],
        contract::ProviderKind::OpenAiCompatible => vec![CollectorTask::Models],
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

fn renew_child_task_budget(budget: &mut RequestBudget, timeout: Duration) {
    *budget = RequestBudget::from_now(timeout);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_driver_output_keeps_child_task_for_full_parent() {
        let output = driver_output_to_adapter_output(
            "openai-compatible",
            CollectorTask::Models,
            contract::DriverOutput {
                facts: facts::CollectorFacts {
                    models: vec![facts::CollectedModelFact {
                        station_id: "station-1".to_string(),
                        model: "gpt-4o-mini".to_string(),
                        available: true,
                        source: "openai_models".to_string(),
                        confidence: 0.9,
                    }],
                    ..facts::CollectorFacts::default()
                },
                evidence: Vec::new(),
                status: contract::DriverOutputStatus::Success,
                diagnostics: contract::RedactedDiagnostics {
                    summary: None,
                    raw_json_redacted: None,
                },
            },
        );

        let prepared = PreparedStationCollection {
            station_id: "station-1".to_string(),
            endpoint_revision: 7,
            adapter: "openai-compatible".to_string(),
            task: CollectorTask::Full,
            outputs: vec![output],
            enabled_key_count: 1,
        };
        let full = aggregate_full_output_v2(&prepared);

        assert_eq!(prepared.outputs[0].task, CollectorTask::Models);
        assert_eq!(full.task, CollectorTask::Full);
        assert_eq!(
            full.normalized_json["childRuns"][0]["task"],
            CollectorTask::Models.as_str()
        );
        assert_eq!(full.normalized_json["models"][0], "gpt-4o-mini");
    }

    #[test]
    fn collector_task_kind_preserves_detect_and_models_for_openai_driver() {
        assert_eq!(
            collector_task_kind(CollectorTask::Detect),
            Some(contract::CollectorTaskKind::Detect)
        );
        assert_eq!(
            collector_task_kind(CollectorTask::Models),
            Some(contract::CollectorTaskKind::Models)
        );
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
            vec![
                CollectorTask::Balance,
                CollectorTask::Groups,
                CollectorTask::Models,
            ],
        );
        assert_eq!(
            full_child_tasks(contract::ProviderKind::OpenAiCompatible),
            vec![CollectorTask::Models],
        );
    }

    #[test]
    fn full_collection_renews_request_budget_for_each_child_task() {
        let mut budget = RequestBudget::from_deadline(std::time::Instant::now());
        assert!(budget.remaining().is_none());

        renew_child_task_budget(&mut budget, Duration::from_secs(20));

        assert!(budget
            .remaining()
            .is_some_and(|remaining| remaining > Duration::from_secs(19)));
    }
}
