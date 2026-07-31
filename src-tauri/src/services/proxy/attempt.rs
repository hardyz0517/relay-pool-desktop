#![allow(
    dead_code,
    reason = "Task 20 adds the explicit dual-terminal composition path before Task 22 production cutover"
)]

use tokio::task::JoinHandle;

use super::{
    lifecycle::{
        attempt::{AttemptContext, AttemptTerminal, AttemptTerminalRecord},
        delivery::DeliveryTerminal,
        request::PendingFinalRequestRecord,
        writer::{AttemptWriteReservation, RequestTerminalReservation},
    },
    limits::RequestLease,
    response_body::FinalizationOutcome,
};
use crate::services::time::now_millis_for_services;

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
    finalized: bool,
}

impl DualTerminalFinalizationLease {
    pub(crate) fn new(
        request: DownstreamRequestFinalizationLease,
        selected_attempt: Option<UpstreamAttemptFinalizationLease>,
    ) -> Self {
        Self {
            request: Some(request),
            selected_attempt,
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

            let request_ack = request.terminal.send(final_record).await;
            let _ = request_ack;
            drop(request.request_lease);
        }))
    }
}
