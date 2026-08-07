use super::routing_health::RouteHealthState;
use crate::models::proxy::ProxyStatus;
use crate::models::routing::{RouteEndpointKind, RoutingGroupFilter};
use serde::Serialize;

#[allow(unused_imports)]
pub(crate) use crate::application::operational_facts::candidate_projector::RouteCandidateProjection;

#[expect(
    dead_code,
    reason = "contract=local_routing_workspace.economics DTO; owner=application/routing; remove_when=workspace DTO removes economics fields"
)]
#[derive(Debug, Clone, Default)]
pub struct RouteCandidateEconomics {
    pub pricing_rule_id: Option<String>,
    pub pricing_model: Option<String>,
    pub group_binding_id: Option<String>,
    pub rate_multiplier: Option<f64>,
    pub normalization_status: Option<String>,
    pub price_confidence: Option<f64>,
    pub base_input_price: Option<f64>,
    pub base_output_price: Option<f64>,
    pub base_fixed_price: Option<f64>,
    pub estimated_input_price: Option<f64>,
    pub estimated_output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub price_currency: Option<String>,
    pub pricing_source: Option<String>,
    pub balance_status: Option<String>,
    pub balance_value: Option<f64>,
    pub low_balance_threshold: Option<f64>,
    pub balance_currency: Option<String>,
    pub balance_scope: Option<String>,
    pub balance_collected_at: Option<String>,
    pub economic_freshness: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionFactKind {
    Capability,
    Health,
    Pricing,
    Balance,
    Policy,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionFactSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionFact {
    pub kind: DecisionFactKind,
    pub label: String,
    pub value: String,
    pub severity: DecisionFactSeverity,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalRoutingPreviewKind {
    BaselineEligibility,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRoutingSettingsView {
    pub enabled: bool,
    pub bind_addr: String,
    pub port: u16,
    pub endpoint: RouteEndpointKind,
    pub policy: String,
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_filter: RoutingGroupFilter,
    pub fallback_enabled: bool,
    pub preview_kind: LocalRoutingPreviewKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRoutingSummary {
    pub candidate_count: i64,
    pub preview_eligible_candidate_count: i64,
    pub preview_excluded_candidate_count: i64,
    pub cooldown_candidate_count: i64,
    pub last_decision_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRoutingCandidateRow {
    pub station_key_id: String,
    pub station_id: String,
    pub station_name: String,
    pub key_name: String,
    pub endpoint: RouteEndpointKind,
    pub priority: i64,
    pub enabled: bool,
    pub schedulable: bool,
    pub health_state: RouteHealthState,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub cooldown_until: Option<String>,
    pub routing_group_scope: RoutingGroupFilter,
    pub routing_group_match: bool,
    pub preview_eligible: bool,
    pub preview_reject_reasons: Vec<String>,
    pub facts: Vec<DecisionFact>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDecisionSummary {
    pub id: String,
    pub decided_at: String,
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub selected_station_key_id: Option<String>,
    pub selected_station_id: Option<String>,
    pub selected_station_name: Option<String>,
    pub policy: String,
    pub status: RouteDecisionStatus,
    pub reason: String,
    pub fallback_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteDecisionStatus {
    Selected,
    Fallback,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDecisionEvent {
    pub id: String,
    pub decision_id: String,
    pub occurred_at: String,
    pub station_key_id: Option<String>,
    pub station_id: Option<String>,
    pub accepted: bool,
    pub facts: Vec<DecisionFact>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingPreviewWorkspace {
    pub proxy_status: ProxyStatus,
    pub settings: LocalRoutingSettingsView,
    pub summary: LocalRoutingSummary,
    pub candidates: Vec<LocalRoutingCandidateRow>,
    pub latest_decision: Option<RouteDecisionSummary>,
    pub recent_events: Vec<RouteDecisionEvent>,
}
