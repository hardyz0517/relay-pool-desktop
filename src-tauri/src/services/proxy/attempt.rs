#![allow(
    dead_code,
    reason = "Task 20 adds the explicit dual-terminal composition path before Task 22 production cutover"
)]

use tokio::task::JoinHandle;

use super::{
    lifecycle::{
        attempt::{AttemptContext, AttemptTerminal, AttemptTerminalRecord},
        delivery::DeliveryTerminal,
        ports::RequestCostAggregateCommitRecord,
        request::{AttemptId, PendingFinalRequestRecord},
        writer::{
            AttemptCostWriteReservation, AttemptWriteReservation,
            RequestCostAggregateWriteReservation, RequestTerminalReservation,
        },
    },
    limits::RequestLease,
    response_body::FinalizationOutcome,
};
use crate::{
    application::request_finalization::outcome_orchestrator::interrupted_attempt_cost,
    services::time::now_millis_for_services,
};

pub(crate) struct UpstreamAttemptFinalizationLease {
    reservation: AttemptWriteReservation,
    context: AttemptContext,
}

impl UpstreamAttemptFinalizationLease {
    pub(crate) fn new(reservation: AttemptWriteReservation, context: AttemptContext) -> Self {
        Self {
            reservation,
            context,
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
        let request_id = record.context().request_id.clone();
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
                        },
                        attempt.reservation,
                    )
                });
        let final_record = match outcome {
            FinalizationOutcome::Completed => record.complete(delivery),
            FinalizationOutcome::Failed { code, detail } => record.fail(code, detail, delivery),
            FinalizationOutcome::Interrupted { detail } => record.interrupt(delivery, detail),
        };

        Some(tokio::spawn(async move {
            if let Some((attempt_record, attempt_reservation)) = attempt {
                let attempt_ack = attempt_reservation.send(attempt_record).await;
                match attempt_ack {
                    Ok(Ok(_)) => {}
                    Ok(Err(_)) | Err(_) => {
                        drop(request.request_lease);
                        return;
                    }
                }
            }

            if let Some(costs) = costs {
                if !costs.write(request_id).await {
                    drop(request.request_lease);
                    return;
                }
            }

            let request_ack = request.terminal.send(final_record).await;
            let _ = request_ack;
            drop(request.request_lease);
        }))
    }
}

pub(crate) struct CostFinalizationReservations {
    attempt_costs: Vec<(u16, AttemptCostWriteReservation)>,
    aggregate: RequestCostAggregateWriteReservation,
}

impl CostFinalizationReservations {
    pub(crate) fn new(
        attempt_costs: Vec<(u16, AttemptCostWriteReservation)>,
        aggregate: RequestCostAggregateWriteReservation,
    ) -> Self {
        Self {
            attempt_costs,
            aggregate,
        }
    }

    async fn write(self, request_id: String) -> bool {
        let now_ms = now_millis_for_services() as i64;
        let attempted_ordinals = self
            .attempt_costs
            .iter()
            .map(|(ordinal, _)| *ordinal)
            .collect::<Vec<_>>();
        for (ordinal, reservation) in self.attempt_costs {
            let record =
                interrupted_attempt_cost(AttemptId::new(request_id.clone(), ordinal), now_ms);
            match reservation.send(record).await {
                Ok(Ok(_)) => {}
                Ok(Err(_)) | Err(_) => return false,
            }
        }

        let aggregate = request_cost_gap_aggregate_record(request_id, attempted_ordinals, now_ms);
        match self.aggregate.send(aggregate).await {
            Ok(Ok(_)) => true,
            Ok(Err(_)) | Err(_) => false,
        }
    }
}

fn request_cost_gap_aggregate_record(
    request_id: String,
    attempted_ordinals: Vec<u16>,
    written_at_ms: i64,
) -> RequestCostAggregateCommitRecord {
    let incomplete_attempts = attempted_ordinals
        .iter()
        .map(|ordinal| {
            serde_json::json!({
                "request_id": request_id,
                "ordinal": ordinal,
                "status": "missing_usage",
            })
        })
        .collect::<Vec<_>>();
    RequestCostAggregateCommitRecord {
        request_id,
        status: if attempted_ordinals.is_empty() {
            "no_attempts".to_string()
        } else {
            "incomplete".to_string()
        },
        totals_by_currency_json: "{}".to_string(),
        compatibility_currency: None,
        compatibility_total_cost_micro: None,
        incomplete_attempts_json: serde_json::to_string(&incomplete_attempts)
            .expect("cost gap aggregate serializes"),
        written_at_ms,
    }
}
