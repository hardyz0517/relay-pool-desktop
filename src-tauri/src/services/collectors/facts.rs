use serde_json::Value;

#[derive(Debug, Clone)]
pub struct CollectedBalanceFact {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub scope: String,
    pub value: Option<f64>,
    pub used_value: Option<f64>,
    pub total_value: Option<f64>,
    pub today_request_count: Option<i64>,
    pub total_request_count: Option<i64>,
    pub today_consumption: Option<f64>,
    pub total_consumption: Option<f64>,
    pub today_base_consumption: Option<f64>,
    pub total_base_consumption: Option<f64>,
    pub today_token_count: Option<i64>,
    pub total_token_count: Option<i64>,
    pub today_input_token_count: Option<i64>,
    pub today_output_token_count: Option<i64>,
    pub total_input_token_count: Option<i64>,
    pub total_output_token_count: Option<i64>,
    pub currency: String,
    pub credit_unit: Option<String>,
    pub status: String,
    pub source: String,
    pub confidence: f64,
    pub collected_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CollectedGroupFact {
    pub station_id: String,
    pub group_id: Option<String>,
    pub group_key_hash: String,
    pub group_name: String,
    pub visibility: String,
    pub inferred_group_category: Option<String>,
    pub source: String,
    pub confidence: f64,
    pub raw_json_redacted: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct CollectedRateFact {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub group_id: Option<String>,
    pub group_key_hash: String,
    pub group_name: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub source: String,
    pub confidence: f64,
    pub checked_at: Option<String>,
    pub raw_json_redacted: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct CollectedModelFact {
    pub station_id: String,
    pub model: String,
    pub available: bool,
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct CollectorDiagnosticFact {
    pub endpoint: String,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub error_code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ManualActionRequiredFact {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct CollectorFacts {
    pub balances: Vec<CollectedBalanceFact>,
    pub groups: Vec<CollectedGroupFact>,
    pub rates: Vec<CollectedRateFact>,
    pub models: Vec<CollectedModelFact>,
    pub diagnostics: Vec<CollectorDiagnosticFact>,
    pub manual_action: Option<ManualActionRequiredFact>,
}
