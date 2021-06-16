use super::color_picker::ColorPicker;
use super::color_ramp::ColorRamp;
use super::resource_editor::ResourceEditor;
use super::size_control::SizeControl;

use crate::lang::{resource, *};
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
    pub svgs: usize,
    pub ramps: usize,
    pub toggles: usize,
    pub entries: usize,
    pub sizes: usize,
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
                Control::SvgResource { .. } => {
                    counts.svgs += 1;
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
                Control::Size { .. } => {
                    counts.sizes += 1;
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
    image_resources: &'a [(Resource<Img>, (ColorSpace, bool))],
    svg_resources: &'a [(Resource<resource::Svg>, bool)],
    type_variables: Option<&'a HashMap<TypeVariable, ImageType>>,
    parent_size: Option<u32>,
    presets: bool,
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
            svg_resources: &[],
            type_variables: None,
            parent_size: None,
            presets: false,
        }
    }

    fn resize_ids(&self, state: &mut widget::State<'_, State>, id_gen: &mut widget::id::Generator) {
        state.update(|state| {
            state.labels.resize(self.description.len(), id_gen);
            state.messages.resize(self.description.len(), id_gen);
            state.exposes.resize(self.description.len(), id_gen);
            state
                .categories
                .resize(self.description.categories(), id_gen);
            state
                .category_expanders
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
                .get_mut(&TypeId::of::<ResourceEditor<Img>>())
                .unwrap()
                .resize(counts.imgs, id_gen);
            state
                .controls
                .get_mut(&TypeId::of::<ResourceEditor<resource::Svg>>())
                .unwrap()
                .resize(counts.svgs, id_gen);
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
            state
                .controls
                .get_mut(&TypeId::of::<SizeControl>())
                .unwrap()
                .resize(counts.sizes, id_gen);
        })
    }

    fn needs_resize(&self, state: &State) -> bool {
        let counts = ControlCounts::from(&*self.description);

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
                .get(&TypeId::of::<ResourceEditor<Img>>())
                .unwrap()
                .len()
                < (counts.imgs)
            || state
                .controls
                .get(&TypeId::of::<ResourceEditor<resource::Svg>>())
                .unwrap()
                .len()
                < (counts.svgs)
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
            || state
                .controls
                .get(&TypeId::of::<SizeControl>())
                .unwrap()
                .len()
                < (counts.sizes)
    }

    builder_methods! {
        pub parent_size { parent_size = Some(u32) }
        pub image_resources { image_resources = &'a [(Resource<Img>, (ColorSpace, bool))] }
        pub svg_resources { svg_resources = &'a [(Resource<resource::Svg>, bool)] }
        pub type_variables { type_variables = Some(&'a HashMap<TypeVariable, ImageType>) }
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub text_size { style.text_size = Some(FontSize) }
        pub text_color { style.text_color = Some(Color) }
        pub presets { presets = bool }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
    #[conrod(default = "theme.font_size_small")]
    text_size: Option<FontSize>,
    #[conrod(default = "theme.label_color")]
    text_color: Option<Color>,
}

widget_ids! {
    pub struct Ids {
        preset_tools,
    }
}

pub struct State {
    ids: Ids,
    labels: widget::id::List,
    messages: widget::id::List,
    exposes: widget::id::List,
    controls: HashMap<TypeId, widget::id::List>,
    categories: widget::id::List,
    category_expanders: widget::id::List,
}

#[derive(Debug)]
pub enum Event {
    ChangeParameter(Lang),
    ExposeParameter(String, String, Control),
    ConcealParameter(String),
}

#[derive(Copy, Clone)]
enum PresetTool {
    Save,
    Load,
}

impl<'a, T> Widget for ParamBox<'a, T>
where
    T: MessageWriter,
{
    type State = State;
    type Style = Style;
    type Event = Vec<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            labels: widget::id::List::new(),
            messages: widget::id::List::new(),
            exposes: widget::id::List::new(),
            controls: hashmap! {
                TypeId::of::<widget::Slider<f32>>() => widget::id::List::new(),
                TypeId::of::<widget::DropDownList<String>>() => widget::id::List::new(),
                TypeId::of::<widget::XYPad<f32,f32>>() => widget::id::List::new(),
                TypeId::of::<ColorPicker<Hsv>>() => widget::id::List::new(),
                TypeId::of::<ColorRamp>() => widget::id::List::new(),
                TypeId::of::<widget::Button<widget::button::Flat>>() => widget::id::List::new(),
                TypeId::of::<ResourceEditor<Img>>() => widget::id::List::new(),
                TypeId::of::<ResourceEditor<resource::Svg>>() => widget::id::List::new(),
                TypeId::of::<widget::TextBox>() => widget::id::List::new(),
                TypeId::of::<widget::Toggle>() => widget::id::List::new(),
                TypeId::of::<SizeControl>() => widget::id::List::new(),
            },
            categories: widget::id::List::new(),
            category_expanders: widget::id::List::new(),
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
        let mut ev = Vec::new();

        // Ensure we have enough ids, allocate more if necessary by resizing the
        // lists. Resizing shouldn't be particularly expensive, but triggering
        // the necessary state.update also triggers a redraw, hence we first
        // check whether it is necessary or not.
        if self.needs_resize(state) {
            self.resize_ids(state, &mut ui.widget_id_generator());
        }

        if self.presets {
            match super::toolbar::Toolbar::flow_right(
                [
                    (IconName::UPLOAD, PresetTool::Save),
                    (IconName::DOWNLOAD, PresetTool::Load),
                ]
                .iter()
                .copied(),
            )
            .icon_font(style.icon_font(&ui.theme))
            .icon_color(color::WHITE)
            .button_color(color::DARK_CHARCOAL)
            .parent(id)
            .h(16.)
            .button_size(16.)
            .icon_size(10)
            .top_left_with_margins(16., 8.)
            .set(state.ids.preset_tools, ui)
            {
                Some(PresetTool::Save) => {
                    match FileSelection::new("Select preset file")
                        .title("Save Preset")
                        .mode(FileSelectionMode::Save)
                        .show()
                    {
                        Ok(Some(path)) => {
                            if let Err(e) = self.description.to_preset().write_to_file(path) {
                                log::error!("{}", e);
                            }
                        }
                        Err(e) => log::error!("Error during file selection {}", e),
                        _ => {}
                    }
                }
                Some(PresetTool::Load) => {
                    match FileSelection::new("Select preset file")
                        .title("Load Preset")
                        .mode(FileSelectionMode::Open)
                        .show()
                    {
                        Ok(Some(path)) => {
                            let preset_evs = ParameterPreset::load_from_file(path)
                                .and_then(|x| self.description.load_preset(self.resource, x));
                            match preset_evs {
                                Ok(mut evs) => {
                                    ev.extend(evs.drain(0..).map(|l| Event::ChangeParameter(l)));
                                }
                                Err(e) => {
                                    log::error!("{}", e);
                                }
                            }
                        }
                        Err(e) => log::error!("Error during file selection {}", e),
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Build widgets for each parameter
        let mut top_margin = 16.0;
        let mut label_count = 0;
        let mut message_count = 0;
        let mut control_idx = ControlCounts::default();

        let description = self.description;
        let language = self.language;
        let controls = description.controls();
        let ty_vars = self.type_variables;

        for (j, category) in description
            .categories
            .iter_mut()
            .enumerate()
            .filter(|(_, category)| category.visibility.run(&controls, ty_vars))
        {
            widget::Text::new(&self.language.get_message(category.name))
                .parent(id)
                .color(style.text_color(&ui.theme))
                .font_size(12)
                .mid_top_with_margin(top_margin)
                .set(state.categories[j], ui);

            for _click in icon_button(
                if category.is_open {
                    IconName::DOWN
                } else {
                    IconName::RIGHT
                },
                style.icon_font(&ui.theme),
            )
            .color(color::DARK_CHARCOAL)
            .label_font_size(12)
            .label_color(style.text_color(&ui.theme))
            .border(0.0)
            .w_h(16., 16.)
            .top_right_with_margins(top_margin, 8.)
            .parent(id)
            .set(state.category_expanders[j], ui)
            {
                category.is_open = !category.is_open;
            }

            top_margin += 16.0;

            if !category.is_open {
                top_margin += 16.;
                continue;
            }

            for parameter in category.parameters.iter_mut() {
                let label_id = state.labels[label_count];
                let message_id = state.messages[message_count];
                let expose_id = state.exposes[label_count];
                label_count += 1;
                message_count += 1;

                // Skip parameter if it's not visible under current conditions
                if !parameter.visibility.run(&controls, ty_vars) {
                    continue;
                }

                if let Some(expose_status) = parameter.expose_status {
                    for _press in icon_button(
                        match &expose_status {
                            ExposeStatus::Unexposed => IconName::EXPOSE,
                            ExposeStatus::Exposed => IconName::UNEXPOSE,
                        },
                        style.icon_font(&ui.theme),
                    )
                    .parent(id)
                    .border(0.)
                    .color(color::DARK_CHARCOAL)
                    .label_color(style.text_color(&ui.theme))
                    .top_right_with_margins(top_margin, 16.0)
                    .label_font_size(12)
                    .wh([20.0, 16.0])
                    .set(expose_id, ui)
                    {
                        if let Some(field) = parameter.transmitter.as_field().map(|x| x.0.clone()) {
                            ev.push(match &expose_status {
                                ExposeStatus::Unexposed => Event::ExposeParameter(
                                    field,
                                    parameter.name.clone(),
                                    parameter.control.clone(),
                                ),
                                ExposeStatus::Exposed => Event::ConcealParameter(field),
                            });
                        }
                    }
                }

                widget::Text::new(&self.language.get_message(&parameter.name))
                    .parent(id)
                    .color(style.text_color(&ui.theme))
                    .font_size(style.text_size(&ui.theme))
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
                            .label_font_size(style.text_size(&ui.theme))
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            if (new - *value).abs() > std::f32::EPSILON {
                                ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                    self.resource,
                                    &value.to_data(),
                                    &new.to_data(),
                                )));
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
                                .label_font_size(style.text_size(&ui.theme))
                                .padded_w_of(id, 16.0)
                                .h(16.0)
                                .set(control_id, ui)
                        {
                            let new = new as i32;
                            if new != *value {
                                ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                    self.resource,
                                    &value.to_data(),
                                    &new.to_data(),
                                )));
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
                                .label_color(style.text_color(&ui.theme))
                                .label_font_size(style.text_size(&ui.theme))
                                .value_font_size(style.text_size(&ui.theme))
                                .line_thickness(1.0)
                                .padded_w_of(id, 16.0)
                                .h(256.0)
                                .set(control_id, ui)
                        {
                            let new = [new_x, new_y];
                            ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                self.resource,
                                &value.to_data(),
                                &new.to_data(),
                            )));
                            *value = new;
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
                            ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                self.resource,
                                &value.to_data(),
                                &new.to_data(),
                            )));
                            *value = new;
                        }
                        top_margin += 256.0;
                        control_idx.rgb_colors += 1;
                    }
                    Control::Enum { selected, variants } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<widget::DropDownList<String>>())
                            .unwrap()[control_idx.enums];
                        let i18nd: Vec<_> =
                            variants.iter().map(|v| language.get_message(v)).collect();
                        if let Some(new_selection) =
                            widget::DropDownList::new(&i18nd, Some(*selected))
                                .label_font_size(style.text_size(&ui.theme))
                                .padded_w_of(id, 16.0)
                                .h(16.0)
                                .set(control_id, ui)
                        {
                            ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                self.resource,
                                &(*selected as u32).to_data(),
                                &(new_selection as u32).to_data(),
                            )));
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
                            .label_font_size(style.text_size(&ui.theme))
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
                                    let new = Some(std::path::PathBuf::from(&path));
                                    ev.push(Event::ChangeParameter(
                                        parameter.transmitter.transmit(
                                            self.resource,
                                            &selected.to_data(),
                                            &new.to_data(),
                                        ),
                                    ));
                                    *selected = new;
                                }
                                Err(e) => log::error!("Error during file selection {}", e),
                                _ => {}
                            }
                        }
                        control_idx.files += 1;
                    }
                    Control::ImageResource { selected } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<ResourceEditor<Img>>())
                            .unwrap()[control_idx.imgs];

                        if let Some(event) = ResourceEditor::new(
                            self.image_resources,
                            selected.clone(),
                            self.language,
                        )
                        .icon_font(style.icon_font(&ui.theme))
                        .text_size(style.text_size(&ui.theme))
                        .text_color(style.text_color(&ui.theme))
                        .padded_w_of(id, 16.0)
                        .h(40.0)
                        .set(control_id, ui)
                        {
                            use super::resource_editor;

                            match event {
                                resource_editor::Event::SelectResource(new_selected) => {
                                    let new = Some(new_selected.clone());
                                    ev.push(Event::ChangeParameter(
                                        parameter.transmitter.transmit(
                                            self.resource,
                                            &selected.to_data(),
                                            &new.to_data(),
                                        ),
                                    ));
                                    *selected = new;
                                }
                                resource_editor::Event::AddFromFile(path) => {
                                    ev.push(Event::ChangeParameter(Lang::UserIOEvent(
                                        UserIOEvent::AddImageResource(path),
                                    )))
                                }
                                resource_editor::Event::Pack => {
                                    if let Some(res) = selected {
                                        ev.push(Event::ChangeParameter(Lang::UserIOEvent(
                                            UserIOEvent::PackImage(res.clone()),
                                        )))
                                    }
                                }
                                resource_editor::Event::TypeEvent(
                                    resource_editor::ImgEvent::SetColorSpace(cs),
                                ) => {
                                    if let Some(res) = selected {
                                        ev.push(Event::ChangeParameter(Lang::UserIOEvent(
                                            UserIOEvent::SetImageColorSpace(res.clone(), cs),
                                        )))
                                    }
                                }
                            }
                        }

                        control_idx.imgs += 1;
                    }
                    Control::SvgResource { selected } => {
                        let control_id = state
                            .controls
                            .get(&TypeId::of::<ResourceEditor<resource::Svg>>())
                            .unwrap()[control_idx.imgs];

                        if let Some(event) =
                            ResourceEditor::new(self.svg_resources, selected.clone(), self.language)
                                .icon_font(style.icon_font(&ui.theme))
                                .text_size(style.text_size(&ui.theme))
                                .text_color(style.text_color(&ui.theme))
                                .padded_w_of(id, 16.0)
                                .h(40.0)
                                .set(control_id, ui)
                        {
                            use super::resource_editor;

                            match event {
                                resource_editor::Event::SelectResource(new_selected) => {
                                    let new = Some(new_selected.clone());
                                    ev.push(Event::ChangeParameter(
                                        parameter.transmitter.transmit(
                                            self.resource,
                                            &selected.to_data(),
                                            &new.to_data(),
                                        ),
                                    ));
                                    *selected = new;
                                }
                                resource_editor::Event::AddFromFile(path) => {
                                    ev.push(Event::ChangeParameter(Lang::UserIOEvent(
                                        UserIOEvent::AddSvgResource(path),
                                    )))
                                }
                                resource_editor::Event::Pack => {
                                    if let Some(res) = selected {
                                        ev.push(Event::ChangeParameter(Lang::UserIOEvent(
                                            UserIOEvent::PackSvg(res.clone()),
                                        )))
                                    }
                                }
                                resource_editor::Event::TypeEvent(..) => {
                                    unreachable!()
                                }
                            }
                        }

                        control_idx.svgs += 1;
                    }
                    Control::Ramp { steps } => {
                        let control_id = state.controls.get(&TypeId::of::<ColorRamp>()).unwrap()
                            [control_idx.ramps];
                        if let Some(event) = ColorRamp::new(steps)
                            .icon_font(style.icon_font(&ui.theme))
                            .padded_w_of(id, 16.0)
                            .h(256.0)
                            .set(control_id, ui)
                        {
                            use super::color_ramp;

                            let old = steps.to_data();
                            match event {
                                color_ramp::Event::ChangeStep(i, step) => {
                                    steps[i] = step;
                                }
                                color_ramp::Event::AddStep(i) => {
                                    use palette::Mix;
                                    if steps.len() < 2 {
                                        let position = 0.5;
                                        let color = palette::LinSrgb::new(
                                            steps[0][0],
                                            steps[0][1],
                                            steps[0][2],
                                        );
                                        steps.push([color.red, color.green, color.blue, position]);
                                    } else {
                                        let before_step = steps
                                            .iter()
                                            .filter(|s| s[3] < steps[i][3])
                                            .max_by(|a, b| {
                                                a[3].partial_cmp(&b[3])
                                                    .unwrap_or(std::cmp::Ordering::Equal)
                                            })
                                            .cloned()
                                            .unwrap_or([0., 0., 0., 0.]);
                                        let current_step = steps[i];

                                        let position = (before_step[3] + current_step[3]) / 2.0;

                                        let before_color = palette::LinSrgb::new(
                                            before_step[0],
                                            before_step[1],
                                            before_step[2],
                                        );
                                        let current_color = palette::LinSrgb::new(
                                            current_step[0],
                                            current_step[1],
                                            current_step[2],
                                        );
                                        let color = before_color.mix(&current_color, 0.5);
                                        steps.push([color.red, color.green, color.blue, position]);
                                    }
                                }
                                color_ramp::Event::DeleteStep(i) => {
                                    if steps.len() > 1 {
                                        steps.remove(i);
                                    }
                                }
                            }

                            ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                self.resource,
                                &old,
                                &steps.to_data(),
                            )))
                        }
                        top_margin += 256.0;
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
                            let old = *value;
                            *value = !*value;
                            ev.push(Event::ChangeParameter(parameter.transmitter.transmit(
                                self.resource,
                                &(if old { 1_u32 } else { 0_u32 }).to_data(),
                                &(if *value { 1_u32 } else { 0_u32 }).to_data(),
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
                            .font_size(style.text_size(&ui.theme))
                            .padded_w_of(id, 16.0)
                            .h(16.0)
                            .set(control_id, ui)
                        {
                            match event {
                                widget::text_box::Event::Update(new) => {
                                    ev.push(Event::ChangeParameter(
                                        parameter.transmitter.transmit(
                                            self.resource,
                                            &value.to_data(),
                                            &new.to_data(),
                                        ),
                                    ));
                                    *value = new;
                                }
                                _ => {}
                            }
                        }
                        control_idx.entries += 1;
                    }
                    Control::ChannelMap {
                        enabled,
                        chan,
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
                        let legal_sockets: Vec<_> = sockets
                            .iter()
                            .filter_map(|(x, ty)| if chan.legal_for(*ty) { Some(x) } else { None })
                            .collect();

                        if legal_sockets.is_empty() {
                            widget::Text::new(&self.language.get_message("layer-type-mismatch"))
                                .parent(id)
                                .color(style.text_color(&ui.theme).alpha(0.5))
                                .font_size(style.text_size(&ui.theme))
                                .set(message_id, ui);
                        } else {
                            for _press in widget::Toggle::new(*enabled)
                                .enabled(!legal_sockets.is_empty())
                                .w(16.0)
                                .h(16.0)
                                .set(toggle_id, ui)
                            {
                                let old = enabled.clone();
                                *enabled = !*enabled;
                                ev.push(Event::ChangeParameter(
                                    parameter.transmitter.transmit(
                                        self.resource,
                                        &((if old { 1_u32 } else { 0_u32 }), (*selected as u32))
                                            .to_data(),
                                        &(
                                            (if *enabled { 1_u32 } else { 0_u32 }),
                                            (*selected as u32),
                                        )
                                            .to_data(),
                                    ),
                                ));
                            }

                            if let Some(new_selection) = widget::DropDownList::new(
                                &legal_sockets,
                                if legal_sockets.is_empty() {
                                    None
                                } else {
                                    Some(*selected)
                                },
                            )
                            .enabled(!legal_sockets.is_empty())
                            .label_font_size(style.text_size(&ui.theme))
                            .right(8.0)
                            .padded_w_of(id, 32.0)
                            .h(16.0)
                            .set(enum_id, ui)
                            {
                                let old = selected.clone();
                                *selected = new_selection;
                                ev.push(Event::ChangeParameter(
                                    parameter.transmitter.transmit(
                                        self.resource,
                                        &((if *enabled { 1_u32 } else { 0_u32 }), (old as u32))
                                            .to_data(),
                                        &(
                                            (if *enabled { 1_u32 } else { 0_u32 }),
                                            (*selected as u32),
                                        )
                                            .to_data(),
                                    ),
                                ));
                            }

                            control_idx.toggles += 1;
                            control_idx.enums += 1;
                        }
                    }
                    Control::Size {
                        size,
                        allow_relative,
                    } => {
                        let control_id = state.controls.get(&TypeId::of::<SizeControl>()).unwrap()
                            [control_idx.sizes];
                        let mut ctrl = SizeControl::new(*size)
                            .text_size(style.text_size(&ui.theme))
                            .color(style.text_color(&ui.theme))
                            .allow_relative(*allow_relative)
                            .parent(id)
                            .padded_w_of(id, 16.0)
                            .h(16.0);

                        if let Some(ps) = self.parent_size {
                            ctrl = ctrl.parent_size(ps);
                        }

                        for event in ctrl.set(control_id, ui) {
                            use super::size_control;
                            let old = size.clone();
                            match event {
                                size_control::Event::ToAbsolute => {
                                    *size = OperatorSize::AbsoluteSize(1024);
                                    ev.push(Event::ChangeParameter(
                                        parameter.transmitter.transmit(
                                            self.resource,
                                            &old.to_data(),
                                            &size.to_data(),
                                        ),
                                    ));
                                }
                                size_control::Event::ToRelative => {
                                    *size = OperatorSize::RelativeToParent(0);
                                    ev.push(Event::ChangeParameter(
                                        parameter.transmitter.transmit(
                                            self.resource,
                                            &old.to_data(),
                                            &size.to_data(),
                                        ),
                                    ));
                                }
                                size_control::Event::NewSize(new) => {
                                    *size = new;
                                    ev.push(Event::ChangeParameter(
                                        parameter.transmitter.transmit(
                                            self.resource,
                                            &old.to_data(),
                                            &new.to_data(),
                                        ),
                                    ));
                                }
                            }
                        }
                        control_idx.sizes += 1;
                    }
                }

                top_margin += 64.0;
            }
        }

        ev
    }
}

/// Determine the height of a parameter box by scanning through its description,
/// given an optional set of type variables to take into account for visibility.
pub fn param_box_height<T: MessageWriter>(
    description: &ParamBoxDescription<T>,
    ty_vars: Option<&HashMap<TypeVariable, ImageType>>,
) -> f64 {
    let controls = description.controls();

    let mut h = 16.;

    for category in description
        .categories
        .iter()
        .filter(|category| category.visibility.run(&controls, ty_vars))
    {
        h += 16.;

        if !category.is_open {
            h += 16.;
            continue;
        }

        for parameter in category.parameters.iter() {
            if !parameter.visibility.run(&controls, ty_vars) {
                continue;
            }

            h += match &parameter.control {
                Control::XYPad { .. } => 256.0,
                Control::RgbColor { .. } => 256.0,
                Control::Ramp { .. } => 256.0,
                _ => 0.,
            };

            h += 64.;
        }
    }

    h
}
