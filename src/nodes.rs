use crate::bus;
use std::collections::HashMap;
use std::thread;

#[repr(C)]
pub struct BlendParameters {
    mix: f32,
}

#[repr(C)]
pub struct PerlinNoiseParameters {
    scale: f32,
    octaves: f32,
}

pub enum Operator {
    Blend(BlendParameters),
    PerlinNoise(PerlinNoiseParameters),
    Image { path: std::path::PathBuf },
    Output { output_type: OutputType },
}

#[derive(PartialEq)]
pub enum ImageType {
    RgbImage,
    RgbaImage,
    Value,
}

#[derive(PartialEq)]
pub enum OutputType {
    Albedo,
    Roughness,
    Normal,
    Displacement,
    Value,
}

pub struct Node {
    pub operator: Operator,
    pub inputs: HashMap<String, ImageType>,
    pub outputs: HashMap<String, ImageType>,
}

impl Node {
    /// A node can be considered a Mask if and only if it has exactly one output
    /// which produces a Value.
    pub fn is_mask(&self) -> bool {
        self.outputs.len() == 1 && self.outputs.iter().all(|(_, x)| *x == ImageType::Value)
    }
}

type NodeGraph = petgraph::graph::Graph<Node, (u8, u8), petgraph::Directed>;

struct NodeManager {
    node_graph: NodeGraph,
}

impl NodeManager {
    pub fn new() -> Self {
        let node_graph = petgraph::graph::Graph::new();
        NodeManager { node_graph }
    }

    pub fn process_event(&mut self, event: bus::Lang) {
        log::debug!("Node Manager processing event {:?}", event);
    }
}

pub fn start_nodes_thread(bus: &bus::Bus) -> thread::JoinHandle<()> {
    let (sender, receiver) = bus.subscribe().unwrap();

    thread::spawn(move || {
        let mut node_mgr = NodeManager::new();

        for event in receiver {
            node_mgr.process_event(event);
        }
    })
}
