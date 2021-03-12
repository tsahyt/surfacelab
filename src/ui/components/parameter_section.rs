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
    image_resources: &'a [(Resource<Img>, ColorSpace, bool)],
    style: Style,
}

impl<'a> ParameterSection<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        description: &'a mut ParamBoxDescription<MessageWriters>,
        resource: &'a Resource<Node>,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            description,
            resource,
            image_resources: &[],
            style: Style::default(),
        }
    }

    builder_methods! {
        pub image_resources { image_resources = &'a [(Resource<Img>, ColorSpace, bool)] }
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

        for ev in widgets::param_box::ParamBox::new(self.description, self.resource, self.language)
            .image_resources(&self.image_resources)
            .parent(id)
            .w_of(id)
            .mid_top()
            .icon_font(style.icon_font(&ui.theme))
            .set(state.ids.param_box, ui)
        {
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
