use crate::lang::{ExportSpec, GraphEvent, Lang, LayersEvent, Resource, UserGraphEvent};
use crate::nodes::{LinearizationMode, NodeCollection, NodeGraph, NodeManager};
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

/// Struct defining a .surf file.
#[derive(Debug, Serialize, Deserialize)]
struct SurfaceFile<'a> {
    parent_size: u32,
    export_specs: Cow<'a, HashMap<String, ExportSpec>>,
    graphs: Cow<'a, HashMap<String, NodeGraph>>,
}

impl NodeManager {
    pub fn save_surface<P: AsRef<Path> + std::fmt::Debug>(&self, path: P) -> Result<(), String> {
        log::info!("Saving to {:?}", path);
        let surf = SurfaceFile {
            parent_size: self.parent_size,
            export_specs: Cow::Borrowed(&self.export_specs),
            graphs: Cow::Borrowed(&self.graphs),
        };

        let output_file = File::create(path).map_err(|_| "Failed to open output file")?;
        serde_cbor::to_writer(output_file, &surf).map_err(|e| format!("Saving failed with {}", e))
    }

    pub fn open_surface<P: AsRef<Path> + std::fmt::Debug>(
        &mut self,
        path: P,
    ) -> Result<Vec<Lang>, String> {
        log::info!("Opening from {:?}", path);
        let input_file =
            File::open(path).map_err(|e| format!("Failed to open input file {}", e))?;
        let surf: SurfaceFile = serde_cbor::from_reader(input_file)
            .map_err(|e| format!("Reading failed with {}", e))?;

        // Rebuilding internal structures
        self.graphs = surf.graphs.into_owned();
        self.export_specs = surf.export_specs.into_owned();
        self.parent_size = surf.parent_size;

        // Rebuild events for all graphs in the surface file
        let mut events = Vec::new();
        for (name, graph) in self.graphs.iter() {
            let res = Resource::graph(&name, None);
            events.push(match graph {
                NodeGraph::NodeGraph(_) => Lang::GraphEvent(GraphEvent::GraphAdded(res.clone())),
                NodeGraph::LayerStack(_) => {
                    Lang::LayersEvent(LayersEvent::LayersAdded(res.clone(), self.parent_size))
                }
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
                Lang::GraphEvent(GraphEvent::NodeAdded(_, op, pbox, _, _)) => {
                    *pbox = self.operator_param_box(&op)
                }
                Lang::LayersEvent(LayersEvent::LayerPushed(res, _, _, op, pbox, _)) => {
                    *pbox = self.element_param_box(&op, res)
                }
                _ => {}
            }
        }

        // Export Specs
        for (name, spec) in self.export_specs.iter() {
            events.push(Lang::SurfaceEvent(
                crate::lang::SurfaceEvent::ExportSpecLoaded(name.clone(), spec.clone()),
            ));
        }

        // Finally make sure base is picked
        events.push(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
            Resource::graph("base", None),
        )));

        Ok(events)
    }
}
