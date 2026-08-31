use crate::application::model_mapping::CandidateModelVariant;
use crate::application::operational_facts::pricing_projector::RoutingCostBasis;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AvailabilityTier {
    Primary,
    ConfiguredBackup,
    DepletedEmergency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionEvidence {
    pub code: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoutePlanCandidate {
    pub station_key_id: String,
    pub station_id: String,
    pub endpoint_revision: i64,
    /// Revision fences captured with the selected planning snapshot. Attempt
    /// finalization must use these, never re-read mutable current revisions.
    pub credential_revision: i64,
    pub account_revision: i64,
    pub group_binding_id: Option<String>,
    pub group_revision: Option<i64>,
    pub resolved_upstream_model: Option<String>,
    pub model_alias_revision: i64,
    pub model_variant: Option<CandidateModelVariant>,
    pub priority: i64,
    pub tier: AvailabilityTier,
    pub pricing: RoutePlanPricingSnapshot,
    pub evidence: Vec<DecisionEvidence>,
}

impl RoutePlanCandidate {
    pub(crate) fn routing_identity(&self) -> String {
        self.model_variant
            .as_ref()
            .map(CandidateModelVariant::identity_key)
            .unwrap_or_else(|| self.station_key_id.clone())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoutePlanPricingSnapshot {
    pub basis: RoutingCostBasis,
    pub rate_multiplier: Option<f64>,
    pub currency: Option<String>,
    pub unit: Option<String>,
    pub estimated_input_price: Option<f64>,
    pub estimated_output_price: Option<f64>,
    pub estimated_cache_creation_price: Option<f64>,
    pub estimated_cache_read_price: Option<f64>,
    pub status_label: String,
}

impl RoutePlanPricingSnapshot {
    pub(crate) fn unpriced(status_label: impl Into<String>) -> Self {
        Self {
            basis: RoutingCostBasis::Unpriced,
            rate_multiplier: None,
            currency: None,
            unit: None,
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_cache_creation_price: None,
            estimated_cache_read_price: None,
            status_label: status_label.into(),
        }
    }
}
