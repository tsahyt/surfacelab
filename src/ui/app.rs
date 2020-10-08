use crate::lang::resource as r;
use crate::{broker::BrokerSender, lang::*};
use conrod_core::*;
use dialog::{DialogBox, FileSelection, FileSelectionMode};
use std::collections::HashMap;

const PANEL_COLOR: Color = color::DARK_CHARCOAL;
const PANEL_GAP: Scalar = 0.5;

// TODO: Unify margins and paddings somehow in UI

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
        export_surface,
        graph_selector,
        graph_add,

        // Main Views
        node_graph,
        render_view,
        add_modal,
        render_modal,

        // Render Modal
        render_params,

        // Parameter Area
        node_param_box,
        graph_param_box,
        surface_param_box,
        exposed_param_title,
        exposed_param_list,

        // Exporting
        export_label,
        export_add,
        export_list,
    }
);

#[derive(Debug, Clone)]
pub struct Graph {
    graph: super::graph::NodeGraph,
    resources: HashMap<Resource<r::Node>, petgraph::graph::NodeIndex>,
    exposed_parameters: Vec<(String, GraphParameter)>,
    param_box: ParamBoxDescription<GraphField>,
}

impl Graph {
    pub fn new(name: &str) -> Self {
        Self {
            graph: petgraph::Graph::new(),
            resources: HashMap::new(),
            exposed_parameters: Vec::new(),
            param_box: ParamBoxDescription::graph_parameters(name),
        }
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new("base")
    }
}

#[derive(Debug)]
pub struct Graphs {
    graphs: HashMap<Resource<r::Graph>, Graph>,
    active_graph: Graph,
    active_resource: Resource<r::Graph>,
}

impl Default for Graphs {
    fn default() -> Self {
        Self::new()
    }
}

impl Graphs {
    pub fn new() -> Self {
        Graphs {
            graphs: HashMap::new(),
            active_graph: Graph::default(),
            active_resource: Resource::graph("base", None),
        }
    }

    pub fn set_active(&mut self, graph: Resource<r::Graph>) {
        self.graphs
            .insert(self.active_resource.clone(), self.active_graph.clone());
        self.active_resource = graph;
        self.active_graph = self.graphs.remove(&self.active_resource).unwrap();
    }

    pub fn rename_graph(&mut self, from: &Resource<r::Graph>, to: &Resource<r::Graph>) {
        fn update(target: &mut Graph, to: &Resource<r::Graph>) {
            target.param_box.categories[0].parameters[0].control = Control::Entry {
                value: to.file().unwrap().to_string(),
            };
            for gp in target.exposed_parameters.iter_mut().map(|x| &mut x.1) {
                gp.parameter.set_graph(to.path());
            }
            for (mut res, idx) in target.resources.drain().collect::<Vec<_>>() {
                res.set_graph(to.path());
                target.resources.insert(res.clone(), idx);
                target.graph.node_weight_mut(idx).unwrap().resource = res;
            }
        }

        if &self.active_resource == from {
            self.active_resource = to.clone();
            update(&mut self.active_graph, to);
        } else if let Some(mut graph) = self.graphs.remove(from) {
            update(&mut graph, to);
            self.graphs.insert(to.clone(), graph);
        }
    }

    pub fn get_active(&self) -> &Resource<r::Graph> {
        &self.active_resource
    }

    pub fn insert_index(&mut self, resource: Resource<r::Node>, index: petgraph::graph::NodeIndex) {
        self.active_graph.resources.insert(resource, index);
    }

    pub fn remove_index(&mut self, resource: &Resource<r::Node>) {
        self.active_graph.resources.remove(resource);
    }

    pub fn clear_all(&mut self) {
        self.active_graph = Graph::default();
        self.graphs.clear();
    }

    pub fn add_graph(&mut self, graph: Resource<r::Graph>) {
        self.graphs
            .insert(graph.clone(), Graph::new(graph.file().unwrap()));
    }

    /// Get a list of graph names for displaying
    pub fn list_graph_names(&self) -> Vec<&str> {
        std::iter::once(self.active_resource.file().unwrap())
            .chain(self.graphs.keys().map(|k| k.file().unwrap()))
            .collect()
    }

    /// Get a reference to the resource denominating the graph at the given
    /// index. This index refers to the ordering returned by `list_graph_names`.
    pub fn get_graph_resource(&self, index: usize) -> Option<&Resource<r::Graph>> {
        std::iter::once(&self.active_resource)
            .chain(self.graphs.keys())
            .nth(index)
    }

    /// Get a slice of the exposed graph parameters of the currently active
    /// graph.
    pub fn get_exposed_parameters_mut(&mut self) -> &mut [(String, GraphParameter)] {
        &mut self.active_graph.exposed_parameters
    }

    pub fn get_graph_parameters_mut(&mut self) -> &mut ParamBoxDescription<GraphField> {
        &mut self.active_graph.param_box
    }

    fn target_graph_from_node(&mut self, node: &Resource<r::Node>) -> Option<&mut Graph> {
        let graph_name = node.directory().unwrap();
        let graph_res = Resource::graph(graph_name, None);

        if self.active_resource == graph_res {
            Some(&mut self.active_graph)
        } else {
            self.graphs.get_mut(&graph_res)
        }
    }

    fn target_graph_from_graph(&mut self, graph_res: &Resource<r::Graph>) -> Option<&mut Graph> {
        if &self.active_resource == graph_res {
            Some(&mut self.active_graph)
        } else {
            self.graphs.get_mut(&graph_res)
        }
    }

    pub fn add_node(&mut self, node: super::graph::NodeData) {
        let node_res = node.resource.clone();

        if let Some(target) = self.target_graph_from_node(&node_res) {
            let idx = target.graph.add_node(node);
            target.resources.insert(node_res, idx);
        }
    }

    pub fn connect_sockets(&mut self, from: &Resource<r::Socket>, to: &Resource<r::Socket>) {
        let from_node = from.socket_node();
        if let Some(target) = self.target_graph_from_node(&from_node) {
            let from_idx = target.resources.get(&from_node).unwrap();
            let to_idx = target.resources.get(&to.socket_node()).unwrap();
            target.graph.add_edge(
                *from_idx,
                *to_idx,
                (
                    from.fragment().unwrap().to_string(),
                    to.fragment().unwrap().to_string(),
                ),
            );
        }
    }

    pub fn disconnect_sockets(&mut self, from: &Resource<r::Socket>, to: &Resource<r::Socket>) {
        let from_node = from.socket_node();
        if let Some(target) = self.target_graph_from_node(&from_node) {
            use petgraph::visit::EdgeRef;

            let from_idx = target.resources.get(&from_node).unwrap();
            let to_idx = target.resources.get(&to.socket_node()).unwrap();

            // Assuming that there's only ever one edge connecting two sockets.
            if let Some(e) = target
                .graph
                .edges_connecting(*from_idx, *to_idx)
                .filter(|e| {
                    (e.weight().0.as_str(), e.weight().1.as_str())
                        == (from.fragment().unwrap(), to.fragment().unwrap())
                })
                .map(|e| e.id())
                .next()
            {
                target.graph.remove_edge(e);
            }
        }
    }

    // FIXME: removal renumbers the nodes, so we need to update the indexing as well
    pub fn remove_node(&mut self, node: &Resource<r::Node>) {
        if let Some(target) = self.target_graph_from_node(&node) {
            if let Some(idx) = target.resources.remove(node) {
                target.graph.remove_node(idx);
            }
        }
    }

    pub fn monomorphize_socket(&mut self, socket: &Resource<r::Socket>, ty: ImageType) {
        let node = socket.socket_node();

        if let Some(target) = self.target_graph_from_node(&node) {
            let idx = target.resources.get(&node).unwrap();
            let node = target.graph.node_weight_mut(*idx).unwrap();
            let var = type_variable_from_socket_iter(
                node.inputs.iter().chain(node.outputs.iter()),
                socket.fragment().unwrap(),
            )
            .unwrap();
            node.set_type_variable(var, Some(ty))
        }
    }

    pub fn demonomorphize_socket(&mut self, socket: &Resource<r::Socket>) {
        let node = socket.socket_node();

        if let Some(target) = self.target_graph_from_node(&node) {
            let idx = target.resources.get(&node).unwrap();
            let node = target.graph.node_weight_mut(*idx).unwrap();
            let var = type_variable_from_socket_iter(
                node.inputs.iter().chain(node.outputs.iter()),
                socket.fragment().unwrap(),
            )
            .unwrap();
            node.set_type_variable(var, None)
        }
    }

    /// Rename a node. Note that this does *not* support moving a node from one
    /// graph to another!
    pub fn rename_node(&mut self, from: &Resource<r::Node>, to: &Resource<r::Node>) {
        if let Some(target) = self.target_graph_from_node(&from) {
            if let Some(idx) = target.resources.get(from).copied() {
                let node = target.graph.node_weight_mut(idx).unwrap();
                node.resource = to.clone();
                target.resources.insert(to.clone(), idx);
                target.resources.remove(from);
            }
        }
    }

    pub fn parameter_exposed(&mut self, graph: &Resource<r::Graph>, param: GraphParameter) {
        if let Some(target) = self.target_graph_from_graph(graph) {
            target
                .exposed_parameters
                .push((param.graph_field.clone(), param));
        }
    }

    pub fn parameter_concealed(&mut self, graph: &Resource<r::Graph>, field: &str) {
        if let Some(target) = self.target_graph_from_graph(graph) {
            target.exposed_parameters.remove(
                target
                    .exposed_parameters
                    .iter()
                    .position(|x| x.0 == field)
                    .expect("Tried to remove unknown parameter"),
            );
        }
    }

    pub fn update_complex_operator(
        &mut self,
        node: &Resource<r::Node>,
        op: &ComplexOperator,
        pbox: &ParamBoxDescription<Field>,
    ) {
        if let Some(target) = self.target_graph_from_node(node) {
            if let Some(idx) = target.resources.get(node) {
                let node_weight = target.graph.node_weight_mut(*idx).unwrap();
                node_weight.update(Operator::ComplexOperator(op.clone()), pbox.clone());
            }
        }
    }

    pub fn register_thumbnail(&mut self, node: &Resource<r::Node>, thumbnail: image::Id) {
        if let Some(node) = self.target_graph_from_node(node).and_then(|target| {
            let idx = target.resources.get(node)?;
            target.graph.node_weight_mut(*idx)
        }) {
            node.thumbnail = Some(thumbnail);
        }
    }

    pub fn unregister_thumbnail(&mut self, node: &Resource<r::Node>) -> Option<image::Id> {
        let mut old_id = None;

        if let Some(node) = self.target_graph_from_node(node).and_then(|target| {
            let idx = target.resources.get(node)?;
            target.graph.node_weight_mut(*idx)
        }) {
            old_id = node.thumbnail;
            node.thumbnail = None;
        }

        old_id
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
    graphs: Graphs,
    active_element: Option<petgraph::graph::NodeIndex>,
    render_image: Option<image::Id>,
    monitor_resolution: (u32, u32),

    add_modal: Option<Point>,
    render_modal: bool,

    render_params: ParamBoxDescription<RenderField>,
    surface_params: ParamBoxDescription<SurfaceField>,

    registered_operators: Vec<Operator>,
    addable_operators: Vec<Operator>,
    registered_sockets: Vec<super::export_row::RegisteredSocket>,
    export_entries: Vec<(String, ExportSpec)>,
}

impl App {
    pub fn new(monitor_size: (u32, u32)) -> Self {
        Self {
            graphs: Graphs::new(),
            active_element: None,
            render_image: None,
            monitor_resolution: (monitor_size.0, monitor_size.1),
            add_modal: None,
            render_modal: false,
            render_params: ParamBoxDescription::render_parameters(),
            surface_params: ParamBoxDescription::surface_parameters(),
            registered_operators: AtomicOperator::all_default()
                .iter()
                .map(|x| Operator::from(x.clone()))
                .collect(),
            addable_operators: AtomicOperator::all_default()
                .iter()
                .map(|x| Operator::from(x.clone()))
                .collect(),
            registered_sockets: Vec::new(),
            export_entries: Vec::new(),
        }
    }

    pub fn active_parameters(
        &mut self,
    ) -> Option<(&mut ParamBoxDescription<MessageWriters>, &Resource<r::Node>)> {
        let ae = self.active_element?;
        let node = self.graphs.node_weight_mut(ae)?;
        Some((&mut node.param_box, &node.resource))
    }

    pub fn add_export_entry(&mut self) {
        if let Some(default) = self.registered_sockets.last() {
            self.export_entries
                .push(("unnamed".to_owned(), ExportSpec::Grayscale(default.spec.clone())));
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
                if let Some(view) = view.clone().to::<B>() {
                    if let Some(img) = renderer.create_image(
                        view,
                        self.app_state.monitor_resolution.0,
                        self.app_state.monitor_resolution.1,
                    ) {
                        let id = self.image_map.insert(img);
                        self.app_state.render_image = Some(id);
                    }
                }
            }
            Lang::RenderEvent(RenderEvent::RendererRedrawn(_id)) => {
                ui.needs_redraw();
            }
            Lang::ComputeEvent(ComputeEvent::ThumbnailCreated(res, thmb)) => {
                if let Some(t) = thmb.clone().to::<B>() {
                    if let Some(img) = renderer.create_image(t, 128, 128) {
                        let id = self.image_map.insert(img);
                        self.app_state.graphs.register_thumbnail(&res, id);
                    }
                }
            }
            Lang::ComputeEvent(ComputeEvent::ThumbnailDestroyed(res)) => {
                if let Some(id) = self.app_state.graphs.unregister_thumbnail(&res) {
                    self.image_map.remove(id);
                }
            }
            Lang::ComputeEvent(ComputeEvent::SocketCreated(res, ty)) => match ty {
                ImageType::Grayscale => {
                    self.app_state.registered_sockets.push(
                        super::export_row::RegisteredSocket::new((res.clone(), ImageChannel::R)),
                    );
                }
                ImageType::Rgb => {
                    self.app_state.registered_sockets.push(
                        super::export_row::RegisteredSocket::new((res.clone(), ImageChannel::R)),
                    );
                    self.app_state.registered_sockets.push(
                        super::export_row::RegisteredSocket::new((res.clone(), ImageChannel::G)),
                    );
                    self.app_state.registered_sockets.push(
                        super::export_row::RegisteredSocket::new((res.clone(), ImageChannel::B)),
                    );
                }
            },
            Lang::ComputeEvent(ComputeEvent::SocketDestroyed(res)) => {
                // self.app_state.registered_sockets.remove(
                //     self.app_state
                //         .registered_sockets
                //         .iter()
                //         .position(|x| x == res)
                //         .expect("Trying to remove unknown registered socket"),
                // );
            }
            Lang::GraphEvent(ev) => self.handle_graph_event(ev),
            _ => {}
        }
    }

    fn handle_graph_event(&mut self, event: &GraphEvent) {
        match event {
            GraphEvent::GraphAdded(res) => {
                self.app_state.graphs.add_graph(res.clone());
                self.app_state
                    .registered_operators
                    .push(Operator::ComplexOperator(ComplexOperator::new(res.clone())));
            }
            GraphEvent::GraphRenamed(from, to) => {
                self.app_state.graphs.rename_graph(from, to);
                let old_op = Operator::ComplexOperator(ComplexOperator::new(from.clone()));
                self.app_state.registered_operators.remove(
                    self.app_state
                        .registered_operators
                        .iter()
                        .position(|x| x == &old_op)
                        .expect("Missing old operator"),
                );
                self.app_state
                    .registered_operators
                    .push(Operator::ComplexOperator(ComplexOperator::new(to.clone())));
            }
            GraphEvent::NodeAdded(res, op, pbox, position, _size) => {
                self.app_state.graphs.add_node(super::graph::NodeData::new(
                    res.clone(),
                    position.map(|(x, y)| [x, y]),
                    &op,
                    pbox.clone(),
                ));
            }
            GraphEvent::NodeRemoved(res) => {
                self.app_state.graphs.remove_node(res);
            }
            GraphEvent::NodeRenamed(from, to) => {
                self.app_state.graphs.rename_node(from, to);
            }
            GraphEvent::ComplexOperatorUpdated(node, op, pbox) => {
                self.app_state
                    .graphs
                    .update_complex_operator(node, op, pbox);
            }
            GraphEvent::ConnectedSockets(from, to) => {
                self.app_state.graphs.connect_sockets(from, to)
            }
            GraphEvent::DisconnectedSockets(from, to) => {
                self.app_state.graphs.disconnect_sockets(from, to)
            }
            GraphEvent::SocketMonomorphized(socket, ty) => {
                self.app_state.graphs.monomorphize_socket(socket, *ty)
            }
            GraphEvent::SocketDemonomorphized(socket) => {
                self.app_state.graphs.demonomorphize_socket(socket)
            }
            GraphEvent::Cleared => {
                self.app_state.graphs.clear_all();
            }
            GraphEvent::ParameterExposed(graph, param) => {
                self.app_state
                    .graphs
                    .parameter_exposed(graph, param.clone());
            }
            GraphEvent::ParameterConcealed(graph, field) => {
                self.app_state.graphs.parameter_concealed(graph, field);
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

        for _press in icon_button(IconName::FOLDER_PLUS, self.fonts.icon_font)
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

        for _press in icon_button(IconName::FOLDER_OPEN, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.open_surface, ui)
        {
            if let Ok(Some(path)) = FileSelection::new("Select a surface file")
                .title("Open Surface")
                .mode(FileSelectionMode::Open)
                .show()
            {
                self.sender
                    .send(Lang::UserIOEvent(UserIOEvent::OpenSurface(
                        std::path::PathBuf::from(path),
                    )))
                    .unwrap();
                self.app_state.graphs.clear_all();
            }
        }

        for _press in icon_button(IconName::CONTENT_SAVE, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.save_surface, ui)
        {
            if let Ok(Some(path)) = FileSelection::new("Select a surface file")
                .title("Save Surface")
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

        for _press in icon_button(IconName::EXPORT, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .wh([32., 32.0])
            .right(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.export_surface, ui)
        {
            if let Ok(Some(path)) = FileSelection::new("Select a base name")
                .title("Export Surface")
                .mode(FileSelectionMode::Save)
                .show()
            {
                for export_entry in self.app_state.export_entries.iter() {
                    let mut e_path = std::path::PathBuf::from(&path);
                    e_path.set_file_name(format!(
                        "{}_{}.png",
                        e_path.file_name().unwrap().to_str().unwrap(),
                        export_entry.0
                    ));
                    self.sender
                        .send(Lang::UserIOEvent(UserIOEvent::ExportImage(
                            export_entry.1.clone(),
                            1024,
                            e_path,
                        )))
                        .unwrap();
                }
            }
        }

        if let Some(selection) =
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
                self.app_state.graphs.set_active(graph);
                self.app_state.addable_operators = self
                    .app_state
                    .registered_operators
                    .iter()
                    .filter(|o| !o.is_graph(self.app_state.graphs.get_active()))
                    .cloned()
                    .collect();
            }
        }

        for _press in icon_button(IconName::GRAPH, self.fonts.icon_font)
            .label_font_size(14)
            .label_color(color::WHITE)
            .color(color::DARK_CHARCOAL)
            .wh([32., 32.0])
            .left(8.0)
            .parent(self.ids.top_bar_canvas)
            .set(self.ids.graph_add, ui)
        {
            self.sender
                .send(Lang::UserGraphEvent(UserGraphEvent::AddGraph))
                .unwrap()
        }
    }

    fn node_graph(&mut self, ui: &mut UiCell) {
        use super::graph;
        for event in graph::Graph::new(&self.app_state.graphs)
            .parent(self.ids.node_graph_canvas)
            .wh_of(self.ids.node_graph_canvas)
            .middle()
            .set(self.ids.node_graph, ui)
        {
            match event {
                graph::Event::NodeDrag(idx, x, y) => {
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
                graph::Event::ConnectionDrawn(from, from_socket, to, to_socket) => {
                    let from_res = self
                        .app_state
                        .graphs
                        .node_weight(from)
                        .unwrap()
                        .resource
                        .node_socket(&from_socket);
                    let to_res = self
                        .app_state
                        .graphs
                        .node_weight(to)
                        .unwrap()
                        .resource
                        .node_socket(&to_socket);
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::ConnectSockets(
                            from_res, to_res,
                        )))
                        .unwrap();
                }
                graph::Event::NodeDelete(idx) => {
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
                graph::Event::SocketClear(idx, socket) => {
                    self.sender
                        .send(Lang::UserNodeEvent(UserNodeEvent::DisconnectSinkSocket(
                            self.app_state
                                .graphs
                                .node_weight(idx)
                                .unwrap()
                                .resource
                                .node_socket(&socket),
                        )))
                        .unwrap();
                }
                graph::Event::ActiveElement(idx) => {
                    self.app_state.active_element = Some(idx);
                }
                graph::Event::AddModal(pt) => {
                    self.app_state.add_modal = Some(pt);
                }
            }
        }

        if let Some(insertion_pt) = self.app_state.add_modal {
            use super::modal;

            let operators = &self.app_state.addable_operators;

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
                modal::Event::ChildEvent(((mut items, scrollbar), _)) => {
                    while let Some(item) = items.next(ui) {
                        let i = item.i;
                        let label = operators[i].title();
                        let button = widget::Button::new()
                            .label(&label)
                            .label_color(conrod_core::color::WHITE)
                            .label_font_size(12)
                            .color(conrod_core::color::LIGHT_CHARCOAL);
                        for _press in item.set(button, ui) {
                            self.app_state.add_modal = None;

                            self.sender
                                .send(Lang::UserNodeEvent(UserNodeEvent::NewNode(
                                    self.app_state.graphs.get_active().clone(),
                                    operators[i].clone(),
                                    (insertion_pt[0], insertion_pt[1]),
                                )))
                                .unwrap();
                        }
                    }

                    if let Some(s) = scrollbar {
                        s.set(ui)
                    }
                }
                modal::Event::Hide => {
                    self.app_state.add_modal = None;
                }
            }
        }
    }

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
                    Some(Event::OpenModal) => {
                        self.app_state.render_modal = true;
                    }
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

        if self.app_state.render_modal {
            use super::modal;
            use super::param_box;

            match modal::Modal::canvas()
                .wh_of(self.ids.drawing_canvas)
                .middle_of(self.ids.drawing_canvas)
                .graphics_for(self.ids.drawing_canvas)
                .set(self.ids.render_modal, ui)
            {
                modal::Event::ChildEvent((_, id)) => {
                    for ev in
                        param_box::ParamBox::new(&mut self.app_state.render_params, &renderer_id)
                            .parent(id)
                            .w_of(id)
                            .mid_top()
                            .icon_font(self.fonts.icon_font)
                            .set(self.ids.render_params, ui)
                    {
                        if let param_box::Event::ChangeParameter(lang) = ev {
                            self.sender.send(lang).unwrap()
                        }
                    }
                }
                modal::Event::Hide => {
                    self.app_state.render_modal = false;
                }
            }
        }
    }

    fn parameter_section(&mut self, ui: &mut UiCell) {
        use super::param_box::*;

        if let Some((description, resource)) = self.app_state.active_parameters() {
            for ev in ParamBox::new(description, resource)
                .parent(self.ids.parameter_canvas)
                .w_of(self.ids.parameter_canvas)
                .mid_top()
                .icon_font(self.fonts.icon_font)
                .set(self.ids.node_param_box, ui)
            {
                let resp = match ev {
                    Event::ChangeParameter(event) => event,
                    Event::ExposeParameter(field, name, control) => Lang::UserGraphEvent({
                        let p_res = resource.clone().node_parameter(&field);
                        UserGraphEvent::ExposeParameter(p_res, field, name, control)
                    }),
                    Event::ConcealParameter(field) => Lang::UserGraphEvent(
                        UserGraphEvent::ConcealParameter(resource.clone().node_graph(), field),
                    ),
                };

                self.sender.send(resp).unwrap();
            }
        }
    }

    fn graph_section(&mut self, ui: &mut UiCell) {
        use super::exposed_param_row;
        use super::param_box;

        let active_graph = self.app_state.graphs.get_active().clone();

        for ev in param_box::ParamBox::new(
            self.app_state.graphs.get_graph_parameters_mut(),
            &active_graph,
        )
        .parent(self.ids.graph_settings_canvas)
        .w_of(self.ids.graph_settings_canvas)
        .mid_top()
        .set(self.ids.graph_param_box, ui)
        {
            if let param_box::Event::ChangeParameter(event) = ev {
                self.sender.send(event).unwrap()
            }
        }

        widget::Text::new("Exposed Parameters")
            .parent(self.ids.graph_settings_canvas)
            .color(color::WHITE)
            .font_size(12)
            .mid_top_with_margin(96.0)
            .set(self.ids.exposed_param_title, ui);

        let exposed_params = self.app_state.graphs.get_exposed_parameters_mut();

        let (mut rows, scrollbar) = widget::List::flow_down(exposed_params.len())
            .parent(self.ids.graph_settings_canvas)
            .item_size(160.0)
            .padded_w_of(self.ids.graph_settings_canvas, 8.0)
            .h(320.0)
            .mid_top_with_margin(112.0)
            .scrollbar_on_top()
            .set(self.ids.exposed_param_list, ui);

        while let Some(row) = rows.next(ui) {
            let widget = exposed_param_row::ExposedParamRow::new(&mut exposed_params[row.i].1)
                .icon_font(self.fonts.icon_font);

            if let Some(ev) = row.set(widget, ui) {
                match ev {
                    exposed_param_row::Event::ConcealParameter => {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::ConcealParameter(
                                active_graph.clone(),
                                exposed_params[row.i].0.clone(),
                            )))
                            .unwrap();
                    }
                    exposed_param_row::Event::UpdateTitle => {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::RetitleParameter(
                                active_graph.clone(),
                                exposed_params[row.i].0.clone(),
                                exposed_params[row.i].1.title.to_owned(),
                            )))
                            .unwrap();
                    }
                    exposed_param_row::Event::UpdateField => {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::RefieldParameter(
                                active_graph.clone(),
                                exposed_params[row.i].0.clone(),
                                exposed_params[row.i].1.graph_field.to_owned(),
                            )))
                            .unwrap();
                    }
                }
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }
    }

    fn surface_section(&mut self, ui: &mut UiCell) {
        use super::{export_row, param_box, util::*};

        for ev in param_box::ParamBox::new(&mut self.app_state.surface_params, &())
            .parent(self.ids.surface_settings_canvas)
            .w_of(self.ids.surface_settings_canvas)
            .mid_top()
            .set(self.ids.surface_param_box, ui)
        {
            if let param_box::Event::ChangeParameter(event) = ev {
                self.sender.send(event).unwrap()
            }
        }

        widget::Text::new("Export Specification")
            .parent(self.ids.surface_settings_canvas)
            .mid_top_with_margin(96.0)
            .color(color::WHITE)
            .font_size(12)
            .set(self.ids.export_label, ui);

        for _ev in icon_button(IconName::PLUS, self.fonts.icon_font)
            .parent(self.ids.surface_settings_canvas)
            .top_right_with_margins(96.0, 16.0)
            .color(color::DARK_CHARCOAL)
            .label_color(color::WHITE)
            .label_font_size(12)
            .wh([20.0, 16.0])
            .set(self.ids.export_add, ui)
        {
            self.app_state.add_export_entry();
        }

        let (mut rows, scrollbar) = widget::List::flow_down(self.app_state.export_entries.len())
            .parent(self.ids.surface_settings_canvas)
            .padded_w_of(self.ids.surface_settings_canvas, 8.0)
            .h(320.0)
            .mid_top_with_margin(112.0)
            .scrollbar_on_top()
            .set(self.ids.export_list, ui);

        while let Some(row) = rows.next(ui) {
            let widget = export_row::ExportRow::new(
                &self.app_state.export_entries[row.i],
                &self.app_state.registered_sockets,
            );
            match row.set(widget, ui) {
                Some(export_row::Event::ChangeToRGB) => {
                    self.app_state.export_entries[row.i].1 = self.app_state.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Rgb)
                        .alpha(false);
                }
                Some(export_row::Event::ChangeToRGBA) => {
                    self.app_state.export_entries[row.i].1 = self.app_state.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Rgb)
                        .alpha(true);
                }
                Some(export_row::Event::ChangeToGrayscale) => {
                    self.app_state.export_entries[row.i].1 = self.app_state.export_entries[row.i]
                        .1
                        .clone()
                        .image_type(ImageType::Grayscale);
                }
                Some(export_row::Event::SetChannelR(spec)) => {
                    self.app_state.export_entries[row.i].1.set_r(spec);
                }
                Some(export_row::Event::SetChannelG(spec)) => {
                    self.app_state.export_entries[row.i].1.set_g(spec);
                }
                Some(export_row::Event::SetChannelB(spec)) => {
                    self.app_state.export_entries[row.i].1.set_b(spec);
                }
                Some(export_row::Event::SetChannelA(spec)) => {
                    self.app_state.export_entries[row.i].1.set_a(spec);
                }
                Some(export_row::Event::Rename(new)) => {
                    self.app_state.export_entries[row.i].0 = new;
                }
                None => {}
            }
        }

        if let Some(s) = scrollbar {
            s.set(ui);
        }
    }
}
