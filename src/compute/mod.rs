use crate::{broker, gpu, lang::*};

use strum::IntoEnumIterator;

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

pub mod export;
pub mod external;
pub mod interpreter;
pub mod io;
pub mod shaders;
pub mod sockets;

use sockets::*;

/// Start the compute manager in a thread. There should only be one such thread.
pub fn start_compute_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
    config: &config::Configuration,
) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (sender, receiver, disconnector) = broker.subscribe("compute");
    match gpu::compute::GPUCompute::new(gpu, config.compute_vram_pct) {
        Err(e) => {
            log::error!("Failed to initialize GPU Compute: {:?}", e);
            panic!("Critical Error");
        }
        Ok(gpu) => thread::Builder::new()
            .name("compute".to_string())
            .spawn(move || {
                let mut compute_mgr = ComputeManager::new(gpu);
                for event in receiver {
                    match compute_mgr.process_event(event, &sender) {
                        None => break,
                        _ => {}
                    }
                }

                log::info!("GPU Compute Handler terminating");
                disconnector.disconnect();
            })
            .expect("Failed to spawn compute thread!"),
    }
}

#[derive(Debug)]
pub struct Linearization {
    instructions: Vec<Instruction>,
    use_points: Vec<(Resource<Node>, UsePoint)>,
}

impl Linearization {
    pub fn retention_set_at(&self, step: usize) -> impl Iterator<Item = &Resource<Node>> {
        self.use_points.iter().filter_map(move |(r, up)| {
            if up.last >= step && up.creation <= step {
                Some(r)
            } else {
                None
            }
        })
    }
}

/// The compute manager is responsible for managing the compute component and
/// processing events from the bus relating to that.
struct ComputeManager<B: gpu::Backend> {
    gpu: gpu::compute::GPUCompute<B>,

    /// Sockets contain all the relevant information for individual node outputs
    /// and inputs.
    sockets: Sockets<B>,

    /// Shader library containing all known shaders
    shader_library: shaders::ShaderLibrary<B>,

    /// Storage for external data
    external_data: external::Externals,

    /// Last known linearization of a graph
    linearizations: HashMap<Resource<Graph>, Rc<Linearization>>,

    /// Number of executions, kept for cache invalidation
    seq: u64,

    /// Current system-wide parent size
    parent_size: u32,

    /// A special socket that the user wants to view, with a seq number for when
    /// it was last updated
    view_socket: Option<(Resource<Socket>, u64)>,
}

impl<B> ComputeManager<B>
where
    B: gpu::Backend,
{
    /// Initialize a new compute manager.
    pub fn new(mut gpu: gpu::compute::GPUCompute<B>) -> Self {
        let shader_library = shaders::ShaderLibrary::new(&mut gpu).unwrap();

        ComputeManager {
            gpu,
            sockets: Sockets::new(),
            shader_library,
            external_data: external::Externals::new(),
            linearizations: HashMap::new(),
            seq: 0,
            parent_size: 1024,
            view_socket: None,
        }
    }

    /// Process one event from the application bus
    pub fn process_event(
        &mut self,
        event: Arc<Lang>,
        sender: &broker::BrokerSender<Lang>,
    ) -> Option<()> {
        match &*event {
            Lang::LayersEvent(event) => match event {
                LayersEvent::LayerPushed(res, _, _, _, _, _, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_group_exists(res, *size);

                    // Blend nodes
                    for channel in MaterialChannel::iter() {
                        let mut blend_node = res.clone();
                        let new_name = format!(
                            "{}.blend.{}",
                            blend_node.file().unwrap(),
                            channel.short_name()
                        );
                        blend_node.rename_file(&new_name);
                        self.sockets
                            .ensure_group_exists(&blend_node, self.parent_size);
                    }
                }
                LayersEvent::MaskPushed(_, res, _, _, _, _, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_group_exists(res, *size);

                    let mut blend_node = res.clone();
                    let new_name = format!("{}.blend", blend_node.file().unwrap(),);
                    blend_node.rename_file(&new_name);
                    self.sockets.ensure_group_exists(&blend_node, *size);
                }
                LayersEvent::LayersAdded(_, size, outputs) => {
                    for output in outputs {
                        self.sockets.ensure_group_exists(output, *size);
                    }
                }
                LayersEvent::LayerRemoved(res) => {
                    for socket in self
                        .sockets
                        .remove_all_for_group(res, &mut self.gpu)
                        .drain(0..)
                    {
                        sender
                            .send(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(socket)))
                            .unwrap();
                    }

                    for channel in MaterialChannel::iter() {
                        let mut blend_node = res.clone();
                        let new_name = format!(
                            "{}.blend.{}",
                            blend_node.file().unwrap(),
                            channel.short_name()
                        );
                        blend_node.rename_file(&new_name);

                        for socket in self
                            .sockets
                            .remove_all_for_group(res, &mut self.gpu)
                            .drain(0..)
                        {
                            sender
                                .send(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(socket)))
                                .unwrap();
                        }
                    }
                }
                _ => {}
            },
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, _, _, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_group_exists(res, *size);
                }
                GraphEvent::OutputSocketAdded(res, ty, external_data, size) => {
                    match ty {
                        OperatorType::Monomorphic(ty) => {
                            // If the type is monomorphic, we can create the image
                            // right away, otherwise creation needs to be delayed
                            // until the type is known.
                            log::trace!(
                                "Adding monomorphic socket {}, {} external data, size {}, type {}",
                                res,
                                if *external_data { "with" } else { "without" },
                                size,
                                ty
                            );
                            let img = self
                                .gpu
                                .create_compute_image(
                                    self.sockets
                                        .get_image_size(&res.socket_node())
                                        .allocation_size(),
                                    *ty,
                                    *external_data,
                                    false,
                                )
                                .unwrap();
                            self.sockets.add_output_socket(
                                res,
                                Some((img, *ty)),
                                *size,
                                *external_data,
                            );
                            sender
                                .send(Lang::ComputeEvent(ComputeEvent::SocketCreated(
                                    res.clone(),
                                    *ty,
                                )))
                                .unwrap();
                        }
                        OperatorType::Polymorphic(_) => {
                            self.sockets
                                .add_output_socket(res, None, *size, *external_data);
                        }
                    }
                }
                GraphEvent::ComplexOperatorUpdated(node, _, _) => {
                    self.sockets.set_force(node);
                }
                GraphEvent::NodeRemoved(res, _, _) => {
                    for socket in self
                        .sockets
                        .remove_all_for_group(res, &mut self.gpu)
                        .drain(0..)
                    {
                        sender
                            .send(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(socket)))
                            .unwrap();
                    }
                }
                GraphEvent::NodeRenamed(from, to) => self.sockets.rename(from, to),
                GraphEvent::NodeResized(res, new_size, scalable) => {
                    if self.sockets.resize(res, *new_size as u32, *scalable) {
                        self.sockets
                            .reinit_output_images(res, &self.gpu, *new_size as u32);
                    }
                }
                GraphEvent::Relinearized(graph, instrs, use_points) => {
                    self.linearizations.insert(
                        graph.clone(),
                        Rc::new(Linearization {
                            instructions: instrs.clone(),
                            use_points: use_points.clone(),
                        }),
                    );
                }
                GraphEvent::Recompute(graph, export_specs) => {
                    self.run_interpretation(graph, export_specs, sender);
                }
                GraphEvent::SocketMonomorphized(res, ty) => {
                    if self.sockets.is_known_output(res) {
                        log::trace!("Adding monomorphized socket {}", res);
                        let size = self
                            .sockets
                            .get_image_size(&res.socket_node())
                            .allocation_size();
                        // Polymorphic operators never have external data.
                        let img = self
                            .gpu
                            .create_compute_image(size, *ty, false, false)
                            .unwrap();
                        // The socket is a known output, and thus the actual
                        // size should also already be known!
                        self.sockets
                            .add_output_socket(res, Some((img, *ty)), size, false);
                        sender
                            .send(Lang::ComputeEvent(ComputeEvent::SocketCreated(
                                res.clone(),
                                *ty,
                            )))
                            .unwrap();
                    }
                }
                GraphEvent::SocketDemonomorphized(res) => {
                    if self.sockets.is_known_output(res) {
                        log::trace!("Removing monomorphized socket {}", res);
                        self.sockets.remove_image(res);
                        let node = res.socket_node();
                        self.sockets.clear_thumbnail(&node, &mut self.gpu);
                        sender
                            .send(Lang::ComputeEvent(ComputeEvent::ThumbnailDestroyed(node)))
                            .unwrap();
                        sender
                            .send(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(
                                res.clone(),
                            )))
                            .unwrap();
                    }
                }
                GraphEvent::GraphRenamed(from, to) => {
                    self.sockets.rename_graph(from, to);
                    self.linearizations.remove(to);
                }
                _ => {}
            },
            Lang::UserIOEvent(event) => match event {
                UserIOEvent::Quit => return None,
                UserIOEvent::OpenSurface(..) => {
                    self.reset();
                    sender
                        .send(Lang::ComputeEvent(ComputeEvent::Cleared))
                        .unwrap();
                }
                UserIOEvent::SaveSurface(..) => {
                    let data = self.serialize().ok()?;
                    sender
                        .send(Lang::ComputeEvent(ComputeEvent::Serialized(data)))
                        .unwrap();
                }
                UserIOEvent::NewSurface => {
                    self.reset();
                    sender
                        .send(Lang::ComputeEvent(ComputeEvent::Cleared))
                        .unwrap();
                }
                UserIOEvent::AddImageResource(path) => {
                    sender.send(self.add_image_resource(path)).unwrap();
                }
                UserIOEvent::RemoveImageResource(res) => {
                    sender.send(self.remove_image_resource(res)).unwrap();
                }
                UserIOEvent::ReloadImageResource(res) => {
                    self.external_data.invalidate_image(res);
                }
                UserIOEvent::SetImageColorSpace(res, cs) => {
                    if let Some(img) = self.external_data.get_image_mut(res) {
                        img.update_satellite(|i| i.set_color_space(*cs));
                        sender
                            .send(Lang::ComputeEvent(ComputeEvent::ImageColorSpaceSet(
                                res.clone(),
                                *cs,
                            )))
                            .unwrap();
                    }
                }
                UserIOEvent::PackImage(res) => {
                    if let Some(img) = self.external_data.get_image_mut(res) {
                        img.pack().ok()?;
                        sender
                            .send(Lang::ComputeEvent(ComputeEvent::ImagePacked(res.clone())))
                            .unwrap();
                    }
                }
                UserIOEvent::AddSvgResource(path) => {
                    sender.send(self.add_svg_resource(path)).unwrap();
                }
                UserIOEvent::PackSvg(res) => {
                    if let Some(svg) = self.external_data.get_svg_mut(res) {
                        svg.pack().ok()?;
                        sender
                            .send(Lang::ComputeEvent(ComputeEvent::SvgPacked(res.clone())))
                            .unwrap();
                    }
                }
                UserIOEvent::RemoveSvgResource(res) => {
                    sender.send(self.remove_svg_resource(res)).unwrap();
                }
                UserIOEvent::ReloadSvgResource(res) => {
                    self.external_data.invalidate_svg(res);
                }
                _ => {}
            },
            Lang::IOEvent(IOEvent::ComputeDataLoaded(data)) => {
                for ev in self.deserialize(data).ok()? {
                    sender.send(ev).unwrap();
                }
            }
            Lang::UserNodeEvent(UserNodeEvent::ViewSocket(socket)) => {
                self.view_socket = socket.as_ref().map(|s| (s.clone(), u64::MIN));
            }
            Lang::SurfaceEvent(SurfaceEvent::ParentSizeSet(size, _)) => {
                self.parent_size = *size;
            }
            Lang::ScheduleEvent(ScheduleEvent::VramUsage) => {
                let usage = self.gpu.allocator_usage();
                sender
                    .send(Lang::ComputeEvent(ComputeEvent::VramUsage(
                        usage.vram_used(),
                        usage.vram_size(),
                    )))
                    .unwrap();
            }
            _ => {}
        }

        Some(())
    }

    fn run_interpretation(
        &mut self,
        graph: &Resource<Graph>,
        export_specs: &[(ExportSpec, std::path::PathBuf)],
        sender: &broker::BrokerSender<Lang>,
    ) {
        let export_specs: HashMap<_, _> =
            export_specs.iter().map(|x| (x.0.node.clone(), x)).collect();

        match interpreter::Interpreter::new(
            &mut self.gpu,
            &mut self.sockets,
            &mut self.external_data,
            &self.shader_library,
            &self.linearizations,
            self.seq,
            graph,
            self.parent_size,
            &mut self.view_socket,
            &export_specs,
        ) {
            Ok(interpreter) => {
                for step_response in interpreter {
                    match step_response {
                        Err(e) => {
                            log::error!("Error during compute interpretation: {:?}", e);
                            log::error!("Aborting compute!");
                            break;
                        }
                        Ok((r, s)) => {
                            for ev in r {
                                sender.send(Lang::ComputeEvent(ev)).unwrap();
                            }
                            self.seq = s;
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("Error building compute interpreter: {:?}", e);
            }
        }
    }

    /// Reset the entire compute manager. This clears all socket data and external images.
    pub fn reset(&mut self) {
        self.sockets.clear(&mut self.gpu);
        self.external_data.clear();
    }

    /// Adds an (unpacked) image resource from a path.
    fn add_image_resource<P: AsRef<Path> + std::fmt::Debug>(&mut self, path: P) -> Lang {
        let res = Resource::image(path.as_ref().file_name().unwrap());
        log::debug!("Adding image resource from path {:?} as {}", path, res);

        self.external_data
            .insert_image(res.clone(), path, ColorSpace::Srgb);

        Lang::ComputeEvent(ComputeEvent::ImageResourceAdded(
            res,
            ColorSpace::Srgb,
            false,
        ))
    }

    /// Removes an image resource
    fn remove_image_resource(&mut self, res: &Resource<Img>) -> Lang {
        log::debug!("Removing image resource {}", res);

        let path = self.external_data.remove_image(res);

        Lang::ComputeEvent(ComputeEvent::ImageResourceRemoved(res.clone(), path))
    }

    /// Adds an (unpacked) SVG resource from a path.
    fn add_svg_resource<P: AsRef<Path> + std::fmt::Debug>(&mut self, path: P) -> Lang {
        let res = Resource::svg(path.as_ref().file_name().unwrap());
        log::debug!("Adding SVG resource from path {:?} as {}", path, res);

        self.external_data.insert_svg(res.clone(), path);

        Lang::ComputeEvent(ComputeEvent::SvgResourceAdded(res, false))
    }

    /// Removes an SVG resource
    fn remove_svg_resource(&mut self, res: &Resource<resource::Svg>) -> Lang {
        log::debug!("Removing SVG resource {}", res);

        let path = self.external_data.remove_svg(res);

        Lang::ComputeEvent(ComputeEvent::SvgResourceRemoved(res.clone(), path))
    }
}
