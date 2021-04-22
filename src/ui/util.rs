use conrod_core::*;

/// Predefined icon names from icon font. Hardcoded for the font used.
#[derive(Debug, Clone, Copy)]
pub struct IconName(pub &'static str);

impl IconName {
    pub const CONTENT_SAVE: IconName = IconName("\u{f0193}");
    pub const FOLDER: IconName = IconName("\u{f024b}");
    pub const NODE: IconName = IconName("\u{f06a0}");
    pub const SOCKET: IconName = IconName("\u{f0427}");
    pub const IMAGE: IconName = IconName("\u{f021f}");
    pub const INPUT: IconName = IconName("\u{f0442}");
    pub const OUTPUT: IconName = IconName("\u{f0440}");
    pub const FOLDER_OPEN: IconName = IconName("\u{f0770}");
    pub const FOLDER_PLUS: IconName = IconName("\u{f0257}");
    pub const EXPOSE: IconName = IconName("\u{f0003}");
    pub const UNEXPOSE: IconName = IconName("\u{f1511}");
    pub const GRAPH: IconName = IconName("\u{f1049}");
    pub const LAYERS: IconName = IconName("\u{f0f58}");
    pub const PLUS: IconName = IconName("\u{f0704}");
    pub const MINUS: IconName = IconName("\u{f06f2}");
    pub const EXPORT: IconName = IconName("\u{f0207}");
    pub const SOLID: IconName = IconName("\u{f068d}");
    pub const FX: IconName = IconName("\u{f0871}");
    pub const TRASH: IconName = IconName("\u{f0a7a}");
    pub const EYE: IconName = IconName("\u{f0208}");
    pub const EYEOFF: IconName = IconName("\u{f0209}");
    pub const MASK: IconName = IconName("\u{f1023}");
    pub const UP: IconName = IconName("\u{f0360}");
    pub const DOWN: IconName = IconName("\u{f035d}");
    pub const LEFT: IconName = IconName("\u{f035e}");
    pub const RIGHT: IconName = IconName("\u{f035f}");
    pub const PACKAGE_OPEN: IconName = IconName("\u{f03d6}");
    pub const PACKAGE_CLOSED: IconName = IconName("\u{f03d7}");
    pub const LINK: IconName = IconName("\u{f0337}");
    pub const SEARCH: IconName = IconName("\u{f0a49}");
}

/// Create an icon button, i.e. a button with an icon in it. Uses an IconName
/// for easy declaration of the icon.
pub fn icon_button<'a>(
    icon: IconName,
    icon_font: text::font::Id,
) -> widget::Button<'a, widget::button::Flat> {
    widget::Button::new().label(icon.0).label_font_id(icon_font)
}
