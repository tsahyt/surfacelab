use crate::ui::util::*;
use conrod_core::*;

pub struct FlowRight;

pub struct FlowLeft;

pub trait Direction {
    fn position_button(
        btn: widget::Button<widget::button::Flat>,
        offset: f64,
    ) -> widget::Button<widget::button::Flat>;
}

impl Direction for FlowRight {
    fn position_button(
        btn: widget::Button<widget::button::Flat>,
        offset: f64,
    ) -> widget::Button<widget::button::Flat> {
        btn.mid_left_with_margin(offset)
    }
}

impl Direction for FlowLeft {
    fn position_button(
        btn: widget::Button<widget::button::Flat>,
        offset: f64,
    ) -> widget::Button<widget::button::Flat> {
        btn.mid_right_with_margin(offset)
    }
}

#[derive(WidgetCommon)]
pub struct Toolbar<T, D, I>
where
    I: Iterator<Item = (IconName, T)>,
{
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    tools: I,
    style: Style,
    direction: std::marker::PhantomData<D>,
    auto_hide: bool,
}

impl<T, I> Toolbar<T, FlowRight, I>
where
    I: Iterator<Item = (IconName, T)>,
{
    /// Construct a toolbar which grows towards the right.
    pub fn flow_right(tools: I) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            tools,
            direction: std::marker::PhantomData,
            auto_hide: false,
        }
    }
}

impl<T, I> Toolbar<T, FlowLeft, I>
where
    I: Iterator<Item = (IconName, T)>,
{
    /// Construct a toolbar which grows towards the left.
    pub fn flow_left(tools: I) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            tools,
            direction: std::marker::PhantomData,
            auto_hide: false,
        }
    }
}

impl<T, D, I> Toolbar<T, D, I>
where
    I: Iterator<Item = (IconName, T)>,
{
    builder_methods! {
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub button_size { style.button_size = Some(Scalar) }
        pub icon_size { style.icon_size = Some(FontSize) }
        pub icon_color { style.icon_color = Some(Color) }
        pub button_color { style.button_color = Some(Color) }
        pub border { style.border = Some(Scalar) }
        pub auto_hide { auto_hide = bool }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
    #[conrod(default = "32.0")]
    button_size: Option<Scalar>,
    #[conrod(default = "14")]
    icon_size: Option<FontSize>,
    #[conrod(default = "theme.label_color")]
    icon_color: Option<Color>,
    #[conrod(default = "theme.shape_color")]
    button_color: Option<Color>,
    #[conrod(default = "0.0")]
    border: Option<Scalar>,
}

widget_ids! {
    pub struct Ids {
        buttons[]
    }
}

pub struct State {
    ids: Ids,
}

impl<T, D, I> Widget for Toolbar<T, D, I>
where
    D: Direction,
    T: Copy,
    I: Iterator<Item = (IconName, T)>,
{
    type State = State;
    type Style = Style;
    type Event = Option<T>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut res = None;

        let widget::UpdateArgs {
            state,
            ui,
            id,
            style,
            rect,
            ..
        } = args;

        let size = style.button_size(&ui.theme);
        let icon_size = style.icon_size(&ui.theme);

        if self.auto_hide && !rect.is_over(ui.global_input().start.mouse.xy) {
            return res;
        }

        state.update(|state| {
            let mut walker = state.ids.buttons.walk();
            let mut offset = 8.0;

            for (tool, answer) in self.tools {
                let mut id_gen = ui.widget_id_generator();
                let button_id = walker.next(&mut state.ids.buttons, &mut id_gen);

                let btn = icon_button(tool, style.icon_font(&ui.theme))
                    .label_font_size(icon_size)
                    .label_color(style.icon_color(&ui.theme))
                    .color(style.button_color(&ui.theme))
                    .border(style.border(&ui.theme))
                    .wh([size, size])
                    .parent(id);

                for _press in D::position_button(btn, offset).set(button_id, ui) {
                    res = Some(answer)
                }

                offset += 8.0 + size;
            }
        });

        res
    }
}
