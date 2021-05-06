use std::path::PathBuf;

use crate::lang::{resource::*, ColorSpace};
use crate::ui::{i18n::Language, util::*};
use conrod_core::*;
use dialog::{DialogBox, FileSelection, FileSelectionMode};

pub trait EditableResource: Scheme + Sized + PartialEq {
    type Extra;
    type Event;

    /// Action to perform when loading/opening a new resource
    fn open_action(language: &Language) -> Option<Event<Self>>;

    /// Whether this resource (type) can be packed or not.
    fn packable() -> bool {
        true
    }

    /// Determine whether any particular resource (evidenced by the extra data)
    /// is packed.
    fn is_packed(extra: &Self::Extra) -> bool;

    /// Extra widgets to create for this resource type. Defaults to nothing.
    fn extra_widgets(
        _args: widget::UpdateArgs<ResourceEditor<Self>>,
        _extra: Option<&Self::Extra>,
    ) -> Option<Self::Event> {
        None
    }
}

pub enum ImgEvent {
    SetColorSpace(ColorSpace),
}

impl EditableResource for Img {
    type Extra = (ColorSpace, bool);
    type Event = ImgEvent;

    fn open_action<'a>(language: &'a Language) -> Option<Event<'a, Self>> {
        match FileSelection::new(language.get_message("image-select"))
            .title(language.get_message("image-select-title"))
            .mode(FileSelectionMode::Open)
            .show()
        {
            Ok(Some(path)) => Some(Event::AddFromFile(PathBuf::from(path))),
            Err(e) => {
                log::error!("Error during file selection {}", e);
                None
            }
            _ => None,
        }
    }

    fn is_packed(extra: &Self::Extra) -> bool {
        extra.1
    }

    fn extra_widgets(
        args: widget::UpdateArgs<ResourceEditor<Img>>,
        extra: Option<&Self::Extra>,
    ) -> Option<ImgEvent> {
        let mut res = None;
        let widget::UpdateArgs {
            id,
            ui,
            state,
            style,
            ..
        } = args;

        let cs_idx = match extra.map(|x| x.0) {
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
                0 => res = Some(ImgEvent::SetColorSpace(ColorSpace::Srgb)),
                1 => res = Some(ImgEvent::SetColorSpace(ColorSpace::Linear)),
                _ => unreachable!(),
            }
        }

        res
    }
}

impl EditableResource for Svg {
    type Extra = bool;
    type Event = !;

    fn open_action<'a>(language: &'a Language) -> Option<Event<'a, Self>> {
        match FileSelection::new(language.get_message("svg-select"))
            .title(language.get_message("svg-select-title"))
            .mode(FileSelectionMode::Open)
            .show()
        {
            Ok(Some(path)) => Some(Event::AddFromFile(PathBuf::from(path))),
            Err(e) => {
                log::error!("Error during file selection {}", e);
                None
            }
            _ => None,
        }
    }

    fn is_packed(extra: &Self::Extra) -> bool {
        *extra
    }
}

#[derive(WidgetCommon)]
pub struct ResourceEditor<'a, S: EditableResource> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    resources: &'a [(Resource<S>, S::Extra)],
    language: &'a Language,
    resource: Option<Resource<S>>,
}

impl<'a, S> ResourceEditor<'a, S>
where
    S: EditableResource,
{
    pub fn new(
        resources: &'a [(Resource<S>, S::Extra)],
        resource: Option<Resource<S>>,
        language: &'a Language,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            resources,
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

pub enum Event<'a, S: EditableResource> {
    AddFromFile(PathBuf),
    SelectResource(&'a Resource<S>),
    Pack,
    TypeEvent(S::Event),
}

impl<'a, S> Widget for ResourceEditor<'a, S>
where
    S: EditableResource,
{
    type State = State;
    type Style = Style;
    type Event = Option<Event<'a, S>>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, mut args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut res = None;

        let widget::UpdateArgs {
            id,
            ui,
            state,
            style,
            ..
        } = &mut args;

        let resources: Vec<_> = self.resources.iter().map(|(x, _)| x.to_string()).collect();
        let idx = self
            .resources
            .iter()
            .position(|z| Some(&z.0) == self.resource.as_ref());

        if let Some(new_selection) = widget::DropDownList::new(&resources, idx)
            .label_font_size(style.text_size(&ui.theme))
            .h(16.0)
            .parent(*id)
            .top_left_of(*id)
            .padded_w_of(*id, 14.0 + if S::packable() { 10. } else { 0. })
            .set(state.ids.resource, *ui)
        {
            res = Some(Event::SelectResource(&self.resources[new_selection].0));
        }

        for _press in icon_button(IconName::FOLDER_OPEN, style.icon_font(&ui.theme))
            .parent(*id)
            .top_right_of(*id)
            .label_font_size(style.text_size(&ui.theme) + 2)
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(style.text_color(&ui.theme))
            .wh([20., 16.])
            .set(state.ids.add_button, *ui)
        {
            res = S::open_action(self.language);
        }

        if S::packable() {
            let is_packed = idx
                .map(|i| S::is_packed(&self.resources[i].1))
                .unwrap_or(false);

            for _press in icon_button(
                if is_packed {
                    IconName::PACKAGE_CLOSED
                } else {
                    IconName::PACKAGE_OPEN
                },
                style.icon_font(&ui.theme),
            )
            .enabled(!is_packed)
            .parent(*id)
            .left_from(state.ids.add_button, 4.)
            .label_font_size(style.text_size(&ui.theme) + 2)
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(style.text_color(&ui.theme))
            .wh([20., 16.])
            .set(state.ids.pack_button, *ui)
            {
                res = Some(Event::Pack);
            }
        }

        res =
            res.or(S::extra_widgets(args, idx.map(|i| &self.resources[i].1)).map(Event::TypeEvent));

        res
    }
}
