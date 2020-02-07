pub mod ui;

pub mod bus {
    use multiqueue2 as mq;

    pub type Lang = String;

    pub struct Bus<T>
    where
        T: Clone + std::fmt::Debug,
    {
        sender: mq::BroadcastSender<T>,
        receiver: Option<mq::BroadcastReceiver<T>>,
    }

    impl<T: Clone + std::fmt::Debug> Bus<T> {
        pub fn new(capacity: u64) -> Self {
            log::info!("Initializing application bus");
            let (sender, receiver) = mq::broadcast_queue(capacity);
            Bus {
                sender,
                receiver: Some(receiver),
            }
        }

        pub fn emit(&self, event: T) -> () {
            debug_assert!(self.receiver.is_none());
            log::debug!("Emitting event {:?}", event);
            self.sender.try_send(event).expect("Bus exceeded capacity!");
        }

        pub fn subscribe(&self) -> Option<mq::BroadcastReceiver<T>> {
            match &self.receiver {
                None => {
                    log::error!("Attempted to subscribe to finalized bus");
                    None
                }
                Some(rcv) => Some(rcv.add_stream()),
            }
        }

        pub fn finalize(&mut self) {
            log::info!("Finalizing application bus for use");
            self.receiver = None
        }
    }
}
