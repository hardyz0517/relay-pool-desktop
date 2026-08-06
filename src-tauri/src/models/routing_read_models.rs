//! Stable, display-safe read models shared by Station, Key Pool and Pricing.
//!
//! These types intentionally contain only masked credentials and projected values.  They are
//! query results, not domain inputs; mutations continue to use the existing command DTOs.

use serde::Serialize;

use super::{
    station_keys::KeyPoolItem, stations::Station,
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationAssetsReadModel {
    pub(crate) rows: Vec<StationAssetReadRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeyPoolReadModel {
    pub(crate) rows: Vec<KeyPoolItem>,
}
