use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, PartialEq)]
pub enum OperationalValidationError {
    EmptyId {
        field: &'static str,
    },
    InvalidRevision {
        field: &'static str,
        value: i64,
    },
    InvalidTimestamp {
        field: &'static str,
        value: i64,
    },
    #[cfg(test)]
    InvalidConfidence {
        field: &'static str,
        value: f64,
    },
    InvalidEndpointOrigin {
        reason: &'static str,
    },
}

impl fmt::Display for OperationalValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyId { field } => write!(formatter, "{field} must not be empty"),
            Self::InvalidRevision { field, value } => {
                write!(formatter, "{field} revision must be positive, got {value}")
            }
            Self::InvalidTimestamp { field, value } => {
                write!(
                    formatter,
                    "{field} timestamp must be non-negative, got {value}"
                )
            }
            #[cfg(test)]
            Self::InvalidConfidence { field, value } => {
                write!(
                    formatter,
                    "{field} confidence must be finite between 0 and 1, got {value}"
                )
            }
            Self::InvalidEndpointOrigin { reason } => write!(formatter, "{reason}"),
        }
    }
}

impl std::error::Error for OperationalValidationError {}

macro_rules! id_type {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, OperationalValidationError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(OperationalValidationError::EmptyId { field: $field });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

id_type!(StationId, "station_id");
id_type!(StationKeyId, "station_key_id");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EndpointId(String);
impl EndpointId {
    pub fn new(value: impl Into<String>) -> Result<Self, OperationalValidationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(OperationalValidationError::EmptyId {
                field: "endpoint_id",
            });
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ModelName(String);
impl ModelName {
    pub fn new(value: impl Into<String>) -> Result<Self, OperationalValidationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(OperationalValidationError::EmptyId { field: "model" });
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OutboundPolicyRef(String);
impl OutboundPolicyRef {
    pub fn new(value: impl Into<String>) -> Result<Self, OperationalValidationError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(OperationalValidationError::EmptyId {
                field: "outbound_policy_ref",
            });
        }
        Ok(Self(value))
    }
}

#[cfg(test)]
id_type!(EvidenceHash, "evidence_hash");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RecordRevision(i64);

impl RecordRevision {
    pub fn new(value: i64) -> Result<Self, OperationalValidationError> {
        if value <= 0 {
            return Err(OperationalValidationError::InvalidRevision {
                field: "record",
                value,
            });
        }
        Ok(Self(value))
    }

    pub fn get(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EndpointRevision(i64);

impl EndpointRevision {
    pub fn new(value: i64) -> Result<Self, OperationalValidationError> {
        if value <= 0 {
            return Err(OperationalValidationError::InvalidRevision {
                field: "endpoint",
                value,
            });
        }
        Ok(Self(value))
    }

    pub fn get(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UnixMillis(i64);

impl UnixMillis {
    pub fn new(value: i64) -> Result<Self, OperationalValidationError> {
        if value < 0 {
            return Err(OperationalValidationError::InvalidTimestamp {
                field: "unix_ms",
                value,
            });
        }
        Ok(Self(value))
    }

    pub fn get(self) -> i64 {
        self.0
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationAccountRef {
    station_id: StationId,
}

#[cfg(test)]
impl StationAccountRef {
    pub fn new(station_id: StationId) -> Self {
        Self { station_id }
    }

    pub fn station_id(&self) -> &StationId {
        &self.station_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointRef {
    station_id: StationId,
    endpoint_id: EndpointId,
    revision: EndpointRevision,
}

impl EndpointRef {
    pub fn new(station_id: StationId, endpoint_id: EndpointId, revision: EndpointRevision) -> Self {
        Self {
            station_id,
            endpoint_id,
            revision,
        }
    }

    #[cfg(test)]
    pub fn station_id(&self) -> &StationId {
        &self.station_id
    }

    #[cfg(test)]
    pub fn endpoint_id(&self) -> &EndpointId {
        &self.endpoint_id
    }

    pub fn revision(&self) -> EndpointRevision {
        self.revision
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SanitizedOrigin(String);

impl SanitizedOrigin {
    pub fn from_endpoint_url(value: &str) -> Result<Self, OperationalValidationError> {
        let url =
            Url::parse(value).map_err(|_| OperationalValidationError::InvalidEndpointOrigin {
                reason: "endpoint origin must be an absolute HTTP(S) URL",
            })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(OperationalValidationError::InvalidEndpointOrigin {
                reason: "endpoint origin scheme must be http or https",
            });
        }
        if url.host_str().is_none() {
            return Err(OperationalValidationError::InvalidEndpointOrigin {
                reason: "endpoint origin must include a host",
            });
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(OperationalValidationError::InvalidEndpointOrigin {
                reason: "endpoint origin must not include user info",
            });
        }

        let mut origin = format!(
            "{}://{}",
            url.scheme(),
            url.host_str().expect("checked host")
        );
        if let Some(port) = url.port() {
            origin.push(':');
            origin.push_str(&port.to_string());
        }
        Ok(Self(origin))
    }

    #[cfg(test)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointFacts {
    endpoint_ref: EndpointRef,
    sanitized_origin: SanitizedOrigin,
    outbound_policy_ref: OutboundPolicyRef,
}

impl EndpointFacts {
    pub fn new(
        endpoint_ref: EndpointRef,
        sanitized_origin: SanitizedOrigin,
        outbound_policy_ref: OutboundPolicyRef,
    ) -> Self {
        Self {
            endpoint_ref,
            sanitized_origin,
            outbound_policy_ref,
        }
    }

    pub fn endpoint_ref(&self) -> &EndpointRef {
        &self.endpoint_ref
    }

    #[cfg(test)]
    pub fn sanitized_origin(&self) -> &SanitizedOrigin {
        &self.sanitized_origin
    }

    #[cfg(test)]
    pub fn outbound_policy_ref(&self) -> &OutboundPolicyRef {
        &self.outbound_policy_ref
    }
}
