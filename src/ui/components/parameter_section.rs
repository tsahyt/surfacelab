use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{i18n::Language, widgets};

use std::collections::HashSet;
use std::sync::Arc;

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ParameterSection<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    description: &'a mut ParamBoxDescription<MessageWriters>,
    resource: &'a Resource<Node>,
    event_buffer: Option<&'a [Arc<Lang>]>,
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
        param_box,
    }
}

pub struct State {
    ids: Ids,
    image_resources: HashSet<Resource<Img>>,
}

impl<'a> Widget for ParameterSection<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            image_resources: HashSet::new(),
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

        let image_resources: Vec<_> = state.image_resources.iter().collect();

        for ev in widgets::param_box::ParamBox::new(self.description, self.resource, self.language)
            .image_resources(&image_resources)
            .parent(id)
            .w_of(id)
            .mid_top()
            .icon_font(self.style.icon_font.unwrap().unwrap())
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

impl<'a> ParameterSection<'a> {
    fn handle_event(&self, state: &mut widget::State<State>, event: &Lang) {
        match event {
            Lang::ComputeEvent(ComputeEvent::ImageResourceAdded(res)) => {
                state.update(|state| {
                    state.image_resources.insert(res.clone());
                });
            }
            _ => {}
        }
    }
}
