#![allow(
    dead_code,
    reason = "Task 20 adds the explicit dual-terminal composition path before Task 22 production cutover"
)]

use tokio::task::JoinHandle;

use super::{
    finalization::FinalizationOutcome,
    lifecycle::{
        attempt::{AttemptContext, AttemptTerminal, AttemptTerminalRecord},
        delivery::DeliveryTerminal,
        request::{AttemptId, PendingFinalRequestRecord, RequestLogAnnotations},
        writer::{
            AttemptCostWriteReservation, AttemptWriteReservation,
            RequestCostAggregateWriteReservation, RequestTerminalReservation,
        },
    },
    limits::RequestLease,
};
use crate::{
    application::request_finalization::{
        outcome::{
            aggregate_request_costs, AttemptCostSnapshot, AttemptCostStatus, AttemptUsageSnapshot,
            AttemptUsageStatus, FrozenPricingAssessment, FrozenPricingBasis, TokenUsage,
        },
        outcome_orchestrator::{
            attempt_cost_commit_record, interrupted_attempt_cost,
            request_cost_aggregate_commit_record,
        },
    },
    observability::correlation,
    services::time::now_millis_for_services,
};

pub(crate) struct UpstreamAttemptFinalizationLease {
    reservation: AttemptWriteReservation,
    context: AttemptContext,
    probe_scope: Option<crate::application::health_protection::HealthProtectionScope>,
    probe_state_revision: Option<u64>,
}

impl UpstreamAttemptFinalizationLease {
    pub(crate) fn new(
        reservation: AttemptWriteReservation,
        context: AttemptContext,
        probe_scope: Option<crate::application::health_protection::HealthProtectionScope>,
        probe_state_revision: Option<u64>,
    ) -> Self {
        Self {
            reservation,
            context,
            probe_scope,
            probe_state_revision,
        }
    }
}

pub(crate) struct DownstreamRequestFinalizationLease {
    terminal: RequestTerminalReservation,
    request_lease: RequestLease,
}

impl DownstreamRequestFinalizationLease {
    pub(crate) fn new(terminal: RequestTerminalReservation, request_lease: RequestLease) -> Self {
        Self {
            terminal,
            request_lease,
        }
    }
}

pub(crate) struct DualTerminalFinalizationLease {
    request: Option<DownstreamRequestFinalizationLease>,
    selected_attempt: Option<UpstreamAttemptFinalizationLease>,
    costs: Option<CostFinalizationReservations>,
    finalized: bool,
}

impl DualTerminalFinalizationLease {
    pub(crate) fn new(
        request: DownstreamRequestFinalizationLease,
        selected_attempt: Option<UpstreamAttemptFinalizationLease>,
        costs: Option<CostFinalizationReservations>,
    ) -> Self {
        Self {
            request: Some(request),
            selected_attempt,
            costs,
            finalized: false,
        }
    }

    pub(crate) fn finalize(
        mut self,
        record: PendingFinalRequestRecord,
        delivery: DeliveryTerminal,
        outcome: FinalizationOutcome,
        attempt_terminal: Option<AttemptTerminal>,
        output_committed: bool,
    ) -> Option<JoinHandle<()>> {
        if self.finalized {
            return None;
        }
        self.finalized = true;
        let request = self.request.take()?;
        let costs = self.costs.take();
        let attempt =
            self.selected_attempt
                .take()
                .zip(attempt_terminal)
                .map(|(attempt, terminal)| {
                    (
                        AttemptTerminalRecord {
                            context: attempt.context,
                            terminal,
                            output_committed,
                            terminal_at_ms: now_millis_for_services() as i64,
                            probe_scope: attempt.probe_scope,
                            probe_state_revision: attempt.probe_state_revision,
                        },
                        attempt.reservation,
                    )
                });
        let correlation_id = correlation::current_or_new();
        Some(tokio::spawn(async move {
            let attempt_persisted = if let Some((attempt_record, attempt_reservation)) = attempt {
                let attempt_ack = attempt_reservation.send(attempt_record).await;
                match attempt_ack {
                    Ok(Ok(_)) => true,
                    Ok(Err(_)) => {
                        record_finalization_failure(
                            "proxy.finalization.attempt_persistence_failed",
                            &correlation_id,
                        );
                        false
                    }
                    Err(_) => {
                        record_finalization_failure(
                            "proxy.finalization.attempt_ack_dropped",
                            &correlation_id,
                        );
                        false
                    }
                }
            } else {
                true
            };

            if attempt_persisted {
                if let Some(costs) = costs {
                    if !costs.write(&record).await {
                        record_finalization_failure(
                            "proxy.finalization.cost_persistence_failed",
                            &correlation_id,
                        );
                    }
                }

                let final_record = match outcome {
                    FinalizationOutcome::Completed => record.complete(delivery),
                    FinalizationOutcome::Failed { code, detail } => {
                        record.fail(code, detail, delivery)
                    }
                    FinalizationOutcome::Interrupted { detail } => {
                        record.interrupt(delivery, detail)
                    }
                };
                persist_request_terminal(request, final_record, &correlation_id).await;
            } else {
                // The request still needs one durable terminal for lifecycle
                // recovery, but it must not claim the selected attempt's
                // successful outcome when that attempt write was unavailable.
                let final_record = record.interrupt(
                    delivery,
                    Some("selected_attempt_terminal_persistence_failed".to_string()),
                );
                persist_request_terminal(request, final_record, &correlation_id).await;
                return;
            }
        }))
    }
}

async fn persist_request_terminal(
    request: DownstreamRequestFinalizationLease,
    record: crate::services::proxy::lifecycle::request::FinalRequestRecord,
    correlation_id: &correlation::CorrelationId,
) {
    let request_ack = request.terminal.send(record).await;
    match request_ack {
        Ok(Ok(_)) => {}
        Ok(Err(_)) => record_finalization_failure(
            "proxy.finalization.request_persistence_failed",
            correlation_id,
        ),
        Err(_) => {
            record_finalization_failure("proxy.finalization.request_ack_dropped", correlation_id)
        }
    }
    drop(request.request_lease);
}

fn record_finalization_failure(code: &'static str, correlation_id: &correlation::CorrelationId) {
    correlation::with_scope(code, correlation_id.clone(), || ());
}

pub(crate) struct CostFinalizationReservations {
    attempt_costs: Vec<(u16, AttemptCostWriteReservation)>,
    aggregate: RequestCostAggregateWriteReservation,
    selected_attempt_cost: Option<SelectedAttemptCostSnapshot>,
}

impl CostFinalizationReservations {
    pub(crate) fn new(
        attempt_costs: Vec<(u16, AttemptCostWriteReservation)>,
        aggregate: RequestCostAggregateWriteReservation,
        selected_attempt_cost: Option<SelectedAttemptCostSnapshot>,
    ) -> Self {
        Self {
            attempt_costs,
            aggregate,
            selected_attempt_cost,
        }
    }

    async fn write(self, record: &PendingFinalRequestRecord) -> bool {
        let now_ms = now_millis_for_services() as i64;
        let request_id = record.context().request_id.clone();
        let attempted_ordinals = self
            .attempt_costs
            .iter()
            .map(|(ordinal, _)| *ordinal)
            .collect::<Vec<_>>();
        let durable_costs = attempted_ordinals
            .iter()
            .map(|ordinal| {
                self.cost_snapshot_for_ordinal(
                    AttemptId::new(request_id.clone(), *ordinal),
                    record.annotations(),
                )
            })
            .collect::<Vec<_>>();
        for (ordinal, reservation) in self.attempt_costs {
            let record = durable_costs
                .iter()
                .find(|snapshot| snapshot.attempt_id.ordinal == ordinal)
                .map(|snapshot| attempt_cost_commit_record(snapshot, now_ms))
                .unwrap_or_else(|| {
                    interrupted_attempt_cost(AttemptId::new(request_id.clone(), ordinal), now_ms)
                });
            match reservation.send(record).await {
                Ok(Ok(_)) => {}
                Ok(Err(_)) | Err(_) => return false,
            }
        }

        let Ok(aggregate) = aggregate_request_costs(
            &attempted_ordinals
                .iter()
                .map(|ordinal| AttemptId::new(request_id.clone(), *ordinal))
                .collect::<Vec<_>>(),
            &durable_costs,
        ) else {
            return false;
        };
        let aggregate = request_cost_aggregate_commit_record(request_id, &aggregate, now_ms);
        match self.aggregate.send(aggregate).await {
            Ok(Ok(_)) => true,
            Ok(Err(_)) | Err(_) => false,
        }
    }

    fn cost_snapshot_for_ordinal(
        &self,
        attempt_id: AttemptId,
        annotations: &RequestLogAnnotations,
    ) -> AttemptCostSnapshot {
        let Some(selected) = self
            .selected_attempt_cost
            .as_ref()
            .filter(|selected| selected.ordinal == attempt_id.ordinal)
        else {
            return interrupted_cost_snapshot(attempt_id);
        };
        selected.to_cost_snapshot(attempt_id, annotations)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SelectedAttemptCostSnapshot {
    pub(crate) ordinal: u16,
    pub(crate) pricing_basis: String,
    pub(crate) pricing_status_label: String,
    pub(crate) currency: Option<String>,
    pub(crate) unit: Option<String>,
    pub(crate) estimated_input_price: Option<f64>,
    pub(crate) estimated_output_price: Option<f64>,
    pub(crate) estimated_cache_creation_price: Option<f64>,
    pub(crate) estimated_cache_read_price: Option<f64>,
}

impl SelectedAttemptCostSnapshot {
    fn to_cost_snapshot(
        &self,
        attempt_id: AttemptId,
        annotations: &RequestLogAnnotations,
    ) -> AttemptCostSnapshot {
        let pricing = self.frozen_pricing();
        if self.pricing_basis == "not_applicable" {
            return AttemptCostSnapshot::unavailable(
                attempt_id,
                pricing,
                AttemptUsageSnapshot {
                    status: AttemptUsageStatus::NotApplicable,
                    tokens: None,
                },
                AttemptCostStatus::NotApplicable,
            )
            .expect("not-applicable cost snapshot");
        }

        let Some(usage) = complete_usage(annotations) else {
            return AttemptCostSnapshot::unavailable(
                attempt_id,
                pricing,
                AttemptUsageSnapshot {
                    status: AttemptUsageStatus::MissingUsage,
                    tokens: None,
                },
                AttemptCostStatus::MissingUsage,
            )
            .expect("missing-usage cost snapshot");
        };
        let usage_snapshot = AttemptUsageSnapshot {
            status: AttemptUsageStatus::Complete,
            tokens: Some(usage.clone()),
        };
        let Some(total_cost_micro) = self.total_cost_micro(&usage) else {
            return AttemptCostSnapshot::unavailable(
                attempt_id,
                pricing,
                usage_snapshot,
                AttemptCostStatus::PricingIncomplete,
            )
            .expect("pricing-incomplete cost snapshot");
        };
        match AttemptCostSnapshot::priced(
            attempt_id.clone(),
            pricing,
            usage_snapshot,
            total_cost_micro,
        ) {
            Ok(snapshot) => snapshot,
            Err(_) => interrupted_cost_snapshot(attempt_id),
        }
    }

    fn frozen_pricing(&self) -> FrozenPricingAssessment {
        FrozenPricingAssessment {
            pricing_context_id: format!("selected_attempt_{}", self.ordinal),
            basis: match self.pricing_basis.as_str() {
                "exact_price" => FrozenPricingBasis::ExactPrice,
                "multiplier_proxy" => FrozenPricingBasis::MultiplierProxy,
                "not_applicable" => FrozenPricingBasis::NotApplicable,
                _ => FrozenPricingBasis::Unpriced,
            },
            currency: self.currency.clone(),
            input_unit_price_micro: self
                .estimated_input_price
                .map(currency_units_to_micro)
                .map(clamp_f64_to_i64),
            output_unit_price_micro: self
                .estimated_output_price
                .map(currency_units_to_micro)
                .map(clamp_f64_to_i64),
            status_label: self.pricing_status_label.clone(),
        }
    }

    fn total_cost_micro(&self, usage: &TokenUsage) -> Option<i64> {
        if self.pricing_basis != "exact_price" {
            return None;
        }
        if self.unit.as_deref().is_some_and(|unit| {
            !unit.eq_ignore_ascii_case("M") && !unit.eq_ignore_ascii_case("per_1m_tokens")
        }) {
            return None;
        }
        self.currency.as_ref().filter(|value| !value.is_empty())?;
        let mut total = 0.0_f64;
        let mut has_component = false;
        if let Some(price) = self.estimated_input_price.filter(|value| value.is_finite()) {
            let input_tokens = usage.input_tokens.max(0);
            let cache_creation_tokens = usage
                .cache_creation_tokens
                .unwrap_or(0)
                .max(0)
                .min(input_tokens);
            let cache_read_tokens = usage
                .cache_read_tokens
                .unwrap_or(0)
                .max(0)
                .min(input_tokens.saturating_sub(cache_creation_tokens));
            let separately_priced_cache_tokens =
                cache_creation_tokens.saturating_add(cache_read_tokens);
            let regular_input_tokens = input_tokens.saturating_sub(separately_priced_cache_tokens);
            total += currency_units_to_micro(price) * regular_input_tokens as f64 / 1_000_000.0;
            if cache_creation_tokens > 0 {
                let cache_price = self
                    .estimated_cache_creation_price
                    .filter(|value| value.is_finite())
                    .unwrap_or(price);
                total += currency_units_to_micro(cache_price) * cache_creation_tokens as f64
                    / 1_000_000.0;
            }
            if cache_read_tokens > 0 {
                let cache_price = self
                    .estimated_cache_read_price
                    .filter(|value| value.is_finite())
                    .unwrap_or(price);
                total +=
                    currency_units_to_micro(cache_price) * cache_read_tokens as f64 / 1_000_000.0;
            }
            has_component = true;
        }
        if let Some(price) = self
            .estimated_output_price
            .filter(|value| value.is_finite())
        {
            total +=
                currency_units_to_micro(price) * usage.output_tokens.max(0) as f64 / 1_000_000.0;
            has_component = true;
        }
        has_component.then(|| clamp_f64_to_i64(total))
    }
}

fn complete_usage(annotations: &RequestLogAnnotations) -> Option<TokenUsage> {
    let input_tokens = annotations.prompt_tokens?;
    let output_tokens = annotations.completion_tokens?;
    let total_tokens = annotations
        .total_tokens
        .unwrap_or_else(|| input_tokens.saturating_add(output_tokens));
    Some(TokenUsage {
        input_tokens,
        output_tokens,
        total_tokens,
        cache_creation_tokens: annotations.cache_creation_tokens,
        cache_read_tokens: annotations.cache_read_tokens,
    })
}

fn interrupted_cost_snapshot(attempt_id: AttemptId) -> AttemptCostSnapshot {
    AttemptCostSnapshot::unavailable(
        attempt_id.clone(),
        FrozenPricingAssessment {
            pricing_context_id: "trace_incomplete".to_string(),
            basis: FrozenPricingBasis::Unpriced,
            currency: None,
            input_unit_price_micro: None,
            output_unit_price_micro: None,
            status_label: "trace_incomplete".to_string(),
        },
        AttemptUsageSnapshot {
            status: AttemptUsageStatus::MissingUsage,
            tokens: None,
        },
        AttemptCostStatus::MissingUsage,
    )
    .expect("interrupted cost snapshot")
}

fn currency_units_to_micro(value: f64) -> f64 {
    value * 1_000_000.0
}

fn clamp_f64_to_i64(value: f64) -> i64 {
    value.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_token_pricing_preserves_explicit_zero_usage_as_zero_cost() {
        let pricing = SelectedAttemptCostSnapshot {
            ordinal: 0,
            pricing_basis: "exact_price".to_string(),
            pricing_status_label: "priced".to_string(),
            currency: Some("USD".to_string()),
            unit: Some("per_1m_tokens".to_string()),
            estimated_input_price: Some(5.0),
            estimated_output_price: Some(30.0),
            estimated_cache_creation_price: None,
            estimated_cache_read_price: None,
        };
        let usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cache_creation_tokens: None,
            cache_read_tokens: None,
        };

        assert_eq!(pricing.total_cost_micro(&usage), Some(0));
    }

    fn exact_token_pricing_with_cache_rates() -> SelectedAttemptCostSnapshot {
        SelectedAttemptCostSnapshot {
            ordinal: 0,
            pricing_basis: "exact_price".to_string(),
            pricing_status_label: "priced".to_string(),
            currency: Some("USD".to_string()),
            unit: Some("M".to_string()),
            estimated_input_price: Some(1.0),
            estimated_output_price: Some(10.0),
            estimated_cache_creation_price: Some(1.25),
            estimated_cache_read_price: Some(0.1),
        }
    }

    #[test]
    fn short_m_unit_prices_cache_read_tokens_separately() {
        let pricing = exact_token_pricing_with_cache_rates();
        let usage = TokenUsage {
            input_tokens: 10_000,
            output_tokens: 0,
            total_tokens: 10_000,
            cache_creation_tokens: None,
            cache_read_tokens: Some(9_000),
        };

        assert_eq!(pricing.total_cost_micro(&usage), Some(1_900));
    }

    #[test]
    fn cache_creation_tokens_use_the_cache_creation_rate() {
        let pricing = exact_token_pricing_with_cache_rates();
        let usage = TokenUsage {
            input_tokens: 10_000,
            output_tokens: 0,
            total_tokens: 10_000,
            cache_creation_tokens: Some(8_000),
            cache_read_tokens: None,
        };

        assert_eq!(pricing.total_cost_micro(&usage), Some(12_000));
    }

    #[test]
    fn missing_cache_rate_falls_back_to_the_input_rate() {
        let mut pricing = exact_token_pricing_with_cache_rates();
        pricing.estimated_cache_read_price = None;
        let usage = TokenUsage {
            input_tokens: 10_000,
            output_tokens: 0,
            total_tokens: 10_000,
            cache_creation_tokens: None,
            cache_read_tokens: Some(9_000),
        };

        assert_eq!(pricing.total_cost_micro(&usage), Some(10_000));
    }
}
