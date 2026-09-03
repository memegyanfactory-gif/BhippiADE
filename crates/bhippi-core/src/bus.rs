use bhippi_types::{BhippiError, Event, Result, ResyncReason, SessionId};
use tokio::sync::broadcast;

const BROADCAST_CAPACITY: usize = 1_024;

#[derive(Clone)]
pub struct EventBus {
    broadcast: broadcast::Sender<Event>,
}

pub struct EventReceiver {
    receiver: broadcast::Receiver<Event>,
    last_session: Option<SessionId>,
}

impl EventBus {
    #[must_use]
    pub fn new() -> Self {
        let (broadcast, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { broadcast }
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
        send_ignoring_absent_receivers(&self.broadcast, event);
        Ok(())
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventReceiver {
    pub async fn recv(&mut self) -> Result<Event> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    self.last_session = event.session_id().or(self.last_session);
                    return Ok(event);
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    return Ok(Event::ResyncRequired {
                        session: self.last_session,
                        reason: ResyncReason::SubscriberLagged,
                    });
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(BhippiError::Invariant {
                        code: "event_bus_closed",
                    });
                }
            }
        }
    }
}

fn send_ignoring_absent_receivers(sender: &broadcast::Sender<Event>, event: Event) {
    let _ = sender.send(event);
}
