use super::util::*;
use conrod_core::*;

#[derive(Clone, WidgetCommon)]
pub struct ExposedParamRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    param_name: &'a str,
    style: Style,
}

impl<'a> ExposedParamRow<'a> {
    pub fn new(param_name: &'a str) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            param_name,
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
        unexpose_button,
        param_name
    }
}

pub enum Event {
    ConcealParameter,
}

impl<'a> Widget for ExposedParamRow<'a> {
    type State = Ids;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Ids::new(id_gen)
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut ev = None;

        for _press in icon_button(IconName::UNEXPOSE, self.style.icon_font.unwrap().unwrap())
            .parent(args.id)
            .mid_left()
            .wh([20.0, 16.0])
            .label_font_size(12)
            .set(args.state.unexpose_button, args.ui)
        {
            ev = Some(Event::ConcealParameter)
        }

        widget::Text::new(self.param_name)
            .parent(args.id)
            .font_size(10)
            .right(8.0)
            .color(color::WHITE)
            .set(args.state.param_name, args.ui);

        ev
    }
}
