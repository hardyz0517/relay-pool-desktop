use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::auth::{validate_profile_header_name, AuthBoundaryViolation},
};

pub mod claude_code;
pub mod codex_cli;
pub mod gemini_cli;
pub mod registry;
pub mod standard;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderValue {
    Static(String),
    RequestValue { kind: RequestValueKind },
    ModelTemplate { template: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestValueKind {
    SessionId,
    RequestId,
}

impl RequestValueKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::SessionId => "session_id",
            Self::RequestId => "request_id",
        }
    }
}

impl HeaderValue {
    fn hash_value(&self) -> HeaderHashValue<'_> {
        match self {
            Self::Static(value) => HeaderHashValue {
                kind: "static",
                value,
            },
            Self::RequestValue { kind } => HeaderHashValue {
                kind: "request_value",
                value: kind.as_str(),
            },
            Self::ModelTemplate { template } => HeaderHashValue {
                kind: "model_template",
                value: template,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProfileAuthScheme {
    BearerAuthorization,
    ApiKeyHeader { name: String },
}

impl ProfileAuthScheme {
    pub fn header_name(&self) -> &str {
        match self {
            Self::BearerAuthorization => "authorization",
            Self::ApiKeyHeader { name } => name,
        }
    }

    pub fn secret_value(&self, secret: &str) -> String {
        match self {
            Self::BearerAuthorization => format!("Bearer {secret}"),
            Self::ApiKeyHeader { .. } => secret.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientProfileHeader {
    pub name: String,
    pub value: HeaderValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientProfileRequestShape {
    pub method: String,
    pub path: String,
    pub headers: Vec<ClientProfileHeader>,
    pub body_defaults: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientProfileDefinition {
    pub id: ClientProfileId,
    pub version: u32,
    pub enabled: bool,
    pub supported_protocols: Vec<ProtocolKind>,
    pub auth: ProfileAuthScheme,
    pub request: ClientProfileRequestShape,
}

impl ClientProfileDefinition {
    pub fn validate_boundaries(&self) -> Result<(), AuthBoundaryViolation> {
        for header in &self.request.headers {
            validate_profile_header_name(&header.name)?;
        }
        Ok(())
    }

    pub fn supports_protocol(&self, protocol: ProtocolKind) -> bool {
        self.enabled && self.supported_protocols.contains(&protocol)
    }

    pub fn profile_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher
            .update(serde_json::to_vec(&self.hash_input()).expect("profile hash input serializes"));
        format!("{:x}", hasher.finalize())
    }

    pub fn golden_summary(&self) -> ClientProfileGoldenSummary {
        ClientProfileGoldenSummary {
            id: self.id,
            version: self.version,
            enabled: self.enabled,
            supported_protocols: self.supported_protocols.clone(),
            auth: self.auth.clone(),
            method: self.request.method.clone(),
            path: self.request.path.clone(),
            header_names: self
                .request
                .headers
                .iter()
                .map(|header| header.name.trim().to_ascii_lowercase())
                .collect(),
            body_defaults: self.request.body_defaults.clone(),
            profile_hash: self.profile_hash(),
        }
    }

    fn hash_input(&self) -> ClientProfileHashInput<'_> {
        ClientProfileHashInput {
            id: self.id,
            version: self.version,
            enabled: self.enabled,
            supported_protocols: &self.supported_protocols,
            auth: &self.auth,
            method: &self.request.method,
            path: &self.request.path,
            headers: self
                .request
                .headers
                .iter()
                .map(|header| HeaderHashInput {
                    name: header.name.trim().to_ascii_lowercase(),
                    value: header.value.hash_value(),
                })
                .collect(),
            body_defaults: &self.request.body_defaults,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientProfileGoldenSummary {
    pub id: ClientProfileId,
    pub version: u32,
    pub enabled: bool,
    pub supported_protocols: Vec<ProtocolKind>,
    pub auth: ProfileAuthScheme,
    pub method: String,
    pub path: String,
    pub header_names: Vec<String>,
    pub body_defaults: Vec<String>,
    pub profile_hash: String,
}

#[derive(Serialize)]
struct ClientProfileHashInput<'a> {
    id: ClientProfileId,
    version: u32,
    enabled: bool,
    supported_protocols: &'a [ProtocolKind],
    auth: &'a ProfileAuthScheme,
    method: &'a str,
    path: &'a str,
    headers: Vec<HeaderHashInput<'a>>,
    body_defaults: &'a [String],
}

#[derive(Serialize)]
struct HeaderHashInput<'a> {
    name: String,
    value: HeaderHashValue<'a>,
}

#[derive(Serialize)]
struct HeaderHashValue<'a> {
    kind: &'static str,
    value: &'a str,
}

pub(crate) fn header(name: &str, value: &str) -> ClientProfileHeader {
    ClientProfileHeader {
        name: name.to_string(),
        value: HeaderValue::Static(value.to_string()),
    }
}

pub(crate) fn request_value_header(name: &str, kind: RequestValueKind) -> ClientProfileHeader {
    ClientProfileHeader {
        name: name.to_string(),
        value: HeaderValue::RequestValue { kind },
    }
}

pub(crate) fn model_template_header(name: &str, template: &str) -> ClientProfileHeader {
    ClientProfileHeader {
        name: name.to_string(),
        value: HeaderValue::ModelTemplate {
            template: template.to_string(),
        },
    }
}

pub(crate) fn shape(
    path: &str,
    headers: Vec<ClientProfileHeader>,
    body_defaults: &[&str],
) -> ClientProfileRequestShape {
    ClientProfileRequestShape {
        method: "POST".to_string(),
        path: path.to_string(),
        headers,
        body_defaults: body_defaults
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    }
}
