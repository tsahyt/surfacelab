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
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub event_buffer { event_buffer = Some(&'a [Arc<Lang>]) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
}

widget_ids! {
    pub struct Ids {
        graph,
        add_modal,
        operator_list,
    }
}

pub struct State {
    ids: Ids,
    add_modal: Option<Box<dyn Fn(Resource<Graph>, Operator) -> Lang + Send>>,
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
        let widget::UpdateArgs {
            state,
            ui,
            id,
            style,
            ..
        } = args;

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

        let mut collection_change: Option<Resource<Graph>> = None;

        for event in graph::Graph::new(&collection)
            .enabled(state.add_modal.is_none())
            .icon_font(style.icon_font(&ui.theme))
            .node_title_color(color::LIGHT_CHARCOAL)
            .node_title_size(14)
            .node_active_color(Color::Rgba(0.9, 0.4, 0.15, 1.0))
            .node_selection_color(Color::Rgba(0.9, 0.8, 0.15, 1.0))
            .crop_kids()
            .parent(id)
            .wh_of(id)
            .middle()
            .set(state.ids.graph, ui)
        {
            match event {
                graph::Event::NodeDrag(res, new_pos, tmp_snap) => {
                    let pos = collection.move_node(&res, new_pos, tmp_snap);

                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::PositionNode(
                            res,
                            (pos[0], pos[1]),
                        )))
                        .unwrap();
                }
                graph::Event::ConnectionDrawn(from, to) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::ConnectSockets(from, to)))
                        .unwrap();
                }
                graph::Event::ConnectBetween(node, source, sink) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::ConnectBetweenSockets(
                            node, source, sink,
                        )))
                        .unwrap();
                }
                graph::Event::QuickCombine(node_1, node_2, true) => {
                    state.update(|state| {
                        state.add_modal = Some(Box::new(move |_, op| {
                            Lang::UserNodeEvent(UserNodeEvent::QuickCombine(
                                op,
                                node_1.clone(),
                                node_2.clone(),
                            ))
                        }))
                    });
                }
                graph::Event::QuickCombine(node_1, node_2, false) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::QuickCombine(
                            AtomicOperator::from(Blend::default()).into(),
                            node_1,
                            node_2,
                        )))
                        .unwrap();
                }
                graph::Event::NodeDelete(node) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::RemoveNode(node)))
                        .unwrap();
                }
                graph::Event::NodeDissolve(node) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::DissolveNode(node)))
                        .unwrap();
                }
                graph::Event::NodeEnter(node) => {
                    collection_change = collection.nodes.get(&node).unwrap().callee.clone();
                }
                graph::Event::NodeInject(node) => {
                    if let Some(g) = &collection.nodes.get(&node).unwrap().callee {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::Inject(
                                node,
                                g.clone(),
                                true,
                            )))
                            .unwrap();
                    }
                }
                graph::Event::SocketClear(socket) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(
                            socket,
                        )))
                        .unwrap();
                }
                graph::Event::ActiveElement(node) => {
                    collection.active_element = Some(node);
                }
                graph::Event::AddNode(pt, socket) => {
                    state.update(|state| {
                        state.add_modal = Some(Box::new(move |g, op| {
                            Lang::UserNodeEvent(UserNodeEvent::NewNode(
                                g,
                                op,
                                (pt[0], pt[1]),
                                socket.clone(),
                                None,
                            ))
                        }))
                    });
                }
                graph::Event::Extract(nodes) => {
                    self.sender
                        .send(Lang::UserGraphEvent(UserGraphEvent::Extract(nodes)))
                        .unwrap();
                }
                graph::Event::AlignNodes(nodes) => {
                    for (res, pos) in collection.align_nodes(&nodes) {
                        self.sender
                            .send(Lang::UserNodeEvent(UserNodeEvent::PositionNode(
                                res.clone(),
                                pos,
                            )))
                            .unwrap();
                    }
                }
                graph::Event::ExportSetup(ress) => {
                    for node in ress
                        .iter()
                        .map(|res| collection.nodes.get(res).unwrap())
                        .filter(|n| n.exportable)
                    {
                        self.sender
                            .send(Lang::UserIOEvent(UserIOEvent::NewExportSpec(
                                ExportSpec::from(&node.resource),
                                false,
                            )))
                            .unwrap();
                    }
                }
                graph::Event::SocketView(socket) => self
                    .sender
                    .send(Lang::UserNodeEvent(UserNodeEvent::ViewSocket(Some(socket))))
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

        if let Some(transmitter) = state.add_modal.as_ref() {
            match modal::Modal::canvas()
                .wh_of(id)
                .middle_of(id)
                .graphics_for(id)
                .set(state.ids.add_modal, ui)
            {
                modal::Event::ChildEvent((_, cid)) => {
                    if let Some(op) = filtered_list::FilteredList::new(state.operators.iter())
                        .icon_font(style.icon_font(&ui.theme))
                        .parent(cid)
                        .padded_wh_of(cid, 8.)
                        .middle()
                        .set(state.ids.operator_list, ui)
                    {
                        self.sender
                            .send(transmitter(self.graphs.get_active().clone(), op.clone()))
                            .unwrap();

                        state.update(|state| state.add_modal = None);
                    }
                }
                modal::Event::Hide => state.update(|state| state.add_modal = None),
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
        self.title().to_lowercase().starts_with(filter_string)
    }

    fn display(&self) -> &str {
        self.title()
    }
}
