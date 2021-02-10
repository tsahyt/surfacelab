use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{
    app_state::{LayerFilter, NodeCollection, NodeCollections},
    i18n::Language,
    util::*,
    widgets::{layer_row, modal, tree},
};

use strum::VariantNames;

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct LayerEditor<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    graphs: &'a mut NodeCollections,
    active_layer_element: &'a mut Option<id_tree::NodeId>,
    operators: &'a [Operator],
    style: Style,
}

impl<'a> LayerEditor<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        graphs: &'a mut NodeCollections,
        active_layer_element: &'a mut Option<id_tree::NodeId>,
        operators: &'a [Operator],
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            graphs,
            active_layer_element,
            operators,
            style: Style::default(),
        }
    }

    pub fn icon_font(mut self, font_id: text::font::Id) -> Self {
        self.style.icon_font = Some(Some(font_id));
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
        modal,
        opacity,
        blend_mode,
        new_fill,
        new_fx,
        new_mask,
        delete,
        list,
    }
}

pub struct State {
    ids: Ids,
    modal: Option<LayerFilter>,
}

impl<'a> Widget for LayerEditor<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            modal: None,
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        for _press in icon_button(IconName::SOLID, self.style.icon_font.unwrap().unwrap())
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.)
            .wh([32., 32.0])
            .top_left_with_margin(8.0)
            .parent(args.id)
            .set(args.state.ids.new_fill, args.ui)
        {
            args.state
                .update(|state| state.modal = Some(LayerFilter::Layer(LayerType::Fill)));
        }

        for _press in icon_button(IconName::FX, self.style.icon_font.unwrap().unwrap())
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.)
            .wh([32., 32.0])
            .right(8.0)
            .parent(args.id)
            .set(args.state.ids.new_fx, args.ui)
        {
            args.state
                .update(|state| state.modal = Some(LayerFilter::Layer(LayerType::Fx)));
        }

        let active_collection = match self.graphs.get_active_collection_mut() {
            NodeCollection::Layers(l) => l,
            _ => panic!("Layers UI built for graph"),
        };

        if let Some((is_base, active_layer)) = self.active_layer_element.clone().map(|node_id| {
            (
                active_collection.is_base_layer(&node_id),
                active_collection
                    .layers
                    .get_mut(&node_id)
                    .unwrap()
                    .data_mut(),
            )
        }) {
            for _press in icon_button(IconName::TRASH, self.style.icon_font.unwrap().unwrap())
                .label_font_size(14)
                .label_color(color::WHITE)
                .color(color::DARK_CHARCOAL)
                .border(0.)
                .wh([32., 32.0])
                .top_right_with_margin(8.0)
                .parent(args.id)
                .set(args.state.ids.delete, args.ui)
            {
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
                *self.active_layer_element = None;
            }

            if !is_base && !active_layer.is_mask {
                for _press in icon_button(IconName::MASK, self.style.icon_font.unwrap().unwrap())
                    .label_font_size(14)
                    .label_color(color::WHITE)
                    .color(color::DARK_CHARCOAL)
                    .border(0.)
                    .wh([32., 32.0])
                    .left(8.0)
                    .parent(args.id)
                    .set(args.state.ids.new_mask, args.ui)
                {
                    args.state.update(|state| {
                        state.modal = Some(LayerFilter::Mask(active_layer.resource.clone()))
                    });
                }
            }

            if let Some(new_selection) =
                widget::DropDownList::new(BlendMode::VARIANTS, Some(active_layer.blend_mode))
                    .label_font_size(10)
                    .down_from(args.state.ids.new_fill, 8.0)
                    .padded_w_of(args.id, 8.0)
                    .h(16.0)
                    .parent(args.id)
                    .set(args.state.ids.blend_mode, args.ui)
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
                .padded_w_of(args.id, 8.0)
                .h(16.0)
                .parent(args.id)
                .set(args.state.ids.opacity, args.ui)
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
                .down_from(args.state.ids.new_fill, 8.0)
                .padded_w_of(args.id, 8.0)
                .h(16.0)
                .parent(args.id)
                .set(args.state.ids.blend_mode, args.ui);

            widget::Slider::new(1.0, 0.0, 1.0)
                .enabled(false)
                .label(&self.language.get_message("opacity"))
                .label_font_size(10)
                .down(8.0)
                .padded_w_of(args.id, 8.0)
                .h(16.0)
                .parent(args.id)
                .set(args.state.ids.opacity, args.ui);
        }

        let (mut rows, scrollbar) = tree::Tree::without_root(&active_collection.layers)
            .parent(args.id)
            .item_size(48.0)
            .padded_w_of(args.id, 8.0)
            .middle_of(args.id)
            .h(512.0)
            .down(8.0)
            .scrollbar_on_top()
            .set(args.state.ids.list, args.ui);

        while let Some(row) = rows.next(args.ui) {
            let node_id = row.node_id.clone();
            let toggleable = !active_collection.is_base_layer(&node_id);
            let expandable = active_collection.expandable(&node_id);
            let data = &mut active_collection
                .layers
                .get_mut(&node_id)
                .unwrap()
                .data_mut();

            let widget =
                layer_row::LayerRow::new(data, Some(row.node_id) == *self.active_layer_element)
                    .toggleable(toggleable)
                    .expandable(expandable)
                    .icon_font(self.style.icon_font.unwrap().unwrap());

            if let Some(event) = row.item.set(widget, args.ui) {
                match event {
                    layer_row::Event::ActiveElement => {
                        *self.active_layer_element = Some(node_id);
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
            s.set(args.ui);
        }

        if let Some(filter) = args.state.modal.as_ref().cloned() {
            let mut operators = self.operators.iter().filter(|o| match filter {
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
            .wh_of(args.id)
            .middle_of(args.id)
            .graphics_for(args.id)
            .set(args.state.ids.modal, args.ui)
            {
                modal::Event::ChildEvent(((mut items, scrollbar), _)) => {
                    while let Some(item) = items.next(args.ui) {
                        let op = operators.next().unwrap();
                        let label = op.title();
                        let button = widget::Button::new()
                            .label(&label)
                            .label_color(conrod_core::color::WHITE)
                            .label_font_size(12)
                            .color(conrod_core::color::CHARCOAL);
                        for _press in item.set(button, args.ui) {
                            args.state.update(|state| state.modal = None);

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
                        s.set(args.ui)
                    }
                }
                modal::Event::Hide => args.state.update(|state| state.modal = None),
            }
        }
    }
}
