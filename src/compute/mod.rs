use crate::{broker, gpu, lang::*};

use image::{imageops, ImageBuffer, Luma, Rgb, Rgba};
use strum::IntoEnumIterator;

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;

pub mod interpreter;
pub mod shaders;
pub mod sockets;

use sockets::*;

/// Start the compute manager in a thread. There should only be one such thread.
pub fn start_compute_thread<B: gpu::Backend>(
    broker: &mut broker::Broker<Lang>,
    gpu: Arc<Mutex<gpu::GPU<B>>>,
) -> thread::JoinHandle<()> {
    log::info!("Starting GPU Compute Handler");
    let (sender, receiver, disconnector) = broker.subscribe();
    match gpu::compute::GPUCompute::new(gpu) {
        Err(e) => {
            log::error!("Failed to initialize GPU Compute: {:?}", e);
            panic!("Critical Error");
        }
        Ok(gpu) => thread::Builder::new()
            .name("compute".to_string())
            .spawn(move || {
                let mut compute_mgr = ComputeManager::new(gpu);
                for event in receiver {
                    match compute_mgr.process_event(event) {
                        None => break,
                        Some(response) => {
                            for ev in response {
                                if let Err(e) = sender.send(ev) {
                                    log::error!(
                                        "Compute lost connection to application bus! {}",
                                        e
                                    );
                                }
                            }
                        }
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

    /// Storage for external images
    external_images: HashMap<(std::path::PathBuf, ColorSpace), Option<interpreter::ExternalImage>>,

    /// Last known linearization of a graph
    linearizations: HashMap<Resource<Graph>, Rc<Linearization>>,

    /// Number of executions, kept for cache invalidation
    seq: u64,

    /// The Compute Manager remembers the hash of the last executed set of
    /// uniforms for each resource. On the next execution this is checked, and
    /// if no changes happen, execution can be skipped entirely.
    last_known: HashMap<Resource<Node>, u64>,
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
            external_images: HashMap::new(),
            linearizations: HashMap::new(),
            seq: 0,
            last_known: HashMap::new(),
        }
    }

    /// Process one event from the application bus
    pub fn process_event(&mut self, event: Arc<Lang>) -> Option<Vec<Lang>> {
        let mut response = Vec::new();
        match &*event {
            Lang::LayersEvent(event) => match event {
                LayersEvent::LayerPushed(res, _, _, _, _, _, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_node_exists(res, *size);

                    // Blend nodes
                    for channel in MaterialChannel::iter() {
                        let mut blend_node = res.clone();
                        let new_name = format!(
                            "{}.blend.{}",
                            blend_node.file().unwrap(),
                            channel.short_name()
                        );
                        blend_node.rename_file(&new_name);
                        self.sockets.ensure_node_exists(&blend_node, *size);
                    }
                }
                LayersEvent::MaskPushed(_, res, _, _, _, _, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_node_exists(res, *size);

                    let mut blend_node = res.clone();
                    let new_name = format!("{}.blend", blend_node.file().unwrap(),);
                    blend_node.rename_file(&new_name);
                    self.sockets.ensure_node_exists(&blend_node, *size);
                }
                LayersEvent::LayersAdded(g, size) => {
                    for channel in MaterialChannel::iter() {
                        self.sockets.ensure_node_exists(
                            &g.graph_node(&format!("output.{}", channel.short_name())),
                            *size,
                        );
                    }
                }
                LayersEvent::LayerRemoved(res) => {
                    for socket in self
                        .sockets
                        .remove_all_for_node(res, &mut self.gpu)
                        .drain(0..)
                    {
                        response.push(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(socket)))
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
                            .remove_all_for_node(res, &mut self.gpu)
                            .drain(0..)
                        {
                            response.push(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(socket)))
                        }
                    }
                }
                _ => {}
            },
            Lang::GraphEvent(event) => match event {
                GraphEvent::NodeAdded(res, _, _, _, size) => {
                    // Ensure socket data exists
                    self.sockets.ensure_node_exists(res, *size);
                }
                GraphEvent::OutputSocketAdded(res, ty, external_data, size) => {
                    match ty {
                        OperatorType::Monomorphic(ty) => {
                            // If the type is monomorphic, we can create the image
                            // right away, otherwise creation needs to be delayed
                            // until the type is known.
                            log::trace!(
                                "Adding monomorphic socket {}, {} external data",
                                res,
                                if *external_data { "with" } else { "without" }
                            );
                            let img = self
                                .gpu
                                .create_compute_image(
                                    self.sockets.get_image_size(&res.socket_node()),
                                    *ty,
                                    *external_data,
                                )
                                .unwrap();
                            self.sockets.add_output_socket(
                                res,
                                Some((img, *ty)),
                                *size,
                                *external_data,
                            );
                            response.push(Lang::ComputeEvent(ComputeEvent::SocketCreated(
                                res.clone(),
                                *ty,
                            )));
                        }
                        OperatorType::Polymorphic(_) => {
                            self.sockets
                                .add_output_socket(res, None, *size, *external_data);
                        }
                    }
                }
                GraphEvent::ComplexOperatorUpdated(node, _, _) => {
                    self.sockets.force(node);
                }
                GraphEvent::NodeRemoved(res) => {
                    for socket in self
                        .sockets
                        .remove_all_for_node(res, &mut self.gpu)
                        .drain(0..)
                    {
                        response.push(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(socket)))
                    }
                }
                GraphEvent::NodeRenamed(from, to) => self.rename(from, to),
                GraphEvent::NodeResized(res, new_size) => {
                    if self.sockets.resize(res, *new_size as u32) {
                        self.sockets
                            .reinit_output_images(res, &self.gpu, *new_size as u32);
                    }
                }
                GraphEvent::Relinearized(graph, instrs, use_points, force_points) => {
                    for fp in force_points {
                        self.sockets.force_all_for_node(fp);
                    }

                    self.linearizations.insert(
                        graph.clone(),
                        Rc::new(Linearization {
                            instructions: instrs.clone(),
                            use_points: use_points.clone(),
                        }),
                    );
                }
                GraphEvent::Recompute(graph) => {
                    let interpreter = interpreter::Interpreter::new(
                        &mut self.gpu,
                        &mut self.sockets,
                        &mut self.last_known,
                        &mut self.external_images,
                        &self.shader_library,
                        &self.linearizations,
                        self.seq,
                        graph,
                    );

                    for step_response in interpreter {
                        match step_response {
                            Err(e) => {
                                log::error!("Error during compute interpretation: {:?}", e);
                                log::error!("Aborting compute!");
                            }
                            Ok((r, s)) => {
                                for ev in r {
                                    response.push(Lang::ComputeEvent(ev))
                                }
                                self.seq = s;
                            }
                        }
                    }
                }
                GraphEvent::SocketMonomorphized(res, ty) => {
                    if self.sockets.is_known_output(res) {
                        log::trace!("Adding monomorphized socket {}", res);
                        // Polymorphic operators never have external data.
                        let img = self
                            .gpu
                            .create_compute_image(
                                self.sockets.get_image_size(&res.socket_node()),
                                *ty,
                                false,
                            )
                            .unwrap();
                        // The socket is a known output, and thus the actual
                        // size should also already be known!
                        self.sockets
                            .add_output_socket(res, Some((img, *ty)), 1024, false);
                        response.push(Lang::ComputeEvent(ComputeEvent::SocketCreated(
                            res.clone(),
                            *ty,
                        )));
                    }
                }
                GraphEvent::SocketDemonomorphized(res) => {
                    if self.sockets.is_known_output(res) {
                        log::trace!("Removing monomorphized socket {}", res);
                        self.sockets.remove_image(res);
                        let node = res.socket_node();
                        self.sockets.clear_thumbnail(&node, &mut self.gpu);
                        response.push(Lang::ComputeEvent(ComputeEvent::ThumbnailDestroyed(node)));
                        response.push(Lang::ComputeEvent(ComputeEvent::SocketDestroyed(
                            res.clone(),
                        )));
                    }
                }
                GraphEvent::GraphRenamed(from, to) => {
                    self.sockets.rename_graph(from, to);
                    self.linearizations.remove(to);
                }
                _ => {}
            },
            Lang::UserIOEvent(UserIOEvent::Quit) => return None,
            Lang::UserIOEvent(UserIOEvent::OpenSurface(..)) => self.reset(),
            Lang::UserIOEvent(UserIOEvent::NewSurface) => self.reset(),
            Lang::SurfaceEvent(SurfaceEvent::ExportImage(export, size, path)) => match export {
                ExportSpec::RGBA(rgba_spec) => self.export_to_rgba(rgba_spec.clone(), *size, path),
                ExportSpec::RGB(rgb_spec) => self.export_to_rgb(rgb_spec.clone(), *size, path),
                ExportSpec::Grayscale(gray_spec) => {
                    self.export_to_grayscale(gray_spec.clone(), *size, path)
                }
            },
            _ => {}
        }

        Some(response)
    }

    /// Rename a node
    fn rename(&mut self, from: &Resource<Node>, to: &Resource<Node>) {
        // Move last known hash so we can save on a recomputation
        if let Some(h) = self.last_known.remove(from) {
            self.last_known.insert(to.clone(), h);
        }

        // Move sockets
        self.sockets.rename(from, to);
    }

    /// Reset the entire compute manager. This clears all socket data and external images.
    pub fn reset(&mut self) {
        self.sockets.clear(&mut self.gpu);
        self.external_images.clear();
        self.last_known.clear();
    }

    /// Export an RGBA image as given by an array of channel specifications to a certain path.
    fn export_to_rgba<P: AsRef<Path>>(&mut self, spec: [ChannelSpec; 4], size: u32, path: P) {
        let mut images = HashMap::new();

        for s in &spec {
            let entry = images.entry(s.0.clone());
            entry.or_insert_with(|| {
                #[allow(clippy::or_fun_call)]
                let (image, ty) = self
                    .sockets
                    .get_input_image_typed(&s.0)
                    .or(self.sockets.get_output_image_typed(&s.0))
                    .expect("Trying to export non-existent socket");
                let img_size = image.get_size();
                imageops::resize(
                    &convert_image(&self.gpu.download_image(image).unwrap(), img_size, ty)
                        .expect("Image conversion failed"),
                    size,
                    size,
                    imageops::Triangle,
                )
            });
        }

        let final_image = ImageBuffer::from_fn(size, size, |x, y| {
            Rgba([
                images.get(&spec[0].0).unwrap().get_pixel(x, y)[spec[0].1.channel_index()],
                images.get(&spec[1].0).unwrap().get_pixel(x, y)[spec[1].1.channel_index()],
                images.get(&spec[2].0).unwrap().get_pixel(x, y)[spec[2].1.channel_index()],
                images.get(&spec[3].0).unwrap().get_pixel(x, y)[spec[3].1.channel_index()],
            ])
        });

        final_image.save(path).unwrap();
    }

    /// Export an RGB image as given by an array of channel specifications to a certain path.
    fn export_to_rgb<P: AsRef<Path>>(&mut self, spec: [ChannelSpec; 3], size: u32, path: P) {
        let mut images = HashMap::new();

        for s in &spec {
            let entry = images.entry(s.0.clone());
            entry.or_insert_with(|| {
                #[allow(clippy::or_fun_call)]
                let (image, ty) = self
                    .sockets
                    .get_input_image_typed(&s.0)
                    .or(self.sockets.get_output_image_typed(&s.0))
                    .expect("Trying to export non-existent socket");
                let img_size = image.get_size();
                imageops::resize(
                    &convert_image(&self.gpu.download_image(image).unwrap(), img_size, ty)
                        .expect("Image conversion failed"),
                    size,
                    size,
                    imageops::Triangle,
                )
            });
        }

        let final_image = ImageBuffer::from_fn(size, size, |x, y| {
            Rgb([
                images.get(&spec[0].0).unwrap().get_pixel(x, y)[spec[0].1.channel_index()],
                images.get(&spec[1].0).unwrap().get_pixel(x, y)[spec[1].1.channel_index()],
                images.get(&spec[2].0).unwrap().get_pixel(x, y)[spec[2].1.channel_index()],
            ])
        });

        final_image.save(path).unwrap();
    }

    /// Export a grayscale image as given by an array of channel specifications to a certain path.
    fn export_to_grayscale<P: AsRef<Path>>(&mut self, spec: ChannelSpec, size: u32, path: P) {
        #[allow(clippy::or_fun_call)]
        let (image, ty) = self
            .sockets
            .get_input_image_typed(&spec.0)
            .or(self.sockets.get_output_image_typed(&spec.0))
            .expect("Trying to export non-existent socket {}");
        let img_size = image.get_size();

        let downloaded = imageops::resize(
            &convert_image(&self.gpu.download_image(image).unwrap(), img_size, ty).unwrap(),
            size,
            size,
            imageops::Triangle,
        );
        let final_image = ImageBuffer::from_fn(size, size, |x, y| {
            Luma([downloaded.get_pixel(x, y)[spec.1.channel_index()]])
        });

        final_image.save(path).unwrap();
    }
}

/// Converts an image from the GPU into a standardized rgba16 image. If the
/// input image type is Rgb, a reverse gamma curve will be applied such that the
/// output image matches what is displayed in the renderers.
fn convert_image(
    raw: &[u8],
    size: u32,
    ty: ImageType,
) -> Result<ImageBuffer<Rgba<u16>, Vec<u16>>, String> {
    fn to_16bit(x: f32) -> u16 {
        (x.clamp(0., 1.) * 65535.) as u16
    }

    fn to_16bit_gamma(x: f32) -> u16 {
        (x.powf(1.0 / 2.2).clamp(0., 1.) * 65535.) as u16
    }

    let converted: Vec<u16> = match ty {
        // Underlying memory is formatted as rgba16f
        ImageType::Rgb => unsafe {
            #[allow(clippy::cast_ptr_alignment)]
            let u16s: Vec<[u16; 4]> =
                std::slice::from_raw_parts(raw.as_ptr() as *const half::f16, raw.len() / 2)
                    .chunks(4)
                    .map(|chunk| {
                        [
                            to_16bit_gamma(chunk[0].to_f32()),
                            to_16bit_gamma(chunk[1].to_f32()),
                            to_16bit_gamma(chunk[2].to_f32()),
                            to_16bit(chunk[3].to_f32()),
                        ]
                    })
                    .collect();
            std::slice::from_raw_parts(u16s.as_ptr() as *const u16, u16s.len() * 4).to_owned()
        },
        // Underlying memory is formatted as r32f, using this value for all channels
        ImageType::Grayscale => unsafe {
            #[allow(clippy::cast_ptr_alignment)]
            let u16s: Vec<[u16; 4]> =
                std::slice::from_raw_parts(raw.as_ptr() as *const f32, raw.len() / 4)
                    .iter()
                    .map(|x| [to_16bit(*x); 4])
                    .collect();
            std::slice::from_raw_parts(u16s.as_ptr() as *const u16, u16s.len() * 4).to_owned()
        },
    };

    ImageBuffer::from_raw(size, size, converted)
        .ok_or_else(|| "Error while creating image buffer".to_string())
}
