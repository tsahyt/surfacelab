use crate::{gpu, lang::*, util::*};
use std::collections::{HashMap, HashSet};

const TIMING_DECAY: f64 = 0.85;

/// An output socket with a type and associated metadata. Outputs are *always*
/// backed by a compute image. However, this image may not necessarily be backed
/// by an allocation at all times.
struct TypedOutput<B: gpu::Backend> {
    /// Sequence number to determine when this output was last touched
    seq: u64,

    /// GPU side image backing the output
    image: gpu::compute::Image<B>,

    /// Image type of the output, stored such that it is known for exporting.
    ty: ImageType,

    /// Determines whether the output must be recomputed under all circumstances
    /// at the next recomputation.
    force: bool,

    /// Whether this output can be transferred to
    transfer_dst: bool,
}

impl<B> TypedOutput<B>
where
    B: gpu::Backend,
{
    /// Reinitialize the GPU image with a (possibly new) size. This will also
    /// force the image on the next evaluation. This image will not be
    /// immediately backed by memory.
    pub fn reinit_image(&mut self, gpu: &gpu::compute::GPUCompute<B>, size: u32) {
        self.image = gpu
            .create_compute_image(size, self.ty, self.transfer_dst)
            .unwrap();
        self.force = true;
    }
}

/// Per "node" socket data. Note that we don't really have a notion of node here
/// in the compute component, but this still very closely corresponds to that.
pub struct SocketGroup<B: gpu::Backend> {
    /// Output sockets always map to an image, which may or may not be
    /// allocated, and a counter determining in which execution the image was
    /// most recently updated. Additionally the image type is stored such that
    /// we know it at export time.
    typed_outputs: HashMap<String, TypedOutput<B>>,

    /// Required to keep track of polymorphic outputs. Kept separately to keep
    /// output_sockets ownership structure simple.
    known_outputs: HashSet<String>,

    /// The image size of output images for sockets managed here.
    output_size: u32,

    /// Input sockets only map to the output sockets they are connected to
    inputs: HashMap<String, Resource<Socket>>,

    /// Exponential Moving Average over computation time for this set of
    /// sockets, to get some estimate of how long computation may take in the
    /// future. Measured in seconds, for easy conversion from Durations.
    time_ema: EMA<f64>,

    /// Optional thumbnail for the "node".
    thumbnail: Option<gpu::compute::ThumbnailIndex>,

    /// Thumbnail related sequence number
    thumbnail_seq: u64,
}

impl<B: gpu::Backend> SocketGroup<B> {
    /// Update timing information
    pub fn update_time_ema(&mut self, seconds: f64) {
        self.time_ema.update(seconds);
        log::trace!(
            "Average execution time {0:.1} µs, last {1:.1} µs",
            self.time_ema.get() * 1e6,
            seconds * 1e6
        );
    }

    /// Construct a new empty SocketGroup with a given output size
    pub fn new(size: u32) -> Self {
        Self {
            typed_outputs: HashMap::new(),
            known_outputs: HashSet::new(),
            output_size: size,
            inputs: HashMap::new(),
            time_ema: EMA::new(TIMING_DECAY),
            thumbnail: None,
            thumbnail_seq: 0,
        }
    }
}

impl<B: gpu::Backend> Default for SocketGroup<B> {
    fn default() -> Self {
        Self::new(1024)
    }
}

/// Mapping to associate socket data to each node as required. Nodes can be from
/// anywhere, and are not required to be in the same graph.
pub struct Sockets<B: gpu::Backend>(HashMap<Resource<Node>, SocketGroup<B>>);

impl<B> Default for Sockets<B>
where
    B: gpu::Backend,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<B> Sockets<B>
where
    B: gpu::Backend,
{
    pub fn new() -> Self {
        Sockets(HashMap::new())
    }

    /// Clear all socket information, releasing all resources.
    pub fn clear(&mut self, gpu: &mut gpu::compute::GPUCompute<B>) {
        for (_, mut socket) in self.0.drain() {
            if let Some(thumbnail) = socket.thumbnail.take() {
                gpu.return_thumbnail(thumbnail);
            }
        }
    }

    /// Update the timing data associated with one node.
    pub fn update_timing_data(&mut self, node: &Resource<Node>, seconds: f64) {
        if let Some(sdata) = self.0.get_mut(&node) {
            sdata.update_time_ema(seconds);
        }
    }

    /// Remove all sockets for the given node.
    pub fn remove_all_for_node(
        &mut self,
        res: &Resource<Node>,
        gpu: &mut gpu::compute::GPUCompute<B>,
    ) -> Vec<Resource<Socket>> {
        let mut result = Vec::new();

        if let Some(mut socket) = self.0.remove(res) {
            result.extend(socket.typed_outputs.keys().map(|s| res.node_socket(s)));
            if let Some(thumbnail) = socket.thumbnail.take() {
                gpu.return_thumbnail(thumbnail);
            }
        }

        result
    }

    /// Force all sockets on this node in the next run
    pub fn force_all_for_node(&mut self, res: &Resource<Node>) {
        if let Some(socket) = self.0.get_mut(res) {
            for typed_output in socket.typed_outputs.values_mut() {
                typed_output.force = true;
            }
        }
    }

    /// Ensure the node is known
    pub fn ensure_node_exists(&mut self, res: &Resource<Node>, size: u32) -> &mut SocketGroup<B> {
        self.0
            .entry(res.clone())
            .or_insert_with(|| SocketGroup::new(size))
    }

    /// Ensure the node described by the resource has a thumbnail image
    /// available, returning whether the thumbnail is newly created.
    pub fn ensure_node_thumbnail_exists(
        &mut self,
        res: &Resource<Node>,
        ty: ImageType,
        gpu: &mut gpu::compute::GPUCompute<B>,
    ) -> bool {
        if let Some(socket) = self.0.get_mut(&res) {
            if socket.thumbnail.is_none() {
                socket.thumbnail = Some(gpu.new_thumbnail(ty));
                return true;
            }
        }
        false
    }

    /// Get thumbnail sequence number for this socket group.
    pub fn get_thumbnail_updated(&self, res: &Resource<Node>) -> Option<u64> {
        self.0.get(&res).map(|x| x.thumbnail_seq)
    }

    /// Set the thumbnail seq number
    pub fn set_thumbnail_updated(&mut self, res: &Resource<Node>, updated: u64) {
        if let Some(s) = self.0.get_mut(&res) {
            s.thumbnail_seq = updated;
        }
    }

    /// Return the thumbnail for the given node
    pub fn clear_thumbnail(&mut self, res: &Resource<Node>, gpu: &mut gpu::compute::GPUCompute<B>) {
        if let Some(socket) = self.0.get_mut(res) {
            if let Some(thumbnail) = socket.thumbnail.take() {
                gpu.return_thumbnail(thumbnail);
            }
        }
    }

    /// Get the thumbnail for a resource (node or socket thereof) if it exists
    pub fn get_thumbnail(&self, res: &Resource<Node>) -> Option<&gpu::compute::ThumbnailIndex> {
        self.0.get(res).and_then(|s| s.thumbnail.as_ref())
    }

    /// Insert a new output socket. Will drop the old socket if it already exists.
    pub fn add_output_socket(
        &mut self,
        res: &Resource<Socket>,
        image: Option<(gpu::compute::Image<B>, ImageType)>,
        size: u32,
        transfer_dst: bool,
    ) {
        let sockets = self.ensure_node_exists(&res.socket_node(), size);
        let socket_name = res.fragment().unwrap().to_string();
        if let Some((img, ty)) = image {
            sockets.typed_outputs.insert(
                socket_name.clone(),
                TypedOutput {
                    seq: 0,
                    image: img,
                    ty,
                    force: false,
                    transfer_dst,
                },
            );
        }
        sockets.known_outputs.insert(socket_name);
    }

    /// Determine whether the given resource points to a known output socket.
    pub fn is_known_output(&self, res: &Resource<Socket>) -> bool {
        self.0
            .get(&res.socket_node())
            .map(|s| s.known_outputs.contains(res.fragment().unwrap()))
            .unwrap_or(false)
    }

    /// Drop the underlying image from an output socket
    pub fn remove_image(&mut self, res: &Resource<Socket>) {
        let sockets = self
            .0
            .get_mut(&res.socket_node())
            .expect("Trying to remove image from unknown resource");
        sockets.typed_outputs.remove(res.fragment().unwrap());
    }

    /// Free the images for one specific node.
    pub fn free_images_for_node(
        &mut self,
        node: &Resource<Node>,
        gpu: &mut gpu::compute::GPUCompute<B>,
    ) {
        let sockets = self
            .0
            .get_mut(node)
            .expect("Trying to free images from unknown node");
        for out in sockets.typed_outputs.values_mut() {
            out.reinit_image(gpu, sockets.output_size)
        }
    }

    /// Reinitialize output images of a node given a new size.
    pub fn reinit_output_images(
        &mut self,
        res: &Resource<Node>,
        gpu: &gpu::compute::GPUCompute<B>,
        size: u32,
    ) {
        if let Some(socket_data) = self.0.get_mut(&res) {
            for out in socket_data.typed_outputs.values_mut() {
                out.reinit_image(gpu, size);
            }
        }
    }

    /// Obtain the output image given a socket resource along with its type
    pub fn get_output_image_typed(
        &self,
        res: &Resource<Socket>,
    ) -> Option<(&gpu::compute::Image<B>, ImageType)> {
        self.0
            .get(&res.socket_node())?
            .typed_outputs
            .get(res.fragment().unwrap())
            .map(|x| (&x.image, x.ty))
    }

    /// Obtain type of output image given a socket resource along with its type
    pub fn get_output_image_type(&self, res: &Resource<Socket>) -> Option<ImageType> {
        self.get_output_image_typed(res).map(|x| x.1)
    }

    /// Obtain the output image given a socket resource
    pub fn get_output_image(&self, res: &Resource<Socket>) -> Option<&gpu::compute::Image<B>> {
        self.get_output_image_typed(res).map(|x| x.0)
    }

    /// Obtain the output image given a socket resource, mutably, along with its type
    pub fn get_output_image_typed_mut(
        &mut self,
        res: &Resource<Socket>,
    ) -> Option<(&mut gpu::compute::Image<B>, ImageType)> {
        self.0
            .get_mut(&res.socket_node())?
            .typed_outputs
            .get_mut(res.fragment().unwrap())
            .map(|x| (&mut x.image, x.ty))
    }

    /// Obtain the output image given a socket resource, mutably
    pub fn get_output_image_mut(
        &mut self,
        res: &Resource<Socket>,
    ) -> Option<&mut gpu::compute::Image<B>> {
        self.get_output_image_typed_mut(res).map(|x| x.0)
    }

    /// Obtain the input image given a socket resource along with its type
    pub fn get_input_image_typed(
        &self,
        res: &Resource<Socket>,
    ) -> Option<(&gpu::compute::Image<B>, ImageType)> {
        let sockets = self.0.get(&res.socket_node())?;
        let output_res = sockets.inputs.get(res.fragment()?)?;
        self.0
            .get(&output_res.socket_node())?
            .typed_outputs
            .get((&output_res).fragment()?)
            .map(|x| (&x.image, x.ty))
    }

    /// Obtain type of input image given a socket resource along with its type
    pub fn get_input_image_type(&self, res: &Resource<Socket>) -> Option<ImageType> {
        self.get_input_image_typed(res).map(|x| x.1)
    }

    /// Obtain the input image given a socket resource
    pub fn get_input_image(&self, res: &Resource<Socket>) -> Option<&gpu::compute::Image<B>> {
        self.get_input_image_typed(res).map(|x| x.0)
    }

    /// Obtain when the input image was last updated
    pub fn get_input_image_updated(&self, res: &Resource<Socket>) -> Option<u64> {
        let sockets = self.0.get(&res.socket_node())?;
        let output_res = sockets.inputs.get(res.fragment()?)?;
        self.0
            .get(&output_res.socket_node())?
            .typed_outputs
            .get((&output_res).fragment()?)
            .map(|x| x.seq)
    }

    /// Obtain when the output image was last updated
    pub fn get_output_image_updated(&self, res: &Resource<Socket>) -> Option<u64> {
        self.0
            .get(&res.socket_node())?
            .typed_outputs
            .get((&res).fragment()?)
            .map(|x| x.seq)
    }

    pub fn get_any_output_updated(&self, res: &Resource<Node>) -> Option<u64> {
        self.0.get(res)?.typed_outputs.values().map(|x| x.seq).max()
    }

    /// Obtain whether a node must be forced
    pub fn get_force(&self, node: &Resource<Node>) -> bool {
        self.0
            .get(&node)
            .unwrap()
            .typed_outputs
            .values()
            .any(|x| x.force)
    }

    /// Force recomputation of a node on the next step
    pub fn force(&mut self, node: &Resource<Node>) {
        for typed_output in self.0.get_mut(&node).unwrap().typed_outputs.values_mut() {
            typed_output.force = true;
        }
    }

    /// Set when the output image was last updated
    pub fn set_output_image_updated(&mut self, socket: &Resource<Socket>, updated: u64) {
        for img in self
            .0
            .get_mut(&socket.socket_node())
            .and_then(|xs| xs.typed_outputs.get_mut(socket.fragment().unwrap()))
        {
            img.seq = updated;
            img.force = false;
        }
    }

    /// Set when the all output images on this group were last updated
    pub fn set_all_outputs_updated(&mut self, node: &Resource<Node>, updated: u64) {
        for img in self.0.get_mut(node).unwrap().typed_outputs.values_mut() {
            img.seq = updated;
            img.force = false;
        }
    }

    /// Connect an output to an input
    pub fn connect_input(&mut self, from: &Resource<Socket>, to: &Resource<Socket>) {
        self.0
            .get_mut(&to.socket_node())
            .unwrap()
            .inputs
            .insert(to.fragment().unwrap().to_string(), from.to_owned());
    }

    /// Get the size of the images associated with this node
    pub fn get_image_size(&self, res: &Resource<Node>) -> u32 {
        self.0.get(&res).unwrap().output_size
    }

    pub fn get_input_resource(&self, res: &Resource<Socket>) -> Option<&Resource<Socket>> {
        let sockets = self.0.get(&res.socket_node())?;
        sockets.inputs.get(res.fragment()?)
    }

    /// Rename a resource, moving all its sockets to the new name
    pub fn rename(&mut self, from: &Resource<Node>, to: &Resource<Node>) {
        if let Some(x) = self.0.remove(from) {
            self.0.insert(to.clone(), x);
        }
    }

    /// Resize outputs
    pub fn resize(&mut self, res: &Resource<Node>, new_size: u32) -> bool {
        let mut resized = false;
        if let Some(x) = self.0.get_mut(res) {
            resized = x.output_size != new_size;
            x.output_size = new_size;
        }
        resized
    }

    /// Renames all sockets that contain the given graph to use the new graph.
    /// In effect this is "moving" sockets.
    pub fn rename_graph(&mut self, from: &Resource<Graph>, to: &Resource<Graph>) {
        for (mut node, mut socket_data) in self.0.drain().collect::<Vec<_>>() {
            if &node.node_graph() == from {
                node.set_graph(to.path());
            }

            for socket in socket_data.inputs.values_mut() {
                if &socket.socket_node().node_graph() == from {
                    socket.set_graph(to.path());
                }
            }

            self.0.insert(node, socket_data);
        }
    }

    /// Obtain iterator over all known nodes
    pub fn known_nodes(&self) -> impl Iterator<Item = &Resource<Node>> {
        self.0.keys()
    }
}
