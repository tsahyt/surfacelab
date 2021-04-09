use crate::{lang::*, ui::i18n::Language};
use conrod_core::*;
use strum::VariantNames;

#[derive(WidgetCommon)]
pub struct ExportRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    spec: &'a mut ExportSpec,
    style: Style,
    language: &'a Language,
}

impl<'a> ExportRow<'a> {
    pub fn new(spec: &'a mut ExportSpec, language: &'a Language) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            spec,
            language,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        header_text,
        prefix_text,
        resource_selector,
        color_space_selector,
        bit_depth_selector,
        format_selector,
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
        let widget::UpdateArgs {
            state,
            id,
            ui,
            style,
            ..
        } = args;

        widget::Text::new(&format!("{} - {}", self.spec.prefix, self.spec.node))
            .font_size(10)
            .color(color::WHITE)
            .top_left()
            .parent(id)
            .set(state.ids.header_text, ui);

        for ev in widget::TextBox::new(&self.spec.prefix)
            .font_size(10)
            .down(8.)
            .w_of(id)
            .h(16.0)
            .parent(id)
            .set(state.ids.prefix_text, ui)
        {
            match ev {
                widget::text_box::Event::Update(new) => {
                    self.spec.prefix = new;
                }
                _ => {}
            }
        }

        widget::DropDownList::new(&["node:base/output.1"], Some(0))
            .label_font_size(10)
            .down(8.)
            .h(16.0)
            .parent(id)
            .set(state.ids.resource_selector, ui);

        let color_spaces: Vec<_> = ColorSpace::VARIANTS
            .iter()
            .map(|v| self.language.get_message(v))
            .collect();
        widget::DropDownList::new(&color_spaces, Some(0))
            .label_font_size(10)
            .down(8.)
            .h(16.0)
            .parent(id)
            .set(state.ids.color_space_selector, ui);

        let export_formats: Vec<_> = ExportFormat::VARIANTS
            .iter()
            .map(|v| self.language.get_message(v))
            .collect();
        widget::DropDownList::new(&export_formats, Some(0))
            .label_font_size(10)
            .down(8.)
            .h(16.0)
            .padded_w_of(id, 32.)
            .parent(id)
            .set(state.ids.format_selector, ui);

        if let Some(new) = widget::DropDownList::new(&["8", "16", "32"], Some(0))
            .label_font_size(10)
            .right(8.)
            .h(16.0)
            .w(56.0)
            .parent(id)
            .set(state.ids.bit_depth_selector, ui)
        {}

        None
    }
}
