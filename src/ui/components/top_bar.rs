use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{app_state::NodeCollections, i18n::Language, util::*, widgets::toolbar};

use conrod_core::*;

use dialog::{DialogBox, FileSelection, FileSelectionMode};

#[derive(WidgetCommon)]
pub struct TopBar<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
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
        }
    }

    builder_methods! {
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
    }
}

pub enum SurfaceTool {
    NewSurface,
    OpenSurface,
    SaveSurface,
    ExportSurface,
}

impl<'a> Widget for TopBar<'a> {
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
        match toolbar::Toolbar::flow_right(&[
            (IconName::FOLDER_PLUS, SurfaceTool::NewSurface),
            (IconName::FOLDER_OPEN, SurfaceTool::OpenSurface),
            (IconName::CONTENT_SAVE, SurfaceTool::SaveSurface),
            (IconName::EXPORT, SurfaceTool::ExportSurface),
        ])
        .icon_font(args.style.icon_font(&args.ui.theme))
        .icon_color(color::WHITE)
        .button_color(color::DARK_CHARCOAL)
        .parent(args.id)
        .h(32.0)
        .middle()
        .set(args.state.surface_tools, args.ui)
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
                .parent(args.id)
                .mid_right_with_margin(8.0)
                .w(256.0)
                .h(32.0)
                .set(args.state.graph_selector, args.ui)
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
    }
}
