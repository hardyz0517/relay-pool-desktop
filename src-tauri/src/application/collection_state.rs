//! Typed collection and authorization state transitions.
//!
//! This module is deliberately a pure domain boundary.  It knows nothing
//! about SQLite, transport payloads, Tauri events, or wall-clock access.  A
//! caller classifies a driver result before entering this module and supplies
//! the already-computed freshness and revision values.  Persistence owners can
//! then apply the returned transition in one terminal transaction.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Version of the first typed collection plan contract.
pub(crate) const CURRENT_COLLECTION_PLAN_VERSION: u16 = 1;

/// Revisions are persisted as signed SQLite integers in the current schema.
/// Negative values are not valid and are rejected at the contract boundary.
pub(crate) type Revision = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CollectionTaskKind {
    Balance,
    Groups,
    PublishedStatus,
    Detect,
}

impl CollectionTaskKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Balance => "balance",
            Self::Groups => "groups",
            Self::PublishedStatus => "published_status",
            Self::Detect => "detect",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        [
            Self::Balance,
            Self::Groups,
            Self::PublishedStatus,
            Self::Detect,
        ]
        .into_iter()
        .find(|task| task.as_str() == value.trim())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TaskRole {
    /// A core task is required to establish the collection summary.
    Core,
    /// Optional task failures are retained in their own task projection and
    /// must not make an otherwise healthy core collection unhealthy.
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FactFamily {
    Balance,
    Groups,
    PublishedStatus,
    Endpoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TaskSpec {
    pub(crate) task: CollectionTaskKind,
    pub(crate) role: TaskRole,
    pub(crate) fact_families: Vec<FactFamily>,
}

impl TaskSpec {
    pub(crate) fn core(task: CollectionTaskKind) -> Self {
        Self {
            task,
            role: TaskRole::Core,
            fact_families: default_fact_families(task),
        }
    }

    pub(crate) fn optional(task: CollectionTaskKind) -> Self {
        Self {
            task,
            role: TaskRole::Optional,
            fact_families: default_fact_families(task),
        }
    }
}

fn default_fact_families(task: CollectionTaskKind) -> Vec<FactFamily> {
    match task {
        CollectionTaskKind::Balance => vec![FactFamily::Balance],
        CollectionTaskKind::Groups => vec![FactFamily::Groups],
        CollectionTaskKind::PublishedStatus => vec![FactFamily::PublishedStatus],
        CollectionTaskKind::Detect => vec![FactFamily::Endpoint],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CollectionPlan {
    pub(crate) plan_version: u16,
    pub(crate) tasks: Vec<TaskSpec>,
}

impl CollectionPlan {
    pub(crate) fn new(plan_version: u16, tasks: Vec<TaskSpec>) -> Self {
        Self {
            plan_version,
            tasks,
        }
    }

    /// The default full plan for the providers currently supported by the
    /// collector.  Providers may publish a different versioned plan later.
    pub(crate) fn full_v1() -> Self {
        Self::new(
            CURRENT_COLLECTION_PLAN_VERSION,
            vec![
                TaskSpec::core(CollectionTaskKind::Balance),
                TaskSpec::core(CollectionTaskKind::Groups),
                TaskSpec::optional(CollectionTaskKind::PublishedStatus),
            ],
        )
    }

    pub(crate) fn single_v1(task: CollectionTaskKind) -> Self {
        Self::new(CURRENT_COLLECTION_PLAN_VERSION, vec![TaskSpec::core(task)])
    }

    fn validate(&self) -> Result<(), CollectionPlanError> {
        if self.plan_version != CURRENT_COLLECTION_PLAN_VERSION {
            return Err(CollectionPlanError::UnsupportedVersion {
                received: self.plan_version,
                supported: CURRENT_COLLECTION_PLAN_VERSION,
            });
        }
        if self.tasks.is_empty() {
            return Err(CollectionPlanError::Empty);
        }

        let mut seen = BTreeSet::new();
        for task in &self.tasks {
            if !seen.insert(task.task) {
                return Err(CollectionPlanError::DuplicateTask { task: task.task });
            }
            if task.fact_families.is_empty() {
                return Err(CollectionPlanError::MissingFactFamily { task: task.task });
            }
        }
        if !self.tasks.iter().any(|task| task.role == TaskRole::Core) {
            return Err(CollectionPlanError::NoCoreTask);
        }
        Ok(())
    }

    fn spec_for(&self, task: CollectionTaskKind) -> Option<&TaskSpec> {
        self.tasks.iter().find(|spec| spec.task == task)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum CollectionPlanError {
    #[error("unsupported collection plan version {received}; supported version is {supported}")]
    UnsupportedVersion { received: u16, supported: u16 },
    #[error("collection plan has no tasks")]
    Empty,
    #[error("collection plan contains duplicate task {task:?}")]
    DuplicateTask { task: CollectionTaskKind },
    #[error("collection plan task {task:?} has no fact family")]
    MissingFactFamily { task: CollectionTaskKind },
    #[error("collection plan has no core task")]
    NoCoreTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Completion {
    Succeeded,
    Partial,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FailureClass {
    AuthRejected,
    Timeout,
    Transport,
    MalformedPayload,
    Unsupported,
    InvalidRequest,
    RateLimited,
    Cancelled,
    ResultUnknown,
    ProviderUnavailable,
    BudgetExhausted,
    BrowserContextRequired,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthEffect {
    None,
    StartsVerification,
    ConfirmsValid,
    RequiresReauthorization,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Freshness {
    Fresh,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReasonCode {
    None,
    CoreTaskFailed,
    CoreTaskPartial,
    CoreTaskSkipped,
    OptionalTaskFailed,
    OptionalTaskPartial,
    OptionalTaskSkipped,
    AuthorizationRequired,
    AuthorizationVerifying,
    AuthIndeterminate,
    NetworkTimeout,
    TransportError,
    MalformedPayload,
    UnsupportedTask,
    InvalidRequest,
    RateLimited,
    Cancelled,
    ResultUnknown,
    ProviderUnavailable,
    BudgetExhausted,
    InternalError,
    NoEvidence,
    FreshnessUnknown,
    StaleData,
    ContractViolation,
    Superseded,
}

impl ReasonCode {
    fn from_failure(class: FailureClass) -> Self {
        match class {
            FailureClass::AuthRejected => Self::AuthorizationRequired,
            FailureClass::Timeout => Self::NetworkTimeout,
            FailureClass::Transport => Self::TransportError,
            FailureClass::MalformedPayload => Self::MalformedPayload,
            FailureClass::Unsupported => Self::UnsupportedTask,
            FailureClass::InvalidRequest => Self::InvalidRequest,
            FailureClass::RateLimited => Self::RateLimited,
            FailureClass::Cancelled => Self::Cancelled,
            FailureClass::ResultUnknown => Self::ResultUnknown,
            FailureClass::ProviderUnavailable => Self::ProviderUnavailable,
            FailureClass::BudgetExhausted => Self::BudgetExhausted,
            FailureClass::BrowserContextRequired => Self::AuthorizationRequired,
            FailureClass::Internal => Self::InternalError,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RevisionAxis {
    Endpoint,
    Credential,
    Intent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RevisionFence {
    pub(crate) endpoint_revision: Revision,
    pub(crate) credential_revision: Revision,
    pub(crate) intent_sequence: Revision,
}

impl RevisionFence {
    pub(crate) fn new(
        endpoint_revision: Revision,
        credential_revision: Revision,
        intent_sequence: Revision,
    ) -> Result<Self, RevisionError> {
        for (axis, value) in [
            (RevisionAxis::Endpoint, endpoint_revision),
            (RevisionAxis::Credential, credential_revision),
            (RevisionAxis::Intent, intent_sequence),
        ] {
            if value < 0 {
                return Err(RevisionError::Negative { axis, value });
            }
        }
        Ok(Self {
            endpoint_revision,
            credential_revision,
            intent_sequence,
        })
    }

    #[cfg(test)]
    pub(crate) const fn from_non_negative_parts(
        endpoint_revision: Revision,
        credential_revision: Revision,
        intent_sequence: Revision,
    ) -> Self {
        Self {
            endpoint_revision,
            credential_revision,
            intent_sequence,
        }
    }

    pub(crate) fn stale_axes_against(&self, current: &Self) -> Vec<RevisionAxis> {
        let mut axes = Vec::new();
        if self.endpoint_revision < current.endpoint_revision {
            axes.push(RevisionAxis::Endpoint);
        }
        if self.credential_revision < current.credential_revision {
            axes.push(RevisionAxis::Credential);
        }
        if self.intent_sequence < current.intent_sequence {
            axes.push(RevisionAxis::Intent);
        }
        axes
    }

    #[cfg(test)]
    pub(crate) fn is_stale_against(&self, current: &Self) -> bool {
        !self.stale_axes_against(current).is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum RevisionError {
    #[error("{axis:?} revision cannot be negative: {value}")]
    Negative { axis: RevisionAxis, value: Revision },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TaskOutcome {
    pub(crate) task: CollectionTaskKind,
    pub(crate) operation_id: String,
    pub(crate) fence: RevisionFence,
    pub(crate) completion: Completion,
    pub(crate) failure_class: Option<FailureClass>,
    pub(crate) auth_effect: AuthEffect,
    pub(crate) freshness: Freshness,
    pub(crate) reason: ReasonCode,
    pub(crate) observed_at_ms: Revision,
}

impl TaskOutcome {
    pub(crate) fn new(
        task: CollectionTaskKind,
        operation_id: impl Into<String>,
        fence: RevisionFence,
        completion: Completion,
        failure_class: Option<FailureClass>,
        auth_effect: AuthEffect,
        freshness: Freshness,
        reason: ReasonCode,
        observed_at_ms: Revision,
    ) -> Self {
        Self {
            task,
            operation_id: operation_id.into(),
            fence,
            completion,
            failure_class,
            auth_effect,
            freshness,
            reason,
            observed_at_ms,
        }
    }

    pub(crate) fn succeeded(
        task: CollectionTaskKind,
        operation_id: impl Into<String>,
        fence: RevisionFence,
        freshness: Freshness,
        observed_at_ms: Revision,
    ) -> Self {
        Self::new(
            task,
            operation_id,
            fence,
            Completion::Succeeded,
            None,
            AuthEffect::None,
            freshness,
            ReasonCode::None,
            observed_at_ms,
        )
    }

    pub(crate) fn failed(
        task: CollectionTaskKind,
        operation_id: impl Into<String>,
        fence: RevisionFence,
        failure_class: FailureClass,
        auth_effect: AuthEffect,
        observed_at_ms: Revision,
    ) -> Self {
        Self::new(
            task,
            operation_id,
            fence,
            Completion::Failed,
            Some(failure_class),
            auth_effect,
            Freshness::Unknown,
            ReasonCode::from_failure(failure_class),
            observed_at_ms,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CollectionStatus {
    NotCollected,
    Collecting,
    Healthy,
    Degraded,
    Failed,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TaskProjection {
    pub(crate) task: CollectionTaskKind,
    pub(crate) role: TaskRole,
    pub(crate) completion: Completion,
    pub(crate) failure_class: Option<FailureClass>,
    pub(crate) auth_effect: AuthEffect,
    pub(crate) freshness: Freshness,
    pub(crate) reason: ReasonCode,
    pub(crate) observed_at_ms: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CollectionProjection {
    pub(crate) status: CollectionStatus,
    pub(crate) operation_id: String,
    pub(crate) fence: RevisionFence,
    pub(crate) core_success_count: u16,
    pub(crate) core_partial_count: u16,
    pub(crate) core_failure_count: u16,
    pub(crate) optional_failure_count: u16,
    pub(crate) freshness: Freshness,
    pub(crate) reasons: Vec<ReasonCode>,
    pub(crate) tasks: Vec<TaskProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CollectionReduction {
    Applied(CollectionProjection),
    Stale { axes: Vec<RevisionAxis> },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum CollectionReducerError {
    #[error(transparent)]
    Plan(#[from] CollectionPlanError),
    #[error("collection operation has no task outcomes")]
    EmptyOutcomes,
    #[error("collection operation id cannot be empty")]
    EmptyOperationId,
    #[error("outcome for unexpected task {task:?}")]
    UnexpectedTask { task: CollectionTaskKind },
    #[error("collection task {task:?} has no outcome")]
    MissingTask { task: CollectionTaskKind },
    #[error("collection task {task:?} appears more than once")]
    DuplicateTask { task: CollectionTaskKind },
    #[error("outcomes in one operation must use one operation id")]
    MixedOperationIds,
    #[error("outcomes in one operation must use one revision fence")]
    MixedRevisionFences,
    #[error("succeeded task {task:?} carries a failure class")]
    SuccessWithFailure { task: CollectionTaskKind },
    #[error("failed task {task:?} has no failure class")]
    FailureWithoutClass { task: CollectionTaskKind },
    #[error("observed timestamp cannot be negative")]
    NegativeObservedAt,
    #[error("equal collection watermark belongs to another operation")]
    WatermarkConflict,
}

/// Reduce a complete operation result into one collection projection.
///
/// Every task declared by the plan must be represented, including an explicit
/// `Skipped` outcome for an unsupported optional task.  A missing or unknown
/// task is a contract error, rather than an implicit success.  A result behind
/// the current watermark returns `Stale` and cannot regress the projection.
pub(crate) fn reduce_collection(
    plan: &CollectionPlan,
    outcomes: &[TaskOutcome],
    previous: Option<&CollectionProjection>,
) -> Result<CollectionReduction, CollectionReducerError> {
    plan.validate()?;
    if outcomes.is_empty() {
        return Err(CollectionReducerError::EmptyOutcomes);
    }

    let operation_id = outcomes[0].operation_id.trim();
    if operation_id.is_empty() {
        return Err(CollectionReducerError::EmptyOperationId);
    }
    let fence = outcomes[0].fence;
    let mut seen = BTreeSet::new();
    for outcome in outcomes {
        if outcome.operation_id.trim().is_empty() {
            return Err(CollectionReducerError::EmptyOperationId);
        }
        if outcome.operation_id != operation_id {
            return Err(CollectionReducerError::MixedOperationIds);
        }
        if outcome.fence != fence {
            return Err(CollectionReducerError::MixedRevisionFences);
        }
        if !seen.insert(outcome.task) {
            return Err(CollectionReducerError::DuplicateTask { task: outcome.task });
        }
        let Some(_) = plan.spec_for(outcome.task) else {
            return Err(CollectionReducerError::UnexpectedTask { task: outcome.task });
        };
        if outcome.completion == Completion::Succeeded && outcome.failure_class.is_some() {
            return Err(CollectionReducerError::SuccessWithFailure { task: outcome.task });
        }
        if outcome.completion == Completion::Failed && outcome.failure_class.is_none() {
            return Err(CollectionReducerError::FailureWithoutClass { task: outcome.task });
        }
        if outcome.observed_at_ms < 0 {
            return Err(CollectionReducerError::NegativeObservedAt);
        }
    }
    for spec in &plan.tasks {
        if !seen.contains(&spec.task) {
            return Err(CollectionReducerError::MissingTask { task: spec.task });
        }
    }

    if let Some(current) = previous {
        let stale_axes = fence.stale_axes_against(&current.fence);
        if !stale_axes.is_empty() {
            return Ok(CollectionReduction::Stale { axes: stale_axes });
        }
        if fence == current.fence && operation_id != current.operation_id {
            return Err(CollectionReducerError::WatermarkConflict);
        }
    }

    let mut tasks = Vec::with_capacity(plan.tasks.len());
    let mut reasons = BTreeSet::new();
    let mut core_success_count = 0_u16;
    let mut core_partial_count = 0_u16;
    let mut core_failure_count = 0_u16;
    let mut optional_failure_count = 0_u16;
    let mut any_core_stale = false;
    let mut any_core_unknown = false;
    let mut core_skipped_count = 0_u16;

    for spec in &plan.tasks {
        let outcome = outcomes
            .iter()
            .find(|outcome| outcome.task == spec.task)
            .expect("validated plan task outcome");
        let mut reason = outcome.reason;
        if reason == ReasonCode::None {
            reason = match (spec.role, outcome.completion) {
                (TaskRole::Core, Completion::Partial) => ReasonCode::CoreTaskPartial,
                (TaskRole::Core, Completion::Failed) => ReasonCode::CoreTaskFailed,
                (TaskRole::Core, Completion::Skipped) => ReasonCode::CoreTaskSkipped,
                (TaskRole::Optional, Completion::Partial) => ReasonCode::OptionalTaskPartial,
                (TaskRole::Optional, Completion::Failed) => ReasonCode::OptionalTaskFailed,
                (TaskRole::Optional, Completion::Skipped) => ReasonCode::OptionalTaskSkipped,
                _ => ReasonCode::None,
            };
        }
        if reason != ReasonCode::None {
            reasons.insert(reason);
        }
        if spec.role == TaskRole::Core {
            match outcome.completion {
                Completion::Succeeded => core_success_count += 1,
                Completion::Partial => core_partial_count += 1,
                Completion::Failed => core_failure_count += 1,
                Completion::Skipped => core_skipped_count += 1,
            }
            any_core_stale |= outcome.freshness == Freshness::Stale;
            any_core_unknown |= outcome.freshness == Freshness::Unknown;
        } else if matches!(outcome.completion, Completion::Partial | Completion::Failed) {
            optional_failure_count += 1;
        }
        tasks.push(TaskProjection {
            task: outcome.task,
            role: spec.role,
            completion: outcome.completion,
            failure_class: outcome.failure_class,
            auth_effect: outcome.auth_effect,
            freshness: outcome.freshness,
            reason,
            observed_at_ms: outcome.observed_at_ms,
        });
    }

    let freshness = if any_core_stale {
        reasons.insert(ReasonCode::StaleData);
        Freshness::Stale
    } else if any_core_unknown {
        reasons.insert(ReasonCode::FreshnessUnknown);
        Freshness::Unknown
    } else {
        Freshness::Fresh
    };

    let core_task_count = plan
        .tasks
        .iter()
        .filter(|task| task.role == TaskRole::Core)
        .count() as u16;
    let status = if core_failure_count == core_task_count {
        CollectionStatus::Failed
    } else if core_skipped_count == core_task_count {
        CollectionStatus::NotCollected
    } else if core_failure_count > 0 || core_partial_count > 0 || core_skipped_count > 0 {
        CollectionStatus::Degraded
    } else if any_core_stale {
        CollectionStatus::Stale
    } else if any_core_unknown {
        CollectionStatus::Degraded
    } else {
        CollectionStatus::Healthy
    };

    Ok(CollectionReduction::Applied(CollectionProjection {
        status,
        operation_id: operation_id.to_string(),
        fence,
        core_success_count,
        core_partial_count,
        core_failure_count,
        optional_failure_count,
        freshness,
        reasons: reasons.into_iter().collect(),
        tasks,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuthorizationStatus {
    Unknown,
    Verifying,
    Valid,
    ReauthorizationRequired,
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceAuthority {
    DriverProbe,
    AuthenticatedResponse,
    WebViewCapture,
    CredentialStore,
    Unknown,
}

impl EvidenceAuthority {
    const fn precedence(self) -> u8 {
        match self {
            Self::DriverProbe => 4,
            Self::AuthenticatedResponse => 3,
            Self::WebViewCapture => 2,
            Self::CredentialStore => 1,
            Self::Unknown => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AuthorizationRevision {
    pub(crate) credential_revision: Revision,
    pub(crate) intent_sequence: Revision,
}

impl AuthorizationRevision {
    pub(crate) fn new(
        credential_revision: Revision,
        intent_sequence: Revision,
    ) -> Result<Self, RevisionError> {
        RevisionFence::new(0, credential_revision, intent_sequence)?;
        Ok(Self {
            credential_revision,
            intent_sequence,
        })
    }

    fn stale_axes_against(&self, current: &Self) -> Vec<RevisionAxis> {
        let mut axes = Vec::new();
        if self.credential_revision < current.credential_revision {
            axes.push(RevisionAxis::Credential);
        }
        if self.credential_revision == current.credential_revision
            && self.intent_sequence < current.intent_sequence
        {
            axes.push(RevisionAxis::Intent);
        }
        axes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AuthorizationEvidence {
    pub(crate) operation_id: String,
    pub(crate) revision: AuthorizationRevision,
    pub(crate) effect: AuthEffect,
    pub(crate) authority: EvidenceAuthority,
    pub(crate) reason: ReasonCode,
    pub(crate) observed_at_ms: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AuthorizationProjection {
    pub(crate) status: AuthorizationStatus,
    pub(crate) revision: AuthorizationRevision,
    pub(crate) authority: EvidenceAuthority,
    pub(crate) reason: ReasonCode,
    /// Latest authorization attempt accepted by the monotonic watermark.
    pub(crate) operation_id: String,
    /// Operation that established the current definitive verdict.  This can
    /// differ from `operation_id` after a non-definitive verification attempt.
    pub(crate) source_operation_id: String,
    pub(crate) observed_at_ms: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthorizationReduction {
    Applied(AuthorizationProjection),
    Stale { axes: Vec<RevisionAxis> },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum AuthorizationReducerError {
    #[error("authorization operation id cannot be empty")]
    EmptyOperationId,
    #[error("authorization observed timestamp cannot be negative")]
    NegativeObservedAt,
    #[error("equal authorization watermark belongs to another operation")]
    WatermarkConflict,
    #[error(
        "authorization effect {effect:?} is not supported by evidence authority {authority:?}"
    )]
    InsufficientAuthority {
        effect: AuthEffect,
        authority: EvidenceAuthority,
    },
}

/// Apply typed authorization evidence without allowing old credentials or
/// old operation attempts to regress the current authorization projection.
pub(crate) fn reduce_authorization(
    previous: Option<&AuthorizationProjection>,
    evidence: &AuthorizationEvidence,
) -> Result<AuthorizationReduction, AuthorizationReducerError> {
    let operation_id = evidence.operation_id.trim();
    if operation_id.is_empty() {
        return Err(AuthorizationReducerError::EmptyOperationId);
    }
    if evidence.observed_at_ms < 0 {
        return Err(AuthorizationReducerError::NegativeObservedAt);
    }
    let definitive_effect = matches!(
        evidence.effect,
        AuthEffect::ConfirmsValid | AuthEffect::RequiresReauthorization
    );
    let definitive_authority = matches!(
        evidence.authority,
        EvidenceAuthority::DriverProbe | EvidenceAuthority::AuthenticatedResponse
    ) || matches!(
        (evidence.effect, evidence.authority),
        (
            AuthEffect::RequiresReauthorization,
            EvidenceAuthority::CredentialStore
        )
    );
    if definitive_effect && !definitive_authority {
        return Err(AuthorizationReducerError::InsufficientAuthority {
            effect: evidence.effect,
            authority: evidence.authority,
        });
    }
    if let Some(current) = previous {
        let stale_axes = evidence.revision.stale_axes_against(&current.revision);
        if !stale_axes.is_empty() {
            return Ok(AuthorizationReduction::Stale { axes: stale_axes });
        }
        if evidence.revision == current.revision && operation_id != current.operation_id {
            return Err(AuthorizationReducerError::WatermarkConflict);
        }
    }

    let same_credential = previous.filter(|current| {
        current.revision.credential_revision == evidence.revision.credential_revision
    });
    let preserves_definitive_verdict = same_credential.is_some_and(|current| {
        let definitive = matches!(
            current.status,
            AuthorizationStatus::Valid | AuthorizationStatus::ReauthorizationRequired
        );
        definitive
            && (matches!(
                evidence.effect,
                AuthEffect::None | AuthEffect::StartsVerification | AuthEffect::Indeterminate
            ) || evidence.authority.precedence() < current.authority.precedence())
    });

    if preserves_definitive_verdict {
        let current = same_credential.expect("checked same credential");
        return Ok(AuthorizationReduction::Applied(AuthorizationProjection {
            status: current.status,
            revision: evidence.revision,
            authority: current.authority,
            reason: current.reason,
            operation_id: operation_id.to_string(),
            source_operation_id: current.source_operation_id.clone(),
            observed_at_ms: current.observed_at_ms,
        }));
    }

    let (status, default_reason) = match evidence.effect {
        AuthEffect::StartsVerification => (
            AuthorizationStatus::Verifying,
            ReasonCode::AuthorizationVerifying,
        ),
        AuthEffect::ConfirmsValid => (AuthorizationStatus::Valid, ReasonCode::None),
        AuthEffect::RequiresReauthorization => (
            AuthorizationStatus::ReauthorizationRequired,
            ReasonCode::AuthorizationRequired,
        ),
        AuthEffect::Indeterminate => (
            AuthorizationStatus::Indeterminate,
            ReasonCode::AuthIndeterminate,
        ),
        AuthEffect::None => (AuthorizationStatus::Unknown, ReasonCode::None),
    };
    let reason = if evidence.reason == ReasonCode::None {
        default_reason
    } else {
        evidence.reason
    };

    Ok(AuthorizationReduction::Applied(AuthorizationProjection {
        status,
        revision: evidence.revision,
        authority: evidence.authority,
        reason,
        operation_id: operation_id.to_string(),
        source_operation_id: operation_id.to_string(),
        observed_at_ms: evidence.observed_at_ms,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fence(endpoint: Revision, credential: Revision, intent: Revision) -> RevisionFence {
        RevisionFence::new(endpoint, credential, intent).expect("valid fence")
    }

    fn success(
        task: CollectionTaskKind,
        operation: &str,
        task_fence: RevisionFence,
        freshness: Freshness,
    ) -> TaskOutcome {
        TaskOutcome::succeeded(task, operation, task_fence, freshness, 10)
    }

    fn failed(
        task: CollectionTaskKind,
        operation: &str,
        task_fence: RevisionFence,
        class: FailureClass,
    ) -> TaskOutcome {
        TaskOutcome::failed(task, operation, task_fence, class, AuthEffect::None, 10)
    }

    #[test]
    fn full_core_success_with_optional_success_is_healthy() {
        let plan = CollectionPlan::full_v1();
        let task_fence = fence(2, 3, 4);
        let outcomes = vec![
            success(
                CollectionTaskKind::Balance,
                "op-1",
                task_fence,
                Freshness::Fresh,
            ),
            success(
                CollectionTaskKind::Groups,
                "op-1",
                task_fence,
                Freshness::Fresh,
            ),
            success(
                CollectionTaskKind::PublishedStatus,
                "op-1",
                task_fence,
                Freshness::Fresh,
            ),
        ];

        let CollectionReduction::Applied(projection) =
            reduce_collection(&plan, &outcomes, None).expect("reduction")
        else {
            panic!("fresh operation cannot be stale")
        };
        assert_eq!(projection.status, CollectionStatus::Healthy);
        assert_eq!(projection.core_success_count, 2);
        assert_eq!(projection.optional_failure_count, 0);
    }

    #[test]
    fn optional_failure_does_not_degrade_core_collection() {
        let plan = CollectionPlan::full_v1();
        let task_fence = fence(1, 1, 1);
        let mut optional = failed(
            CollectionTaskKind::PublishedStatus,
            "op-optional",
            task_fence,
            FailureClass::Unsupported,
        );
        optional.reason = ReasonCode::UnsupportedTask;
        let outcomes = vec![
            success(
                CollectionTaskKind::Balance,
                "op-optional",
                task_fence,
                Freshness::Fresh,
            ),
            success(
                CollectionTaskKind::Groups,
                "op-optional",
                task_fence,
                Freshness::Fresh,
            ),
            optional,
        ];
        let CollectionReduction::Applied(projection) =
            reduce_collection(&plan, &outcomes, None).expect("reduction")
        else {
            panic!("fresh operation cannot be stale")
        };
        assert_eq!(projection.status, CollectionStatus::Healthy);
        assert_eq!(projection.optional_failure_count, 1);
        assert!(projection.reasons.contains(&ReasonCode::UnsupportedTask));
    }

    #[test]
    fn core_partial_and_failure_have_distinct_statuses() {
        let plan = CollectionPlan::new(
            CURRENT_COLLECTION_PLAN_VERSION,
            vec![TaskSpec::core(CollectionTaskKind::Balance)],
        );
        let task_fence = fence(1, 1, 1);
        let partial = TaskOutcome::new(
            CollectionTaskKind::Balance,
            "op-partial",
            task_fence,
            Completion::Partial,
            None,
            AuthEffect::None,
            Freshness::Fresh,
            ReasonCode::None,
            10,
        );
        let CollectionReduction::Applied(projection) =
            reduce_collection(&plan, &[partial], None).expect("reduction")
        else {
            panic!("fresh operation cannot be stale")
        };
        assert_eq!(projection.status, CollectionStatus::Degraded);
        assert_eq!(projection.reasons, vec![ReasonCode::CoreTaskPartial]);

        let failure = failed(
            CollectionTaskKind::Balance,
            "op-failed",
            task_fence,
            FailureClass::Transport,
        );
        let CollectionReduction::Applied(projection) =
            reduce_collection(&plan, &[failure], None).expect("reduction")
        else {
            panic!("fresh operation cannot be stale")
        };
        assert_eq!(projection.status, CollectionStatus::Failed);
        assert!(projection.reasons.contains(&ReasonCode::TransportError));
    }

    #[test]
    fn successful_but_stale_core_data_is_stale() {
        let plan = CollectionPlan::single_v1(CollectionTaskKind::Balance);
        let task_fence = fence(1, 1, 1);
        let result = success(
            CollectionTaskKind::Balance,
            "op-stale-data",
            task_fence,
            Freshness::Stale,
        );
        let CollectionReduction::Applied(projection) =
            reduce_collection(&plan, &[result], None).expect("reduction")
        else {
            panic!("fresh operation cannot be stale")
        };
        assert_eq!(projection.status, CollectionStatus::Stale);
        assert_eq!(projection.freshness, Freshness::Stale);
    }

    #[test]
    fn old_revision_is_ignored_without_regressing_current_projection() {
        let plan = CollectionPlan::single_v1(CollectionTaskKind::Balance);
        let current_fence = fence(2, 2, 2);
        let current = success(
            CollectionTaskKind::Balance,
            "new-operation",
            current_fence,
            Freshness::Fresh,
        );
        let CollectionReduction::Applied(current_projection) =
            reduce_collection(&plan, &[current], None).expect("current reduction")
        else {
            panic!("current operation cannot be stale")
        };
        let old = success(
            CollectionTaskKind::Balance,
            "old-operation",
            fence(1, 2, 1),
            Freshness::Fresh,
        );
        assert_eq!(
            reduce_collection(&plan, &[old], Some(&current_projection)),
            Ok(CollectionReduction::Stale {
                axes: vec![RevisionAxis::Endpoint, RevisionAxis::Intent]
            })
        );
        assert_eq!(current_projection.status, CollectionStatus::Healthy);
    }

    #[test]
    fn unknown_plan_version_fails_closed() {
        let plan = CollectionPlan::new(99, vec![TaskSpec::core(CollectionTaskKind::Balance)]);
        let outcome = success(
            CollectionTaskKind::Balance,
            "op-unknown-plan",
            fence(1, 1, 1),
            Freshness::Fresh,
        );
        assert!(matches!(
            reduce_collection(&plan, &[outcome], None),
            Err(CollectionReducerError::Plan(
                CollectionPlanError::UnsupportedVersion { received: 99, .. }
            ))
        ));
    }

    #[test]
    fn missing_core_result_is_a_contract_error() {
        let plan = CollectionPlan::full_v1();
        let task_fence = fence(1, 1, 1);
        let outcomes = vec![success(
            CollectionTaskKind::Balance,
            "op-incomplete",
            task_fence,
            Freshness::Fresh,
        )];
        assert!(matches!(
            reduce_collection(&plan, &outcomes, None),
            Err(CollectionReducerError::MissingTask {
                task: CollectionTaskKind::Groups
            })
        ));
    }

    #[test]
    fn authorization_success_then_same_revision_indeterminate_preserves_valid() {
        let revision = AuthorizationRevision::new(5, 10).expect("valid revision");
        let valid = AuthorizationEvidence {
            operation_id: "probe-1".to_string(),
            revision,
            effect: AuthEffect::ConfirmsValid,
            authority: EvidenceAuthority::DriverProbe,
            reason: ReasonCode::None,
            observed_at_ms: 10,
        };
        let AuthorizationReduction::Applied(projection) =
            reduce_authorization(None, &valid).expect("valid reduction")
        else {
            panic!("valid evidence cannot be stale")
        };
        assert_eq!(projection.status, AuthorizationStatus::Valid);

        let indeterminate = AuthorizationEvidence {
            operation_id: "probe-1".to_string(),
            effect: AuthEffect::Indeterminate,
            authority: EvidenceAuthority::DriverProbe,
            reason: ReasonCode::None,
            observed_at_ms: 11,
            ..valid
        };
        let AuthorizationReduction::Applied(next) =
            reduce_authorization(Some(&projection), &indeterminate).expect("reduction")
        else {
            panic!("same operation cannot be stale")
        };
        assert_eq!(next.status, AuthorizationStatus::Valid);
    }

    #[test]
    fn stale_old_credential_cannot_require_reauthorization() {
        let current_revision = AuthorizationRevision::new(8, 4).expect("valid revision");
        let current = AuthorizationEvidence {
            operation_id: "reauth".to_string(),
            revision: current_revision,
            effect: AuthEffect::ConfirmsValid,
            // The WebView only supplies the browser session.  The success
            // verdict is established by the authenticated self-probe before
            // it reaches this reducer.
            authority: EvidenceAuthority::AuthenticatedResponse,
            reason: ReasonCode::None,
            observed_at_ms: 20,
        };
        let AuthorizationReduction::Applied(current_projection) =
            reduce_authorization(None, &current).expect("current reduction")
        else {
            panic!("current evidence cannot be stale")
        };
        let old = AuthorizationEvidence {
            operation_id: "late-old-probe".to_string(),
            revision: AuthorizationRevision::new(7, 99).expect("valid revision"),
            effect: AuthEffect::RequiresReauthorization,
            authority: EvidenceAuthority::DriverProbe,
            reason: ReasonCode::AuthorizationRequired,
            observed_at_ms: 21,
        };
        assert_eq!(
            reduce_authorization(Some(&current_projection), &old),
            Ok(AuthorizationReduction::Stale {
                axes: vec![RevisionAxis::Credential]
            })
        );
        assert_eq!(current_projection.status, AuthorizationStatus::Valid);
    }

    #[test]
    fn reauthorization_effect_is_explicit_and_not_collection_status() {
        let evidence = AuthorizationEvidence {
            operation_id: "reauth-required".to_string(),
            revision: AuthorizationRevision::new(3, 3).expect("valid revision"),
            effect: AuthEffect::RequiresReauthorization,
            authority: EvidenceAuthority::DriverProbe,
            reason: ReasonCode::None,
            observed_at_ms: 30,
        };
        let AuthorizationReduction::Applied(projection) =
            reduce_authorization(None, &evidence).expect("reduction")
        else {
            panic!("fresh evidence cannot be stale")
        };
        assert_eq!(
            projection.status,
            AuthorizationStatus::ReauthorizationRequired
        );
        assert_eq!(projection.reason, ReasonCode::AuthorizationRequired);
    }

    #[test]
    fn equal_watermark_with_different_operation_is_rejected() {
        let plan = CollectionPlan::single_v1(CollectionTaskKind::Balance);
        let task_fence = fence(1, 1, 1);
        let first = success(
            CollectionTaskKind::Balance,
            "first",
            task_fence,
            Freshness::Fresh,
        );
        let CollectionReduction::Applied(projection) =
            reduce_collection(&plan, &[first], None).expect("reduction")
        else {
            panic!("fresh operation cannot be stale")
        };
        let replay_with_other_id = success(
            CollectionTaskKind::Balance,
            "other",
            task_fence,
            Freshness::Fresh,
        );
        assert!(matches!(
            reduce_collection(&plan, &[replay_with_other_id], Some(&projection)),
            Err(CollectionReducerError::WatermarkConflict)
        ));
    }

    #[test]
    fn revision_fence_rejects_negative_values() {
        assert!(matches!(
            RevisionFence::new(-1, 0, 0),
            Err(RevisionError::Negative {
                axis: RevisionAxis::Endpoint,
                value: -1
            })
        ));
    }
}
