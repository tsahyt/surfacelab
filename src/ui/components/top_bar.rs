use crate::ui::util::*;
use conrod_core::*;

#[derive(WidgetCommon)]
pub struct TopBar {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style
}

impl TopBar {
    pub fn new() -> Self {
        Self {
            common: widget::CommonBuilder::default(),
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
        new_surface,
        open_surface,
        save_surface,
        export_surface,
        graph_selector,
        graph_add,
        layers_add,
    }
}

impl Widget for TopBar {
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
        for _press in icon_button(IconName::FOLDER_PLUS, self.style.icon_font.unwrap().unwrap())
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .mid_left_with_margin(8.0)
            .parent(args.id)
            .set(args.state.new_surface, args.ui)
        {
            // self.sender
            //     .send(Lang::UserIOEvent(UserIOEvent::NewSurface))
            //     .unwrap();
        }

        for _press in icon_button(IconName::FOLDER_OPEN, self.style.icon_font.unwrap().unwrap())
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .right(8.0)
            .parent(args.id)
            .set(args.state.open_surface, args.ui)
        {
            // if let Ok(Some(path)) = FileSelection::new(self.label_text("surface-file-select"))
            //     .title(self.label_text("surface-open-title"))
            //     .mode(FileSelectionMode::Open)
            //     .show()
            // {
            //     self.sender
            //         .send(Lang::UserIOEvent(UserIOEvent::OpenSurface(
            //             std::path::PathBuf::from(path),
            //         )))
            //         .unwrap();
            //     self.app_state.graphs.clear_all();
            // }
        }

        for _press in icon_button(IconName::CONTENT_SAVE, self.style.icon_font.unwrap().unwrap())
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .right(8.0)
            .parent(args.id)
            .set(args.state.save_surface, args.ui)
        {
            // if let Ok(Some(path)) = FileSelection::new(self.label_text("surface-file-select"))
            //     .title(self.label_text("surface-save-title"))
            //     .mode(FileSelectionMode::Save)
            //     .show()
            // {
            //     self.sender
            //         .send(Lang::UserIOEvent(UserIOEvent::SaveSurface(
            //             std::path::PathBuf::from(path),
            //         )))
            //         .unwrap();
            // }
        }

        for _press in icon_button(IconName::EXPORT, self.style.icon_font.unwrap().unwrap())
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .right(8.0)
            .parent(args.id)
            .set(args.state.export_surface, args.ui)
        {
            // if let Ok(Some(path)) = FileSelection::new(self.label_text("base-name-select"))
            //     .title(self.label_text("surface-export-title"))
            //     .mode(FileSelectionMode::Save)
            //     .show()
            // {
            //     let e_path = std::path::PathBuf::from(&path);
            //     self.sender
            //         .send(Lang::UserIOEvent(UserIOEvent::RunExports(e_path)))
            //         .unwrap();
            // }
        }

        if let Some(selection) =
            // widget::DropDownList::new(&self.app_state.graphs.list_collection_names(), Some(0))
            widget::DropDownList::new(&["foo"], Some(0))
                .label_font_size(12)
                .parent(args.id)
                .mid_right_with_margin(8.0)
                .w(256.0)
                .set(args.state.graph_selector, args.ui)
        {
            // if let Some(graph) = self
            //     .app_state
            //     .graphs
            //     .get_collection_resource(selection)
            //     .cloned()
            // {
            //     self.sender
            //         .send(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
            //             graph.clone(),
            //         )))
            //         .unwrap();
            //     self.app_state.graphs.set_active(graph);
            //     self.app_state.addable_operators = self
            //         .app_state
            //         .registered_operators
            //         .iter()
            //         .filter(|o| !o.is_graph(self.app_state.graphs.get_active()))
            //         .cloned()
            //         .collect();
            // }
        }

        for _press in icon_button(IconName::GRAPH, self.style.icon_font.unwrap().unwrap())
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .left(8.0)
            .parent(args.id)
            .set(args.state.graph_add, args.ui)
        {
            // self.sender
            //     .send(Lang::UserGraphEvent(UserGraphEvent::AddGraph))
            //     .unwrap()
        }

        for _press in icon_button(IconName::LAYERS, self.style.icon_font.unwrap().unwrap())
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .border(0.0)
            .wh([32., 32.0])
            .left(8.0)
            .parent(args.id)
            .set(args.state.layers_add, args.ui)
        {
            // self.sender
            //     .send(Lang::UserLayersEvent(UserLayersEvent::AddLayers))
            //     .unwrap()
        }
    }
}
