//! A test states its preconditions with `unwrap`/`expect`: a panic here is a failing
//! test, not a crashed app. The workspace `deny` stands everywhere else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_core::EventBus;
use bhippi_types::{Event, Health, ProviderId};

#[tokio::test]
async fn events_broadcast_to_subscribers() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();
    let provider = ProviderId::new();

    bus.emit(Event::ProviderHealth {
        provider,
        health: Health::Healthy { latency_ms: 10 },
    })
    .unwrap();

    let event = receiver.recv().await.unwrap();
    match event {
        Event::ProviderHealth {
            provider: p,
            health,
        } => {
            assert_eq!(p, provider);
            assert_eq!(health, Health::Healthy { latency_ms: 10 });
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn subscriber_lag_becomes_an_explicit_resync_event() {
    let bus = EventBus::new();
    let mut receiver = bus.subscribe();

    for _ in 0..1_100 {
        bus.emit(Event::ProviderHealth {
            provider: ProviderId::new(),
            health: Health::Healthy { latency_ms: 10 },
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

    for _ in 0..2_000 {
        bus.emit(Event::ProviderHealth {
            provider: ProviderId::new(),
            health: Health::Healthy { latency_ms: 10 },
        })
        .unwrap_or_else(|error| panic!("absent UI must not fail emission: {error}"));
    }
}
