use conrod_core::*;

pub struct IconName(&'static str);

impl IconName {
    pub const CONTENT_SAVE: IconName = IconName("\u{f0193}");
}

pub fn icon_button<'a>(
    icon: IconName,
    fonts: &super::app::AppFonts,
) -> widget::Button<'a, widget::button::Flat> {
    widget::Button::new().label(icon.0).label_font_id(fonts.icon_font)
}
