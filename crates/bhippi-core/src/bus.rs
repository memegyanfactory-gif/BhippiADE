use bhippi_types::{BhippiError, Event, NodeDotDelta, Result, ResyncReason, SessionId};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval_at, Instant, MissedTickBehavior};

const BROADCAST_CAPACITY: usize = 1_024;
const COALESCER_CAPACITY: usize = 1_024;
const EMISSION_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone)]
pub struct EventBus {
    broadcast: broadcast::Sender<Event>,
    coalescer: mpsc::Sender<Event>,
}

pub struct EventReceiver {
    receiver: broadcast::Receiver<Event>,
    last_session: Option<SessionId>,
}

impl EventBus {
    #[must_use]
    pub fn new() -> Self {
        let (broadcast, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (coalescer, receiver) = mpsc::channel(COALESCER_CAPACITY);
        tokio::spawn(run_coalescer(receiver, broadcast.clone()));
        Self {
            broadcast,
            coalescer,
        }
    }

    #[must_use]
    pub fn subscribe(&self) -> EventReceiver {
        EventReceiver {
            receiver: self.broadcast.subscribe(),
            last_session: None,
        }
    }

    pub fn emit(&self, event: Event) -> Result<()> {
        tracing::trace!(kind = event.kind(), session = ?event.session_id(), "event emitted");
        if matches!(event, Event::MindmapDelta { .. } | Event::DotAdded { .. }) {
            let session = event.session_id();
            match self.coalescer.try_send(event) {
                Ok(()) => Ok(()),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    send_ignoring_absent_receivers(
                        &self.broadcast,
                        Event::ResyncRequired {
                            session,
                            reason: ResyncReason::CoalescerOverloaded,
                        },
                    );
                    Ok(())
                }
                Err(mpsc::error::TrySendError::Closed(_)) => Err(BhippiError::Invariant {
                    code: "event_coalescer_closed",
                }),
            }
        } else {
            send_ignoring_absent_receivers(&self.broadcast, event);
            Ok(())
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventReceiver {
    pub async fn recv(&mut self) -> Result<Event> {
        match self.receiver.recv().await {
            Ok(event) => {
                if let Some(session) = event.session_id() {
                    self.last_session = Some(session);
                }
                Ok(event)
            }
            Err(broadcast::error::RecvError::Lagged(_)) => Ok(Event::ResyncRequired {
                session: self.last_session,
                reason: ResyncReason::SubscriberLagged,
            }),
            Err(broadcast::error::RecvError::Closed) => Err(BhippiError::Invariant {
                code: "event_bus_closed",
            }),
        }
    }
}

#[derive(Default)]
struct MindmapBatch {
    nodes: Vec<bhippi_types::NodeDelta>,
    edges: Vec<bhippi_types::EdgeDelta>,
    merged: u16,
}

#[derive(Default)]
struct DotBatch {
    dots: Vec<NodeDotDelta>,
    merged: u16,
}

async fn run_coalescer(mut receiver: mpsc::Receiver<Event>, broadcast: broadcast::Sender<Event>) {
    let mut mindmaps = BTreeMap::<SessionId, MindmapBatch>::new();
    let mut dots = BTreeMap::<SessionId, DotBatch>::new();
    let mut prefer_dots = false;
    let mut ticker = interval_at(Instant::now() + EMISSION_INTERVAL, EMISSION_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        while let Ok(event) = receiver.try_recv() {
            merge(event, &mut mindmaps, &mut dots);
        }

        let event = take_next(&mut mindmaps, &mut dots, &mut prefer_dots);
        if let Some(event) = event {
            send_ignoring_absent_receivers(&broadcast, event);
        } else if receiver.is_closed() {
            break;
        }
    }
}

fn merge(
    event: Event,
    mindmaps: &mut BTreeMap<SessionId, MindmapBatch>,
    dots: &mut BTreeMap<SessionId, DotBatch>,
) {
    match event {
        Event::MindmapDelta {
            session,
            nodes,
            edges,
            merged,
        } => {
            let batch = mindmaps.entry(session).or_default();
            batch.nodes.extend(nodes);
            batch.edges.extend(edges);
            batch.merged = batch.merged.saturating_add(merged.max(1));
        }
        Event::DotAdded {
            session,
            dots: added,
            merged,
        } => {
            let batch = dots.entry(session).or_default();
            batch.dots.extend(added);
            batch.merged = batch.merged.saturating_add(merged.max(1));
        }
        _ => {}
    }
}

fn take_next(
    mindmaps: &mut BTreeMap<SessionId, MindmapBatch>,
    dots: &mut BTreeMap<SessionId, DotBatch>,
    prefer_dots: &mut bool,
) -> Option<Event> {
    let take_dot = !dots.is_empty() && (*prefer_dots || mindmaps.is_empty());
    *prefer_dots = !*prefer_dots;

    if take_dot {
        let session = dots.keys().next().copied()?;
        let batch = dots.remove(&session)?;
        Some(Event::DotAdded {
            session,
            dots: batch.dots,
            merged: batch.merged,
        })
    } else {
        let session = mindmaps.keys().next().copied()?;
        let batch = mindmaps.remove(&session)?;
        Some(Event::MindmapDelta {
            session,
            nodes: batch.nodes,
            edges: batch.edges,
            merged: batch.merged,
        })
    }
}

fn send_ignoring_absent_receivers(sender: &broadcast::Sender<Event>, event: Event) {
    let _ = sender.send(event);
}
