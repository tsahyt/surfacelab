#![feature(tau_constant)]

pub mod compute;
pub mod nodes;
pub mod render;
pub mod ui;
pub mod lang;

pub mod bus {
    use multiqueue2 as mq;

    pub type Lang = crate::lang::Lang;
    pub type Sender = mq::BroadcastSender<Lang>;
    pub type Receiver = mq::BroadcastReceiver<Lang>;

    pub struct Bus {
        sender: Option<mq::BroadcastSender<Lang>>,
        receiver: Option<mq::BroadcastReceiver<Lang>>,
    }

    impl Bus {
        pub fn new(capacity: u64) -> Self {
            log::info!("Initializing application bus");
            let (sender, receiver) = mq::broadcast_queue(capacity);
            Bus {
                sender: Some(sender),
                receiver: Some(receiver),
            }
        }

        pub fn subscribe(&self) -> Option<(Sender, Receiver)> {
            if let (Some(snd), Some(rcv)) = (&self.sender, &self.receiver) {
                Some((snd.clone(), rcv.add_stream().clone()))
            } else {
                log::error!("Attempted to subscribe to finalized bus");
                None
            }
        }

        pub fn finalize(&mut self) {
            log::info!("Finalizing application bus for use");
            self.receiver = None;
            self.sender = None;
        }
    }

    pub fn emit(sender: &Sender, event: Lang) -> () {
        log::debug!("Emitting event {:?}", event);
        sender.try_send(event).expect("Bus exceeded capacity!");
    }
}
