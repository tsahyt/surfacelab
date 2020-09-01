use crate::lang::parameters::*;
use conrod_core::*;

#[derive(Copy, Clone, Debug, WidgetCommon)]
pub struct ParamBox<'a, T: MessageWriter> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    description: &'a ParamBoxDescription<T>,
}

impl<'a, T: MessageWriter> ParamBox<'a, T> {
    pub fn new(description: &'a ParamBoxDescription<T>) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            description,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

#[derive(Clone, Debug)]
pub struct State {
    labels: widget::id::List,
    controls: widget::id::List,
    categories: widget::id::List,
}

impl<'a, T> Widget for ParamBox<'a, T>
where
    T: MessageWriter,
{
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            labels: widget::id::List::new(),
            controls: widget::id::List::new(),
            categories: widget::id::List::new(),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs { state, ui, id, .. } = args;

        // Ensure we have enough ids, allocate more if necessary by resizing the lists
        {
            let c = self.description.categories.len();
            let n = self
                .description
                .categories
                .iter()
                .map(|x| x.parameters.len())
                .sum::<usize>();
            state.update(|state| {
                state.labels.resize(n, &mut ui.widget_id_generator());
                state.controls.resize(n, &mut ui.widget_id_generator());
                state.categories.resize(c, &mut ui.widget_id_generator());
            })
        }

        // Build widgets for each parameter
        let mut top_margin = 16.0;
        for (j, category) in self.description.categories.iter().enumerate() {
            widget::Text::new(&category.name)
                .parent(id)
                .color(color::WHITE)
                .font_size(12)
                .mid_top_with_margin(top_margin)
                .set(state.categories[j], ui);

            top_margin += 16.0;

            for (i, parameter) in category
                .parameters
                .iter()
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

                let control_id = state.controls[i + j];
                match &parameter.control {
                    Control::Slider { value, min, max } => {
                        build_slider(control_id, id, ui, *value, *min, *max)
                    }
                    Control::DiscreteSlider { value, min, max } => {
                        build_discrete_slider(control_id, id, ui, *value, *min, *max)
                    }
                    Control::RgbColor { value } => build_rgb_color(*value),
                    Control::RgbaColor { value } => build_rgba_color(*value),
                    Control::Enum { selected, variants } => {
                        build_enum(control_id, id, ui, *selected, variants)
                    }
                    Control::File { selected } => todo!(),
                    Control::Ramp { steps } => build_ramp(steps),
                    Control::Toggle { def } => build_toggle(*def),
                    Control::Entry { value } => build_entry(value),
                };

                top_margin += 64.0;
            }
        }
    }
}

fn build_slider(
    id: widget::Id,
    parent: widget::Id,
    ui: &mut UiCell,
    value: f32,
    min: f32,
    max: f32,
) {
    widget::Slider::new(value, min, max)
        .label(&format!("{:.1}", value))
        .label_font_size(10)
        .padded_w_of(parent, 16.0)
        .h(16.0)
        .set(id, ui);
}

fn build_discrete_slider(
    id: widget::Id,
    parent: widget::Id,
    ui: &mut UiCell,
    value: i32,
    min: i32,
    max: i32,
) {
    widget::Slider::new(value as f32, min as f32, max as f32)
        .label(&format!("{}", value))
        .label_font_size(10)
        .padded_w_of(parent, 16.0)
        .h(16.0)
        .set(id, ui);
}

fn build_rgb_color(_value: [f32; 3]) {}

fn build_rgba_color(_value: [f32; 4]) {}

fn build_enum(
    id: widget::Id,
    parent: widget::Id,
    ui: &mut UiCell,
    selected: usize,
    variants: &[String],
) {
    widget::DropDownList::new(variants, Some(selected))
        .label_font_size(10)
        .padded_w_of(parent, 16.0)
        .h(16.0)
        .set(id, ui);
}

fn build_ramp(_steps: &[[f32; 4]]) {}

fn build_toggle(_def: bool) {}

fn build_entry(_text: &str) {}
