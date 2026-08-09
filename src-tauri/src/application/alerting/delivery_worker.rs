use std::{
    future::Future,
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
    time::Duration,
};

use tokio_util::sync::CancellationToken;

use crate::models::alerting::{DeliveryKind, NotificationChannel};
use crate::persistence::{
    error::PersistenceError, runtime::PersistenceHandle, stores::alerting::DeliveryStore,
};
use crate::services::alerting::{DesktopNotificationError, DesktopNotificationPayload};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryClaim {
    pub id: String,
    pub token: String,
    pub incident_id: String,
    pub episode_number: u32,
    pub channel: NotificationChannel,
    pub delivery_kind: DeliveryKind,
    pub policy_snapshot_json: String,
    pub attempt_count: u32,
    pub object_type: Option<String>,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
}

impl DeliveryClaim {
    pub(crate) fn desktop_payload(
        &self,
    ) -> Result<DesktopNotificationPayload, DesktopNotificationError> {
        DesktopNotificationPayload::new(
            &self.id,
            &self.incident_id,
            self.episode_number,
            self.delivery_kind.as_str(),
            build_alerting_deep_link(
                &self.incident_id,
                self.episode_number,
                self.object_type.as_deref(),
                self.object_id.as_deref(),
                self.station_id.as_deref(),
                self.station_key_id.as_deref(),
            ),
        )
    }
}

#[derive(Clone)]
pub(crate) struct DeliveryWorker {
    runtime: PersistenceHandle,
    store: DeliveryStore,
    lease_ms: i64,
    max_attempts: u32,
    token_counter: Arc<AtomicU64>,
}

impl DeliveryWorker {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            store: DeliveryStore,
            lease_ms: 30_000,
            max_attempts: 3,
            token_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.delivery-worker-limits; owner=application/alerting; remove_when=worker limits are fixed by runtime composition"
    )]
    pub(crate) fn with_limits(mut self, lease_ms: i64, max_attempts: u32) -> Self {
        self.lease_ms = lease_ms.max(1);
        self.max_attempts = max_attempts.clamp(1, 10);
        self
    }

    pub(crate) async fn claim_due(
        &self,
        now_ms: i64,
        limit: u32,
    ) -> Result<Vec<DeliveryClaim>, PersistenceError> {
        let ids = {
            let mut read = self.runtime.begin_read().await?;
            self.store.due_ids(&mut read, now_ms, limit).await?
        };
        let mut claims = Vec::with_capacity(ids.len());
        for id in ids {
            let token = self.next_token(now_ms, &id);
            let lease_ms = self.lease_ms;
            let claimed = self
                .runtime
                .write(|write| {
                    let store = DeliveryStore;
                    let id = id.clone();
                    let token = token.clone();
                    Box::pin(
                        async move { store.claim_due(write, &id, &token, now_ms, lease_ms).await },
                    )
                })
                .await?;
            if claimed {
                let metadata = {
                    let mut read = self.runtime.begin_read().await?;
                    self.store
                        .metadata_for_claim(&mut read, &id, &token)
                        .await?
                };
                if let Some(metadata) = metadata {
                    claims.push(DeliveryClaim {
                        id,
                        token,
                        incident_id: metadata.incident_id,
                        episode_number: metadata.episode_number,
                        channel: metadata.channel,
                        delivery_kind: metadata.delivery_kind,
                        policy_snapshot_json: metadata.policy_snapshot_json,
                        attempt_count: metadata.attempt_count,
                        object_type: metadata.object_type,
                        object_id: metadata.object_id,
                        station_id: metadata.station_id,
                        station_key_id: metadata.station_key_id,
                    });
                }
            }
        }
        Ok(claims)
    }

    pub(crate) async fn mark_delivered(
        &self,
        claim: &DeliveryClaim,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let id = claim.id.clone();
        let token = claim.token.clone();
        let store = DeliveryStore;
        self.runtime
            .write(|write| {
                Box::pin(async move { store.mark_delivered(write, &id, &token, now_ms).await })
            })
            .await
    }

    pub(crate) async fn mark_adapter_failure(
        &self,
        claim: &DeliveryClaim,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let id = claim.id.clone();
        let token = claim.token.clone();
        let retry_at = now_ms.saturating_add(backoff_ms(1));
        let max_attempts = self.max_attempts;
        let store = DeliveryStore;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .release_for_retry(
                            write,
                            &id,
                            &token,
                            "delivery_adapter_failed",
                            retry_at,
                            now_ms,
                            max_attempts,
                        )
                        .await
                })
            })
            .await
    }

    pub(crate) async fn mark_failed(
        &self,
        claim: &DeliveryClaim,
        error_code: &'static str,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let id = claim.id.clone();
        let token = claim.token.clone();
        let store = DeliveryStore;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .mark_failed(write, &id, &token, error_code, now_ms)
                        .await
                })
            })
            .await
    }

    pub(crate) async fn recover_expired(
        &self,
        now_ms: i64,
        limit: u32,
    ) -> Result<u64, PersistenceError> {
        let retry_at = now_ms.saturating_add(backoff_ms(1));
        let max_attempts = self.max_attempts;
        let store = DeliveryStore;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .expire_claims(write, now_ms, retry_at, max_attempts, limit)
                        .await
                })
            })
            .await
    }

    /// Run a bounded worker loop. The adapter is called only after a durable
    /// claim and never while a database transaction is open.
    #[expect(
        dead_code,
        reason = "contract=alerting.delivery-worker-loop; owner=application/alerting; remove_when=delivery worker is retired"
    )]
    pub(crate) async fn run<F, Fut>(
        &self,
        cancellation: CancellationToken,
        interval: Duration,
        mut dispatch: F,
    ) -> Result<(), PersistenceError>
    where
        F: FnMut(DeliveryClaim) -> Fut,
        Fut: Future<Output = Result<(), ()>>,
    {
        let interval = interval.max(Duration::from_millis(25));
        loop {
            if cancellation.is_cancelled() {
                return Ok(());
            }
            let now_ms = chrono::Utc::now().timestamp_millis();
            let claims = self.claim_due(now_ms, 50).await?;
            for claim in claims {
                let outcome = dispatch(claim.clone()).await;
                let now_ms = chrono::Utc::now().timestamp_millis();
                match outcome {
                    Ok(()) => self.mark_delivered(&claim, now_ms).await?,
                    Err(()) => self.mark_adapter_failure(&claim, now_ms).await?,
                }
            }
            self.recover_expired(now_ms, 50).await?;
            tokio::select! {
                _ = cancellation.cancelled() => return Ok(()),
                _ = tokio::time::sleep(interval) => {}
            }
        }
    }

    fn next_token(&self, now_ms: i64, id: &str) -> String {
        let seq = self.token_counter.fetch_add(1, Ordering::Relaxed);
        format!("claim-{now_ms}-{seq}-{}", stable_id_hash(id))
    }
}

fn stable_id_hash(id: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(id.as_bytes());
    digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn backoff_ms(attempt: u32) -> i64 {
    let exponent = attempt.saturating_sub(1).min(5);
    1_000_i64.saturating_mul(1_i64 << exponent)
}

/// Build a stable internal URI from already validated object identifiers.
/// Notification payloads never contain station names, URLs, credentials or
/// raw observation JSON.
pub(crate) fn build_alerting_deep_link(
    incident_id: &str,
    episode_number: u32,
    object_type: Option<&str>,
    object_id: Option<&str>,
    station_id: Option<&str>,
    station_key_id: Option<&str>,
) -> String {
    let encoded_incident = encode_component(incident_id);
    let target = match (object_type, object_id, station_key_id, station_id) {
        (Some("request"), Some(request_id), _, _) => {
            format!("request/{}", encode_component(request_id))
        }
        (_, _, Some(key_id), _) => format!("key/{}", encode_component(key_id)),
        (_, _, _, Some(station)) => format!("station/{}", encode_component(station)),
        _ => "changes".to_string(),
    };
    format!("relaypool://{target}?incident_id={encoded_incident}&episode={episode_number}")
}

fn encode_component(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_bounded_and_monotonic() {
        let values = (1..=8).map(backoff_ms).collect::<Vec<_>>();
        assert!(values.windows(2).all(|window| window[0] <= window[1]));
        assert!(values.last().copied().unwrap_or_default() <= 32_000);
    }

    #[test]
    fn deep_link_prefers_request_then_key_then_station_and_redacts_names() {
        let request = build_alerting_deep_link(
            "incident/1",
            2,
            Some("request"),
            Some("request/1"),
            Some("station-1"),
            Some("key-1"),
        );
        assert!(request.starts_with("relaypool://request/request%2F1?"));
        assert!(request.contains("incident_id=incident%2F1"));

        let key = build_alerting_deep_link("incident", 1, None, None, Some("station"), Some("key"));
        assert!(key.starts_with("relaypool://key/key?"));

        let station = build_alerting_deep_link("incident", 1, None, None, Some("station"), None);
        assert!(station.starts_with("relaypool://station/station?"));
    }
}
