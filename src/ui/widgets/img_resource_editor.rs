use std::path::PathBuf;

use crate::lang::resource::*;
use crate::ui::{i18n::Language, util::*};
use conrod_core::*;
use dialog::{DialogBox, FileSelection, FileSelectionMode};

#[derive(WidgetCommon)]
pub struct ImageResourceEditor<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    img_resources: &'a [Resource<Img>],
    language: &'a Language,
    resource: Option<Resource<Img>>,
}

impl<'a> ImageResourceEditor<'a> {
    pub fn new(
        img_resources: &'a [Resource<Img>],
        resource: Option<Resource<Img>>,
        language: &'a Language,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            img_resources,
            language,
            resource,
        }
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
    AddFromFile(PathBuf),
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
            .position(|z| Some(z) == self.resource.as_ref());

        if let Some(new_selection) = widget::DropDownList::new(&resources, idx)
            .label_font_size(10)
            .h_of(id)
            .parent(id)
            .mid_left_of(id)
            .padded_w_of(id, 16.0)
            .set(state.ids.dropdown, ui)
        {
            res = Some(Event::SelectResource(&self.img_resources[new_selection]));
        }

        for _press in icon_button(
            IconName::FOLDER_OPEN,
            self.style.icon_font.unwrap().unwrap(),
        )
        .parent(id)
        .mid_right_of(id)
        .label_font_size(12)
        .border(0.)
        .color(color::DARK_CHARCOAL)
        .label_color(color::WHITE)
        .wh([20., 16.])
        .set(state.ids.add_button, ui)
        {
            match FileSelection::new(self.language.get_message("image-select"))
                .title(self.language.get_message("image-select-title"))
                .mode(FileSelectionMode::Open)
                .show()
            {
                Ok(Some(path)) => res = Some(Event::AddFromFile(PathBuf::from(path))),
                Err(e) => log::error!("Error during file selection {}", e),
                _ => {}
            }
        }

        res
    }
}
