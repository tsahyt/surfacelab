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
    output_resources: Vec<Resource<Node>>,
    export_entries: Vec<ExportSpec>,
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
            output_resources: Vec::new(),
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
                .icon_font(style.icon_font(&ui.theme))
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
            .enabled(!state.output_resources.is_empty())
            .parent(id)
            .top_right_with_margins(96.0, 8.0)
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .label_font_size(12)
            .wh([20.0, 16.0])
            .set(state.ids.export_add, ui)
        {
            if let Some(res) = state.output_resources.iter().next() {
                self.sender
                    .send(Lang::UserIOEvent(UserIOEvent::NewExportSpec(
                        ExportSpec::from(res),
                    )))
                    .unwrap()
            }
        }

        let (mut rows, scrollbar) = widget::List::flow_down(state.export_entries.len())
            .parent(id)
            .padded_w_of(id, 8.0)
            .item_size(120.)
            .h(120. * state.export_entries.len() as f64)
            .mid_top_with_margin(120.0)
            .set(state.ids.export_list, ui);

        state.update(|state| {
            while let Some(row) = rows.next(ui) {
                let widget = export_row::ExportRow::new(
                    &mut state.export_entries[row.i],
                    &state.output_resources,
                    self.language,
                )
                .icon_font(style.icon_font(&ui.theme));

                if let Some(ev) = row.set(widget, ui) {
                    match ev {
                        export_row::Event::Updated => {
                            let spec = &state.export_entries[row.i];
                            self.sender
                                .send(Lang::UserIOEvent(UserIOEvent::UpdateExportSpec(
                                    spec.name.clone(),
                                    spec.clone(),
                                )))
                                .unwrap();
                        }
                        export_row::Event::Renamed(from) => {
                            let spec = &state.export_entries[row.i];
                            self.sender
                                .send(Lang::UserIOEvent(UserIOEvent::UpdateExportSpec(
                                    from,
                                    spec.clone(),
                                )))
                                .unwrap();
                        }
                        export_row::Event::Remove => {
                            let spec = &state.export_entries[row.i];
                            self.sender
                                .send(Lang::UserIOEvent(UserIOEvent::RemoveExportSpec(
                                    spec.name.clone(),
                                )))
                                .unwrap();
                        }
                    }
                }
            }
        });

        if let Some(s) = scrollbar {
            s.set(ui);
        }
    }
}

impl<'a> SurfaceSection<'a> {
    fn handle_event(&mut self, state: &mut widget::State<State>, event: &Lang) {
        match event {
            Lang::SurfaceEvent(SurfaceEvent::ExportSpecDeclared(spec)) => state.update(|state| {
                state.export_entries.push(spec.clone());
            }),
            Lang::SurfaceEvent(SurfaceEvent::ExportSpecRemoved(name)) => state.update(|state| {
                if let Some(idx) = state
                    .export_entries
                    .iter()
                    .position(|spec| &spec.name == name)
                {
                    state.export_entries.remove(idx);
                }
            }),
            Lang::SurfaceEvent(SurfaceEvent::ParentSizeSet(size)) => {
                state.update(|state| {
                    state.parameters.categories[0].parameters[0]
                        .control
                        .set_value(&OperatorSize::AbsoluteSize(*size).to_data())
                });
            }
            Lang::GraphEvent(GraphEvent::Cleared) => {
                state.update(|state| {
                    state.output_resources.clear();
                    state.export_entries.clear();
                });
            }
            Lang::GraphEvent(GraphEvent::NodeAdded(
                res,
                Operator::AtomicOperator(AtomicOperator::Output(..)),
                _,
                _,
                _,
            )) => {
                state.update(|state| {
                    state.output_resources.push(res.clone());
                });
            }
            Lang::GraphEvent(GraphEvent::NodeRemoved(res)) => {
                if let Some(idx) = state.output_resources.iter().position(|r| r == res) {
                    state.update(|state| {
                        state.output_resources.remove(idx);
                    });
                }
            }
            Lang::GraphEvent(GraphEvent::NodeRenamed(from, to)) => {
                if let Some(idx) = state.output_resources.iter().position(|r| r == from) {
                    state.update(|state| {
                        state.output_resources.remove(idx);
                        state.output_resources.push(to.clone());

                        for spec in state
                            .export_entries
                            .iter_mut()
                            .filter(|spec| &spec.node == from)
                        {
                            spec.node = to.clone();
                        }
                    });
                }
            }
            _ => {}
        }
    }
}
