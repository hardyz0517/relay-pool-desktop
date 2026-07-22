use std::sync::Arc;

use futures_util::future::BoxFuture;

use crate::{
    application::clock::{Clock, SystemClock},
    application::request_lifecycle::{
        attempt::{AttemptTerminal, AttemptTerminalRecord, HealthEffect},
        ports::{
            AttemptCommitAck, LifecycleWriteError, RequestCommitAck, RequestLifecycleStore,
            RequestStartAck,
        },
        request::{FinalRequestRecord, RequestStartRecord, RequestTerminal},
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::request_log_store::{
            AttemptPersistenceResult, RequestLogStore, RequestStartPersistenceResult,
            RequestTerminalPersistenceResult,
        },
        stores::request_log_write::{
            AttemptHealthUpdate, AttemptTerminalWrite, RequestLogAnnotationsWrite,
            RequestStartWrite, RequestTerminalWrite,
        },
    },
};

#[derive(Clone)]
pub(crate) struct RequestFinalizationService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
}

impl RequestFinalizationService {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            clock: Arc::new(SystemClock),
        }
    }
}

impl RequestLifecycleStore for RequestFinalizationService {
    fn start_request(
        &self,
        record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        let runtime = self.runtime.clone();
        let created_at_ms = self.clock.now_utc().timestamp_millis();
        let write = map_request_start(record);
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: RequestStartPersistenceResult = RequestLogStore
                .start_request(&mut session, &write, created_at_ms)
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
            Ok(RequestStartAck {
                inserted: outcome.inserted,
            })
        })
    }

    fn finish_attempt(
        &self,
        record: AttemptTerminalRecord,
    ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
        let runtime = self.runtime.clone();
        let write = map_attempt_terminal(record);
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: AttemptPersistenceResult = RequestLogStore
                .finish_attempt(&mut session, &write)
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
            Ok(AttemptCommitAck {
                inserted: outcome.inserted,
                health_applied: outcome.health_applied,
            })
        })
    }

    fn finish_request(
        &self,
        record: FinalRequestRecord,
    ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
        let runtime = self.runtime.clone();
        let terminal_at_ms = self.clock.now_utc().timestamp_millis();
        let write = map_request_terminal(record, terminal_at_ms);
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: RequestTerminalPersistenceResult = RequestLogStore
                .finish_request(&mut session, &write)
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
            Ok(RequestCommitAck {
                finalized: outcome.finalized,
            })
        })
    }
}

fn map_request_start(record: RequestStartRecord) -> RequestStartWrite {
    RequestStartWrite {
        request_id: record.context.request_id,
        method: record.context.method,
        local_path: record.context.local_path,
        endpoint: record.context.endpoint,
        received_at_ms: record.context.received_at_ms,
    }
}

fn map_attempt_terminal(record: AttemptTerminalRecord) -> AttemptTerminalWrite {
    let (
        terminal_kind,
        failure_kind,
        failure_blame,
        retry_disposition,
        health_effect,
        health_update,
        public_code,
        sanitized_detail,
    ) = match record.terminal {
        AttemptTerminal::Succeeded => (
            "succeeded".to_string(),
            None,
            None,
            None,
            "success".to_string(),
            AttemptHealthUpdate::Success,
            None,
            None,
        ),
        AttemptTerminal::Failed(failure) => {
            let health_update = match failure.health {
                HealthEffect::Success => AttemptHealthUpdate::Success,
                HealthEffect::ObserveFailure => AttemptHealthUpdate::ObserveFailure,
                HealthEffect::Cooldown { retry_after_ms } => {
                    AttemptHealthUpdate::Cooldown { retry_after_ms }
                }
                HealthEffect::HardFail => AttemptHealthUpdate::HardFail,
                HealthEffect::Neutral => AttemptHealthUpdate::Neutral,
            };
            (
                "failed".to_string(),
                Some(format!("{:?}", failure.kind)),
                Some(format!("{:?}", failure.blame)),
                Some(format!("{:?}", failure.retry)),
                format!("{:?}", failure.health),
                health_update,
                Some(failure.public_code),
                failure.sanitized_detail,
            )
        }
        AttemptTerminal::Abandoned { reason } => (
            "abandoned".to_string(),
            None,
            None,
            Some("StopRequest".to_string()),
            "neutral".to_string(),
            AttemptHealthUpdate::Neutral,
            Some(reason),
            None,
        ),
    };

    AttemptTerminalWrite {
        request_id: record.context.attempt_id.request_id,
        ordinal: record.context.attempt_id.ordinal,
        station_id: record.context.station_id,
        station_key_id: record.context.station_key_id,
        endpoint_revision: record.context.endpoint_revision,
        started_at_ms: record.context.started_at_ms,
        terminal_kind,
        failure_kind,
        failure_blame,
        retry_disposition,
        health_effect,
        health_cooldown_until_ms: None,
        health_update,
        public_code,
        sanitized_detail,
        output_committed: record.output_committed,
        terminal_at_ms: record.terminal_at_ms,
    }
}

fn map_request_terminal(record: FinalRequestRecord, terminal_at_ms: i64) -> RequestTerminalWrite {
    let (
        status,
        lifecycle_status,
        terminal_kind,
        terminal_code,
        terminal_detail,
        protocol_completed,
    ) = match record.terminal.terminal {
        RequestTerminal::Completed(_) => (
            "success",
            "completed",
            "completed",
            Some("request_completed".to_string()),
            None,
            true,
        ),
        RequestTerminal::PartialSuccess(_) => (
            "success",
            "partial_success",
            "partial_success",
            Some("request_partial_success".to_string()),
            None,
            true,
        ),
        RequestTerminal::Failed(failure) => (
            "failed",
            "failed",
            "failed",
            Some(failure.code),
            failure.detail,
            false,
        ),
        RequestTerminal::Interrupted(failure) => (
            "interrupted",
            "interrupted",
            "interrupted",
            Some(format!("{:?}", failure.terminal)),
            failure
                .detail
                .or_else(|| Some("downstream disconnected".to_string())),
            false,
        ),
    };
    let annotations = record.annotations;

    RequestTerminalWrite {
        request_id: record.context.request_id,
        received_at_ms: record.context.received_at_ms,
        status: status.to_string(),
        lifecycle_status: lifecycle_status.to_string(),
        terminal_kind: terminal_kind.to_string(),
        terminal_code,
        terminal_detail,
        protocol_completed,
        delivery_terminal: format!("{:?}", record.terminal.delivery),
        selected_attempt_ordinal: record.selected_attempt_id.map(|attempt| attempt.ordinal),
        attempt_count: record.attempt_count,
        fallback_count: record.fallback_count,
        terminal_at_ms,
        annotations: RequestLogAnnotationsWrite {
            model: annotations.model,
            stream: annotations.stream,
            selected_station_key_id: annotations.selected_station_key_id,
            selected_station_id: annotations.selected_station_id,
            upstream_base_url: annotations.upstream_base_url,
            route_policy: annotations.route_policy,
            route_reason: annotations.route_reason,
            rejected_candidates_json: annotations.rejected_candidates_json,
            body_bytes: annotations.body_bytes,
            route_wait_ms: annotations.route_wait_ms,
            upstream_headers_ms: annotations.upstream_headers_ms,
            failure_source: annotations.failure_source,
            attempts_json: annotations.attempts_json,
            completion_source: annotations.completion_source,
            prompt_tokens: annotations.prompt_tokens,
            completion_tokens: annotations.completion_tokens,
            total_tokens: annotations.total_tokens,
            cache_creation_tokens: annotations.cache_creation_tokens,
            cache_read_tokens: annotations.cache_read_tokens,
            reasoning_effort: annotations.reasoning_effort,
            first_token_ms: annotations.first_token_ms,
        },
    }
}

fn map_persistence_error(error: PersistenceError) -> LifecycleWriteError {
    match error {
        PersistenceError::CommitOutcomeUnknown => LifecycleWriteError::CommitOutcomeUnknown(
            "request lifecycle commit outcome is unknown".to_string(),
        ),
        other => LifecycleWriteError::Unavailable(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::request_lifecycle::{
        attempt::{
            AttemptContext, AttemptFailureKind, ClassifiedAttemptFailure, FailureBlame,
            RetryDisposition,
        },
        delivery::DeliveryTerminal,
        request::{
            AttemptId, DeliveryFailure, RequestContextSnapshot, RequestLogAnnotations,
            RequestTerminalSnapshot,
        },
    };

    fn context(request_id: &str) -> RequestContextSnapshot {
        RequestContextSnapshot {
            request_id: request_id.to_string(),
            method: "POST".to_string(),
            local_path: "/v1/responses".to_string(),
            endpoint: "responses".to_string(),
            received_at_ms: 1_000,
        }
    }

    #[test]
    fn request_start_mapping_preserves_the_canonical_context() {
        let write = map_request_start(RequestStartRecord {
            context: context("req-start"),
        });

        assert_eq!(write.request_id, "req-start");
        assert_eq!(write.method, "POST");
        assert_eq!(write.local_path, "/v1/responses");
        assert_eq!(write.endpoint, "responses");
        assert_eq!(write.received_at_ms, 1_000);
    }

    #[test]
    fn attempt_mapping_preserves_failure_and_health_fields() {
        let write = map_attempt_terminal(AttemptTerminalRecord {
            context: AttemptContext {
                attempt_id: AttemptId::new("req-attempt", 7),
                station_id: "station-1".to_string(),
                station_key_id: "key-1".to_string(),
                endpoint_revision: 4,
                started_at_ms: 1_010,
            },
            terminal: AttemptTerminal::Failed(ClassifiedAttemptFailure {
                kind: AttemptFailureKind::RateLimit,
                blame: FailureBlame::Upstream,
                retry: RetryDisposition::TryNextCandidate,
                health: HealthEffect::Cooldown {
                    retry_after_ms: Some(30_000),
                },
                public_code: "upstream_rate_limited".to_string(),
                sanitized_detail: Some("retry later".to_string()),
            }),
            output_committed: false,
            terminal_at_ms: 1_100,
        });

        assert_eq!(write.request_id, "req-attempt");
        assert_eq!(write.ordinal, 7);
        assert_eq!(write.station_id, "station-1");
        assert_eq!(write.station_key_id, "key-1");
        assert_eq!(write.endpoint_revision, 4);
        assert_eq!(write.started_at_ms, 1_010);
        assert_eq!(write.terminal_kind, "failed");
        assert_eq!(write.failure_kind.as_deref(), Some("RateLimit"));
        assert_eq!(write.failure_blame.as_deref(), Some("Upstream"));
        assert_eq!(write.retry_disposition.as_deref(), Some("TryNextCandidate"));
        assert_eq!(
            write.health_effect,
            "Cooldown { retry_after_ms: Some(30000) }"
        );
        assert_eq!(
            write.health_update,
            AttemptHealthUpdate::Cooldown {
                retry_after_ms: Some(30_000)
            }
        );
        assert_eq!(write.public_code.as_deref(), Some("upstream_rate_limited"));
        assert_eq!(write.sanitized_detail.as_deref(), Some("retry later"));
        assert!(!write.output_committed);
        assert_eq!(write.terminal_at_ms, 1_100);
    }

    #[test]
    fn request_terminal_mapping_preserves_annotations_and_delivery_failure() {
        let annotations = RequestLogAnnotations {
            model: Some("gpt-test".to_string()),
            stream: true,
            selected_station_key_id: Some("key-1".to_string()),
            selected_station_id: Some("station-1".to_string()),
            upstream_base_url: Some("https://station.test/v1".to_string()),
            route_policy: Some("stable_first".to_string()),
            route_reason: Some("healthy key".to_string()),
            rejected_candidates_json: Some("[]".to_string()),
            body_bytes: Some(128),
            route_wait_ms: Some(3),
            upstream_headers_ms: Some(7),
            failure_source: Some("downstream".to_string()),
            attempts_json: Some("[]".to_string()),
            completion_source: Some("response.completed".to_string()),
            prompt_tokens: Some(11),
            completion_tokens: Some(13),
            total_tokens: Some(24),
            cache_creation_tokens: Some(2),
            cache_read_tokens: Some(5),
            reasoning_effort: Some("high".to_string()),
            first_token_ms: Some(17),
        };
        let write = map_request_terminal(
            FinalRequestRecord::new(
                context("req-terminal"),
                RequestTerminalSnapshot {
                    terminal: RequestTerminal::Interrupted(DeliveryFailure {
                        terminal: DeliveryTerminal::DownstreamWriteFailed,
                        detail: None,
                    }),
                    delivery: DeliveryTerminal::DownstreamWriteFailed,
                },
                Some(AttemptId::new("req-terminal", 2)),
                3,
                2,
                annotations,
            ),
            1_250,
        );

        assert_eq!(write.request_id, "req-terminal");
        assert_eq!(write.received_at_ms, 1_000);
        assert_eq!(write.status, "interrupted");
        assert_eq!(write.lifecycle_status, "interrupted");
        assert_eq!(write.terminal_kind, "interrupted");
        assert_eq!(
            write.terminal_code.as_deref(),
            Some("DownstreamWriteFailed")
        );
        assert_eq!(
            write.terminal_detail.as_deref(),
            Some("downstream disconnected")
        );
        assert!(!write.protocol_completed);
        assert_eq!(write.delivery_terminal, "DownstreamWriteFailed");
        assert_eq!(write.selected_attempt_ordinal, Some(2));
        assert_eq!(write.attempt_count, 3);
        assert_eq!(write.fallback_count, 2);
        assert_eq!(write.terminal_at_ms, 1_250);
        assert_eq!(write.annotations.model.as_deref(), Some("gpt-test"));
        assert!(write.annotations.stream);
        assert_eq!(
            write.annotations.selected_station_key_id.as_deref(),
            Some("key-1")
        );
        assert_eq!(write.annotations.total_tokens, Some(24));
        assert_eq!(write.annotations.first_token_ms, Some(17));
    }

    #[test]
    fn unknown_commit_outcome_remains_distinguishable_at_the_lifecycle_port() {
        let error = map_persistence_error(PersistenceError::CommitOutcomeUnknown);

        assert!(matches!(
            error,
            LifecycleWriteError::CommitOutcomeUnknown(_)
        ));
    }
}
