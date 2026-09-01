use bhippi_core::EventBus;
use bhippi_types::{DotId, DotSummary, Event, NodeDotDelta, NodeId, SessionId, SourceId};
use std::time::{Duration, Instant};

fn dot_event(session: SessionId, node: NodeId) -> Event {
    Event::DotAdded {
        session,
        dots: vec![NodeDotDelta {
            node,
            dot: DotSummary {
                id: DotId::new(),
                claim: "A locally verified evidence point".to_owned(),
                source_id: SourceId::new(),
                confidence: 0.9,
            },
        }],
        merged: 1,
    }
}

#[tokio::test]
async fn burst_is_batched_without_losing_dots() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();
    let session = SessionId::new();
    let node = NodeId::new();

    for _ in 0..100 {
        bus.emit(dot_event(session, node))
            .unwrap_or_else(|error| panic!("burst event must enqueue: {error}"));
    }

    let event = tokio::time::timeout(Duration::from_millis(200), receiver.recv())
        .await
        .unwrap_or_else(|error| panic!("batch must arrive: {error}"))
        .unwrap_or_else(|error| panic!("batch must be readable: {error}"));

    let Event::DotAdded { dots, merged, .. } = event else {
        panic!("expected a dot batch");
    };
    assert_eq!(dots.len(), 100);
    assert_eq!(merged, 100);
}

#[tokio::test]
async fn coalesced_lane_never_emits_faster_than_twenty_per_second() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();
    let session = SessionId::new();
    let node = NodeId::new();

    let started = Instant::now();
    for _ in 0..8 {
        bus.emit(dot_event(session, node))
            .unwrap_or_else(|error| panic!("event must enqueue: {error}"));
        bus.emit(Event::MindmapDelta {
            session,
            nodes: Vec::new(),
            edges: Vec::new(),
            merged: 1,
        })
        .unwrap_or_else(|error| panic!("map event must enqueue: {error}"));
        tokio::time::sleep(Duration::from_millis(55)).await;
    }

    let mut received = 0_u32;
    while started.elapsed() < Duration::from_millis(500) {
        if tokio::time::timeout(Duration::from_millis(10), receiver.recv())
            .await
            .is_ok()
        {
            received += 1;
        }
    }

    assert!(received <= 10, "received {received} coalesced events");
}

#[tokio::test]
async fn subscriber_lag_becomes_an_explicit_resync_event() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();
    let session = SessionId::new();

    for _ in 0..1_100 {
        bus.emit(Event::SessionStageChanged {
            session,
            from: bhippi_types::Stage::Planning,
            to: bhippi_types::Stage::Expanding,
        })
        .unwrap_or_else(|error| panic!("fact event must emit: {error}"));
    }

    let event = receiver
        .recv()
        .await
        .unwrap_or_else(|error| panic!("lag must produce a readable marker: {error}"));
    assert!(matches!(event, Event::ResyncRequired { .. }));
}

#[tokio::test]
async fn absent_subscribers_never_block_or_fail_the_engine() {
    let bus = EventBus::new();
    let session = SessionId::new();

    for _ in 0..2_000 {
        bus.emit(Event::SessionStageChanged {
            session,
            from: bhippi_types::Stage::Planning,
            to: bhippi_types::Stage::Expanding,
        })
        .unwrap_or_else(|error| panic!("absent UI must not fail emission: {error}"));
    }
}
