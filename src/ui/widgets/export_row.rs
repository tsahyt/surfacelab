use crate::{lang::ExportSpec, ui::i18n::Language};
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ExportRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    name: &'a str,
    spec: &'a ExportSpec,
    style: Style,
    language: &'a Language,
}

impl<'a> ExportRow<'a> {
    pub fn new(spec: &'a (String, ExportSpec), language: &'a Language) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            name: &spec.0,
            spec: &spec.1,
            language,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
    }
}

pub struct State {
    ids: Ids,
}

pub enum Event {}

impl<'a> Widget for ExportRow<'a> {
    type State = State;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        None
    }
}
