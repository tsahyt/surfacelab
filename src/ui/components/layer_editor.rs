use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{
    app_state::{LayerFilter, NodeCollection, NodeCollections},
    i18n::Language,
    util::*,
    widgets::{layer_row, modal, toolbar, tree},
};

use std::sync::Arc;

use strum::VariantNames;

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct LayerEditor<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    graphs: &'a mut NodeCollections,
    event_buffer: Option<&'a [Arc<Lang>]>,
    style: Style,
}

impl<'a> LayerEditor<'a> {
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
        modal,
        opacity,
        blend_mode,
        main_toolbar,
        context_toolbar,
        list,
    }
}

pub struct State {
    ids: Ids,
    modal: Option<LayerFilter>,
    operators: Vec<Operator>,
}

#[derive(Copy, Clone)]
pub enum MainTool {
    NewFill,
    NewFx,
}

#[derive(Copy, Clone)]
pub enum ContextTool {
    Delete,
    NewMask,
}

impl<'a> Widget for LayerEditor<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            modal: None,
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

        match toolbar::Toolbar::flow_right(
            [
                (IconName::SOLID, MainTool::NewFill),
                (IconName::FX, MainTool::NewFx),
            ]
            .iter()
            .copied(),
        )
        .icon_font(style.icon_font(&ui.theme))
        .icon_color(color::WHITE)
        .button_color(color::DARK_CHARCOAL)
        .parent(id)
        .w(64.0 + 8.0)
        .h(32.0)
        .top_left_with_margins(8.0, 0.0)
        .set(state.ids.main_toolbar, ui)
        {
            Some(MainTool::NewFill) => {
                state.update(|state| state.modal = Some(LayerFilter::Layer(LayerType::Fill)));
            }
            Some(MainTool::NewFx) => {
                state.update(|state| state.modal = Some(LayerFilter::Layer(LayerType::Fx)));
            }
            _ => {}
        }

        let active_collection = match self.graphs.get_active_collection_mut() {
            NodeCollection::Layers(l) => l,
            _ => panic!("Layers UI built for graph"),
        };

        if let Some((is_base, active_layer)) =
            active_collection
                .active_element
                .clone()
                .and_then(|node_id| {
                    Some((
                        active_collection.is_base_layer(&node_id),
                        active_collection.layers.get_mut(&node_id).ok()?.data_mut(),
                    ))
                })
        {
            let mut context_tools = vec![(IconName::TRASH, ContextTool::Delete)];

            if !is_base && !active_layer.is_mask {
                context_tools.push((IconName::MASK, ContextTool::NewMask));
            }

            match toolbar::Toolbar::flow_left(context_tools.drain(0..))
                .icon_font(style.icon_font(&ui.theme))
                .icon_color(color::WHITE)
                .button_color(color::DARK_CHARCOAL)
                .parent(id)
                .w(64.0 + 8.0)
                .h(32.0)
                .top_right_with_margins(8.0, 0.0)
                .set(state.ids.context_toolbar, ui)
            {
                Some(ContextTool::Delete) => {
                    self.sender
                        .send(if active_layer.is_mask {
                            Lang::UserLayersEvent(UserLayersEvent::RemoveMask(
                                active_layer.resource.clone(),
                            ))
                        } else {
                            Lang::UserLayersEvent(UserLayersEvent::RemoveLayer(
                                active_layer.resource.clone(),
                            ))
                        })
                        .unwrap();
                }
                Some(ContextTool::NewMask) => {
                    state.update(|state| {
                        state.modal = Some(LayerFilter::Mask(active_layer.resource.clone()))
                    });
                }
                _ => {}
            }

            if let Some(new_selection) =
                widget::DropDownList::new(BlendMode::VARIANTS, Some(active_layer.blend_mode))
                    .label_font_size(10)
                    .top_left_with_margins(48.0, 8.0)
                    .padded_w_of(id, 8.0)
                    .h(16.0)
                    .parent(id)
                    .set(state.ids.blend_mode, ui)
            {
                use strum::IntoEnumIterator;

                active_layer.blend_mode = new_selection;

                self.sender
                    .send(Lang::UserLayersEvent(UserLayersEvent::SetBlendMode(
                        active_layer.resource.clone(),
                        BlendMode::iter().nth(new_selection).unwrap(),
                    )))
                    .unwrap();
            }

            if let Some(new_value) = widget::Slider::new(active_layer.opacity, 0.0, 1.0)
                .label(&self.language.get_message("opacity"))
                .label_font_size(10)
                .down(8.0)
                .padded_w_of(id, 8.0)
                .h(16.0)
                .parent(id)
                .set(state.ids.opacity, ui)
            {
                active_layer.opacity = new_value;

                self.sender
                    .send(Lang::UserLayersEvent(UserLayersEvent::SetOpacity(
                        active_layer.resource.clone(),
                        new_value,
                    )))
                    .unwrap();
            }
        } else {
            widget::DropDownList::new(BlendMode::VARIANTS, Some(0))
                .enabled(false)
                .label_font_size(10)
                .parent(id)
                .top_left_with_margins(48.0, 8.0)
                .padded_w_of(id, 8.0)
                .h(16.0)
                .set(state.ids.blend_mode, ui);

            widget::Slider::new(1.0, 0.0, 1.0)
                .enabled(false)
                .label(&self.language.get_message("opacity"))
                .label_font_size(10)
                .down(8.0)
                .padded_w_of(id, 8.0)
                .h(16.0)
                .parent(id)
                .set(state.ids.opacity, ui);
        }

        let (mut rows, scrollbar) = tree::Tree::without_root(&active_collection.layers)
            .parent(id)
            .item_size(48.0)
            .padded_w_of(id, 8.0)
            .middle_of(id)
            .h(512.0)
            .down(8.0)
            .scrollbar_on_top()
            .set(state.ids.list, ui);

        while let Some(row) = rows.next(ui) {
            let node_id = row.node_id.clone();
            let toggleable = !active_collection.is_base_layer(&node_id);
            let expandable = active_collection.expandable(&node_id);
            let data = &mut active_collection
                .layers
                .get_mut(&node_id)
                .unwrap()
                .data_mut();

            let widget = layer_row::LayerRow::new(
                data,
                Some(row.node_id) == active_collection.active_element,
            )
            .toggleable(toggleable)
            .expandable(expandable)
            .color(color::WHITE)
            .icon_size(10)
            .icon_size_large(14)
            .icon_font(style.icon_font(&ui.theme));

            if let Some(event) = row.item.set(widget, ui) {
                match event {
                    layer_row::Event::ActiveElement => {
                        active_collection.active_element = Some(node_id);
                    }
                    layer_row::Event::Retitled(new) => {
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::SetTitle(
                                data.resource.to_owned(),
                                new,
                            )))
                            .unwrap();
                    }
                    layer_row::Event::ToggleEnabled => {
                        data.enabled = !data.enabled;
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::SetEnabled(
                                data.resource.to_owned(),
                                data.enabled,
                            )))
                            .unwrap();
                    }
                    layer_row::Event::ToggleExpanded => {
                        data.toggle_expanded();
                    }
                    layer_row::Event::MoveUp => {
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::MoveUp(
                                data.resource.clone(),
                            )))
                            .unwrap();
                    }
                    layer_row::Event::MoveDown => {
                        self.sender
                            .send(Lang::UserLayersEvent(UserLayersEvent::MoveDown(
                                data.resource.clone(),
                            )))
                            .unwrap();
                    }
                }
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }

        if let Some(filter) = state.modal.as_ref().cloned() {
            let mut hide_modal = false;
            let mut operators = state.operators.iter().filter(|o| match filter {
                LayerFilter::Layer(LayerType::Fill) => o.inputs().is_empty(),
                LayerFilter::Layer(LayerType::Fx) => !o.inputs().is_empty(),
                LayerFilter::Mask(..) => o.is_mask(),
            });

            match modal::Modal::new(
                widget::List::flow_down(operators.clone().count())
                    .item_size(50.0)
                    .scrollbar_on_top(),
            )
            .padding(32.0)
            .wh_of(id)
            .middle_of(id)
            .graphics_for(id)
            .set(state.ids.modal, ui)
            {
                modal::Event::ChildEvent(((mut items, scrollbar), _)) => {
                    while let Some(item) = items.next(ui) {
                        let op = operators.next().unwrap();
                        let label = op.title();
                        let button = widget::Button::new()
                            .label(&label)
                            .label_color(conrod_core::color::WHITE)
                            .label_font_size(12)
                            .color(conrod_core::color::CHARCOAL);
                        for _press in item.set(button, ui) {
                            hide_modal = true;

                            self.sender
                                .send(match &filter {
                                    LayerFilter::Layer(filter) => {
                                        Lang::UserLayersEvent(UserLayersEvent::PushLayer(
                                            self.graphs.get_active().clone(),
                                            *filter,
                                            op.clone(),
                                        ))
                                    }
                                    LayerFilter::Mask(for_layer) => Lang::UserLayersEvent(
                                        UserLayersEvent::PushMask(for_layer.clone(), op.clone()),
                                    ),
                                })
                                .unwrap();
                        }
                    }

                    if let Some(s) = scrollbar {
                        s.set(ui)
                    }
                }
                modal::Event::Hide => hide_modal = true,
            }

            if hide_modal {
                state.update(|state| state.modal = None);
            }
        }
    }
}

impl<'a> LayerEditor<'a> {
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
