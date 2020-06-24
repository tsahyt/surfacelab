use crossbeam_channel::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Broker<T> {
    capacity: usize,
    sender: Sender<T>,
    receiver: Receiver<T>,
    subscribers: Vec<(Sender<Arc<T>>, Arc<AtomicBool>)>,
}

pub type BrokerSender<T> = Sender<T>;

pub type BrokerReceiver<T> = Receiver<Arc<T>>;

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

    fn sender(&self) -> BrokerSender<T> {
        self.sender.clone()
    }

    pub fn subscribe(&mut self) -> (BrokerSender<T>, BrokerReceiver<T>, BrokerDisconnect) {
        let (s, r) = bounded(self.capacity);
        let alive = Arc::new(AtomicBool::new(true));
        self.subscribers.push((s, alive.clone()));
        (self.sender(), r, BrokerDisconnect(alive))
    }

    pub fn run(&mut self) {
        for ev in &self.receiver {
            self.subscribers
                .drain_filter(|(_, alive)| !alive.load(Ordering::Relaxed));
            let arc = Arc::new(ev);
            for (subscriber, _) in &self.subscribers {
                let res = subscriber.send(Arc::clone(&arc));
                if let Err(e) = res {
                    log::error!("Disconnected Component: {}", e);
                }
            }
        }
    }
}
