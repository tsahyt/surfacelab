use std::path::PathBuf;

use crate::lang::{resource::*, ColorSpace};
use crate::ui::{i18n::Language, util::*};
use conrod_core::*;
use dialog::{DialogBox, FileSelection, FileSelectionMode};

#[derive(WidgetCommon)]
pub struct ImageResourceEditor<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    img_resources: &'a [(Resource<Img>, ColorSpace, bool)],
    language: &'a Language,
    resource: Option<Resource<Img>>,
}

impl<'a> ImageResourceEditor<'a> {
    pub fn new(
        img_resources: &'a [(Resource<Img>, ColorSpace, bool)],
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

    builder_methods! {
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub text_size { style.text_size = Some(FontSize) }
        pub text_color { style.text_color = Some(Color) }
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
        resource,
        add_button,
        color_space,
        pack_button,
    }
}

pub struct State {
    ids: Ids,
}

pub enum Event<'a> {
    AddFromFile(PathBuf),
    SelectResource(&'a Resource<Img>),
    SetColorSpace(ColorSpace),
    PackImage,
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

        let widget::UpdateArgs {
            id,
            ui,
            state,
            style,
            ..
        } = args;

        let resources: Vec<_> = self
            .img_resources
            .iter()
            .map(|(x, _, _)| x.to_string())
            .collect();
        let idx = self
            .img_resources
            .iter()
            .position(|z| Some(&z.0) == self.resource.as_ref());

        if let Some(new_selection) = widget::DropDownList::new(&resources, idx)
            .label_font_size(style.text_size(&ui.theme))
            .h(16.0)
            .parent(id)
            .top_left_of(id)
            .padded_w_of(id, 24.0)
            .set(state.ids.resource, ui)
        {
            res = Some(Event::SelectResource(&self.img_resources[new_selection].0));
        }

        for _press in icon_button(IconName::FOLDER_OPEN, style.icon_font(&ui.theme))
            .parent(id)
            .top_right_of(id)
            .label_font_size(style.text_size(&ui.theme) + 2)
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(style.text_color(&ui.theme))
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

        let is_packed = idx.map(|i| self.img_resources[i].2).unwrap_or(false);

        for _press in icon_button(
            if is_packed {
                IconName::PACKAGE_CLOSED
            } else {
                IconName::PACKAGE_OPEN
            },
            style.icon_font(&ui.theme),
        )
        .enabled(!is_packed)
        .parent(id)
        .left_from(state.ids.add_button, 4.)
        .label_font_size(style.text_size(&ui.theme) + 2)
        .border(0.)
        .color(color::DARK_CHARCOAL)
        .label_color(style.text_color(&ui.theme))
        .wh([20., 16.])
        .set(state.ids.pack_button, ui)
        {
            res = Some(Event::PackImage);
        }

        let cs_idx = match idx.map(|i| self.img_resources[i].1) {
            Some(ColorSpace::Srgb) => Some(0),
            Some(ColorSpace::Linear) => Some(1),
            None => None,
        };

        if let Some(new_selection) = widget::DropDownList::new(&["sRGB", "Linear"], cs_idx)
            .label_font_size(style.text_size(&ui.theme))
            .parent(id)
            .mid_bottom_of(id)
            .h(16.0)
            .w_of(id)
            .set(state.ids.color_space, ui)
        {
            match new_selection {
                0 => res = Some(Event::SetColorSpace(ColorSpace::Srgb)),
                1 => res = Some(Event::SetColorSpace(ColorSpace::Linear)),
                _ => unreachable!(),
            }
        }

        res
    }
}
