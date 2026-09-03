//! Stable, display-safe read models shared by Station, Key Pool and Pricing.
//!
//! These types intentionally contain only masked credentials and projected values.  They are
//! query results, not domain inputs; mutations continue to use the existing command DTOs.

use serde::Serialize;

use super::{
    collector::CollectorSnapshot,
    collector_runs::CollectorRun,
    credentials::StationCredentials,
    group_facts::{GroupRateRecord, StationGroupBinding},
    pricing::BalanceSnapshot,
    station_keys::KeyPoolItem,
    stations::Station,
};

pub(crate) const ASSET_READ_MODEL_SCHEMA_VERSION: u16 = 1;

/// Canonical server-side join identity. Display names are deliberately not accepted here.
pub(crate) fn group_identity_hash(identity: &str) -> Option<String> {
    let identity = identity.trim();
    if identity.is_empty() {
        return None;
    }
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"relay-pool:group-identity:v1:");
    hasher.update(identity.as_bytes());
    Some(format!("sha256:{:x}", hasher.finalize()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadModelPage {
    pub(crate) limit: u32,
    pub(crate) returned: u32,
    pub(crate) next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadModelEnvelope<T> {
    pub(crate) schema_version: u16,
    pub(crate) generated_at_ms: i64,
    pub(crate) domain_revision: i64,
    pub(crate) page: ReadModelPage,
    pub(crate) data: T,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationAssetReadRow {
    pub(crate) station: Station,
    pub(crate) keys: Vec<KeyPoolItem>,
    /// Server-issued identity used by pricing and monitoring joins.
    pub(crate) group_identity_hashes: Vec<String>,
    pub(crate) collection_summary: StationCollectionReadSummary,
    pub(crate) authorization_summary: StationAuthorizationReadSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationCollectionReadSummary {
    pub(crate) status: String,
    pub(crate) reason_codes: Vec<String>,
    pub(crate) revision: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationAuthorizationReadSummary {
    pub(crate) status: String,
    pub(crate) credential_revision: i64,
    pub(crate) reason_code: Option<String>,
    pub(crate) revision: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationAssetsReadModel {
    pub(crate) rows: Vec<StationAssetReadRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationDetailLimits {
    pub(crate) group_bindings: u32,
    pub(crate) group_rates: u32,
    pub(crate) collector_runs: u32,
    pub(crate) balances: u32,
    pub(crate) incidents: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationDetailIncident {
    pub(crate) id: String,
    pub(crate) event_type: String,
    pub(crate) lifecycle_state: String,
    pub(crate) severity: String,
    pub(crate) group_name: Option<String>,
    pub(crate) station_id: Option<String>,
    pub(crate) episode_number: i64,
    pub(crate) occurrence_count: i64,
    pub(crate) last_seen_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationDetailReadModel {
    pub(crate) asset: StationAssetReadRow,
    pub(crate) credentials: StationCredentials,
    pub(crate) group_bindings: Vec<StationGroupBinding>,
    pub(crate) group_rates: Vec<GroupRateRecord>,
    pub(crate) collector_runs: Vec<CollectorRun>,
    pub(crate) latest_snapshot: Option<CollectorSnapshot>,
    pub(crate) balances: Vec<BalanceSnapshot>,
    pub(crate) incidents: Vec<StationDetailIncident>,
    pub(crate) limits: StationDetailLimits,
}
