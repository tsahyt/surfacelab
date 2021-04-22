use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{
    app_state,
    widgets::{filtered_list, graph, modal},
};

use std::sync::Arc;

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct NodeEditor<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    sender: &'a BrokerSender<Lang>,
    graphs: &'a mut app_state::NodeCollections,
    event_buffer: Option<&'a [Arc<Lang>]>,
    style: Style,
}

impl<'a> NodeEditor<'a> {
    pub fn new(sender: &'a BrokerSender<Lang>, graphs: &'a mut app_state::NodeCollections) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            sender,
            graphs,
            event_buffer: None,
            style: Style::default(),
        }
    }

    builder_methods! {
        pub event_buffer { event_buffer = Some(&'a [Arc<Lang>]) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        graph,
        add_modal,
    }
}

pub struct State {
    ids: Ids,
    add_modal: Option<Point>,
    operators: Vec<Operator>,
}

impl<'a> Widget for NodeEditor<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            add_modal: None,
            operators: AtomicOperator::all_default()
                .iter()
                .map(|x| Operator::from(x.clone()))
                .collect(),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs { state, ui, id, .. } = args;

        if let Some(ev_buf) = self.event_buffer {
            for ev in ev_buf {
                self.handle_event(state, ev);
            }
        }

        let collection = self
            .graphs
            .get_active_collection_mut()
            .as_graph_mut()
            .expect("Node Graph UI built for non-graph");

        let mut collection_change = None;

        for event in graph::Graph::new(&collection.graph)
            .node_title_color(color::LIGHT_CHARCOAL)
            .node_title_size(14)
            .node_active_color(Color::Rgba(0.9, 0.4, 0.15, 1.0))
            .node_selection_color(Color::Rgba(0.9, 0.8, 0.15, 1.0))
            .parent(id)
            .wh_of(id)
            .middle()
            .set(state.ids.graph, ui)
        {
            match event {
                graph::Event::NodeDrag(idx, x, y, tmp_snap) => {
                    let mut node = collection.graph.node_weight_mut(idx).unwrap();

                    node.position[0] = x;
                    node.position[1] = y;

                    if tmp_snap {
                        node.position[0] = (node.position[0] / 32.).round() * 32.;
                        node.position[1] = (node.position[1] / 32.).round() * 32.;
                    }

                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::PositionNode(
                            node.resource.clone(),
                            (node.position[0], node.position[1]),
                        )))
                        .unwrap();
                }
                graph::Event::ConnectionDrawn(from, from_socket, to, to_socket) => {
                    let from_res = collection
                        .graph
                        .node_weight(from)
                        .unwrap()
                        .resource
                        .node_socket(&from_socket);
                    let to_res = collection
                        .graph
                        .node_weight(to)
                        .unwrap()
                        .resource
                        .node_socket(&to_socket);
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::ConnectSockets(
                            from_res, to_res,
                        )))
                        .unwrap();
                }
                graph::Event::NodeDelete(idx) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::RemoveNode(
                            collection.graph.node_weight(idx).unwrap().resource.clone(),
                        )))
                        .unwrap();
                }
                graph::Event::NodeEnter(idx) => {
                    collection_change = collection.graph.node_weight(idx).unwrap().callee.clone();
                }
                graph::Event::SocketClear(idx, socket) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(
                            collection
                                .graph
                                .node_weight(idx)
                                .unwrap()
                                .resource
                                .node_socket(&socket),
                        )))
                        .unwrap();
                }
                graph::Event::ActiveElement(idx) => {
                    collection.active_element = Some(idx);
                }
                graph::Event::AddModal(pt) => {
                    state.update(|state| state.add_modal = Some(pt));
                }
                graph::Event::Extract(mut idxs) => {
                    self.sender
                        .send(Lang::UserGraphEvent(UserGraphEvent::Extract(
                            idxs.drain(0..)
                                .map(|i| collection.graph.node_weight(i).unwrap().resource.clone())
                                .collect(),
                        )))
                        .unwrap();
                }
                graph::Event::AlignNodes(idxs) => {
                    for (res, pos) in collection.align_nodes(&idxs) {
                        self.sender
                            .send(Lang::UserNodeEvent(UserNodeEvent::PositionNode(
                                res.clone(),
                                pos,
                            )))
                            .unwrap();
                    }
                }
                graph::Event::ExportSetup(idxs) => {
                    for node in idxs
                        .iter()
                        .map(|idx| collection.graph.node_weight(*idx).unwrap())
                        .filter(|n| n.exportable)
                    {
                        self.sender
                            .send(Lang::UserIOEvent(UserIOEvent::NewExportSpec(
                                ExportSpec::from(&node.resource),
                            )))
                            .unwrap();
                    }
                }
                graph::Event::SocketView(idx, socket) => self
                    .sender
                    .send(Lang::UserNodeEvent(UserNodeEvent::ViewSocket(Some(
                        collection
                            .graph
                            .node_weight(idx)
                            .unwrap()
                            .resource
                            .node_socket(&socket),
                    ))))
                    .unwrap(),
                graph::Event::SocketViewClear => self
                    .sender
                    .send(Lang::UserNodeEvent(UserNodeEvent::ViewSocket(None)))
                    .unwrap(),
            }
        }

        if let Some(g) = collection_change {
            self.graphs.set_active_collection(g.clone());
            self.sender
                .send(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(g)))
                .unwrap();
        }

        if let Some(insertion_pt) = state.add_modal {
            match modal::Modal::new(
                // widget::List::flow_down(state.operators.len())
                //     .item_size(50.0)
                //     .scrollbar_on_top(),
                filtered_list::FilteredList::new(state.operators.iter()),
            )
            .wh_of(id)
            .middle_of(id)
            .graphics_for(id)
            .set(state.ids.add_modal, ui)
            {
                modal::Event::ChildEvent((Some(op), _)) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::NewNode(
                            self.graphs.get_active().clone(),
                            op.clone(),
                            (insertion_pt[0], insertion_pt[1]),
                        )))
                        .unwrap();

                    state.update(|state| state.add_modal = None);
                }
                modal::Event::Hide => state.update(|state| state.add_modal = None),
                _ => {}
            }
        }
    }
}

impl<'a> NodeEditor<'a> {
    fn handle_event(&self, state: &mut widget::State<State>, event: &Lang) {
        match event {
            Lang::GraphEvent(GraphEvent::Cleared) => state.update(|state| {
                state.operators = AtomicOperator::all_default()
                    .iter()
                    .map(|x| Operator::from(x.clone()))
                    .collect();
            }),
            Lang::GraphEvent(GraphEvent::GraphAdded(res)) => {
                state.update(|state| {
                    state
                        .operators
                        .push(Operator::ComplexOperator(ComplexOperator::new(res.clone())))
                });
            }
            Lang::GraphEvent(GraphEvent::GraphRenamed(from, to)) => {
                state.update(|state| {
                    let old_op = Operator::ComplexOperator(ComplexOperator::new(from.clone()));
                    state.operators.remove(
                        state
                            .operators
                            .iter()
                            .position(|x| x == &old_op)
                            .expect("Missing old operator"),
                    );
                    state
                        .operators
                        .push(Operator::ComplexOperator(ComplexOperator::new(to.clone())));
                });
            }
            Lang::LayersEvent(LayersEvent::LayersAdded(res, _, _)) => {
                state.update(|state| {
                    state
                        .operators
                        .push(Operator::ComplexOperator(ComplexOperator::new(res.clone())))
                });
            }
            _ => {}
        }
    }
}

impl filtered_list::FilteredListItem for Operator {
    fn filter(&self, filter_string: &str) -> bool {
        true
    }

    fn display(&self) -> &str {
        self.title()
    }
}
