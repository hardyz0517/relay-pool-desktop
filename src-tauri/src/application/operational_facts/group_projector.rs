use crate::models::operational::{PriceConfidence, RecordRevision, UnixMillis};

pub(crate) const GROUP_PROJECTOR_VERSION: &str = "group_identity_v1";

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
    pub(crate) projector_version: &'static str,
    pub(crate) source_chain: Vec<&'static str>,
    pub(crate) source_refs: Vec<String>,
    pub(crate) confidence: PriceConfidence,
    pub(crate) observed_at: UnixMillis,
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
            projector_version: "operational_facts_legacy_v1",
            source_chain,
            source_refs: Vec::new(),
            confidence,
            observed_at: resolved_at,
            resolved_at,
            reason,
            revision_refs,
        }
    }

    pub(crate) fn for_projector(
        projector_version: &'static str,
        source_chain: Vec<&'static str>,
        source_refs: Vec<String>,
        confidence: PriceConfidence,
        observed_at: UnixMillis,
        reason: &'static str,
        revision_refs: Vec<RecordRevision>,
    ) -> Self {
        Self {
            projector_version,
            source_chain,
            source_refs,
            confidence,
            observed_at,
            resolved_at: observed_at,
            reason,
            revision_refs,
        }
    }

    pub(crate) fn with_projector(mut self, projector_version: &'static str) -> Self {
        self.projector_version = projector_version;
        self
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupVerdict {
    Available,
    Disabled,
    Legacy,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroupProjectionOutcome {
    pub(crate) verdict: GroupVerdict,
    pub(crate) projection: Option<GroupProjection>,
    pub(crate) trace: ProjectionTrace,
}

pub(crate) fn project_group(input: GroupProjectionInput) -> Option<GroupProjection> {
    reduce_group(input).projection
}

pub(crate) fn reduce_group(input: GroupProjectionInput) -> GroupProjectionOutcome {
    let mut trace = input.trace.with_projector(GROUP_PROJECTOR_VERSION);
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
        });
    let verdict = match (identity.as_ref(), input.status) {
        (None, _) => GroupVerdict::Missing,
        (Some(GroupIdentity::LegacyNormalizedName(_)), GroupStatus::Available) => {
            GroupVerdict::Legacy
        }
        (Some(_), GroupStatus::Available) => GroupVerdict::Available,
        (Some(_), GroupStatus::Disabled) => GroupVerdict::Disabled,
        (Some(_), GroupStatus::Legacy) => GroupVerdict::Legacy,
        (Some(_), GroupStatus::Missing) => GroupVerdict::Invalid,
    };
    let Some(identity) = identity else {
        return GroupProjectionOutcome {
            verdict,
            projection: None,
            trace,
        };
    };
    if trace.source_refs.is_empty() {
        trace.source_refs.push(identity.stable_key());
    }
    let display_name = input
        .group_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Unnamed group")
        .to_string();
    let available = verdict == GroupVerdict::Available;
    let projection = GroupProjection {
        identity,
        display_name,
        available,
        trace: trace.clone(),
    };
    GroupProjectionOutcome {
        verdict,
        projection: Some(projection),
        trace,
    }
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
