use crate::lang::resource::*;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct ImageResourceEditor<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    img_resources: &'a [&'a Resource<Img>],
    resource: Option<Resource<Img>>,
}

impl<'a> ImageResourceEditor<'a> {
    pub fn new(img_resources: &'a [&'a Resource<Img>], resource: Option<Resource<Img>>) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            img_resources,
            resource,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        dropdown,
        add_button,
    }
}

pub struct State {
    ids: Ids,
}

pub enum Event<'a> {
    AddFromFile(std::path::PathBuf),
    SelectResource(&'a Resource<Img>),
}

impl<'a> Widget for ImageResourceEditor<'a> {
    type State = State;
    type Style = Style;
    type Event = Option<Event<'a>>;

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

        let widget::UpdateArgs { id, ui, state, .. } = args;

        let resources: Vec<_> = self.img_resources.iter().map(|x| x.to_string()).collect();
        let idx = self
            .img_resources
            .iter()
            .position(|z| Some(*z) == self.resource.as_ref());

        if let Some(new_selection) = widget::DropDownList::new(&resources, idx)
            .label_font_size(10)
            .h_of(id)
            .parent(id)
            .mid_left_of(id)
            .padded_w_of(id, 24.0)
            .set(state.ids.dropdown, ui)
        {
            res = Some(Event::SelectResource(&self.img_resources[new_selection]));
        }

        res
    }
}
