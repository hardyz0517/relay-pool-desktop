use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable machine-readable error code shared by collection producers and the
/// application-owned recovery projection. Keeping the value in the model
/// layer avoids coupling those two boundary owners to each other.
pub(crate) const MANUAL_AUTHORIZATION_ERROR_CODE: &str = "manual_authorization_required";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorSnapshot {
    pub id: String,
    pub station_id: String,
    pub endpoint_revision: i64,
    pub source: String,
    pub status: String,
    pub fetched_at: String,
    pub summary_json: Value,
    pub normalized_json: Value,
    pub raw_json_redacted: Option<Value>,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorEvent {
    pub event_type: String,
    pub message: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationRevision {
    pub scope: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutationReceipt {
    pub mutation_id: String,
    pub committed_at_ms: i64,
    pub affected_scopes: Vec<String>,
    pub revision_vector: Vec<MutationRevision>,
}

impl MutationReceipt {
    pub fn without_revision(mutation_id: impl Into<String>, committed_at_ms: i64) -> Self {
        Self {
            mutation_id: mutation_id.into(),
            committed_at_ms: committed_at_ms.max(0),
            affected_scopes: Vec::new(),
            revision_vector: Vec::new(),
        }
    }

    pub fn for_scope(
        mutation_id: impl Into<String>,
        committed_at_ms: i64,
        scope: impl Into<String>,
        revision: i64,
    ) -> Self {
        let scope = scope.into();
        Self {
            mutation_id: mutation_id.into(),
            committed_at_ms: committed_at_ms.max(0),
            affected_scopes: vec![scope.clone()],
            revision_vector: vec![MutationRevision { scope, revision }],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorRunResult {
    pub snapshot: CollectorSnapshot,
    pub events: Vec<CollectorEvent>,
    pub receipt: MutationReceipt,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationLoginTestInput {
    #[serde(default)]
    pub station_type: Option<String>,
    pub website_url: String,
    pub login_username: String,
    pub login_password: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationLoginTestResult {
    pub status: String,
    pub message: String,
    pub diagnosis: Option<String>,
    pub token_present: bool,
}
