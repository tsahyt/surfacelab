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
            export_entries: vec![ExportSpec {
                prefix: "something".to_string(),
                node: Resource::node("base/output.1"),
                color_space: ColorSpace::Srgb,
                bit_depth: 8,
                format: ExportFormat::Png,
            }], // Vec::new(),
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
            .parent(id)
            .top_right_with_margins(96.0, 16.0)
            .border(0.)
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .label_font_size(12)
            .wh([20.0, 16.0])
            .set(state.ids.export_add, ui)
        {}

        let (mut rows, scrollbar) = widget::List::flow_down(state.export_entries.len())
            .parent(id)
            .padded_w_of(id, 8.0)
            .h(320.0)
            .mid_top_with_margin(112.0)
            .scrollbar_on_top()
            .set(state.ids.export_list, ui);

        state.update(|state| {
            while let Some(row) = rows.next(ui) {
                let widget =
                    export_row::ExportRow::new(&mut state.export_entries[row.i], self.language);

                row.set(widget, ui);
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
            Lang::SurfaceEvent(SurfaceEvent::ExportSpecLoaded(name, spec)) => {}
            Lang::SurfaceEvent(SurfaceEvent::ParentSizeSet(size)) => {}
            Lang::GraphEvent(GraphEvent::Cleared) => {}
            Lang::ComputeEvent(ComputeEvent::SocketCreated(res, ty)) => {}
            Lang::ComputeEvent(ComputeEvent::SocketDestroyed(res)) => {}
            _ => {}
        }
    }
}
