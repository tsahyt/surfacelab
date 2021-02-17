use conrod_core::*;

#[derive(Copy, Clone, Debug, WidgetCommon)]
pub struct TransformEditor {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
}

impl TransformEditor {
    pub fn new() -> Self {
        Self {
            common: widget::CommonBuilder::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {}

widget_ids! {
    #[derive(Debug)]
    pub struct Ids {
        translation,
        rotation,
        scale,
        shear,
    }
}

#[derive(Debug)]
pub struct State {
    ids: Ids,
    translation: Point,
    rotation: f32,
    scale: Point,
    shear: Point,
}

impl Widget for TransformEditor {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Self::State {
            ids: Ids::new(id_gen),
            translation: [0., 0.],
            rotation: 0.,
            scale: [1., 1.],
            shear: [0., 0.],
        }
    }

    fn style(&self) -> Self::Style {
        Self::Style::default()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {}
}
