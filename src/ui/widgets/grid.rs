use crate::lang::OperatorSize;
use conrod_core::*;

const DEFAULT_RESOLUTION: f64 = 16.0;

#[derive(WidgetCommon)]
pub struct Grid {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    resolution: f64,
}

impl Grid {
    pub fn new() -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            resolution: DEFAULT_RESOLUTION,
        }
    }

    builder_methods! {
        pub resolution { resolution = f64 }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

pub struct State {
    area: Rect,
    resolution: f64,
}

impl Widget for Grid {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            area: Rect::from_corners([0., 0.], [100., 100.]),
            resolution: DEFAULT_RESOLUTION,
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {}
}
