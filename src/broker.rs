use crossbeam_channel::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A type annotated with a name.
type Named<T> = (&'static str, T);

pub struct Broker<T> {
    /// Capacity of the broadcast channel
    capacity: usize,

    /// Sender towards the broker, to be cloned out to subscribers
    sender: Sender<Named<T>>,

    /// Receiver on the broker side, unique.
    receiver: Receiver<Named<T>>,

    /// List of subscribers with their aliveness status
    subscribers: Vec<(Sender<Arc<T>>, &'static str, Arc<AtomicBool>)>,
}

/// Named senders, i.e. senders that also attach their name to the message
pub struct NamedSender<T> {
    name: &'static str,
    inner: Sender<Named<T>>,
}

impl<T> Clone for NamedSender<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            inner: self.inner.clone(),
        }
    }
}

impl<T> NamedSender<T> {
    fn new(name: &'static str, inner: Sender<Named<T>>) -> Self {
        Self { name, inner }
    }

    /// Send a message to the broker
    pub fn send(&self, msg: T) -> Option<()> {
        self.inner.send((self.name, msg)).ok()
    }

    /// Send a message to the broker anonymously. This will result in the
    /// message getting echoed back at the calling thread!
    pub fn send_anonymous(&self, msg: T) -> Option<()> {
        self.inner.send(("", msg)).ok()
    }
}

/// A type used to send messages to the broker, i.e. to broadcast them
pub type BrokerSender<T> = NamedSender<T>;

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

    /// Obtain a sender to send towards the broker, i.e. to broadcast messages.
    fn sender(&self) -> Sender<Named<T>> {
        self.sender.clone()
    }

    /// Subscribe to the application bus controlled by the broker, yielding
    /// sender, receiver, and disconnector.
    pub fn subscribe(
        &mut self,
        name: &'static str,
    ) -> (BrokerSender<T>, BrokerReceiver<T>, BrokerDisconnect) {
        let (s, r) = bounded(self.capacity);
        let alive = Arc::new(AtomicBool::new(true));
        self.subscribers.push((s, name, alive.clone()));
        (
            NamedSender::new(name, self.sender()),
            r,
            BrokerDisconnect(alive),
        )
    }

    /// Broker loop
    pub fn run(&mut self) {
        let mut count: usize = 0;
        for (origin, ev) in &self.receiver {
            count += 1;

            // Purge all dead subscribers periodically
            if count > 1024 {
                self.subscribers
                    .drain_filter(|(_, _, alive)| !alive.load(Ordering::Relaxed));
                count = 0;
            }

            // Wrap the event and send to all live subscribers other than origin
            let arc = Arc::new(ev);
            for (subscriber, _, _) in self
                .subscribers
                .iter()
                .filter(|x| x.1 != origin && x.2.load(Ordering::Relaxed))
            {
                let res = subscriber.send(Arc::clone(&arc));
                if let Err(e) = res {
                    // Should only happen in case the disconnector wasn't called
                    log::error!("Disconnected Component: {}", e);
                }
            }
        }
    }
}
