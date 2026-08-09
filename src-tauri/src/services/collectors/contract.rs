use std::{fmt, sync::Arc};

use futures_util::future::BoxFuture;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{
    models::remote_keys::RemoteStationKey,
    outbound::{AsyncOutboundClient, ProxyPolicy, RequestBudget},
    services::collectors::{
        evidence::{EndpointEvidence, EndpointRole},
        facts::CollectorFacts,
        failure::DriverFailure,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProviderKind {
    Sub2Api,
    NewApi,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sub2Api => "sub2api",
            Self::NewApi => "newapi",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "sub2api" => Some(Self::Sub2Api),
            "newapi" => Some(Self::NewApi),
            _ => None,
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub kind: ProviderKind,
    pub display_name: &'static str,
    pub station_types: &'static [&'static str],
    pub capabilities: DriverCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverCapabilities {
    pub collector: Option<CollectorCapabilityDescriptor>,
    pub remote_key: Option<RemoteKeyCapabilityDescriptor>,
    pub authorization: Option<AuthorizationCapabilityDescriptor>,
}

impl DriverCapabilities {
    pub const fn none() -> Self {
        Self {
            collector: None,
            remote_key: None,
            authorization: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorCapabilityDescriptor {
    pub supported_tasks: &'static [CollectorTaskKind],
    pub full_tasks: &'static [CollectorTaskKind],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteKeyCapabilityDescriptor {
    pub supports_list: bool,
    pub supports_create: bool,
    pub supports_delete: bool,
    pub supports_reveal: bool,
    pub supports_result_unknown_reconciliation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationCapabilityDescriptor {
    pub supports_header_validation: bool,
    pub supports_session_validation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollectorTaskKind {
    Balance,
    Groups,
    Detect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationIdentity {
    pub station_id: String,
    pub endpoint_revision: i64,
    pub provider: ProviderKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEndpoints {
    pub api_base_url: Option<String>,
    pub website_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpaqueCredentialHandle {
    pub station_id: String,
    pub credential_revision: i64,
    pub scope: CredentialScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CredentialScope {
    StationKey,
    LoginSession,
    LoginPassword,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CredentialSecretPurpose {
    AuthorizationHeader,
    SessionCookie,
    LoginPassword,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderAuthContext {
    NewApi {
        user_id: String,
        secret_purpose: CredentialSecretPurpose,
    },
    Sub2Api {
        station_keys: Vec<Sub2ApiStationKeyCredential>,
        access_token: Option<OpaqueCredentialHandle>,
        session_cookie: Option<OpaqueCredentialHandle>,
        login: Option<Sub2ApiLoginCredential>,
        credit_per_cny: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sub2ApiStationKeyCredential {
    pub station_key_id: String,
    pub credential: OpaqueCredentialHandle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sub2ApiLoginCredential {
    pub username: String,
    pub password: OpaqueCredentialHandle,
}

pub struct CredentialSecret {
    value: Zeroizing<String>,
}

impl CredentialSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: Zeroizing::new(value.into()),
        }
    }

    pub fn expose(&self) -> &str {
        self.value.as_str()
    }
}

pub trait DriverSecretAccessor: Send + Sync {
    fn resolve_secret<'a>(
        &'a self,
        handle: &'a OpaqueCredentialHandle,
        purpose: CredentialSecretPurpose,
    ) -> BoxFuture<'a, Result<CredentialSecret, DriverFailure>>;
}

pub struct CollectorContext<'a> {
    pub station: StationIdentity,
    pub endpoints: ProviderEndpoints,
    pub credential: OpaqueCredentialHandle,
    pub auth: Option<ProviderAuthContext>,
    pub user_agent: Option<String>,
    pub secrets: &'a dyn DriverSecretAccessor,
    pub outbound: &'a AsyncOutboundClient,
    pub proxy: ProxyPolicy,
    pub budget: RequestBudget,
    pub cancellation: CancellationToken,
    pub correlation_id: String,
}

#[derive(Debug, Clone)]
pub struct DriverOutput {
    pub facts: CollectorFacts,
    pub evidence: Vec<EndpointEvidence>,
    pub status: DriverOutputStatus,
    pub diagnostics: RedactedDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverOutputStatus {
    Success,
    Partial,
    ManualRequired,
}

#[derive(Debug, Clone)]
pub struct RedactedDiagnostics {
    pub summary: Option<String>,
    pub raw_json_redacted: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct RemoteKeyRequest {
    pub station: StationIdentity,
    pub endpoints: ProviderEndpoints,
    pub credential: OpaqueCredentialHandle,
}

#[derive(Debug, Clone)]
pub struct CreateRemoteKeyRequest {
    pub station: StationIdentity,
    pub endpoints: ProviderEndpoints,
    pub credential: OpaqueCredentialHandle,
    pub name: String,
    pub provider_group_id: Option<String>,
    pub group_name: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RevealRemoteKeyRequest {
    pub station: StationIdentity,
    pub endpoints: ProviderEndpoints,
    pub credential: OpaqueCredentialHandle,
    pub remote_key_id: String,
}

#[derive(Debug, Clone)]
pub struct DeleteRemoteKeyRequest {
    pub station: StationIdentity,
    pub endpoints: ProviderEndpoints,
    pub credential: OpaqueCredentialHandle,
    pub remote_key_id: String,
}

#[derive(Debug, Clone)]
pub struct RemoteKeyOutput {
    pub keys: Vec<RemoteStationKey>,
    pub evidence: Vec<EndpointEvidence>,
    pub diagnostics: RedactedDiagnostics,
}

pub struct RevealedRemoteKeyOutput {
    pub remote_key: RemoteStationKey,
    pub full_key: RemoteKeySecret,
    pub evidence: Vec<EndpointEvidence>,
    pub diagnostics: RedactedDiagnostics,
}

pub struct CreatedRemoteKeyOutput {
    pub remote_key: RemoteStationKey,
    pub full_key_once: RemoteKeySecret,
    pub evidence: Vec<EndpointEvidence>,
    pub diagnostics: RedactedDiagnostics,
}

pub struct DeletedRemoteKeyOutput {
    pub keys: Vec<RemoteStationKey>,
    pub already_absent: bool,
    pub evidence: Vec<EndpointEvidence>,
    pub diagnostics: RedactedDiagnostics,
}

pub struct RemoteKeySecret {
    value: Zeroizing<String>,
}

impl RemoteKeySecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: Zeroizing::new(value.into()),
        }
    }

    pub fn expose(&self) -> &str {
        self.value.as_str()
    }

    pub fn into_plaintext(self) -> String {
        self.value.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub station: StationIdentity,
    pub endpoints: ProviderEndpoints,
    pub credential: OpaqueCredentialHandle,
    pub endpoint_role: EndpointRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationStatus {
    Authorized,
    ReauthorizationRequired,
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct AuthorizationOutput {
    pub status: AuthorizationStatus,
    pub evidence: Vec<EndpointEvidence>,
    pub diagnostics: RedactedDiagnostics,
}

pub trait CollectorDriver: Send + Sync {
    fn kind(&self) -> ProviderKind;

    fn collect<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        task: CollectorTaskKind,
    ) -> BoxFuture<'a, Result<DriverOutput, DriverFailure>>;
}

pub trait RemoteKeyDriver: Send + Sync {
    fn kind(&self) -> ProviderKind;

    fn list_remote_keys<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: RemoteKeyRequest,
    ) -> BoxFuture<'a, Result<RemoteKeyOutput, DriverFailure>>;

    fn reveal_remote_key<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: RevealRemoteKeyRequest,
    ) -> BoxFuture<'a, Result<RevealedRemoteKeyOutput, DriverFailure>>;

    fn create_remote_key<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: CreateRemoteKeyRequest,
    ) -> BoxFuture<'a, Result<CreatedRemoteKeyOutput, DriverFailure>>;

    fn delete_remote_key<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: DeleteRemoteKeyRequest,
    ) -> BoxFuture<'a, Result<DeletedRemoteKeyOutput, DriverFailure>>;
}

pub trait AuthorizationDriver: Send + Sync {
    fn kind(&self) -> ProviderKind;

    fn validate_authorization<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        request: AuthorizationRequest,
    ) -> BoxFuture<'a, Result<AuthorizationOutput, DriverFailure>>;
}

pub struct ProviderEntry {
    pub descriptor: ProviderDescriptor,
    pub collector: Option<Arc<dyn CollectorDriver>>,
    pub remote_key: Option<Arc<dyn RemoteKeyDriver>>,
    pub authorization: Option<Arc<dyn AuthorizationDriver>>,
}

impl ProviderEntry {
    pub fn unsupported(descriptor: ProviderDescriptor) -> Self {
        Self {
            descriptor,
            collector: None,
            remote_key: None,
            authorization: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthRefreshKey {
    pub provider: ProviderKind,
    pub station_id: String,
    pub endpoint_revision: i64,
    pub credential_revision: i64,
    pub scope: CredentialScope,
}

impl AuthRefreshKey {
    pub fn from_context(context: &CollectorContext<'_>) -> Self {
        Self {
            provider: context.station.provider,
            station_id: context.station.station_id.clone(),
            endpoint_revision: context.station.endpoint_revision,
            credential_revision: context.credential.credential_revision,
            scope: context.credential.scope,
        }
    }
}

pub trait AuthRefreshCoordinator: Send + Sync {
    fn refresh<'a>(
        &'a self,
        key: AuthRefreshKey,
        budget: RequestBudget,
        cancellation: CancellationToken,
    ) -> BoxFuture<'a, Result<AuthRefreshOutcome, DriverFailure>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRefreshOutcome {
    pub credential_revision: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_is_closed_and_does_not_map_custom() {
        assert_eq!(ProviderKind::parse("sub2api"), Some(ProviderKind::Sub2Api));
        assert_eq!(ProviderKind::parse("newapi"), Some(ProviderKind::NewApi));
        assert_eq!(ProviderKind::parse("openai-compatible"), None);
        assert_eq!(ProviderKind::parse("openai_compatible"), None);
        assert_eq!(ProviderKind::parse("custom"), None);
        assert_eq!(ProviderKind::parse("unknown"), None);
    }

    #[test]
    fn credential_secret_is_not_clone_or_debug() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CredentialSecret>();
        let secret = CredentialSecret::new("sk-secret");
        assert_eq!(secret.expose(), "sk-secret");
    }
}
