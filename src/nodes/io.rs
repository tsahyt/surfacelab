use super::*;

/// Struct defining a .surf file.
#[derive(Debug, Serialize, Deserialize)]
struct SurfaceFile {
    parent_size: i32,
    node_graph: NodeGraph,
}

impl NodeManager {
    pub fn save_node_graph<P: AsRef<Path> + std::fmt::Debug>(&self, path: P) -> Result<(), String> {
        log::info!("Saving to {:?}", path);
        let surf = SurfaceFile {
            parent_size: self.parent_size,
            node_graph: self.node_graph.to_owned(), // TODO: make serde work with references
        };

        let output_file = File::create(path).map_err(|_| "Failed to open output file")?;
        serde_cbor::to_writer(output_file, &surf)
            .map_err(|e| format!("Saving failed with {}", e))
    }

    pub fn open_node_graph<P: AsRef<Path> + std::fmt::Debug>(
        &mut self,
        path: P,
    ) -> Result<Vec<lang::Lang>, String> {
        log::info!("Opening from {:?}", path);
        let input_file =
            File::open(path).map_err(|e| format!("Failed to open input file {}", e))?;
        let surf: SurfaceFile = serde_cbor::from_reader(input_file)
            .map_err(|e| format!("Reading failed with {}", e))?;

        // Rebuilding internal structures
        self.node_graph = surf.node_graph;
        self.parent_size = surf.parent_size;
        self.node_indices.clear();
        self.outputs.clear();

        for idx in self.node_graph.node_indices() {
            let node = self.node_graph.node_weight(idx).unwrap();

            self.node_indices.insert(node.resource.clone(), idx);
            if let lang::Operator::Output { .. } = node.operator {
                self.outputs.insert(idx);
            }
        }

        // Accumulate graph events detailing reconstruction
        let mut events = Vec::new();

        for idx in self.node_graph.node_indices() {
            let node = self.node_graph.node_weight(idx).unwrap();
            events.push(lang::Lang::GraphEvent(lang::GraphEvent::NodeAdded(
                node.resource.clone(),
                node.operator.clone(),
                Some(node.position),
                1024,
            )));
        }

        for idx in self.node_graph.edge_indices() {
            let conn = self.node_graph.edge_weight(idx).unwrap();
            let (source_idx, sink_idx) = self.node_graph.edge_endpoints(idx).unwrap();
            events.push(lang::Lang::GraphEvent(lang::GraphEvent::ConnectedSockets(
                self.node_graph
                    .node_weight(source_idx)
                    .unwrap()
                    .resource
                    .extend_fragment(&conn.0),
                self.node_graph
                    .node_weight(sink_idx)
                    .unwrap()
                    .resource
                    .extend_fragment(&conn.1),
            )));
        }

        // Create monomorphization events for all known type variables
        for idx in self.node_graph.node_indices() {
            let node = self.node_graph.node_weight(idx).unwrap();
            for tvar in node.type_variables.iter() {
                for res in node
                    .operator
                    .inputs()
                    .iter()
                    .chain(node.operator.outputs().iter())
                    .filter(|(_, t)| **t == lang::OperatorType::Polymorphic(*tvar.0))
                    .map(|x| node.resource.extend_fragment(x.0))
                {
                    events.push(lang::Lang::GraphEvent(
                        lang::GraphEvent::SocketMonomorphized(res, *tvar.1),
                    ));
                }
            }
        }

        Ok(events)
    }

}
