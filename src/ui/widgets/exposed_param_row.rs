use crate::lang::*;
use crate::ui::i18n::Language;
use crate::ui::util::*;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ExposedParamRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    param: &'a mut GraphParameter,
    style: Style,
    language: &'a Language,
}

impl<'a> ExposedParamRow<'a> {
    pub fn new(param: &'a mut GraphParameter, language: &'a Language) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            param,
            language,
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
        resource,
        control,
        field_label,
        field,
        title_label,
        title,
    }
}

pub enum Event {
    ConcealParameter,
    UpdateTitle,
    UpdateField,
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
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .top_left()
            .wh([20.0, 16.0])
            .label_font_size(12)
            .set(args.state.unexpose_button, args.ui)
        {
            ev = Some(Event::ConcealParameter)
        }

        widget::Text::new(&self.param.parameter.to_string())
            .parent(args.id)
            .font_size(10)
            .right(8.0)
            .color(color::WHITE)
            .set(args.state.resource, args.ui);

        widget::Text::new(control_name(&self.param.control))
            .parent(args.id)
            .font_size(14)
            .top_right()
            .color(color::GRAY)
            .set(args.state.control, args.ui);

        widget::Text::new(&self.language.get_message("exposed-field"))
            .parent(args.id)
            .top_left_with_margin(32.0)
            .font_size(10)
            .down(16.0)
            .color(color::WHITE)
            .set(args.state.field_label, args.ui);

        for event in widget::TextBox::new(&self.param.graph_field)
            .parent(args.id)
            .font_size(10)
            .down(16.0)
            .padded_w_of(args.id, 16.0)
            .h(16.0)
            .set(args.state.field, args.ui)
        {
            match event {
                widget::text_box::Event::Update(new) => {
                    self.param.graph_field = new;
                }
                widget::text_box::Event::Enter => {
                    ev = Some(Event::UpdateField);
                }
            }
        }

        widget::Text::new(&self.language.get_message("exposed-title"))
            .parent(args.id)
            .font_size(10)
            .down(16.0)
            .color(color::WHITE)
            .set(args.state.title_label, args.ui);

        for event in widget::TextBox::new(&self.param.title)
            .parent(args.id)
            .font_size(10)
            .down(16.0)
            .padded_w_of(args.id, 16.0)
            .h(16.0)
            .set(args.state.title, args.ui)
        {
            match event {
                widget::text_box::Event::Update(new) => {
                    self.param.title = new;
                }
                widget::text_box::Event::Enter => {
                    ev = Some(Event::UpdateTitle);
                }
            }
        }

        ev
    }
}

fn control_name(control: &Control) -> &'static str {
    match control {
        Control::Slider { .. } => "f32",
        Control::DiscreteSlider { .. } => "i32",
        Control::RgbColor { .. } => "rgb",
        Control::Enum { .. } => "list",
        Control::File { .. } => "file",
        Control::Ramp { .. } => "ramp",
        Control::Toggle { .. } => "bool",
        Control::Entry { .. } => "text",
        Control::ChannelMap { .. } => "chn",
    }
}
