use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use futures_util::future::{join_all, BoxFuture};

mod services {
    pub(crate) mod proxy {
        #[path = "../../../src/services/proxy/lifecycle/mod.rs"]
        pub(crate) mod lifecycle;
    }
}

use services::proxy::lifecycle::{
    attempt::{AttemptContext, AttemptTerminal, AttemptTerminalRecord},
    delivery::DeliveryTerminal,
    ports::{
        AttemptCommitAck, LifecycleWriteError, RequestCommitAck, RequestLifecycleStore,
        RequestStartAck,
    },
    request::{
        AttemptId, FinalRequestRecord, RequestCompletion, RequestContextSnapshot,
        RequestLogAnnotations, RequestStartRecord, RequestTerminal, RequestTerminalSnapshot,
    },
    writer::LifecycleWriter,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    Start(String),
    Attempt(String, u16),
    Finish(String),
}

#[derive(Default)]
struct RecordingStore {
    events: Arc<Mutex<Vec<Event>>>,
}

impl RequestLifecycleStore for RecordingStore {
    fn start_request(
        &self,
        record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        let events = Arc::clone(&self.events);
        Box::pin(async move {
            events
                .lock()
                .expect("events")
                .push(Event::Start(record.context.request_id));
            Ok(RequestStartAck { inserted: true })
        })
    }

    fn finish_attempt(
        &self,
        record: AttemptTerminalRecord,
    ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
        let events = Arc::clone(&self.events);
        Box::pin(async move {
            events.lock().expect("events").push(Event::Attempt(
                record.context.attempt_id.request_id,
                record.context.attempt_id.ordinal,
            ));
            Ok(AttemptCommitAck {
                inserted: true,
                health_applied: true,
            })
        })
    }

    fn finish_request(
        &self,
        record: FinalRequestRecord,
    ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
        let events = Arc::clone(&self.events);
        Box::pin(async move {
            events
                .lock()
                .expect("events")
                .push(Event::Finish(record.context.request_id));
            Ok(RequestCommitAck { finalized: true })
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lifecycle_writer_preserves_per_request_order_under_concurrency() {
    let store = RecordingStore::default();
    let events = Arc::clone(&store.events);
    let (writer, worker) = LifecycleWriter::start(128, Arc::new(store)).expect("writer");

    let tasks = (0..32).map(|index| {
        let writer = writer.clone();
        tokio::spawn(async move {
            let request_id = format!("req-concurrent-{index}");
            let request = writer.try_reserve_request().expect("request permits");
            let (terminal, start_ack) = request.send_start(start_record(&request_id));
            assert!(
                start_ack
                    .await
                    .expect("start ack channel")
                    .expect("start ack")
                    .inserted
            );

            let attempt = writer.try_reserve_attempt().expect("attempt permit");
            let attempt_ack = attempt.send(attempt_record(&request_id));
            assert!(
                attempt_ack
                    .await
                    .expect("attempt ack channel")
                    .expect("attempt ack")
                    .inserted
            );

            assert!(
                terminal
                    .send(final_record(&request_id))
                    .await
                    .expect("finish ack channel")
                    .expect("finish ack")
                    .finalized
            );
        })
    });

    for result in join_all(tasks).await {
        result.expect("request task");
    }
    drop(writer);
    worker.join().await.expect("worker join");

    let events = events.lock().expect("events").clone();
    assert_eq!(events.len(), 32 * 3);

    let mut by_request: HashMap<String, Vec<Event>> = HashMap::new();
    for event in events {
        let request_id = match &event {
            Event::Start(request_id) | Event::Finish(request_id) => request_id.clone(),
            Event::Attempt(request_id, _) => request_id.clone(),
        };
        by_request.entry(request_id).or_default().push(event);
    }

    assert_eq!(by_request.len(), 32);
    for (request_id, events) in by_request {
        assert_eq!(
            events,
            vec![
                Event::Start(request_id.clone()),
                Event::Attempt(request_id.clone(), 0),
                Event::Finish(request_id),
            ]
        );
    }
}

fn start_record(request_id: &str) -> RequestStartRecord {
    RequestStartRecord {
        context: context(request_id),
    }
}

fn attempt_record(request_id: &str) -> AttemptTerminalRecord {
    AttemptTerminalRecord {
        context: AttemptContext {
            attempt_id: AttemptId::new(request_id, 0),
            station_id: "station-concurrent".to_string(),
            station_key_id: "key-concurrent".to_string(),
            endpoint_revision: 1,
            started_at_ms: 2,
        },
        terminal: AttemptTerminal::Succeeded,
        output_committed: true,
        terminal_at_ms: 3,
    }
}

fn final_record(request_id: &str) -> FinalRequestRecord {
    FinalRequestRecord {
        context: context(request_id),
        terminal: RequestTerminalSnapshot {
            terminal: RequestTerminal::Completed(RequestCompletion {
                protocol_completed: true,
                attempt_id: Some(AttemptId::new(request_id, 0)),
            }),
            delivery: DeliveryTerminal::BodyCompleted,
        },
        selected_attempt_id: Some(AttemptId::new(request_id, 0)),
        attempt_count: 1,
        fallback_count: 0,
        annotations: RequestLogAnnotations::default(),
    }
}

fn context(request_id: &str) -> RequestContextSnapshot {
    RequestContextSnapshot {
        request_id: request_id.to_string(),
        method: "POST".to_string(),
        local_path: "/v1/chat/completions".to_string(),
        endpoint: "/v1/chat/completions".to_string(),
        received_at_ms: 1,
    }
}
