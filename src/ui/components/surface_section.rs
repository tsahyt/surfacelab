use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{
    i18n::Language,
    util::*,
    widgets::{export_row, param_box},
};

use std::sync::Arc;

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct SurfaceSection<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    event_buffer: Option<&'a [Arc<Lang>]>,
    style: Style,
}

impl<'a> SurfaceSection<'a> {
    pub fn new(language: &'a Language, sender: &'a BrokerSender<Lang>) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            event_buffer: None,
            style: Style::default(),
        }
    }

    builder_methods! {
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub event_buffer { event_buffer = Some(&'a [Arc<Lang>]) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
}

widget_ids! {
    pub struct Ids {
        param_box,
        export_label,
        export_add,
        export_list,
    }
}

pub struct State {
    ids: Ids,
    parameters: ParamBoxDescription<SurfaceField>,
    export_entries: Vec<(String, ExportSpec)>,
    registered_sockets: Vec<export_row::RegisteredSocket>,
}

impl<'a> Widget for SurfaceSection<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            parameters: ParamBoxDescription::surface_parameters(),
            export_entries: Vec::new(),
            registered_sockets: Vec::new(),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(mut self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let widget::UpdateArgs {
            state,
            ui,
            id,
            style,
            ..
        } = args;

        if let Some(ev_buf) = self.event_buffer {
            for ev in ev_buf {
                self.handle_event(state, ev);
            }
        }

        state.update(|state| {
            for ev in param_box::ParamBox::new(&mut state.parameters, &(), &self.language)
                .parent(id)
                .w_of(id)
                .mid_top()
                .text_color(color::WHITE)
                .text_size(10)
                .set(state.ids.param_box, ui)
            {
                if let param_box::Event::ChangeParameter(event) = ev {
                    self.sender.send(event).unwrap()
                }
            }
        });

        widget::Text::new(&self.language.get_message("export-spec"))
            .parent(id)
            .mid_top_with_margin(96.0)
            .color(color::WHITE)
            .font_size(12)
            .set(state.ids.export_label, ui);

        for _ev in icon_button(IconName::PLUS, style.icon_font(&ui.theme))
            .parent(id)
            .top_right_with_margins(96.0, 16.0)
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .label_font_size(12)
            .wh([20.0, 16.0])
            .set(state.ids.export_add, ui)
        {
            state.update(|state| {
                if let Some(default) = state.registered_sockets.last() {
                    state.export_entries.push((
                        "unnamed".to_owned(),
                        ExportSpec::Grayscale([default.spec.clone()]),
                    ))
                }
            });
        }

        let (mut rows, scrollbar) = widget::List::flow_down(state.export_entries.len())
            .parent(id)
            .padded_w_of(id, 8.0)
            .h(320.0)
            .mid_top_with_margin(112.0)
            .scrollbar_on_top()
            .set(state.ids.export_list, ui);

        while let Some(row) = rows.next(ui) {
            let widget = export_row::ExportRow::new(
                &state.export_entries[row.i],
                &state.registered_sockets,
                &self.language,
            );
            let mut updated_spec = false;
            match row.set(widget, ui) {
                Some(export_row::Event::ChangeToRGB) => {
                    state.update(|state| {
                        state.export_entries[row.i].1 = state.export_entries[row.i]
                            .1
                            .clone()
                            .image_type(ImageType::Rgb)
                            .set_has_alpha(false)
                    });
                    updated_spec = true;
                }
                Some(export_row::Event::ChangeToRGBA) => {
                    state.update(|state| {
                        state.export_entries[row.i].1 = state.export_entries[row.i]
                            .1
                            .clone()
                            .image_type(ImageType::Rgb)
                            .set_has_alpha(true)
                    });
                    updated_spec = true;
                }
                Some(export_row::Event::ChangeToGrayscale) => {
                    state.update(|state| {
                        state.export_entries[row.i].1 = state.export_entries[row.i]
                            .1
                            .clone()
                            .image_type(ImageType::Grayscale)
                    });
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelR(spec)) => {
                    state.update(|state| state.export_entries[row.i].1.set_red(spec));
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelG(spec)) => {
                    state.update(|state| state.export_entries[row.i].1.set_green(spec));
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelB(spec)) => {
                    state.update(|state| state.export_entries[row.i].1.set_blue(spec));
                    updated_spec = true;
                }
                Some(export_row::Event::SetChannelA(spec)) => {
                    state.update(|state| state.export_entries[row.i].1.set_alpha(spec));
                    updated_spec = true;
                }
                Some(export_row::Event::Rename(new)) => {
                    self.sender
                        .send(Lang::UserIOEvent(UserIOEvent::RenameExport(
                            state.export_entries[row.i].0.clone(),
                            new.clone(),
                        )))
                        .unwrap();
                    state.update(|state| state.export_entries[row.i].0 = new);
                }
                None => {}
            }

            if updated_spec {
                self.sender
                    .send(Lang::UserIOEvent(UserIOEvent::DeclareExport(
                        state.export_entries[row.i].0.clone(),
                        state.export_entries[row.i].1.clone(),
                    )))
                    .unwrap();
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }
    }
}

impl<'a> SurfaceSection<'a> {
    fn handle_event(&mut self, state: &mut widget::State<State>, event: &Lang) {
        match event {
            Lang::SurfaceEvent(SurfaceEvent::ExportSpecLoaded(name, spec)) => {
                state.update(|state| state.export_entries.push((name.clone(), spec.clone())));
            }
            Lang::SurfaceEvent(SurfaceEvent::ParentSizeSet(size)) => {
                state.update(|state| {
                    state.parameters.categories[0].parameters[0]
                        .control
                        .set_value(&OperatorSize::AbsoluteSize(*size).to_data())
                });
            }
            Lang::GraphEvent(GraphEvent::Cleared) => {
                state.update(|state| {
                    state.export_entries.clear();
                    state.registered_sockets.clear();
                });
            }
            Lang::ComputeEvent(ComputeEvent::SocketCreated(res, ty)) => match ty {
                ImageType::Grayscale => {
                    state.update(|state| {
                        state.registered_sockets.push(
                            crate::ui::widgets::export_row::RegisteredSocket::new((
                                res.clone(),
                                ImageChannel::R,
                            )),
                        )
                    });
                }
                ImageType::Rgb => {
                    state.update(|state| {
                        state.registered_sockets.push(
                            crate::ui::widgets::export_row::RegisteredSocket::new((
                                res.clone(),
                                ImageChannel::R,
                            )),
                        );
                        state.registered_sockets.push(
                            crate::ui::widgets::export_row::RegisteredSocket::new((
                                res.clone(),
                                ImageChannel::G,
                            )),
                        );
                        state.registered_sockets.push(
                            crate::ui::widgets::export_row::RegisteredSocket::new((
                                res.clone(),
                                ImageChannel::B,
                            )),
                        );
                    });
                }
            },
            Lang::ComputeEvent(ComputeEvent::SocketDestroyed(res)) => {
                state.update(|state| {
                    state
                        .registered_sockets
                        .drain_filter(|x| x.resource() == res);
                });
            }
            _ => {}
        }
    }
}
