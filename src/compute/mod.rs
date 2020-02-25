use crate::{broker, gpu, lang::*};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

pub fn start_compute_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (sender, receiver) = broker.subscribe();
    match gpu::compute::GPUCompute::new(gpu) {
        Err(e) => {
            log::error!("Failed to initialize GPU Compute: {}", e);
            panic!("Critical Error");
        }
        Ok(gpu) => thread::spawn(move || {
            let mut compute_mgr = ComputeManager::new(gpu);
            for event in receiver {
                match compute_mgr.process_event(event) {
                    None => break,
                    Some(response) => {
                        for ev in response {
                            if let Err(e) = sender.send(ev) {
                                log::error!("Compute lost connection to application bus! {}", e);
                            }
                        }
                    }
                }
            }

            log::info!("GPU Compute Handler terminating");
        }),
    }
}

struct ComputeManager<B: gpu::Backend> {
    gpu: gpu::compute::GPUCompute<B>,
    sockets: HashMap<Resource, ImageType>,
}

impl<B> ComputeManager<B>
where
    B: gpu::Backend,
{
    pub fn new(gpu: gpu::compute::GPUCompute<B>) -> Self {
        ComputeManager {
            gpu,
            sockets: HashMap::new(),
        }
    }

    pub fn process_event(&mut self, event: Arc<Lang>) -> Option<Vec<Lang>> {
        let mut response = Vec::new();
        match &*event {
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, op) => {
                    for (socket, imgtype) in op.inputs().into_iter().chain(op.outputs()) {
                        let socket_res = res.extend_fragment(&socket);
                        self.sockets.insert(socket_res, imgtype);
                    }
                }
                GraphEvent::NodeRemoved(res) => self.sockets.retain(|s, _| !s.is_fragment_of(res)),
                GraphEvent::Recomputed(instrs) => {
                    for i in instrs.iter() {
                        self.interpret(i)
                    }
                }
                _ => {}
            },
            Lang::UserEvent(UserEvent::Quit) => return None,
            _ => {}
        }

        Some(response)
    }

    fn interpret(&mut self, instr: &Instruction) {
        match instr {
            Instruction::Move(from, to) => {
                log::trace!("Moving texture from {} to {}", from, to);
            }
            Instruction::Execute(res, op) => {
                log::trace!("Executing operator {:?} of {}", op, res);
                match op.operator_type() {
                    OperatorType::SinkOperator => {}
                    OperatorType::SourceOperator => {}
                    OperatorType::ProcessOperator => {}
                }
            }
        }
    }
}
