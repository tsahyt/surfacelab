use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{
    i18n::Language,
    util::*,
    widgets::{export_row, param_box},
};

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct SurfaceSection<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    surface_params: &'a mut ParamBoxDescription<SurfaceField>,
    export_entries: &'a mut Vec<(String, ExportSpec)>,
    registered_sockets: &'a [export_row::RegisteredSocket],
    style: Style,
}

impl<'a> SurfaceSection<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        surface_params: &'a mut ParamBoxDescription<SurfaceField>,
        export_entries: &'a mut Vec<(String, ExportSpec)>,
        registered_sockets: &'a [export_row::RegisteredSocket],
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            surface_params,
            export_entries,
            registered_sockets,
            style: Style::default(),
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
        param_box,
        export_label,
        export_add,
        export_list,
    }
}

impl<'a> Widget for SurfaceSection<'a> {
    type State = Ids;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Ids::new(id_gen)
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        for ev in param_box::ParamBox::new(self.surface_params, &(), &self.language)
            .parent(args.id)
            .w_of(args.id)
            .mid_top()
            .set(args.state.param_box, args.ui)
        {
            if let param_box::Event::ChangeParameter(event) = ev {
                self.sender.send(event).unwrap()
            }
        }

        widget::Text::new(&self.language.get_message("export-spec"))
            .parent(args.id)
            .mid_top_with_margin(96.0)
            .color(color::WHITE)
            .font_size(12)
            .set(args.state.export_label, args.ui);

        for _ev in icon_button(IconName::PLUS, self.style.icon_font.unwrap().unwrap())
            .parent(args.id)
            .top_right_with_margins(96.0, 16.0)
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .label_font_size(12)
            .wh([20.0, 16.0])
            .set(args.state.export_add, args.ui)
        {
            if let Some(default) = self.registered_sockets.last() {
                self.export_entries.push((
                    "unnamed".to_owned(),
                    ExportSpec::Grayscale([default.spec.clone()]),
                ));
            }
        }

        let (mut rows, scrollbar) = widget::List::flow_down(self.export_entries.len())
            .parent(args.id)
            .padded_w_of(args.id, 8.0)
            .h(320.0)
            .mid_top_with_margin(112.0)
            .scrollbar_on_top()
            .set(args.state.export_list, args.ui);

        while let Some(row) = rows.next(args.ui) {
            let widget = export_row::ExportRow::new(
                &self.export_entries[row.i],
                &self.registered_sockets,
                &self.language,
            );
            let mut updated_spec = false;
            match row.set(widget, args.ui) {
                Some(export_row::Event::ChangeToRGB) => {
                    self.export_entries[row.i].1 = self.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Rgb)
                        .set_has_alpha(false);
                    updated_spec = true;
                }
                Some(export_row::Event::ChangeToRGBA) => {
                    self.export_entries[row.i].1 = self.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Rgb)
                        .set_has_alpha(true);
                    updated_spec = true;
                }
                Some(export_row::Event::ChangeToGrayscale) => {
                    self.export_entries[row.i].1 = self.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Grayscale);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelR(spec)) => {
                    self.export_entries[row.i].1.set_red(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelG(spec)) => {
                    self.export_entries[row.i].1.set_green(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelB(spec)) => {
                    self.export_entries[row.i].1.set_blue(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelA(spec)) => {
                    self.export_entries[row.i].1.set_alpha(spec);
                    updated_spec = true;
                }
                Some(export_row::Event::Rename(new)) => {
                    // TODO: renaming two specs to the same name causes discrepancies with the backend
                    self.sender
                        .send(Lang::UserIOEvent(UserIOEvent::RenameExport(
                            self.export_entries[row.i].0.clone(),
                            new.clone(),
                        )))
                        .unwrap();
                    self.export_entries[row.i].0 = new;
                }
                None => {}
            }

            if updated_spec {
                self.sender
                    .send(Lang::UserIOEvent(UserIOEvent::DeclareExport(
                        self.export_entries[row.i].0.clone(),
                        self.export_entries[row.i].1.clone(),
                    )))
                    .unwrap();
            }
        }

        if let Some(s) = scrollbar {
            s.set(args.ui);
        }
    }
}
