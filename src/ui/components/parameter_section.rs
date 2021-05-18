use std::collections::HashMap;

use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{i18n::Language, widgets};

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ParameterSection<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    description: &'a mut ParamBoxDescription<MessageWriters>,
    resource: &'a Resource<Node>,
    image_resources: &'a [(Resource<Img>, (ColorSpace, bool))],
    svg_resources: &'a [(Resource<resource::Svg>, bool)],
    type_variables: Option<&'a HashMap<TypeVariable, ImageType>>,
    parent_size: u32,
    style: Style,
}

impl<'a> ParameterSection<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        description: &'a mut ParamBoxDescription<MessageWriters>,
        resource: &'a Resource<Node>,
        parent_size: u32,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            description,
            resource,
            parent_size,
            image_resources: &[],
            svg_resources: &[],
            type_variables: None,
            style: Style::default(),
        }
    }

    builder_methods! {
        pub image_resources { image_resources = &'a [(Resource<Img>, (ColorSpace, bool))] }
        pub svg_resources { svg_resources = &'a [(Resource<resource::Svg>, bool)] }
        pub type_variables { type_variables = Some(&'a HashMap<TypeVariable, ImageType>) }
        pub icon_font { style.icon_font = Some(text::font::Id) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
}

widget_ids! {
    pub struct Ids {
        param_box,
    }
}

pub struct State {
    ids: Ids,
}

impl<'a> Widget for ParameterSection<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
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

        let mut pbox =
            widgets::param_box::ParamBox::new(self.description, self.resource, self.language)
                .image_resources(&self.image_resources)
                .svg_resources(&self.svg_resources)
                .parent_size(self.parent_size)
                .parent(id)
                .w_of(id)
                .mid_top()
                .icon_font(style.icon_font(&ui.theme))
                .text_size(10)
                .text_color(color::WHITE)
                .presets(true);

        if let Some(ty_vars) = self.type_variables {
            pbox = pbox.type_variables(&ty_vars);
        }

        for ev in pbox.set(state.ids.param_box, ui) {
            let resp = match ev {
                widgets::param_box::Event::ChangeParameter(event) => event,
                widgets::param_box::Event::ExposeParameter(field, name, control) => {
                    Lang::UserGraphEvent({
                        let p_res = self.resource.clone().node_parameter(&field);
                        UserGraphEvent::ExposeParameter(p_res, field, name, control)
                    })
                }
                widgets::param_box::Event::ConcealParameter(field) => Lang::UserGraphEvent(
                    UserGraphEvent::ConcealParameter(self.resource.clone().node_graph(), field),
                ),
            };

            self.sender.send(resp).unwrap();
        }
    }
}
