use super::colorpicker::ColorPicker;
use crate::lang::*;
use conrod_core::*;
use maplit::hashmap;
use palette::{Hsv, LinSrgb};
use std::any::TypeId;
use std::collections::HashMap;

#[derive(Debug, WidgetCommon)]
pub struct ParamBox<'a, T: MessageWriter> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    resource: &'a Resource,
    style: Style,
    description: &'a mut ParamBoxDescription<T>,
}

impl<'a, T: MessageWriter> ParamBox<'a, T> {
    pub fn new(description: &'a mut ParamBoxDescription<T>, resource: &'a Resource) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            description,
            resource,
        }
    }

    fn resize_ids(&self, state: &mut widget::State<'_, State>, id_gen: &mut widget::id::Generator) {
        state.update(|state| {
            state.labels.resize(self.description.len(), id_gen);
            state
                .categories
                .resize(self.description.categories(), id_gen);

            let counts = self.description.control_counts();
            state
                .controls
                .get_mut(&TypeId::of::<widget::Slider<f32>>())
                .unwrap()
                .resize(counts.sliders + counts.discrete_sliders, id_gen);
            state
                .controls
                .get_mut(&TypeId::of::<widget::DropDownList<String>>())
                .unwrap()
                .resize(counts.enums, id_gen);
            state
                .controls
                .get_mut(&TypeId::of::<ColorPicker<Hsv>>())
                .unwrap()
                .resize(counts.rgb_colors, id_gen);
            state
                .controls
                .get_mut(&TypeId::of::<widget::TextBox>())
                .unwrap()
                .resize(counts.entries, id_gen);
            state
                .controls
                .get_mut(&TypeId::of::<widget::Toggle>())
                .unwrap()
                .resize(counts.toggles, id_gen);
        })
    }

    fn needs_resize(&self, state: &State) -> bool {
        let counts = self.description.control_counts();

        state.labels.len() < self.description.len()
            || state.categories.len() < self.description.categories()
            || state
                .controls
                .get(&TypeId::of::<widget::Slider<f32>>())
                .unwrap()
                .len()
                < (counts.sliders + counts.discrete_sliders)
            || state
                .controls
                .get(&TypeId::of::<widget::DropDownList<String>>())
                .unwrap()
                .len()
                < (counts.enums)
            || state
                .controls
                .get(&TypeId::of::<ColorPicker<Hsv>>())
                .unwrap()
                .len()
                < (counts.rgb_colors)
            || state
                .controls
                .get(&TypeId::of::<widget::TextBox>())
                .unwrap()
                .len()
                < (counts.entries)
            || state
                .controls
                .get(&TypeId::of::<widget::Toggle>())
                .unwrap()
                .len()
                < (counts.toggles)
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

#[derive(Clone, Debug)]
pub struct State {
    labels: widget::id::List,
    controls: HashMap<TypeId, widget::id::List>,
    categories: widget::id::List,
}

#[derive(Debug)]
pub enum Event {
    ChangeParameter(Lang),
}

impl<'a, T> Widget for ParamBox<'a, T>
where
    T: MessageWriter,
{
    type State = State;
    type Style = Style;
    type Event = Vec<Event>;

    fn init_state(&self, _id_gen: widget::id::Generator) -> Self::State {
        State {
            labels: widget::id::List::new(),
            controls: hashmap! {
                TypeId::of::<widget::Slider<f32>>() => widget::id::List::new(),
                TypeId::of::<widget::DropDownList<String>>() => widget::id::List::new(),
                TypeId::of::<ColorPicker<Hsv>>() => widget::id::List::new(),
                TypeId::of::<widget::TextBox>() => widget::id::List::new(),
                TypeId::of::<widget::Toggle>() => widget::id::List::new(),
            },
            categories: widget::id::List::new(),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs { state, ui, id, .. } = args;
        let mut ev = Vec::new();

        // Ensure we have enough ids, allocate more if necessary by resizing the
        // lists. Resizing shouldn't be particularly expensive, but triggering
        // the necessary state.update also triggers a redraw, hence we first
        // check whether it is necessary or not.
        if self.needs_resize(state) {
            self.resize_ids(state, &mut ui.widget_id_generator());
        }

        // Build widgets for each parameter
        let mut top_margin = 16.0;
        let mut control_idx = ControlCounts::default();
        for (j, category) in self.description.categories.iter_mut().enumerate() {
            widget::Text::new(&category.name)
                .parent(id)
                .color(color::WHITE)
                .font_size(12)
                .mid_top_with_margin(top_margin)
                .set(state.categories[j], ui);

            top_margin += 16.0;

            for (i, parameter) in category
                .parameters
                .iter_mut()
                .filter(|p| p.available)
                .enumerate()
            {
                let label_id = state.labels[i + j];
                widget::Text::new(&parameter.name)
                    .parent(id)
                    .color(color::WHITE)
                    .font_size(10)
                    .top_left_with_margins(top_margin, 16.0)
                    .set(label_id, ui);

                match &mut parameter.control {
                    Control::Slider { value, min, max } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<widget::Slider<f32>>())
                            .unwrap()[control_idx.sliders + control_idx.discrete_sliders];
                        for new in widget::Slider::new(*value, *min, *max)
                            .label(&format!("{:.1}", *value))
                            .label_font_size(10)
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            if new != *value {
                                ev.push(Event::ChangeParameter(
                                    parameter
                                        .transmitter
                                        .transmit(self.resource.clone(), &new.to_data()),
                                ));
                                *value = new;
                            }
                        }
                        control_idx.sliders += 1;
                    }
                    Control::DiscreteSlider { value, min, max } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<widget::Slider<f32>>())
                            .unwrap()[control_idx.sliders + control_idx.discrete_sliders];
                        for new in widget::Slider::new(*value as f32, *min as f32, *max as f32)
                            .label(&format!("{}", *value))
                            .label_font_size(10)
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            let new = new as i32;
                            if new != *value {
                                ev.push(Event::ChangeParameter(
                                    parameter
                                        .transmitter
                                        .transmit(self.resource.clone(), &new.to_data()),
                                ));
                                *value = new as i32;
                            }
                        }
                        control_idx.discrete_sliders += 1;
                    }
                    Control::RgbColor { value } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<ColorPicker<Hsv>>())
                            .unwrap()[control_idx.rgb_colors];
                        for new_color in
                            ColorPicker::new(Hsv::from(LinSrgb::new(value[0], value[1], value[2])))
                                .padded_w_of(id, 16.0)
                                .h(256.0)
                                .set(control_id, ui)
                        {
                            let rgb = LinSrgb::from(new_color);
                            let new = [rgb.red, rgb.green, rgb.blue];
                            *value = new;
                            ev.push(Event::ChangeParameter(
                                parameter
                                    .transmitter
                                    .transmit(self.resource.clone(), &new.to_data()),
                            ));
                        }
                        control_idx.rgb_colors += 1;
                    }
                    Control::Enum { selected, variants } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<widget::DropDownList<String>>())
                            .unwrap()[control_idx.enums];
                        for new_selection in widget::DropDownList::new(variants, Some(*selected))
                            .label_font_size(10)
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                self.resource.clone(),
                                &(new_selection as u32).to_data(),
                            )));
                            *selected = new_selection;
                        }
                        control_idx.enums += 1;
                    }
                    Control::File { .. } => {}
                    Control::Ramp { .. } => {}
                    Control::Toggle { def: value } => {
                        let control_id =
                            state.controls.get(&TypeId::of::<widget::Toggle>()).unwrap()
                                [control_idx.toggles];
                        for _press in widget::Toggle::new(*value)
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                self.resource.clone(),
                                &(if *value { 1 as u32 } else { 0 as u32 }).to_data(),
                            )));
                            *value = !*value
                        }
                        control_idx.toggles += 1;
                    }
                    Control::Entry { value } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<widget::TextBox>())
                            .unwrap()[control_idx.entries];
                        for event in widget::TextBox::new(value)
                            .font_size(10)
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            match event {
                                widget::text_box::Event::Update(new) => *value = new,
                                widget::text_box::Event::Enter => {
                                    ev.push(Event::ChangeParameter(
                                        parameter.transmitter.transmit(
                                            self.resource.clone(),
                                            &value.as_bytes().to_vec(),
                                        ),
                                    ));
                                }
                            }
                        }
                        control_idx.entries += 1;
                    }
                }

                top_margin += 64.0;
            }
        }

        ev
    }
}
