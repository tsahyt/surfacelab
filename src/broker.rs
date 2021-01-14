use crossbeam_channel::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Broker<T> {
    /// Capacity of the broadcast channel
    capacity: usize,

    /// Sender towards the broker, to be cloned out to subscribers
    sender: Sender<T>,

    /// Receiver on the broker side, unique.
    receiver: Receiver<T>,

    /// List of subscribers with their aliveness status
    subscribers: Vec<(Sender<Arc<T>>, Arc<AtomicBool>)>,
}

/// A type used to send messages to the broker, i.e. to broadcast them
pub type BrokerSender<T> = Sender<T>;

/// A type to read messages from the broker, i.e. from the broadcast channel
pub type BrokerReceiver<T> = Receiver<Arc<T>>;

/// Type to control clean disconnect from broker
pub struct BrokerDisconnect(Arc<AtomicBool>);

impl BrokerDisconnect {
    /// Disconnect from the broker
    pub fn disconnect(self) {
        self.0.store(false, Ordering::Release);
    }
}

impl<T: std::fmt::Debug> Broker<T> {
    /// Create a new Broker with a given capacity.
    pub fn new(capacity: usize) -> Self {
        let (s, r) = bounded(capacity);

        Broker {
            capacity,
            sender: s,
            receiver: r,
            subscribers: Vec::new(),
        }
    }

    /// Obtain a sender to send towards the broker, i.e. to broadcast messages.
    fn sender(&self) -> BrokerSender<T> {
        self.sender.clone()
    }

    /// Subscribe to the application bus controlled by the broker, yielding
    /// sender, receiver, and disconnector.
    pub fn subscribe(&mut self) -> (BrokerSender<T>, BrokerReceiver<T>, BrokerDisconnect) {
        let (s, r) = bounded(self.capacity);
        let alive = Arc::new(AtomicBool::new(true));
        self.subscribers.push((s, alive.clone()));
        (self.sender(), r, BrokerDisconnect(alive))
    }

    /// Broker loop
    pub fn run(&mut self) {
        for ev in &self.receiver {
            // Purge all dead subscribers
            self.subscribers
                .drain_filter(|(_, alive)| !alive.load(Ordering::Relaxed));

            // Wrap the event and send to all subscribers
            let arc = Arc::new(ev);
            for (subscriber, _) in &self.subscribers {
                let res = subscriber.send(Arc::clone(&arc));
                if let Err(e) = res {
                    // Should only happen in case the disconnector wasn't called
                    log::error!("Disconnected Component: {}", e);
                }
            }
        }
    }
}
