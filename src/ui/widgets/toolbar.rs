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
pub struct Toolbar<'a, T, D> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    tools: &'a [(IconName, T)],
    style: Style,
    direction: std::marker::PhantomData<D>,
}

impl<'a, T> Toolbar<'a, T, FlowRight> {
    /// Construct a toolbar which grows towards the right.
    pub fn flow_right(tools: &'a [(IconName, T)]) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            tools,
            direction: std::marker::PhantomData,
        }
    }
}

impl<'a, T> Toolbar<'a, T, FlowLeft> {
    /// Construct a toolbar which grows towards the left.
    pub fn flow_left(tools: &'a [(IconName, T)]) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            tools,
            direction: std::marker::PhantomData,
        }
    }
}

impl<'a, T, D> Toolbar<'a, T, D> {
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
        buttons[]
    }
}

pub struct State {
    ids: Ids,
}

impl<'a, T, D> Widget for Toolbar<'a, T, D>
where
    D: Direction,
{
    type State = State;
    type Style = Style;
    type Event = Option<&'a T>;

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

        let widget::UpdateArgs { state, ui, id, .. } = args;

        state.update(|state| {
            let mut walker = state.ids.buttons.walk();

            let mut offset = 8.0;

            for (tool, answer) in self.tools {
                let mut id_gen = ui.widget_id_generator();
                let button_id = walker.next(&mut state.ids.buttons, &mut id_gen);

                let btn = icon_button(*tool, self.style.icon_font.unwrap().unwrap())
                    .label_font_size(14)
                    .label_color(color::WHITE)
                    .color(color::DARK_CHARCOAL)
                    .border(0.0)
                    .wh([32., 32.0])
                    .parent(id);

                for _press in D::position_button(btn, offset).set(button_id, ui) {
                    res = Some(answer)
                }

                offset += 40.0;
            }
        });

        res
    }
}
