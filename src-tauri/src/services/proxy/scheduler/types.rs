#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveMultiplierFact {
    pub station_key_id: String,
    pub value: f64,
    pub source: String,
    pub collected_at_ms: Option<i64>,
    pub valid_until_ms: Option<i64>,
    pub confidence: f64,
    pub group_binding_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiplierSourceFacts {
    pub station_key_id: String,
    pub manual_rate_multiplier: Option<f64>,
    pub manual_rate_updated_at: Option<String>,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
    pub collected_rate_multiplier: Option<f64>,
    pub collected_rate_source: Option<String>,
    pub collected_rate_confidence: Option<f64>,
    pub collected_rate_collected_at_ms: Option<i64>,
    pub collected_rate_valid_until_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiplierRejectReason {
    Missing,
    Invalid,
    Negative,
    Expired,
    UnboundGroup,
    LowConfidence,
}
