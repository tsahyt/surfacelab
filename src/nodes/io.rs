use crate::lang::{
    ExportSpec, GraphEvent, Lang, LayersEvent, OperatorSize, Resource, SurfaceEvent, UserGraphEvent,
};
use crate::nodes::{LinearizationMode, ManagedNodeCollection, NodeCollection, NodeManager};
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

/// Struct defining a .surf file.
#[derive(Debug, Serialize, Deserialize)]
struct NodeData<'a> {
    parent_size: u32,
    export_size: OperatorSize,
    export_specs: Cow<'a, Vec<ExportSpec>>,
    graphs: Cow<'a, HashMap<String, ManagedNodeCollection>>,
}

impl NodeManager {
    /// Serialize contained data into plain old data
    pub fn serialize(&self) -> Result<Vec<u8>, serde_cbor::Error> {
        log::info!("Serializing node data");
        let surf = NodeData {
            parent_size: self.parent_size,
            export_size: self.export_size,
            export_specs: Cow::Borrowed(&self.export_specs),
            graphs: Cow::Borrowed(&self.graphs),
        };

        serde_cbor::ser::to_vec_packed(&surf)
    }

    /// Deserialize plain old data into self
    pub fn deserialize(&mut self, data: &[u8]) -> Result<Vec<Lang>, serde_cbor::Error> {
        log::info!("Deserializing node data");
        let node_data: NodeData<'_> = serde_cbor::de::from_slice(data)?;

        // Rebuilding internal structures
        self.graphs = node_data.graphs.into_owned();
        self.export_specs = node_data.export_specs.into_owned();
        self.parent_size = node_data.parent_size;
        self.export_size = node_data.export_size;

        // Rebuild events for all graphs in the node data
        let mut events = Vec::new();

        events.push(Lang::SurfaceEvent(SurfaceEvent::ParentSizeSet(
            self.parent_size,
            false,
        )));
        events.push(Lang::SurfaceEvent(SurfaceEvent::ExportSizeSet(
            self.export_size,
        )));

        for (name, graph) in self.graphs.iter() {
            let res = Resource::graph(&name);
            events.push(match graph {
                ManagedNodeCollection::NodeGraph(_) => {
                    Lang::GraphEvent(GraphEvent::GraphAdded(res.clone()))
                }
                ManagedNodeCollection::LayerStack(l) => Lang::LayersEvent(
                    LayersEvent::LayersAdded(res.clone(), self.parent_size, l.output_resources()),
                ),
            });
            events.append(&mut graph.rebuild_events(self.parent_size));
            if let Some((instrs, last_use)) = graph.linearize(LinearizationMode::TopoSort) {
                events.push(Lang::GraphEvent(GraphEvent::Relinearized(
                    res, instrs, last_use,
                )))
            }
        }

        // Rebuild parameter boxes for node added events
        for ev in events.iter_mut() {
            match ev {
                Lang::GraphEvent(GraphEvent::NodeAdded(res, op, pbox, _, _)) => {
                    *pbox = self.element_param_box(&op, res)
                }
                Lang::LayersEvent(LayersEvent::LayerPushed(res, _, _, op, _, _, pbox, _)) => {
                    *pbox = self.element_param_box(&op, res)
                }
                Lang::LayersEvent(LayersEvent::MaskPushed(_, res, _, op, _, _, pbox, _)) => {
                    *pbox = self.element_param_box(&op, res)
                }
                _ => {}
            }
        }

        // Export Specs
        for spec in self.export_specs.iter() {
            events.push(Lang::SurfaceEvent(
                crate::lang::SurfaceEvent::ExportSpecDeclared(spec.clone()),
            ));
        }

        // Finally make sure base is picked
        events.push(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
            Resource::graph("base"),
        )));

        Ok(events)
    }
}
