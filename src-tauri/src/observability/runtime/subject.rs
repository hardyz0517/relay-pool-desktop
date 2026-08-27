use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

#[cfg(test)]
use sha2::{Digest, Sha256};

pub(crate) const MAX_STABLE_CODE_BYTES: usize = 64;
const RESOURCE_HASH_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct StableEventCode(String);

impl StableEventCode {
    pub(crate) fn new(value: &str) -> Result<Self, SubjectError> {
        if !is_stable_token(value) {
            return Err(SubjectError::InvalidStableCode);
        }
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn from_command_name(value: &str) -> Result<Self, SubjectError> {
        if value.is_empty()
            || value.len() > MAX_STABLE_CODE_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(SubjectError::InvalidStableCode);
        }
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for StableEventCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for StableEventCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RedactedResourceId(String);

impl RedactedResourceId {
    #[cfg(test)]
    pub(crate) fn from_raw(scope: &str, raw: &str) -> Result<Self, SubjectError> {
        if !is_stable_token(scope) {
            return Err(SubjectError::InvalidResourceScope);
        }
        let digest = Sha256::digest([scope.as_bytes(), b"\0", raw.as_bytes()].concat());
        let hex = format!("{digest:x}");
        Ok(Self(format!("res_{}", &hex[..RESOURCE_HASH_BYTES])))
    }

    pub(crate) fn from_public(value: &str) -> Result<Self, SubjectError> {
        let valid = value.len() == 4 + RESOURCE_HASH_BYTES
            && value.starts_with("res_")
            && value[4..].bytes().all(|byte| byte.is_ascii_hexdigit());
        if !valid {
            return Err(SubjectError::InvalidResourceId);
        }
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for RedactedResourceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RedactedResourceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_public(&value).map_err(D::Error::custom)
    }
}

macro_rules! anonymous_id {
    ($name:ident, $prefix:literal, $max:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub(crate) struct $name(String);

        impl $name {
            #[cfg(test)]
            pub(crate) fn new() -> Self {
                Self(format!("{}{}", $prefix, Uuid::now_v7().simple()))
            }

            pub(crate) fn from_public(value: &str) -> Result<Self, SubjectError> {
                let valid = value.len() == $prefix.len() + 32
                    && value.len() <= $max
                    && value.starts_with($prefix)
                    && value[$prefix.len()..]
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
                if !valid {
                    return Err(SubjectError::InvalidIdentifier);
                }
                Ok(Self(value.to_owned()))
            }

            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::from_public(&value).map_err(D::Error::custom)
            }
        }
    };
}

anonymous_id!(SessionId, "ses_", 40);
anonymous_id!(InteractionId, "int_", 40);
anonymous_id!(OperationId, "op_", 40);

#[cfg(not(test))]
impl SessionId {
    pub(crate) fn new() -> Self {
        Self(format!("ses_{}", Uuid::now_v7().simple()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CorrelationIdRef(String);

impl CorrelationIdRef {
    pub(crate) fn from_public(value: &str) -> Result<Self, SubjectError> {
        if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(SubjectError::InvalidIdentifier);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for CorrelationIdRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CorrelationIdRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_public(&value).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SubjectKind {
    None,
    Installation,
    Session,
    Interaction,
    Operation,
    Task,
    Command,
    Station,
    Provider,
    Resource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub(crate) enum SubjectRef {
    Installation,
    Session(SessionId),
    Interaction(InteractionId),
    Operation(OperationId),
    Correlation(CorrelationIdRef),
    Task(StableEventCode),
    Command(StableEventCode),
    Station(RedactedResourceId),
    Provider(RedactedResourceId),
    Resource(RedactedResourceId),
}

impl SubjectRef {
    pub(crate) fn kind(&self) -> SubjectKind {
        match self {
            Self::Installation => SubjectKind::Installation,
            Self::Session(_) => SubjectKind::Session,
            Self::Interaction(_) => SubjectKind::Interaction,
            Self::Operation(_) => SubjectKind::Operation,
            Self::Correlation(_) => SubjectKind::Resource,
            Self::Task(_) => SubjectKind::Task,
            Self::Command(_) => SubjectKind::Command,
            Self::Station(_) => SubjectKind::Station,
            Self::Provider(_) => SubjectKind::Provider,
            Self::Resource(_) => SubjectKind::Resource,
        }
    }
}

pub(crate) fn is_stable_token(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_STABLE_CODE_BYTES {
        return false;
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    !value.contains("://")
        && !value.contains('?')
        && !value.contains('=')
        && !value.contains('\\')
        && !value.contains('/')
        && !lower.contains("authorization")
        && !lower.contains("bearer")
        && !lower.contains("cookie")
        && !lower.contains("password")
        && !lower.contains("sk-")
        && !lower.contains("token")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubjectError {
    InvalidStableCode,
    #[cfg(test)]
    InvalidResourceScope,
    InvalidResourceId,
    InvalidIdentifier,
}

impl std::fmt::Display for SubjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidStableCode => "invalid stable event code",
            #[cfg(test)]
            Self::InvalidResourceScope => "invalid resource scope",
            Self::InvalidResourceId => "invalid redacted resource id",
            Self::InvalidIdentifier => "invalid anonymized identifier",
        })
    }
}

impl std::error::Error for SubjectError {}
