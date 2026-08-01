use crate::models::proxy::ProxyStatus;
use crate::models::routing::{RouteEndpointKind, RoutingGroupFilter};
use serde::Serialize;

#[allow(unused_imports)]
pub(crate) use crate::application::operational_facts::candidate_projector::RouteCandidateProjection;

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub(crate) struct RouteCandidateEconomics {
    pub(crate) pricing_rule_id: Option<String>,
    pub(crate) pricing_model: Option<String>,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) rate_multiplier: Option<f64>,
    pub(crate) normalization_status: Option<String>,
    pub(crate) price_confidence: Option<f64>,
    pub(crate) base_input_price: Option<f64>,
    pub(crate) base_output_price: Option<f64>,
    pub(crate) base_fixed_price: Option<f64>,
    pub(crate) estimated_input_price: Option<f64>,
    pub(crate) estimated_output_price: Option<f64>,
    pub(crate) fixed_price: Option<f64>,
    pub(crate) price_currency: Option<String>,
    pub(crate) pricing_source: Option<String>,
    pub(crate) balance_status: Option<String>,
    pub(crate) balance_value: Option<f64>,
    pub(crate) low_balance_threshold: Option<f64>,
    pub(crate) balance_currency: Option<String>,
    pub(crate) balance_scope: Option<String>,
    pub(crate) balance_collected_at: Option<String>,
    pub(crate) economic_freshness: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteHealthState {
    Ready,
    Cooldown,
    Degraded,
    Offline,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionFactKind {
    Capability,
    Health,
    Pricing,
    Balance,
    Policy,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionFactSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DecisionFact {
    pub(crate) kind: DecisionFactKind,
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) severity: DecisionFactSeverity,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LocalRoutingPreviewKind {
    BaselineEligibility,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalRoutingSettingsView {
    pub(crate) enabled: bool,
    pub(crate) bind_addr: String,
    pub(crate) port: u16,
    pub(crate) endpoint: RouteEndpointKind,
    pub(crate) policy: String,
    pub(crate) max_rate_multiplier: Option<f64>,
    pub(crate) routing_group_filter: RoutingGroupFilter,
    pub(crate) fallback_enabled: bool,
    pub(crate) preview_kind: LocalRoutingPreviewKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalRoutingSummary {
    pub(crate) candidate_count: i64,
    pub(crate) preview_eligible_candidate_count: i64,
    pub(crate) preview_excluded_candidate_count: i64,
    pub(crate) cooldown_candidate_count: i64,
    pub(crate) last_decision_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalRoutingCandidateRow {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) station_name: String,
    pub(crate) key_name: String,
    pub(crate) endpoint: RouteEndpointKind,
    pub(crate) priority: i64,
    pub(crate) enabled: bool,
    pub(crate) schedulable: bool,
    pub(crate) health_state: RouteHealthState,
    pub(crate) last_success_at: Option<String>,
    pub(crate) last_failure_at: Option<String>,
    pub(crate) cooldown_until: Option<String>,
    pub(crate) routing_group_scope: RoutingGroupFilter,
    pub(crate) routing_group_match: bool,
    pub(crate) preview_eligible: bool,
    pub(crate) preview_reject_reasons: Vec<String>,
    pub(crate) facts: Vec<DecisionFact>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteDecisionSummary {
    pub(crate) id: String,
    pub(crate) decided_at: String,
    pub(crate) endpoint: RouteEndpointKind,
    pub(crate) model: Option<String>,
    pub(crate) selected_station_key_id: Option<String>,
    pub(crate) selected_station_id: Option<String>,
    pub(crate) selected_station_name: Option<String>,
    pub(crate) policy: String,
    pub(crate) status: RouteDecisionStatus,
    pub(crate) reason: String,
    pub(crate) fallback_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteDecisionStatus {
    Selected,
    Fallback,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteDecisionEvent {
    pub(crate) id: String,
    pub(crate) decision_id: String,
    pub(crate) occurred_at: String,
    pub(crate) station_key_id: Option<String>,
    pub(crate) station_id: Option<String>,
    pub(crate) accepted: bool,
    pub(crate) facts: Vec<DecisionFact>,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalRoutingWorkspace {
    pub(crate) proxy_status: ProxyStatus,
    pub(crate) settings: LocalRoutingSettingsView,
    pub(crate) summary: LocalRoutingSummary,
    pub(crate) candidates: Vec<LocalRoutingCandidateRow>,
    pub(crate) latest_decision: Option<RouteDecisionSummary>,
    pub(crate) recent_events: Vec<RouteDecisionEvent>,
}
