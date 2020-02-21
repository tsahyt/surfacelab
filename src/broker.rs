use crossbeam_channel::*;
use std::sync::Arc;

pub struct Broker<T> {
    capacity: usize,
    sender: Sender<T>,
    receiver: Receiver<T>,
    subscribers: Vec<Sender<Arc<T>>>,
}

pub struct BrokerSender<T>(Sender<T>);

pub struct BrokerReceiver<T>(Receiver<Arc<T>>);

impl<T> Broker<T> {
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
        BrokerSender(self.sender.clone())
    }

    pub fn subscribe(&mut self) -> (BrokerSender<T>, BrokerReceiver<T>) {
        let (s, r) = bounded(self.capacity);
        self.subscribers.push(s);
        (self.sender(), BrokerReceiver(r))
    }

    pub fn run(&self) {
        for ev in &self.receiver {
            let arc = Arc::new(ev);
            for subscriber in &self.subscribers {
                let res = subscriber.send(arc.clone());
                if let Err(e) = res {
                    log::error!("Disconnected Component: {}", e);
                }
            }
        }
    }
}
