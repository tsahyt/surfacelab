use crate::{lang::*, ui::i18n::Language};
use conrod_core::*;
use strum::IntoEnumIterator;

#[derive(WidgetCommon)]
pub struct ExportRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    spec: &'a mut ExportSpec,
    style: Style,
    resources: &'a [Resource<Node>],
    language: &'a Language,
}

impl<'a> ExportRow<'a> {
    pub fn new(
        spec: &'a mut ExportSpec,
        resources: &'a [Resource<Node>],
        language: &'a Language,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            resources,
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

pub enum Event {
    Updated,
    Renamed(String),
}

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
        let widget::UpdateArgs { state, id, ui, .. } = args;
        let mut ev = None;

        widget::Text::new(&format!("{} - {}", self.spec.name, self.spec.node))
            .font_size(10)
            .color(color::WHITE)
            .h(16.)
            .top_left()
            .parent(id)
            .set(state.ids.header_text, ui);

        for event in widget::TextBox::new(&self.spec.name)
            .font_size(10)
            .top_left_with_margins(24., 8.)
            .padded_w_of(id, 8.)
            .h(16.0)
            .parent(id)
            .set(state.ids.prefix_text, ui)
        {
            match event {
                widget::text_box::Event::Update(new) => {
                    let old_name = self.spec.name.clone();
                    self.spec.name = new;
                    ev = Some(Event::Renamed(old_name));
                }
                _ => {}
            }
        }

        let resource_names: Vec<_> = self.resources.iter().map(|x| x.to_string()).collect();
        let resource_idx = self.resources.iter().position(|r| r == &self.spec.node);

        if let Some(new) = widget::DropDownList::new(&resource_names, resource_idx)
            .label_font_size(10)
            .down(8.)
            .h(16.0)
            .parent(id)
            .set(state.ids.resource_selector, ui)
        {
            self.spec.node = self.resources[new].clone();
            ev = Some(Event::Updated)
        }

        // Color Spaces
        let legal_color_spaces: Vec<_> = ColorSpace::iter()
            .filter(|c| self.spec.color_space_legal(*c))
            .collect();
        let color_space_idx = legal_color_spaces
            .iter()
            .position(|c| *c == self.spec.color_space);
        let color_space_names: Vec<_> = legal_color_spaces
            .iter()
            .map(|v| self.language.get_message(&v.to_string()))
            .collect();
        if let Some(new) = widget::DropDownList::new(&color_space_names, color_space_idx)
            .label_font_size(10)
            .down(8.)
            .h(16.0)
            .parent(id)
            .set(state.ids.color_space_selector, ui)
        {
            self.spec.color_space = legal_color_spaces[new];
            self.spec.sanitize_for_color_space();
            ev = Some(Event::Updated)
        }

        // Export Formats, we allow everything here for choice and sanitize later
        let export_format_names: Vec<_> = ExportFormat::iter()
            .map(|v| self.language.get_message(&v.to_string()))
            .collect();
        if let Some(new) =
            widget::DropDownList::new(&export_format_names, Some(self.spec.format as usize))
                .label_font_size(10)
                .down(8.)
                .h(16.0)
                .padded_w_of(id, 40.)
                .parent(id)
                .set(state.ids.format_selector, ui)
        {
            self.spec.format = ExportFormat::iter().nth(new).unwrap();
            self.spec.sanitize_for_format();
            ev = Some(Event::Updated)
        }

        // Bit Depths
        let legal_bit_depths: Vec<u8> = [8, 16, 32]
            .iter()
            .copied()
            .filter(|b| self.spec.bit_depth_legal(*b))
            .collect();
        let bit_depth_idx = legal_bit_depths
            .iter()
            .position(|b| *b == self.spec.bit_depth);
        let bit_depth_names: Vec<_> = legal_bit_depths
            .iter()
            .map(|b| format!("{} bit", *b))
            .collect();

        if let Some(new) = widget::DropDownList::new(&bit_depth_names, bit_depth_idx)
            .label_font_size(10)
            .right(8.)
            .h(16.0)
            .w(56.0)
            .parent(id)
            .set(state.ids.bit_depth_selector, ui)
        {
            self.spec.bit_depth = legal_bit_depths[new];
            self.spec.sanitize_for_bit_depth();
            ev = Some(Event::Updated)
        }

        ev
    }
}
