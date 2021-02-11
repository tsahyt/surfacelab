use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{
    app_state,
    widgets::{graph, modal},
};

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct NodeEditor<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    sender: &'a BrokerSender<Lang>,
    graphs: &'a mut app_state::NodeCollections,
    operators: &'a [Operator],
    style: Style,
}

impl<'a> NodeEditor<'a> {
    pub fn new(
        sender: &'a BrokerSender<Lang>,
        graphs: &'a mut app_state::NodeCollections,
        operators: &'a [Operator],
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            sender,
            graphs,
            operators,
            style: Style::default(),
        }
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
    active: Option<petgraph::graph::NodeIndex>,
}

impl<'a> Widget for NodeEditor<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            add_modal: None,
            active: None,
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs { state, ui, id, .. } = args;

        let active = &mut self
            .graphs
            .get_active_collection_mut()
            .as_graph_mut()
            .expect("Node Graph UI built for non-graph")
            .graph;

        for event in graph::Graph::new(&active)
            .parent(id)
            .wh_of(id)
            .middle()
            .set(state.ids.graph, ui)
        {
            match event {
                graph::Event::NodeDrag(idx, x, y) => {
                    let mut node = active.node_weight_mut(idx).unwrap();
                    node.position[0] += x;
                    node.position[1] += y;

                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::PositionNode(
                            node.resource.clone(),
                            (node.position[0], node.position[1]),
                        )))
                        .unwrap();
                }
                graph::Event::ConnectionDrawn(from, from_socket, to, to_socket) => {
                    let from_res = active
                        .node_weight(from)
                        .unwrap()
                        .resource
                        .node_socket(&from_socket);
                    let to_res = active
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
                            active.node_weight(idx).unwrap().resource.clone(),
                        )))
                        .unwrap();
                }
                graph::Event::SocketClear(idx, socket) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(
                            active
                                .node_weight(idx)
                                .unwrap()
                                .resource
                                .node_socket(&socket),
                        )))
                        .unwrap();
                }
                graph::Event::ActiveElement(idx) => {
                    state.update(|state| state.active = Some(idx));
                }
                graph::Event::AddModal(pt) => {
                    state.update(|state| state.add_modal = Some(pt));
                }
            }
        }

        if let Some(insertion_pt) = state.add_modal {
            match modal::Modal::new(
                widget::List::flow_down(self.operators.len())
                    .item_size(50.0)
                    .scrollbar_on_top(),
            )
            .wh_of(id)
            .middle_of(id)
            .graphics_for(id)
            .set(state.ids.add_modal, ui)
            {
                modal::Event::ChildEvent(((mut items, scrollbar), _)) => {
                    while let Some(item) = items.next(ui) {
                        let i = item.i;
                        let label = self.operators[i].title();
                        let button = widget::Button::new()
                            .label(&label)
                            .label_color(conrod_core::color::WHITE)
                            .label_font_size(12)
                            .color(conrod_core::color::CHARCOAL);
                        for _press in item.set(button, ui) {
                            state.update(|state| state.add_modal = None);

                            self.sender
                                .send(Lang::UserNodeEvent(UserNodeEvent::NewNode(
                                    self.graphs.get_active().clone(),
                                    self.operators[i].clone(),
                                    (insertion_pt[0], insertion_pt[1]),
                                )))
                                .unwrap();
                        }
                    }

                    if let Some(s) = scrollbar {
                        s.set(ui)
                    }
                }
                modal::Event::Hide => state.update(|state| state.add_modal = None),
            }
        }
    }
}
