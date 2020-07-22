use crate::{broker, lang};
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use maplit::hashmap;

pub mod io;
pub mod nodegraph;

struct NodeManager {
    parent_size: u32,
    graphs: HashMap<String, nodegraph::NodeGraph>,
    active_graph: lang::Resource,
}

// FIXME: Changing output socket type after connection has already been made does not propagate type changes into preceeding polymorphic nodes!
impl NodeManager {
    pub fn new() -> Self {
        NodeManager {
            parent_size: 1024,
            graphs: hashmap! { "base".to_string() => nodegraph::NodeGraph::new("base") },
            active_graph: lang::Resource::graph("base", None),
        }
    }

    pub fn process_event(&mut self, event: Arc<lang::Lang>) -> Option<Vec<lang::Lang>> {
        use crate::lang::*;
        let mut response = vec![];

        match &*event {
            Lang::UserNodeEvent(event) => match event {
                UserNodeEvent::NewNode(graph, op) => {
                    let graph_name = graph.path().to_str().unwrap();

                    let op = match op {
                        lang::Operator::ComplexOperator(co) => {
                            let mut co = co.clone();
                            co.outputs =
                                self.graphs.get(co.graph.file().unwrap()).unwrap().outputs();
                            lang::Operator::ComplexOperator(co)
                        }
                        lang::Operator::AtomicOperator(_) => op.clone(),
                    };

                    let (node_id, size) = self
                        .graphs
                        .get_mut(graph_name)
                        .unwrap()
                        .new_node(&op, self.parent_size);
                    response.push(Lang::GraphEvent(GraphEvent::NodeAdded(
                        Resource::node(
                            [graph_name, &node_id]
                                .iter()
                                .collect::<std::path::PathBuf>(),
                            None,
                        ),
                        op.clone(),
                        None,
                        size as u32,
                    )))
                }
                UserNodeEvent::RemoveNode(res) => {
                    let node = res.file().unwrap();
                    let graph = res.directory().unwrap();
                    match self.graphs.get_mut(graph).unwrap().remove_node(node) {
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
                    let from_node = from.file().unwrap();
                    let from_socket = from.fragment().unwrap();
                    let to_node = to.file().unwrap();
                    let to_socket = to.fragment().unwrap();
                    let graph = from.directory().unwrap();
                    debug_assert_eq!(graph, to.directory().unwrap());
                    match self.graphs.get_mut(graph).unwrap().connect_sockets(
                        from_node,
                        from_socket,
                        to_node,
                        to_socket,
                    ) {
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
                    let node = sink.file().unwrap();
                    let socket = sink.fragment().unwrap();
                    let graph = sink.directory().unwrap();
                    match self
                        .graphs
                        .get_mut(graph)
                        .unwrap()
                        .disconnect_sink_socket(node, socket)
                    {
                        Ok(mut r) => response.append(&mut r),
                        Err(e) => log::error!("Error while disconnecting sink {}", e),
                    }
                }
                UserNodeEvent::ParameterChange(res, field, data) => {
                    let node = res.file().unwrap();
                    let graph = res.directory().unwrap();
                    self.graphs
                        .get_mut(graph)
                        .unwrap()
                        .parameter_change(node, field, data)
                        .unwrap_or_else(|e| log::error!("{}", e));
                    let instructions = self.graphs.get_mut(graph).unwrap().linearize();
                    response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                        Resource::graph(graph, None),
                        instructions,
                    )));
                    response.push(Lang::GraphEvent(GraphEvent::Recompute(
                        self.active_graph.clone(),
                    )));
                }
                UserNodeEvent::PositionNode(res, (x, y)) => {
                    let node = res.file().unwrap();
                    let graph = res.directory().unwrap();
                    self.graphs
                        .get_mut(graph)
                        .unwrap()
                        .position_node(node, *x, *y);
                }
                UserNodeEvent::RenameNode(from, to) => {
                    let from_node = from.file().unwrap();
                    let to_node = to.file().unwrap();
                    let graph = from.directory().unwrap();
                    debug_assert_eq!(graph, to.directory().unwrap());
                    if let Some(r) = self
                        .graphs
                        .get_mut(graph)
                        .unwrap()
                        .rename_node(from_node, to_node)
                    {
                        response.push(r);
                    }
                }
                UserNodeEvent::OutputSizeChange(res, size) => {
                    let node = res.file().unwrap();
                    let graph = res.directory().unwrap();
                    if let Some(r) = self.graphs.get_mut(graph).unwrap().resize_node(
                        node,
                        Some(*size),
                        None,
                        self.parent_size,
                    ) {
                        response.push(r);
                    };
                }
                UserNodeEvent::OutputSizeAbsolute(res, abs) => {
                    let node = res.file().unwrap();
                    let graph = res.directory().unwrap();
                    if let Some(r) = self.graphs.get_mut(graph).unwrap().resize_node(
                        node,
                        None,
                        Some(*abs),
                        self.parent_size,
                    ) {
                        response.push(r);
                    };
                }
            },
            Lang::UserGraphEvent(event) => match event {
                UserGraphEvent::AddGraph(name) => {
                    self.graphs
                        .insert(name.clone(), nodegraph::NodeGraph::new(&name));
                    response.push(lang::Lang::GraphEvent(lang::GraphEvent::GraphAdded(
                        Resource::graph(name, None),
                    )));
                }
                UserGraphEvent::ChangeGraph(res) => {
                    let graph = self
                        .graphs
                        .get(res.path_str().unwrap())
                        .expect("Node Graph not found");
                    self.active_graph = res.clone();
                    response.push(lang::Lang::GraphEvent(lang::GraphEvent::Report(
                        graph.nodes(),
                        graph.connections(),
                    )));
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
                match self.open_surface(path) {
                    Ok(mut evs) => {
                        response.push(Lang::GraphEvent(GraphEvent::Cleared));
                        response.append(&mut evs);

                        // Automatically recompute on load
                        let instructions = self.graphs.get_mut("base").unwrap().linearize();
                        response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                            self.active_graph.clone(),
                            instructions,
                        )));
                        response.push(Lang::GraphEvent(GraphEvent::Recompute(
                            self.active_graph.clone(),
                        )));
                    }
                    Err(e) => log::error!("{}", e),
                }
            }
            Lang::UserIOEvent(UserIOEvent::SaveSurface(path)) => {
                if let Err(e) = self.save_surface(path) {
                    log::error!("{}", e)
                }
            }
            Lang::UserIOEvent(UserIOEvent::NewSurface) => {
                self.graphs.clear();
                self.graphs
                    .insert("base".to_string(), nodegraph::NodeGraph::new("base"));
                response.push(Lang::GraphEvent(GraphEvent::Cleared));
                response.push(Lang::GraphEvent(GraphEvent::GraphAdded(Resource::graph(
                    "base", None,
                ))));
            }
            Lang::UserIOEvent(UserIOEvent::SetParentSize(size)) => {
                self.parent_size = *size;
                for g in self.graphs.values_mut() {
                    response.append(&mut g.resize_all(self.parent_size));
                }

                // Recompute on size change
                let instructions = self.graphs.get_mut("base").unwrap().linearize();
                response.push(Lang::GraphEvent(GraphEvent::Relinearized(
                    self.active_graph.clone(),
                    instructions,
                )));
                response.push(Lang::GraphEvent(GraphEvent::Recompute(
                    self.active_graph.clone(),
                )));
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
