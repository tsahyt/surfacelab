use conrod_core::*;

pub struct IconName(&'static str);

impl IconName {
    pub const CONTENT_SAVE: IconName = IconName("\u{f0193}");
    pub const FOLDER_OPEN: IconName = IconName("\u{f0770}");
    pub const FOLDER_PLUS: IconName = IconName("\u{f0257}");
    pub const EXPOSE: IconName = IconName("\u{f0003}");
    pub const UNEXPOSE: IconName = IconName("\u{f1511}");
    pub const GRAPH: IconName = IconName("\u{f1049}");
    pub const PLUS: IconName = IconName("\u{f0704}");
    pub const MINUS: IconName = IconName("\u{f06f2}");
    pub const EXPORT: IconName = IconName("\u{f0207}");
}

pub fn icon_button<'a>(
    icon: IconName,
    icon_font: text::font::Id,
) -> widget::Button<'a, widget::button::Flat> {
    widget::Button::new().label(icon.0).label_font_id(icon_font)
}
