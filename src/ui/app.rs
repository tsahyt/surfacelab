use crate::{broker::BrokerSender, lang::*};
use conrod_core::*;
use dialog::{DialogBox, FileSelection, FileSelectionMode};
use std::collections::HashMap;

const PANEL_COLOR: Color = color::DARK_CHARCOAL;
const PANEL_GAP: Scalar = 0.5;

widget_ids!(
    pub struct Ids {
        // Main Areas
        window_canvas,
        top_bar_canvas,
        main_canvas,
        node_graph_canvas,
        drawing_canvas,
        sidebar_canvas,
        parameter_canvas,
        graph_settings_canvas,
        surface_settings_canvas,

        // Sidebar
        sidebar_tabs,

        // Top Buttons
        new_surface,
        open_surface,
        save_surface,
        graph_selector,
        graph_add,

        // Main Views
        node_graph,
        render_view,
        add_modal,

        // Parameter Area
        node_param_box,
        exposed_param_list,
    }
);

#[derive(Debug, Clone)]
pub struct Graph {
    graph: super::graph::NodeGraph,
    resources: HashMap<Resource, petgraph::graph::NodeIndex>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            graph: petgraph::Graph::new(),
            resources: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct Graphs {
    graphs: HashMap<Resource, Graph>,
    active_graph: Graph,
    active_resource: Resource,
}

impl Graphs {
    pub fn new() -> Self {
        Graphs {
            graphs: HashMap::new(),
            active_graph: Graph::default(),
            active_resource: Resource::graph("base", None),
        }
    }

    pub fn set_active(&mut self, graph: Resource) {
        self.graphs
            .insert(self.active_resource.clone(), self.active_graph.clone());
        self.active_resource = graph;
        self.active_graph = self.graphs.remove(&self.active_resource).unwrap();
    }

    pub fn get_active(&self) -> &Resource {
        &self.active_resource
    }

    pub fn index_of(&self, resource: &Resource) -> Option<petgraph::graph::NodeIndex> {
        self.active_graph.resources.get(&resource).copied()
    }

    pub fn insert_index(&mut self, resource: Resource, index: petgraph::graph::NodeIndex) {
        self.active_graph.resources.insert(resource, index);
    }

    pub fn remove_index(&mut self, resource: &Resource) {
        self.active_graph.resources.remove(resource);
    }

    pub fn clear_indices(&mut self) {
        self.active_graph.resources.clear();
    }

    pub fn add_graph(&mut self, graph: Resource) {
        self.graphs.insert(graph, Graph::default());
    }

    /// Get a list of graph names for displaying
    pub fn list_graph_names(&self) -> Vec<&str> {
        std::iter::once(self.active_resource.file().unwrap())
            .chain(self.graphs.keys().map(|k| k.file().unwrap()))
            .collect()
    }

    /// Get a reference to the resource denominating the graph at the given
    /// index. This index refers to the ordering returned by `list_graph_names`.
    pub fn get_graph_resource(&self, index: usize) -> Option<&Resource> {
        std::iter::once(&self.active_resource)
            .chain(self.graphs.keys())
            .nth(index)
    }
}

impl std::ops::Deref for Graphs {
    type Target = super::graph::NodeGraph;

    fn deref(&self) -> &Self::Target {
        &self.active_graph.graph
    }
}

impl std::ops::DerefMut for Graphs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.active_graph.graph
    }
}

pub struct App {
    pub graphs: Graphs,
    pub active_element: Option<petgraph::graph::NodeIndex>,
    pub render_image: Option<image::Id>,

    pub monitor_resolution: (u32, u32),

    pub add_modal: bool,
}

impl App {
    pub fn new(monitor_size: (u32, u32)) -> Self {
        Self {
            graphs: Graphs::new(),
            active_element: None,
            render_image: None,
            monitor_resolution: (monitor_size.0, monitor_size.1),
            add_modal: false,
        }
    }

    pub fn active_parameters(
        &mut self,
    ) -> Option<(&mut ParamBoxDescription<impl MessageWriter>, &Resource)> {
        let ae = self.active_element?;
        let node = self.graphs.node_weight_mut(ae)?;
        Some((&mut node.param_box, &node.resource))
    }

    pub fn register_thumbnail(&mut self, resource: &Resource, thumbnail: image::Id) {
        if let Some(idx) = self.graphs.index_of(resource) {
            if let Some(node) = self.graphs.node_weight_mut(idx) {
                node.thumbnail = Some(thumbnail);
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct AppFonts {
    pub text_font: text::font::Id,
    pub icon_font: text::font::Id,
}

pub struct Gui<B: crate::gpu::Backend> {
    ids: Ids,
    fonts: AppFonts,
    app_state: App,
    sender: BrokerSender<Lang>,
    image_map: image::Map<crate::gpu::ui::Image<B>>,
}

impl<B> Gui<B>
where
    B: crate::gpu::Backend,
{
    pub fn new(
        ids: Ids,
        fonts: AppFonts,
        sender: BrokerSender<Lang>,
        monitor_size: (u32, u32),
        image_map: image::Map<crate::gpu::ui::Image<B>>,
    ) -> Self {
        Self {
            ids,
            fonts,
            app_state: App::new(monitor_size),
            sender,
            image_map,
        }
    }

    pub fn image_map(&self) -> &image::Map<crate::gpu::ui::Image<B>> {
        &self.image_map
    }

    pub fn handle_event(
        &mut self,
        ui: &mut Ui,
        renderer: &mut crate::gpu::ui::Renderer<B>,
        event: &Lang,
    ) {
        match event {
            Lang::RenderEvent(RenderEvent::RendererAdded(_id, view)) => {
                if let Some(view) = view.to::<B>() {
                    let id = self.image_map.insert(renderer.create_image(
                        view,
                        self.app_state.monitor_resolution.0,
                        self.app_state.monitor_resolution.1,
                    ));
                    self.app_state.render_image = Some(id);
                }
            }
            Lang::RenderEvent(RenderEvent::RendererRedrawn(_id)) => {
                ui.needs_redraw();
            }
            Lang::ComputeEvent(ComputeEvent::ThumbnailCreated(res, thmb)) => {
                if let Some(t) = thmb.to::<B>() {
                    let id = self.image_map.insert(renderer.create_image(t, 128, 128));
                    self.app_state.register_thumbnail(&res.drop_fragment(), id);
                }
            }
            Lang::ComputeEvent(ComputeEvent::ThumbnailDestroyed(_res)) => {
                // TODO: purge old thumbnail descriptors
            }
            Lang::GraphEvent(ev) => self.handle_graph_event(ev),
            _ => {}
        }
    }

    fn handle_graph_event(&mut self, event: &GraphEvent) {
        match event {
            GraphEvent::GraphAdded(res) => self.app_state.graphs.add_graph(res.clone()),
            GraphEvent::NodeAdded(res, op, pbox, position, _size) => {
                let idx = self.app_state.graphs.add_node(super::graph::NodeData::new(
                    res.clone(),
                    position.map(|(x, y)| [x, y]),
                    &op,
                    pbox.clone(),
                ));
                self.app_state.graphs.insert_index(res.clone(), idx);
            }
            GraphEvent::NodeRemoved(res) => {
                if let Some(idx) = self.app_state.graphs.index_of(res) {
                    self.app_state.graphs.remove_node(idx);
                }
                self.app_state.graphs.remove_index(res);
                // FIXME: removal renumbers the nodes, so we need to update the indexing as well
            }
            GraphEvent::NodeRenamed(from, to) => {
                if let Some(idx) = self.app_state.graphs.index_of(from) {
                    let node = self.app_state.graphs.node_weight_mut(idx).unwrap();
                    node.resource = to.clone();
                    self.app_state.graphs.insert_index(to.clone(), idx);
                    self.app_state.graphs.remove_index(from);
                }
            }
            GraphEvent::ConnectedSockets(from, to) => {
                let from_idx = self
                    .app_state
                    .graphs
                    .index_of(&from.drop_fragment())
                    .unwrap();
                let to_idx = self.app_state.graphs.index_of(&to.drop_fragment()).unwrap();
                self.app_state.graphs.add_edge(
                    from_idx,
                    to_idx,
                    (
                        from.fragment().unwrap().to_string(),
                        to.fragment().unwrap().to_string(),
                    ),
                );
            }
            GraphEvent::DisconnectedSockets(from, to) => {
                use petgraph::visit::EdgeRef;

                let from_idx = self
                    .app_state
                    .graphs
                    .index_of(&from.drop_fragment())
                    .unwrap();
                let to_idx = self.app_state.graphs.index_of(&to.drop_fragment()).unwrap();

                // Assuming that there's only ever one edge connecting two sockets.
                if let Some(e) = self
                    .app_state
                    .graphs
                    .edges_connecting(from_idx, to_idx)
                    .filter(|e| {
                        (e.weight().0.as_str(), e.weight().1.as_str())
                            == (from.fragment().unwrap(), to.fragment().unwrap())
                    })
                    .map(|e| e.id())
                    .next()
                {
                    self.app_state.graphs.remove_edge(e);
                }
            }
            GraphEvent::SocketMonomorphized(socket, ty) => {
                let idx = self
                    .app_state
                    .graphs
                    .index_of(&socket.drop_fragment())
                    .unwrap();
                let node = self.app_state.graphs.node_weight_mut(idx).unwrap();
                let var = type_variable_from_socket_iter(
                    node.inputs.iter().chain(node.outputs.iter()),
                    socket.fragment().unwrap(),
                )
                .unwrap();
                node.set_type_variable(var, Some(*ty))
            }
            GraphEvent::SocketDemonomorphized(socket) => {
                let idx = self
                    .app_state
                    .graphs
                    .index_of(&socket.drop_fragment())
                    .unwrap();
                let node = self.app_state.graphs.node_weight_mut(idx).unwrap();
                let var = type_variable_from_socket_iter(
                    node.inputs.iter().chain(node.outputs.iter()),
                    socket.fragment().unwrap(),
                )
                .unwrap();
                node.set_type_variable(var, None)
            }
            GraphEvent::Cleared => {
                self.app_state.graphs.clear();
                self.app_state.graphs.clear_indices();
            }
            _ => {}
        }
    }

    pub fn update_gui(&mut self, ui: &mut UiCell) {
        use super::tabs;

        widget::Canvas::new()
            .border(0.0)
            .color(PANEL_COLOR)
            .flow_down(&[
                (
                    self.ids.top_bar_canvas,
                    widget::Canvas::new()
                        .length(48.0)
                        .border(PANEL_GAP)
                        .color(color::CHARCOAL),
                ),
                (
                    self.ids.main_canvas,
                    widget::Canvas::new()
                        .border(PANEL_GAP)
                        .color(PANEL_COLOR)
                        .flow_right(&[
                            (
                                self.ids.node_graph_canvas,
                                widget::Canvas::new()
                                    .scroll_kids()
                                    .color(PANEL_COLOR)
                                    .border(PANEL_GAP),
                            ),
                            (
                                self.ids.drawing_canvas,
                                widget::Canvas::new().color(PANEL_COLOR).border(PANEL_GAP),
                            ),
                            (
                                self.ids.sidebar_canvas,
                                widget::Canvas::new()
                                    .length_weight(0.4)
                                    .color(PANEL_COLOR)
                                    .border(PANEL_GAP),
                            ),
                        ]),
                ),
            ])
            .set(self.ids.window_canvas, ui);

        tabs::Tabs::new(&[
            (self.ids.parameter_canvas, "Parameters"),
            (self.ids.graph_settings_canvas, "Graph"),
            (self.ids.surface_settings_canvas, "Surface"),
        ])
        .color(PANEL_COLOR)
        .label_color(color::WHITE)
        .label_font_size(10)
        .parent(self.ids.sidebar_canvas)
        .wh_of(self.ids.sidebar_canvas)
        .middle()
        .set(self.ids.sidebar_tabs, ui);

        self.top_bar(ui);
        self.node_graph(ui);
        self.render_view(ui);
        self.parameter_section(ui);
        self.graph_section(ui);
        self.surface_section(ui);
    }

    fn top_bar(&mut self, ui: &mut UiCell) {
        use super::util::*;

        for _press in icon_button(IconName::FOLDER_PLUS, &self.fonts)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .wh([32., 32.0])
            .mid_left_with_margin(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.new_surface, ui)
        {
            self.sender
                .send(Lang::UserIOEvent(UserIOEvent::NewSurface))
                .unwrap();
        }

        for _press in icon_button(IconName::FOLDER_OPEN, &self.fonts)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.open_surface, ui)
        {
            match FileSelection::new("Select a surface file")
                .title("Open Surface")
                .mode(FileSelectionMode::Open)
                .show()
            {
                Ok(Some(path)) => {
                    self.sender
                        .send(Lang::UserIOEvent(UserIOEvent::OpenSurface(
                            std::path::PathBuf::from(path),
                        )))
                        .unwrap();
                    self.app_state.graphs.clear();
                    self.app_state.graphs.clear_indices();
                }
                _ => {}
            }
        }

        for _press in icon_button(IconName::CONTENT_SAVE, &self.fonts)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.save_surface, ui)
        {
            match FileSelection::new("Select a surface file")
                .title("Save Surface")
                .mode(FileSelectionMode::Save)
                .show()
            {
                Ok(Some(path)) => {
                    self.sender
                        .send(Lang::UserIOEvent(UserIOEvent::SaveSurface(
                            std::path::PathBuf::from(path),
                        )))
                        .unwrap();
                }
                _ => {}
            }
        }

        for selection in
            widget::DropDownList::new(&self.app_state.graphs.list_graph_names(), Some(0))
                .label_font_size(12)
                .parent(self.ids.top_bar_canvas)
                .mid_right_with_margin(8.0)
                .w(256.0)
                .set(self.ids.graph_selector, ui)
        {
            if let Some(graph) = self.app_state.graphs.get_graph_resource(selection).cloned() {
                self.sender
                    .send(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
                        graph.clone(),
                    )))
                    .unwrap();
                self.app_state.graphs.set_active(graph)
            }
        }

        for _press in icon_button(IconName::GRAPH, &self.fonts)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .wh([32., 32.0])
            .left(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.graph_add, ui)
        {
            self.sender
                .send(Lang::UserGraphEvent(UserGraphEvent::AddGraph(
                    "untitled".to_string(),
                )))
                .unwrap()
        }
    }

    fn node_graph(&mut self, ui: &mut UiCell) {
        use super::graph::*;
        for event in Graph::new(&self.app_state.graphs)
            .parent(self.ids.node_graph_canvas)
            .wh_of(self.ids.node_graph_canvas)
            .middle()
            .set(self.ids.node_graph, ui)
        {
            match event {
                Event::NodeDrag(idx, x, y) => {
                    let mut node = self.app_state.graphs.node_weight_mut(idx).unwrap();
                    node.position[0] += x;
                    node.position[1] += y;

                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::PositionNode(
                            node.resource.clone(),
                            (node.position[0], node.position[1]),
                        )))
                        .unwrap();
                }
                Event::ConnectionDrawn(from, from_socket, to, to_socket) => {
                    let from_res = self
                        .app_state
                        .graphs
                        .node_weight(from)
                        .unwrap()
                        .resource
                        .extend_fragment(&from_socket);
                    let to_res = self
                        .app_state
                        .graphs
                        .node_weight(to)
                        .unwrap()
                        .resource
                        .extend_fragment(&to_socket);
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::ConnectSockets(
                            from_res, to_res,
                        )))
                        .unwrap();
                }
                Event::NodeDelete(idx) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::RemoveNode(
                            self.app_state
                                .graphs
                                .node_weight(idx)
                                .unwrap()
                                .resource
                                .clone(),
                        )))
                        .unwrap();
                }
                Event::SocketClear(idx, socket) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(
                            self.app_state
                                .graphs
                                .node_weight(idx)
                                .unwrap()
                                .resource
                                .extend_fragment(&socket),
                        )))
                        .unwrap();
                }
                Event::ActiveElement(idx) => {
                    self.app_state.active_element = Some(idx);
                }
                Event::AddModal => {
                    self.app_state.add_modal = true;
                }
            }
        }

        if self.app_state.add_modal {
            use super::modal;

            let operators = crate::lang::AtomicOperator::all_default();

            match modal::Modal::new(
                widget::List::flow_down(operators.len())
                    .item_size(50.0)
                    .scrollbar_on_top(),
            )
            .wh_of(self.ids.node_graph_canvas)
            .middle_of(self.ids.node_graph_canvas)
            .graphics_for(self.ids.node_graph_canvas)
            .set(self.ids.add_modal, ui)
            {
                modal::Event::ChildEvent((mut items, scrollbar)) => {
                    while let Some(item) = items.next(ui) {
                        let i = item.i;
                        let label = operators[i].title();
                        let toggle = widget::Button::new()
                            .label(&label)
                            .label_color(conrod_core::color::WHITE)
                            .label_font_size(12)
                            .color(conrod_core::color::LIGHT_CHARCOAL);
                        for _press in item.set(toggle, ui) {
                            self.app_state.add_modal = false;

                            self.sender
                                .send(Lang::UserNodeEvent(UserNodeEvent::NewNode(
                                    self.app_state.graphs.get_active().clone(),
                                    Operator::AtomicOperator(operators[i].clone()),
                                )))
                                .unwrap();
                        }
                    }

                    if let Some(s) = scrollbar {
                        s.set(ui)
                    }
                }
                modal::Event::Hide => {
                    self.app_state.add_modal = false;
                }
            }
        }
    }

    // FIXME: Render View shows nothing in release builds
    fn render_view(&mut self, ui: &mut UiCell) {
        use super::renderview::*;

        let renderer_id = self.ids.render_view.index() as u64;

        // If there is a known render image, create a render view for it
        match self.app_state.render_image {
            Some(render_image) => {
                let rv = RenderView::new(render_image, self.app_state.monitor_resolution)
                    .parent(self.ids.drawing_canvas)
                    .wh_of(self.ids.drawing_canvas)
                    .middle()
                    .set(self.ids.render_view, ui);

                // The widget itself does not communicate with the backend. Process
                // events here
                match rv {
                    Some(Event::Resized(w, h)) => self
                        .sender
                        .send(Lang::UIEvent(UIEvent::RendererResize(renderer_id, w, h)))
                        .unwrap(),
                    Some(Event::Rotate(x, y)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::Rotate(
                            renderer_id,
                            x,
                            y,
                        )))
                        .unwrap(),
                    Some(Event::Pan(x, y)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::Pan(
                            renderer_id,
                            x,
                            y,
                        )))
                        .unwrap(),
                    Some(Event::LightPan(x, y)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::LightMove(
                            renderer_id,
                            x,
                            y,
                        )))
                        .unwrap(),
                    Some(Event::Zoom(delta)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::Zoom(
                            renderer_id,
                            delta,
                        )))
                        .unwrap(),
                    _ => {}
                }
            }
            None => {
                // Otherwise create one by notifying the render component
                let [w, h] = ui.wh_of(self.ids.drawing_canvas).unwrap();
                self.sender
                    .send(Lang::UIEvent(UIEvent::RendererRequested(
                        renderer_id,
                        (
                            self.app_state.monitor_resolution.0,
                            self.app_state.monitor_resolution.1,
                        ),
                        (w as u32, h as u32),
                        RendererType::Renderer3D,
                    )))
                    .expect("Error contacting renderer backend");
            }
        }
    }

    fn parameter_section(&mut self, ui: &mut UiCell) {
        use super::param_box::*;

        if let Some((description, resource)) = self.app_state.active_parameters() {
            for ev in ParamBox::new(description, resource, &self.fonts)
                .parent(self.ids.parameter_canvas)
                .w_of(self.ids.parameter_canvas)
                .mid_top()
                .set(self.ids.node_param_box, ui)
            {
                let resp = match ev {
                    Event::ChangeParameter(event) => event,
                    Event::ExposeParameter(field, name, control) => Lang::UserGraphEvent(
                        UserGraphEvent::ExposeParameter(resource.clone(), field, name, control),
                    ),
                    Event::ConcealParameter(field) => Lang::UserGraphEvent(
                        UserGraphEvent::ConcealParameter(resource.clone(), field),
                    ),
                };

                self.sender.send(resp).unwrap();
            }
        }
    }

    fn graph_section(&mut self, ui: &mut UiCell) {
        use super::table;

        // table::Table::new(&mut [ExposedParameter { field: "Hello World".to_string() }])
        //     .parent(self.ids.graph_settings_canvas)
        //     .mid_top()
        //     .set(self.ids.exposed_param_list, ui);
    }

    fn surface_section(&mut self, ui: &mut UiCell) {}
}
