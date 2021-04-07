use std::sync::Arc;

use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{app_state::NodeCollections, i18n::Language, util::*, widgets::toolbar};

use conrod_core::*;

use dialog::{DialogBox, FileSelection, FileSelectionMode};

#[derive(WidgetCommon)]
pub struct TopBar<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    event_buffer: Option<&'a [Arc<Lang>]>,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    graphs: &'a mut NodeCollections,
    style: Style,
}

impl<'a> TopBar<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        graphs: &'a mut NodeCollections,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            graphs,
            style: Style::default(),
            event_buffer: None,
        }
    }

    builder_methods! {
        pub event_buffer { event_buffer = Some(&'a [Arc<Lang>]) }
        pub icon_font { style.icon_font = Some(text::font::Id) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
}

widget_ids! {
    pub struct Ids {
        surface_tools,
        graph_tools,
        graph_selector,
        status_line,
    }
}

pub struct State {
    ids: Ids,
    vram_usage: (f32, f32, f32),
}

#[derive(Clone, Copy)]
pub enum SurfaceTool {
    NewSurface,
    OpenSurface,
    SaveSurface,
    ExportSurface,
}

impl<'a> Widget for TopBar<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            vram_usage: (0., 0., 0.),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
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

        match toolbar::Toolbar::flow_right(
            [
                (IconName::FOLDER_PLUS, SurfaceTool::NewSurface),
                (IconName::FOLDER_OPEN, SurfaceTool::OpenSurface),
                (IconName::CONTENT_SAVE, SurfaceTool::SaveSurface),
                (IconName::EXPORT, SurfaceTool::ExportSurface),
            ]
            .iter()
            .copied(),
        )
        .icon_font(style.icon_font(&ui.theme))
        .icon_color(color::WHITE)
        .button_color(color::DARK_CHARCOAL)
        .parent(id)
        .h(32.0)
        .middle()
        .set(state.ids.surface_tools, ui)
        {
            Some(SurfaceTool::NewSurface) => {
                self.sender
                    .send(Lang::UserIOEvent(UserIOEvent::NewSurface))
                    .unwrap();
            }
            Some(SurfaceTool::OpenSurface) => {
                if let Ok(Some(path)) =
                    FileSelection::new(self.language.get_message("surface-file-select"))
                        .title(self.language.get_message("surface-open-title"))
                        .mode(FileSelectionMode::Open)
                        .show()
                {
                    self.sender
                        .send(Lang::UserIOEvent(UserIOEvent::OpenSurface(
                            std::path::PathBuf::from(path),
                        )))
                        .unwrap();
                    self.graphs.clear_all();
                }
            }
            Some(SurfaceTool::SaveSurface) => {
                if let Ok(Some(path)) =
                    FileSelection::new(self.language.get_message("surface-file-select"))
                        .title(self.language.get_message("surface-save-title"))
                        .mode(FileSelectionMode::Save)
                        .show()
                {
                    self.sender
                        .send(Lang::UserIOEvent(UserIOEvent::SaveSurface(
                            std::path::PathBuf::from(path),
                        )))
                        .unwrap();
                }
            }
            Some(SurfaceTool::ExportSurface) => {
                if let Ok(Some(path)) =
                    FileSelection::new(self.language.get_message("base-name-select"))
                        .title(self.language.get_message("surface-export-title"))
                        .mode(FileSelectionMode::Save)
                        .show()
                {
                    let e_path = std::path::PathBuf::from(&path);
                    self.sender
                        .send(Lang::UserIOEvent(UserIOEvent::RunExports(e_path)))
                        .unwrap();
                }
            }
            _ => {}
        }

        if let Some(selection) =
            widget::DropDownList::new(&self.graphs.list_collection_names(), Some(0))
                .label_font_size(12)
                .parent(id)
                .mid_right_with_margin(8.0)
                .w(256.0)
                .h(32.0)
                .set(state.ids.graph_selector, ui)
        {
            if let Some(graph) = self.graphs.get_collection_resource(selection).cloned() {
                self.sender
                    .send(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
                        graph.clone(),
                    )))
                    .unwrap();
                self.graphs.set_active_collection(graph);
            }
        }

        let status_text = format!(
            "VRAM: {:.1}MB/{:.1}MB ({:.2}%)",
            state.vram_usage.0, state.vram_usage.1, state.vram_usage.2
        );

        widget::Text::new(&status_text)
            .color(color::WHITE.alpha(0.5))
            .font_size(10)
            .parent(id)
            .left(8.0)
            .align_middle_y()
            .set(state.ids.status_line, ui);
    }
}

impl<'a> TopBar<'a> {
    fn handle_event(&self, state: &mut widget::State<State>, event: &Lang) {
        match event {
            Lang::ComputeEvent(ComputeEvent::VramUsage(used, total)) => {
                const MEGABYTES: f32 = 1024. * 1024.;
                let used = *used as f32 / MEGABYTES;
                let total = *total as f32 / MEGABYTES;
                state.update(|state| state.vram_usage = (used, total, 100. * used / total));
            }
            _ => {}
        }
    }
}
