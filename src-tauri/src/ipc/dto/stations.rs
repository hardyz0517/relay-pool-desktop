use serde::{Deserialize, Serialize};

use crate::models::stations::Station;

use super::TypeDescriptor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StationDto {
    pub id: String,
    pub name: String,
    pub station_type: String,
    pub website_url: String,
    pub api_base_url: String,
    pub endpoint_revision: i64,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub api_key_masked: String,
    pub api_key_present: bool,
    pub key_count: i64,
    pub enabled: bool,
    pub priority: i64,
    pub credit_per_cny: f64,
    pub balance_raw: Option<f64>,
    pub balance_cny: Option<f64>,
    pub low_balance_threshold_cny: Option<f64>,
    pub collection_interval_minutes: u16,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub last_checked_at: Option<String>,
    pub last_pricing_fetched_at: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Station> for StationDto {
    fn from(value: Station) -> Self {
        Self {
            id: value.id,
            name: value.name,
            station_type: value.station_type,
            website_url: value.website_url,
            api_base_url: value.api_base_url,
            endpoint_revision: value.endpoint_revision,
            collector_proxy_mode: value.collector_proxy_mode,
            collector_proxy_url: value.collector_proxy_url,
            api_key_masked: value.api_key_masked,
            api_key_present: value.api_key_present,
            key_count: value.key_count,
            enabled: value.enabled,
            priority: value.priority,
            credit_per_cny: value.credit_per_cny,
            balance_raw: value.balance_raw,
            balance_cny: value.balance_cny,
            low_balance_threshold_cny: value.low_balance_threshold_cny,
            collection_interval_minutes: value.collection_interval_minutes,
            status: value.status,
            latency_ms: value.latency_ms,
            last_checked_at: value.last_checked_at,
            last_pricing_fetched_at: value.last_pricing_fetched_at,
            note: value.note,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub const STATION_TYPE: TypeDescriptor = TypeDescriptor {
    name: "StationDto",
    typescript: r#"export type StationDto = {
  id: string;
  name: string;
  stationType: string;
  websiteUrl: string;
  apiBaseUrl: string;
  endpointRevision: number;
  collectorProxyMode: string;
  collectorProxyUrl: string | null;
  apiKeyMasked: string;
  apiKeyPresent: boolean;
  keyCount: number;
  enabled: boolean;
  priority: number;
  creditPerCny: number;
  balanceRaw: number | null;
  balanceCny: number | null;
  lowBalanceThresholdCny: number | null;
  collectionIntervalMinutes: number;
  status: string;
  latencyMs: number | null;
  lastCheckedAt: string | null;
  lastPricingFetchedAt: string | null;
  note: string | null;
  createdAt: string;
  updatedAt: string;
};"#,
};

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn fixture() -> StationDto {
    StationDto {
        id: "station-fixture".into(),
        name: "Fixture Station".into(),
        station_type: "newapi".into(),
        website_url: "https://provider.invalid".into(),
        api_base_url: "https://provider.invalid/v1".into(),
        endpoint_revision: 1,
        collector_proxy_mode: "inherit".into(),
        collector_proxy_url: None,
        api_key_masked: "sk-fixture-...redacted".into(),
        api_key_present: true,
        key_count: 1,
        enabled: true,
        priority: 0,
        credit_per_cny: 1.0,
        balance_raw: None,
        balance_cny: None,
        low_balance_threshold_cny: Some(15.0),
        collection_interval_minutes: 5,
        status: "unchecked".into(),
        latency_ms: None,
        last_checked_at: None,
        last_pricing_fetched_at: None,
        note: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}
