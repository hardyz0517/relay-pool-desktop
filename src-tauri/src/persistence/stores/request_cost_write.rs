#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestCostAggregateWrite {
    pub(crate) request_id: String,
    pub(crate) status: String,
    pub(crate) totals_by_currency_json: String,
    pub(crate) compatibility_currency: Option<String>,
    pub(crate) compatibility_total_cost_micro: Option<i64>,
    pub(crate) incomplete_attempts_json: String,
    pub(crate) written_at_ms: i64,
}
