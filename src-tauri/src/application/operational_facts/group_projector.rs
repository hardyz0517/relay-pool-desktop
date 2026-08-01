use crate::models::operational::{PriceConfidence, RecordRevision, UnixMillis};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupStatus {
    Available,
    Missing,
    Disabled,
    Legacy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupIdentity {
    BindingId(String),
    GroupKeyHash(String),
    GroupIdHash(String),
    LegacyNormalizedName(String),
}

impl GroupIdentity {
    pub(crate) fn stable_key(&self) -> String {
        match self {
            Self::BindingId(value) => format!("binding:{value}"),
            Self::GroupKeyHash(value) => format!("group-key:{value}"),
            Self::GroupIdHash(value) => format!("group-id:{value}"),
            Self::LegacyNormalizedName(value) => format!("legacy-name:{value}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectionTrace {
    pub(crate) source_chain: Vec<&'static str>,
    pub(crate) confidence: PriceConfidence,
    pub(crate) resolved_at: UnixMillis,
    pub(crate) reason: &'static str,
    pub(crate) revision_refs: Vec<RecordRevision>,
}

impl ProjectionTrace {
    pub(crate) fn new(
        source_chain: Vec<&'static str>,
        confidence: PriceConfidence,
        resolved_at: UnixMillis,
        reason: &'static str,
        revision_refs: Vec<RecordRevision>,
    ) -> Self {
        Self {
            source_chain,
            confidence,
            resolved_at,
            reason,
            revision_refs,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroupProjectionInput {
    pub(crate) group_binding_id: Option<String>,
    pub(crate) group_key_hash: Option<String>,
    pub(crate) group_id_hash: Option<String>,
    pub(crate) group_name: Option<String>,
    pub(crate) status: GroupStatus,
    pub(crate) trace: ProjectionTrace,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroupProjection {
    pub(crate) identity: GroupIdentity,
    pub(crate) display_name: String,
    pub(crate) available: bool,
    pub(crate) trace: ProjectionTrace,
}

pub(crate) fn project_group(input: GroupProjectionInput) -> Option<GroupProjection> {
    let identity = first_non_empty(input.group_binding_id)
        .map(GroupIdentity::BindingId)
        .or_else(|| first_non_empty(input.group_key_hash).map(GroupIdentity::GroupKeyHash))
        .or_else(|| first_non_empty(input.group_id_hash).map(GroupIdentity::GroupIdHash))
        .or_else(|| {
            input
                .group_name
                .as_deref()
                .map(normalize_legacy_group_name)
                .filter(|value| !value.is_empty())
                .map(GroupIdentity::LegacyNormalizedName)
        })?;
    let display_name = input
        .group_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Unnamed group")
        .to_string();
    let available = matches!(input.status, GroupStatus::Available);
    Some(GroupProjection {
        identity,
        display_name,
        available,
        trace: input.trace,
    })
}

fn first_non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_legacy_group_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
