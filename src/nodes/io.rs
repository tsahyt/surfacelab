use super::{nodegraph, NodeManager};
use crate::lang::{GraphEvent, Lang, Resource, UserGraphEvent};
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

/// Struct defining a .surf file.
#[derive(Debug, Serialize, Deserialize)]
struct SurfaceFile<'a> {
    parent_size: u32,
    graphs: Cow<'a, HashMap<String, nodegraph::NodeGraph>>,
}

impl NodeManager {
    pub fn save_surface<P: AsRef<Path> + std::fmt::Debug>(&self, path: P) -> Result<(), String> {
        log::info!("Saving to {:?}", path);
        let surf = SurfaceFile {
            parent_size: self.parent_size,
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
        self.parent_size = surf.parent_size;

        // Rebuild events for all graphs in the surface file
        let mut events = Vec::new();
        for (name, graph) in self.graphs.iter() {
            let res = Resource::graph(&name, None);
            events.push(Lang::GraphEvent(GraphEvent::GraphAdded(res.clone())));
            events.append(&mut graph.rebuild_events(self.parent_size));
            if let Some(instrs) = graph.linearize() {
                events.push(Lang::GraphEvent(GraphEvent::Relinearized(res, instrs)))
            }
        }

        // Rebuild parameter boxes for node added events
        for ev in events.iter_mut() {
            if let Lang::GraphEvent(GraphEvent::NodeAdded(_, op, pbox, _, _)) = ev {
                *pbox = self.operator_param_box(&op);
            }
        }

        // Finally make sure base is picked
        events.push(Lang::UserGraphEvent(UserGraphEvent::ChangeGraph(
            Resource::graph("base", None),
        )));

        Ok(events)
    }
}
