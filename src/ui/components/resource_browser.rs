use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{
    app_state::{NodeCollections, ResourceTree},
    i18n::Language,
    widgets::{resource_row, tree},
};

use std::sync::Arc;

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ResourceBrowser<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    graphs: &'a mut NodeCollections,
    event_buffer: Option<&'a [Arc<Lang>]>,
    style: Style,
}

impl<'a> ResourceBrowser<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        graphs: &'a mut NodeCollections,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            graphs,
            event_buffer: None,
            style: Style::default(),
        }
    }

    pub fn icon_font(mut self, font_id: text::font::Id) -> Self {
        self.style.icon_font = Some(Some(font_id));
        self
    }

    pub fn event_buffer(mut self, buffer: &'a [Arc<Lang>]) -> Self {
        self.event_buffer = Some(buffer);
        self
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id")]
    icon_font: Option<Option<text::font::Id>>,
}

widget_ids! {
    pub struct Ids {
        tree
    }
}

pub struct State {
    ids: Ids,
    tree: ResourceTree,
}

impl<'a> Widget for ResourceBrowser<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            tree: ResourceTree::default(),
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

        let (mut rows, scrollbar) = tree::Tree::new(state.tree.get_tree())
            .parent(id)
            .middle_of(id)
            .padded_wh_of(id, 8.0)
            .scrollbar_on_top()
            .set(state.ids.tree, ui);

        while let Some(row) = rows.next(ui) {
            let expandable = state.tree.expandable(&row.node_id);
            let data = state.tree.get_resource_info(&row.node_id);

            let mut active = data.represents_resource(self.graphs.get_active());
            if let Some(aelem) = self.graphs.get_active_element() {
                active = active || data.represents_resource(aelem);
            }

            let widget = resource_row::ResourceRow::new(&data, row.level)
                .expandable(expandable)
                .active(active)
                .icon_font(self.style.icon_font.unwrap().unwrap())
                .h(32.0);

            match row.item.set(widget, ui) {
                None => {}
                Some(resource_row::Event::ToggleExpanded) => {
                    state.update(|state| {
                        state
                            .tree
                            .get_resource_info_mut(&row.node_id)
                            .toggle_expanded();
                    });
                }
                Some(resource_row::Event::Clicked) => {
                    if let Some(collection) = data.get_resource() {
                        self.graphs.set_active_collection(collection);
                    }

                    if let Some(node) = data.get_resource() {
                        self.graphs.set_active_element(node);
                    }
                }
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }
    }
}

impl<'a> ResourceBrowser<'a> {
    fn handle_event(&self, state: &mut widget::State<State>, event: &Lang) {
        match event {
            Lang::GraphEvent(GraphEvent::GraphAdded(res)) => {
                state.update(|state| state.tree.insert_graph(res.clone()));
            }
            Lang::GraphEvent(GraphEvent::GraphRenamed(from, to)) => {
                state.update(|state| state.tree.rename_resource(from, to));
            }
            Lang::GraphEvent(GraphEvent::NodeAdded(res, _, _, _, _)) => {
                state.update(|state| state.tree.insert_node(res.clone()));
            }
            Lang::GraphEvent(GraphEvent::NodeRemoved(res)) => {
                state.update(|state| state.tree.remove_resource_and_children(res));
            }
            Lang::GraphEvent(GraphEvent::NodeRenamed(from, to)) => {
                state.update(|state| state.tree.rename_resource(from, to));
            }
            Lang::LayersEvent(LayersEvent::LayersAdded(res, _)) => {
                state.update(|state| state.tree.insert_graph(res.clone()));
            }
            _ => {}
        }
    }
}
