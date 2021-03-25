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

    builder_methods! {
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub icon_size { style.icon_size = Some(FontSize) }
        pub text_size { style.text_size = Some(FontSize) }
        pub ctrl_size { style.ctrl_size = Some(FontSize) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
    #[conrod(default = "theme.font_size_small")]
    icon_size: Option<FontSize>,
    #[conrod(default = "theme.font_size_small")]
    text_size: Option<FontSize>,
    #[conrod(default = "theme.font_size_large")]
    ctrl_size: Option<FontSize>,
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
        let widget::UpdateArgs {
            state,
            ui,
            id,
            style,
            ..
        } = args;

        let mut ev = None;

        for _press in icon_button(IconName::UNEXPOSE, style.icon_font(&ui.theme))
            .parent(id)
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .top_left()
            .wh([20.0, 16.0])
            .label_font_size(style.icon_size(&ui.theme))
            .set(state.unexpose_button, ui)
        {
            ev = Some(Event::ConcealParameter)
        }

        widget::Text::new(&self.param.parameter.to_string())
            .parent(id)
            .font_size(style.text_size(&ui.theme))
            .right(8.0)
            .color(color::WHITE)
            .set(state.resource, ui);

        widget::Text::new(control_name(&self.param.control))
            .parent(id)
            .font_size(style.ctrl_size(&ui.theme))
            .top_right()
            .color(color::GRAY)
            .set(state.control, ui);

        widget::Text::new(&self.language.get_message("exposed-field"))
            .parent(id)
            .top_left_with_margin(32.0)
            .font_size(style.text_size(&ui.theme))
            .down(16.0)
            .color(color::WHITE)
            .set(state.field_label, ui);

        for event in widget::TextBox::new(&self.param.graph_field)
            .parent(id)
            .font_size(style.text_size(&ui.theme))
            .down(16.0)
            .padded_w_of(id, 16.0)
            .h(16.0)
            .set(state.field, ui)
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
            .parent(id)
            .font_size(style.text_size(&ui.theme))
            .down(16.0)
            .color(color::WHITE)
            .set(state.title_label, ui);

        for event in widget::TextBox::new(&self.param.title)
            .parent(id)
            .font_size(style.text_size(&ui.theme))
            .down(16.0)
            .padded_w_of(id, 16.0)
            .h(16.0)
            .set(state.title, ui)
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
        Control::XYPad { .. } => "xy",
        Control::RgbColor { .. } => "rgb",
        Control::Enum { .. } => "list",
        Control::File { .. } => "file",
        Control::ImageResource { .. } => "img",
        Control::Ramp { .. } => "ramp",
        Control::Toggle { .. } => "bool",
        Control::Entry { .. } => "text",
        Control::ChannelMap { .. } => "chn",
        Control::Size { .. } => "size",
    }
}
