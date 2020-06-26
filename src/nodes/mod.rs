use crate::{broker, lang};
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::path::PathBuf;

use maplit::hashmap;

pub mod io;
pub mod nodegraph;

struct NodeManager {
    parent_size: u32,
    graphs: HashMap<String, nodegraph::NodeGraph>,
}

// FIXME: Changing output socket type after connection has already been made does not propagate type changes into preceeding polymorphic nodes!
impl NodeManager {
    pub fn new() -> Self {
        NodeManager {
            parent_size: 1024,
            graphs: hashmap! { "base".to_string() => nodegraph::NodeGraph::new() },
        }
    }

    pub fn process_event(&mut self, event: Arc<lang::Lang>) -> Option<Vec<lang::Lang>> {
        use crate::lang::*;
        let mut response = vec![];

        match &*event {
            Lang::UserNodeEvent(event) => match event {
                UserNodeEvent::NewNode(graph, op) => {
                    let graph_name = graph.path();
                    let (node_id, size) = self
                        .graphs
                        .get_mut(graph_name.to_str().unwrap())
                        .unwrap()
                        .new_node(op, self.parent_size);
                    let mut path = graph_name.to_path_buf();
                    path.push(node_id);
                    response.push(Lang::GraphEvent(GraphEvent::NodeAdded(
                        Resource::node(path, None),
                        op.clone(),
                        None,
                        size as u32,
                    )))
                }
                UserNodeEvent::RemoveNode(res) => {
                    match self.graphs.get_mut("base").unwrap().remove_node(res) {
                        Ok((ty, removed_conns)) => {
                            response = removed_conns
                                .iter()
                                .map(|c| {
                                    Lang::GraphEvent(GraphEvent::DisconnectedSockets(
                                        c.0.clone(),
                                        c.1.clone(),
                                    ))
                                })
                                .collect();
                            response.push(Lang::GraphEvent(GraphEvent::NodeRemoved(res.clone())));
                            if let Some(ty) = ty {
                                response.push(Lang::GraphEvent(GraphEvent::OutputRemoved(
                                    res.clone(),
                                    ty,
                                )))
                            }
                        }
                        Err(e) => log::error!("{}", e),
                    }
                }
                UserNodeEvent::ConnectSockets(from, to) => {
                    match self
                        .graphs
                        .get_mut("base")
                        .unwrap()
                        .connect_sockets(from, to)
                    {
                        Ok(mut res) => {
                            response.push(Lang::GraphEvent(GraphEvent::ConnectedSockets(
                                from.clone(),
                                to.clone(),
                            )));
                            response.append(&mut res);
                        }
                        Err(e) => log::error!("{}", e),
                    }
                }
                UserNodeEvent::DisconnectSinkSocket(sink) => {
                    match self
                        .graphs
                        .get_mut("base")
                        .unwrap()
                        .disconnect_sink_socket(sink)
                    {
                        Ok(mut r) => response.append(&mut r),
                        Err(e) => log::error!("Error while disconnecting sink {}", e),
                    }
                }
                UserNodeEvent::ParameterChange(res, field, data) => {
                    self.graphs
                        .get_mut("base")
                        .unwrap()
                        .parameter_change(res, field, data)
                        .unwrap_or_else(|e| log::error!("{}", e));
                    let instructions = self.graphs.get_mut("base").unwrap().linearize();
                    response.push(Lang::GraphEvent(GraphEvent::Recomputed(instructions)));
                }
                UserNodeEvent::ForceRecompute => {
                    let instructions = self.graphs.get_mut("base").unwrap().linearize();
                    response.push(Lang::GraphEvent(GraphEvent::Recomputed(instructions)));
                }
                UserNodeEvent::PositionNode(res, (x, y)) => self
                    .graphs
                    .get_mut("base")
                    .unwrap()
                    .position_node(res, *x, *y),
                UserNodeEvent::RenameNode(from, to) => {
                    if let Some(r) = self.graphs.get_mut("base").unwrap().rename_node(from, to) {
                        response.push(r);
                    }
                }
                UserNodeEvent::OutputSizeChange(res, size) => {
                    if let Some(r) = self.graphs.get_mut("base").unwrap().resize_node(
                        res,
                        Some(*size),
                        None,
                        self.parent_size,
                    ) {
                        response.push(r);
                    };
                }
                UserNodeEvent::OutputSizeAbsolute(res, abs) => {
                    if let Some(r) = self.graphs.get_mut("base").unwrap().resize_node(
                        res,
                        None,
                        Some(*abs),
                        self.parent_size,
                    ) {
                        response.push(r);
                    };
                }
            },
            Lang::UserIOEvent(UserIOEvent::Quit) => return None,
            Lang::UserIOEvent(UserIOEvent::RequestExport(None)) => {
                let exportable = self.graphs.get_mut("base").unwrap().get_output_sockets();
                response.push(Lang::UserIOEvent(UserIOEvent::RequestExport(Some(
                    exportable,
                ))));
            }
            Lang::UserIOEvent(UserIOEvent::OpenSurface(path)) => {
                match self.open_node_graph(path) {
                    Ok(mut evs) => {
                        response.push(Lang::GraphEvent(GraphEvent::Cleared));
                        response.append(&mut evs);

                        // Automatically recompute on load
                        let instructions = self.graphs.get_mut("base").unwrap().linearize();
                        response.push(Lang::GraphEvent(GraphEvent::Recomputed(instructions)));
                    }
                    Err(e) => log::error!("{}", e),
                }
            }
            Lang::UserIOEvent(UserIOEvent::SaveSurface(path)) => {
                if let Err(e) = self.save_node_graph(path) {
                    log::error!("{}", e)
                }
            }
            Lang::UserIOEvent(UserIOEvent::NewSurface) => {
                self.graphs.get_mut("base").unwrap().reset();
                response.push(Lang::GraphEvent(GraphEvent::Cleared));
            }
            Lang::UserIOEvent(UserIOEvent::SetParentSize(size)) => {
                self.parent_size = *size;
                response.append(
                    &mut self
                        .graphs
                        .get_mut("base")
                        .unwrap()
                        .resize_all(self.parent_size),
                );

                // Recompute on size change
                let instructions = self.graphs.get_mut("base").unwrap().linearize();
                response.push(Lang::GraphEvent(GraphEvent::Recomputed(instructions)));
            }
            _ => {}
        }

        Some(response)
    }
}

pub fn start_nodes_thread(broker: &mut broker::Broker<lang::Lang>) -> thread::JoinHandle<()> {
    log::info!("Starting Node Manager");
    let (sender, receiver, disconnector) = broker.subscribe();

    thread::Builder::new()
        .name("nodes".to_string())
        .spawn(move || {
            let mut node_mgr = NodeManager::new();

            for event in receiver {
                match node_mgr.process_event(event) {
                    None => break,
                    Some(response) => {
                        for ev in response {
                            if let Err(e) = sender.send(ev) {
                                log::error!(
                                    "Node Manager lost connection to application bus! {}",
                                    e
                                );
                            }
                        }
                    }
                }
            }

            log::info!("Node Manager terminating");
            disconnector.disconnect();
        })
        .expect("Failed to start nodes thread!")
}
