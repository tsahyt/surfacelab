use super::color_picker::ColorPicker;
use super::color_ramp::ColorRamp;
use super::img_resource_editor::ImageResourceEditor;

use crate::lang::*;
use crate::ui::i18n::Language;
use crate::ui::util::*;

use conrod_core::*;
use dialog::{DialogBox, FileSelection, FileSelectionMode};
use maplit::hashmap;
use palette::{Hsv, LinSrgb};
use std::any::TypeId;
use std::collections::HashMap;

/// Struct for storing the number of controls used by some parameter box.
#[derive(Default, Copy, Clone, Debug)]
pub struct ControlCounts {
    pub sliders: usize,
    pub discrete_sliders: usize,
    pub xy_pads: usize,
    pub rgb_colors: usize,
    pub enums: usize,
    pub files: usize,
    pub imgs: usize,
    pub ramps: usize,
    pub toggles: usize,
    pub entries: usize,
}

/// Get control counts from a parameter box description
impl<T> From<&ParamBoxDescription<T>> for ControlCounts
where
    T: MessageWriter,
{
    fn from(pbox: &ParamBoxDescription<T>) -> Self {
        let mut counts = ControlCounts::default();

        for parameter in pbox
            .categories
            .iter()
            .map(|c| c.parameters.iter())
            .flatten()
        {
            match parameter.control {
                Control::Slider { .. } => {
                    counts.sliders += 1;
                }
                Control::DiscreteSlider { .. } => {
                    counts.discrete_sliders += 1;
                }
                Control::XYPad { .. } => {
                    counts.xy_pads += 1;
                }
                Control::RgbColor { .. } => {
                    counts.rgb_colors += 1;
                }
                Control::Enum { .. } => {
                    counts.enums += 1;
                }
                Control::File { .. } => {
                    counts.files += 1;
                }
                Control::ImageResource { .. } => {
                    counts.imgs += 1;
                }
                Control::Ramp { .. } => {
                    counts.ramps += 1;
                }
                Control::Toggle { .. } => {
                    counts.toggles += 1;
                }
                Control::Entry { .. } => {
                    counts.entries += 1;
                }
                Control::ChannelMap { .. } => {
                    counts.enums += 1;
                    counts.toggles += 1;
                }
            }
        }

        counts
    }
}

#[derive(WidgetCommon)]
pub struct ParamBox<'a, T: MessageWriter> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    resource: &'a T::Resource,
    style: Style,
    description: &'a mut ParamBoxDescription<T>,
    language: &'a Language,
    image_resources: &'a [Resource<Img>],
}

impl<'a, T: MessageWriter> ParamBox<'a, T> {
    pub fn new(
        description: &'a mut ParamBoxDescription<T>,
        resource: &'a T::Resource,
        language: &'a Language,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            description,
            resource,
            language,
            image_resources: &[],
        }
    }

    fn resize_ids(&self, state: &mut widget::State<'_, State>, id_gen: &mut widget::id::Generator) {
        state.update(|state| {
            state.labels.resize(self.description.len(), id_gen);
            state.exposes.resize(self.description.len(), id_gen);
            state
                .categories
                .resize(self.description.categories(), id_gen);

            let counts = ControlCounts::from(&*self.description);
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
                .get_mut(&TypeId::of::<widget::XYPad<f32, f32>>())
                .unwrap()
                .resize(counts.xy_pads, id_gen);
            state
                .controls
                .get_mut(&TypeId::of::<ColorPicker<Hsv>>())
                .unwrap()
                .resize(counts.rgb_colors, id_gen);
            state
                .controls
                .get_mut(&TypeId::of::<ColorRamp>())
                .unwrap()
                .resize(counts.ramps, id_gen);
            state
                .controls
                .get_mut(&TypeId::of::<widget::Button<widget::button::Flat>>())
                .unwrap()
                .resize(counts.files, id_gen);
            state
                .controls
                .get_mut(&TypeId::of::<ImageResourceEditor>())
                .unwrap()
                .resize(counts.imgs, id_gen);
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
        let counts = ControlCounts::from(&*self.description);

        state.labels.len() < self.description.len()
            || state.exposes.len() < self.description.len()
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
                .get(&TypeId::of::<widget::XYPad<f32, f32>>())
                .unwrap()
                .len()
                < (counts.xy_pads)
            || state
                .controls
                .get(&TypeId::of::<ColorPicker<Hsv>>())
                .unwrap()
                .len()
                < (counts.rgb_colors)
            || state
                .controls
                .get(&TypeId::of::<ColorRamp>())
                .unwrap()
                .len()
                < (counts.ramps)
            || state
                .controls
                .get(&TypeId::of::<widget::Button<widget::button::Flat>>())
                .unwrap()
                .len()
                < (counts.files)
            || state
                .controls
                .get(&TypeId::of::<ImageResourceEditor>())
                .unwrap()
                .len()
                < (counts.imgs)
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

    pub fn image_resources(mut self, image_resources: &'a [Resource<Img>]) -> Self {
        self.image_resources = image_resources;
        self
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

#[derive(Clone, Debug)]
pub struct State {
    labels: widget::id::List,
    exposes: widget::id::List,
    controls: HashMap<TypeId, widget::id::List>,
    categories: widget::id::List,
}

#[derive(Debug)]
pub enum Event {
    ChangeParameter(Lang),
    ExposeParameter(String, String, Control),
    ConcealParameter(String),
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
            exposes: widget::id::List::new(),
            controls: hashmap! {
                TypeId::of::<widget::Slider<f32>>() => widget::id::List::new(),
                TypeId::of::<widget::DropDownList<String>>() => widget::id::List::new(),
                TypeId::of::<widget::XYPad<f32,f32>>() => widget::id::List::new(),
                TypeId::of::<ColorPicker<Hsv>>() => widget::id::List::new(),
                TypeId::of::<ColorRamp>() => widget::id::List::new(),
                TypeId::of::<widget::Button<widget::button::Flat>>() => widget::id::List::new(),
                TypeId::of::<ImageResourceEditor>() => widget::id::List::new(),
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
        let mut label_count = 0;
        let mut control_idx = ControlCounts::default();

        let controls = self.description.controls();

        for (j, category) in self.description.categories.iter_mut().enumerate() {
            widget::Text::new(&self.language.get_message(category.name))
                .parent(id)
                .color(color::WHITE)
                .font_size(12)
                .mid_top_with_margin(top_margin)
                .set(state.categories[j], ui);

            top_margin += 16.0;

            for parameter in category.parameters.iter_mut() {
                let label_id = state.labels[label_count];
                let expose_id = state.exposes[label_count];
                label_count += 1;

                // Skip parameter if it's not visible under current conditions
                if !parameter.visibility.run(&controls) {
                    continue;
                }

                if let Some(expose_status) = parameter.expose_status {
                    for _press in icon_button(
                        match &expose_status {
                            ExposeStatus::Unexposed => IconName::EXPOSE,
                            ExposeStatus::Exposed => IconName::UNEXPOSE,
                        },
                        self.style.icon_font.unwrap().unwrap(),
                    )
                    .parent(id)
                    .border(0.)
                    .color(color::DARK_CHARCOAL)
                    .label_color(color::WHITE)
                    .top_right_with_margins(top_margin, 16.0)
                    .label_font_size(12)
                    .wh([20.0, 16.0])
                    .set(expose_id, ui)
                    {
                        if let Some(field) = parameter.transmitter.as_field().map(|x| x.0.clone()) {
                            ev.push(Event::ExposeParameter(
                                field,
                                parameter.name.clone(),
                                parameter.control.clone(),
                            ));
                        }
                    }
                }

                widget::Text::new(&self.language.get_message(&parameter.name))
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
                        if let Some(new) = widget::Slider::new(*value, *min, *max)
                            .label(&format!("{:.2}", *value))
                            .label_font_size(10)
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            if (new - *value).abs() > std::f32::EPSILON {
                                ev.push(Event::ChangeParameter(
                                    parameter
                                        .transmitter
                                        .transmit(self.resource, &new.to_data()),
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
                        if let Some(new) =
                            widget::Slider::new(*value as f32, *min as f32, *max as f32)
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
                                        .transmit(self.resource, &new.to_data()),
                                ));
                                *value = new as i32;
                            }
                        }
                        control_idx.discrete_sliders += 1;
                    }
                    Control::XYPad { value, min, max } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<widget::XYPad<f32, f32>>())
                            .unwrap()[control_idx.xy_pads];
                        if let Some((new_x, new_y)) =
                            widget::XYPad::new(value[0], min[0], max[0], value[1], min[1], max[1])
                                .color(color::DARK_CHARCOAL)
                                .label_color(color::WHITE)
                                .label_font_size(10)
                                .value_font_size(10)
                                .line_thickness(1.0)
                                .padded_w_of(id, 16.0)
                                .h(256.0)
                                .set(control_id, ui)
                        {
                            *value = [new_x, new_y];
                            ev.push(Event::ChangeParameter(
                                parameter
                                    .transmitter
                                    .transmit(self.resource, &value.to_data()),
                            ));
                        }
                        top_margin += 256.0;
                        control_idx.xy_pads += 1;
                    }
                    Control::RgbColor { value } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<ColorPicker<Hsv>>())
                            .unwrap()[control_idx.rgb_colors];
                        if let Some(new_color) =
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
                                    .transmit(self.resource, &new.to_data()),
                            ));
                        }
                        control_idx.rgb_colors += 1;
                    }
                    Control::Enum { selected, variants } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<widget::DropDownList<String>>())
                            .unwrap()[control_idx.enums];
                        if let Some(new_selection) =
                            widget::DropDownList::new(variants, Some(*selected))
                                .label_font_size(10)
                                .padded_w_of(id, 16.0)
                                .h(16.0)
                                .set(control_id, ui)
                        {
                            ev.push(Event::ChangeParameter(
                                parameter
                                    .transmitter
                                    .transmit(self.resource, &(new_selection as u32).to_data()),
                            ));
                            *selected = new_selection;
                        }
                        control_idx.enums += 1;
                    }
                    Control::File { selected } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<widget::Button<widget::button::Flat>>())
                            .unwrap()[control_idx.files];
                        let btn_text = match selected {
                            Some(file) => file.file_name().unwrap().to_str().unwrap(),
                            None => "None",
                        };
                        for _click in widget::Button::new()
                            .label(btn_text)
                            .label_font_size(10)
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            match FileSelection::new("Select image file")
                                .title("Open Image")
                                .mode(FileSelectionMode::Open)
                                .show()
                            {
                                Ok(Some(path)) => {
                                    *selected = Some(std::path::PathBuf::from(&path));
                                    ev.push(Event::ChangeParameter(
                                        parameter
                                            .transmitter
                                            .transmit(self.resource, path.as_bytes()),
                                    ));
                                }
                                Err(e) => log::error!("Error during file selection {}", e),
                                _ => {}
                            }

                            if let Some(file) = selected {
                                let buf = file.to_str().unwrap().as_bytes().to_vec();
                                ev.push(Event::ChangeParameter(
                                    parameter.transmitter.transmit(self.resource, &buf),
                                ));
                            }
                        }
                        control_idx.files += 1;
                    }
                    Control::ImageResource { selected } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<ImageResourceEditor>())
                            .unwrap()[control_idx.imgs];

                        if let Some(event) =
                            ImageResourceEditor::new(self.image_resources, selected.clone())
                                .padded_w_of(id, 16.0)
                                .h(16.0)
                                .set(control_id, ui)
                        {
                            use super::img_resource_editor;

                            match event {
                                img_resource_editor::Event::SelectResource(new_selected) => {
                                    *selected = Some(new_selected.clone());
                                    ev.push(Event::ChangeParameter(
                                        parameter
                                            .transmitter
                                            .transmit(self.resource, &new_selected.to_data()),
                                    ))
                                }
                                _ => {}
                            }
                        }

                        control_idx.imgs += 1;
                    }
                    Control::Ramp { steps } => {
                        let control_id = state.controls.get(&TypeId::of::<ColorRamp>()).unwrap()
                            [control_idx.ramps];
                        if let Some(event) = ColorRamp::new(steps)
                            .padded_w_of(id, 16.0)
                            .h(256.0)
                            .set(control_id, ui)
                        {
                            use super::color_ramp;
                            match event {
                                color_ramp::Event::ChangeStep(i, step) => {
                                    steps[i] = step;
                                }
                                color_ramp::Event::AddStep => {
                                    use palette::Mix;
                                    let position = (steps[0][3] + steps[1][3]) / 2.0;
                                    let before = palette::LinSrgb::new(
                                        steps[0][0],
                                        steps[0][1],
                                        steps[0][2],
                                    );
                                    let after = palette::LinSrgb::new(
                                        steps[1][0],
                                        steps[1][1],
                                        steps[1][2],
                                    );
                                    let color = before.mix(&after, 0.5);
                                    steps.insert(1, [color.red, color.green, color.blue, position]);
                                }
                                color_ramp::Event::DeleteStep(i) => {
                                    if steps.len() > 1 {
                                        steps.remove(i);
                                    }
                                }
                            }

                            let mut buf = Vec::new();
                            for step in steps.iter() {
                                buf.extend_from_slice(&step[0].to_ne_bytes());
                                buf.extend_from_slice(&step[1].to_ne_bytes());
                                buf.extend_from_slice(&step[2].to_ne_bytes());
                                buf.extend_from_slice(&step[3].to_ne_bytes());
                            }

                            ev.push(Event::ChangeParameter(
                                parameter.transmitter.transmit(self.resource, &buf),
                            ))
                        }
                        control_idx.ramps += 1;
                    }
                    Control::Toggle { def: value } => {
                        let control_id =
                            state.controls.get(&TypeId::of::<widget::Toggle>()).unwrap()
                                [control_idx.toggles];
                        for _press in widget::Toggle::new(*value)
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            *value = !*value;
                            ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                self.resource,
                                &(if *value { 1 as u32 } else { 0 as u32 }).to_data(),
                            )));
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
                                        parameter
                                            .transmitter
                                            .transmit(self.resource, &value.as_bytes().to_vec()),
                                    ));
                                }
                            }
                        }
                        control_idx.entries += 1;
                    }
                    Control::ChannelMap {
                        enabled,
                        selected,
                        sockets,
                    } => {
                        let toggle_id =
                            state.controls.get(&TypeId::of::<widget::Toggle>()).unwrap()
                                [control_idx.toggles];
                        let enum_id = state
                            .controls
                            .get(&TypeId::of::<widget::DropDownList<String>>())
                            .unwrap()[control_idx.enums];

                        for _press in widget::Toggle::new(*enabled)
                            .w(16.0)
                            .h(16.0)
                            .set(toggle_id, ui)
                        {
                            *enabled = !*enabled;
                            ev.push(Event::ChangeParameter(
                                parameter.transmitter.transmit(
                                    self.resource,
                                    &(
                                        (if *enabled { 1 as u32 } else { 0 as u32 }),
                                        (*selected as u32),
                                    )
                                        .to_data(),
                                ),
                            ));
                        }

                        if let Some(new_selection) =
                            widget::DropDownList::new(sockets, Some(*selected))
                                .label_font_size(10)
                                .right(8.0)
                                .padded_w_of(id, 32.0)
                                .h(16.0)
                                .set(enum_id, ui)
                        {
                            ev.push(Event::ChangeParameter(
                                parameter.transmitter.transmit(
                                    self.resource,
                                    &(
                                        (if *enabled { 1 as u32 } else { 0 as u32 }),
                                        (*selected as u32),
                                    )
                                        .to_data(),
                                ),
                            ));
                            *selected = new_selection;
                        }

                        control_idx.toggles += 1;
                        control_idx.enums += 1;
                    }
                }

                top_margin += 64.0;
            }
        }

        ev
    }
}
