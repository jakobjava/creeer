//! Instanced mesh rendering (camera view and shadow casters share one instance buffer).

use super::levels;
use super::palette::GpuPalette;
use super::skin_layout::skinned_vertex_layouts;
use super::skin_pass::SkinPass;
use super::*;
use crate::scene::Instance;
use creeer_math::glam::DVec3;
use gather::Cull;

#[path = "mesh_gather.rs"]
mod gather;

/// One draw call: a mesh's range in the arena (one of its levels) and a range of the instance
/// buffer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Draw {
    range: usize,
    first_instance: u32,
    instance_count: u32,
}

const MESH_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 6 => Uint32, 8 => Uint32, 9 => Float32x2];
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4, 4 => Float32x3, 7 => Uint32, 5 => Float32x4];

/// Vertex buffer layouts shared by every pipeline that draws instanced meshes.
pub(crate) fn instanced_vertex_layouts() -> [Option<wgpu::VertexBufferLayout<'static>>; 2] {
    [
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MeshVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &MESH_VERTEX_ATTRIBUTES,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        }),
    ]
}

/// The buffers needed to issue instanced draws.
pub(crate) struct MeshBuffers<'a> {
    vertices: &'a wgpu::Buffer,
    indices: &'a wgpu::Buffer,
    instances: &'a wgpu::Buffer,
}

impl MeshBuffers<'_> {
    /// Binds the buffers and issues `draws` (pipeline and bind groups are set by the caller).
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, meshes: &MeshArena, draws: &[Draw]) {
        if draws.is_empty() {
            return;
        }
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for draw in draws {
            let range = meshes.ranges[draw.range];
            pass.draw_indexed(
                range.first_index..range.first_index + range.index_count,
                range.base_vertex,
                draw.first_instance..draw.first_instance + draw.instance_count,
            );
        }
    }
}

/// Instanced mesh rendering. One instance buffer holds the instances visible to the camera,
/// followed by the shadow casters of each cascade.
pub(super) struct MeshPass {
    /// Procedural meshes, in their instance's material: drawn alone, and after the depth
    /// prepass (ambient occlusion on).
    plain: [wgpu::RenderPipeline; 2],
    /// Models, with the full surface (ADR-0026): alone, and after the prepass.
    model: [wgpu::RenderPipeline; 2],
    /// Skinned characters (G1.3e), drawn two-sided: alone, and after the prepass.
    two_sided: [wgpu::RenderPipeline; 2],
    /// Depth only, for the prepass: procedural meshes, models, two-sided models.
    prepass: [wgpu::RenderPipeline; 3],
    /// The tokens model vertices name and the noise (bind group 2).
    palette: GpuPalette,
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    /// Per pass (camera, then cascades): instances gathered this frame.
    pass_scratch: Vec<Vec<InstanceRaw>>,
    scratch: Vec<InstanceRaw>,
    /// The camera's draws: of procedural meshes, and of models.
    draws: Vec<Draw>,
    model_draws: Vec<Draw>,
    pub(super) shadow_draws: [Vec<Draw>; CASCADES],
    /// Each pass and level's instances while a batch is gathered (`mesh_gather.rs`).
    buckets: Vec<Vec<InstanceRaw>>,
}

impl MeshPass {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals_layout: &wgpu::BindGroupLayout,
        shadow_sample_layout: &wgpu::BindGroupLayout,
        ao_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let source = concat!(
            include_str!("../shaders/ao_sample.wgsl"),
            "\n",
            include_str!("../shaders/heatmap.wgsl"),
            "\n",
            include_str!("../shaders/lighting.wgsl"),
            "\n",
            include_str!("../shaders/surface.wgsl"),
            "\n",
            include_str!("../shaders/mesh.wgsl")
        );
        let module = shader(device, "mesh", source);
        let palette = GpuPalette::new(device, queue);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[
                Some(globals_layout),
                Some(shadow_sample_layout),
                Some(&palette.layout),
                Some(ao_layout),
            ],
            immediate_size: 0,
        });
        let (plain_layouts, skinned_layouts) =
            (instanced_vertex_layouts(), skinned_vertex_layouts());
        let pipeline = |label, vertex, fragment, depth_stencil, skinned: bool| {
            let (buffers, cull_mode): (&[_], _) = if skinned {
                (&skinned_layouts, None)
            } else {
                (&plain_layouts, Some(wgpu::Face::Back))
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vertex),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil,
                multisample: scene_multisample(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(HDR_FORMAT.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let back = Some(wgpu::Face::Back);
        let depths = || [scene_depth_state(), after_prepass_depth_state()];
        let prepass = |label, vertex, skinned: bool| {
            let (buffers, cull): (&[_], _) = if skinned {
                (&skinned_layouts, None)
            } else {
                (&plain_layouts, back)
            };
            prepass_pipeline(
                device,
                label,
                globals_layout,
                &module,
                vertex,
                buffers,
                cull,
            )
        };
        let instance_capacity = 1024;
        Self {
            plain: depths().map(|d| pipeline("mesh", "vs_plain", "fs_plain", d, false)),
            model: depths().map(|d| pipeline("model", "vs_main", "fs_main", d, false)),
            two_sided: depths().map(|d| pipeline("two-sided", "vs_main", "fs_two_sided", d, true)),
            prepass: [
                prepass("mesh prepass", "vs_plain", false),
                prepass("model prepass", "vs_main", false),
                prepass("two-sided prepass", "vs_main", true),
            ],
            palette,
            vertices: None,
            indices: None,
            instances: create_instance_buffer(device, instance_capacity),
            instance_capacity,
            pass_scratch: vec![Vec::new(); 1 + CASCADES],
            scratch: Vec::new(),
            draws: Vec::new(),
            model_draws: Vec::new(),
            shadow_draws: std::array::from_fn(|_| Vec::new()),
            buckets: vec![Vec::new(); (1 + CASCADES) * gather::MAX_LEVELS],
        }
    }

    pub(super) fn upload_meshes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[MeshVertex],
        indices: &[u32],
    ) {
        let make = |label, bytes: &[u8], usage| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes.len() as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, bytes);
            buffer
        };
        self.vertices = Some(make(
            "mesh vertices",
            bytemuck::cast_slice(vertices),
            wgpu::BufferUsages::VERTEX,
        ));
        self.indices = Some(make(
            "mesh indices",
            bytemuck::cast_slice(indices),
            wgpu::BufferUsages::INDEX,
        ));
    }

    /// Culls (camera frustum and per-cascade caster tests), converts and uploads all
    /// instances; records the draw lists. Meshes with levels of detail are drawn at the level
    /// each instance needs (`levels.rs`), for a viewport `focal` pixels over 2·tan(fov/2).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &MeshArena,
        scene: &Scene,
        camera: &Camera,
        view_proj: Mat4,
        focal: f32,
        shadows: Option<(&[Cascade; CASCADES], &LightBasis)>,
    ) -> RenderStats {
        let frustum = Frustum::from_view_projection(view_proj);
        for list in &mut self.pass_scratch {
            list.clear();
        }
        // Draws are recorded relative to each pass's own list, then offset when concatenated.
        let mut pass_draws: Vec<Vec<Draw>> = vec![Vec::new(); 1 + CASCADES];
        let mut stats = RenderStats::default();
        let cull = Cull {
            camera: camera.position,
            frustum,
            shadows,
            focal,
        };
        for (mesh, batch) in scene.batches.iter().enumerate() {
            if let Some(&levels) = meshes.meshes.get(mesh) {
                self.gather_mesh(&mut pass_draws, meshes, levels, batch, &cull, &mut stats);
            }
        }

        self.scratch.clear();
        for (p, list) in self.pass_scratch.iter().enumerate() {
            let offset = u32::try_from(self.scratch.len()).expect("instance count");
            for draw in &mut pass_draws[p] {
                draw.first_instance += offset;
            }
            self.scratch.extend_from_slice(list);
        }
        let mut lists = pass_draws.into_iter();
        (self.model_draws, self.draws) = lists
            .next()
            .unwrap_or_default()
            .into_iter()
            .partition(|draw| meshes.ranges[draw.range].model);
        for (target, list) in self.shadow_draws.iter_mut().zip(lists) {
            *target = list;
        }

        stats.instances_drawn = self.pass_scratch[0].len();
        stats.shadow_instances = self.pass_scratch[1..].iter().map(Vec::len).sum();
        stats.draw_calls = self.draws.len()
            + self.model_draws.len()
            + self.shadow_draws.iter().map(Vec::len).sum::<usize>();

        if self.scratch.len() > self.instance_capacity {
            self.instance_capacity = self.scratch.len().next_power_of_two();
            self.instances = create_instance_buffer(device, self.instance_capacity);
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&self.scratch));
        stats
    }

    pub(super) fn buffers(&self) -> Option<MeshBuffers<'_>> {
        Some(MeshBuffers {
            vertices: self.vertices.as_ref()?,
            indices: self.indices.as_ref()?,
            instances: &self.instances,
        })
    }

    pub(super) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        meshes: &MeshArena,
        shadow_sampling: &wgpu::BindGroup,
        ao: &wgpu::BindGroup,
        prepassed: bool,
        skin: &SkinPass,
    ) {
        pass.set_bind_group(1, shadow_sampling, &[]);
        pass.set_bind_group(2, &self.palette.bind_group, &[]);
        pass.set_bind_group(3, ao, &[]);
        let p = usize::from(prepassed);
        if let Some(buffers) = self.buffers() {
            if !self.draws.is_empty() {
                pass.set_pipeline(&self.plain[p]);
                buffers.draw(pass, meshes, &self.draws);
            }
            if !self.model_draws.is_empty() {
                pass.set_pipeline(&self.model[p]);
                buffers.draw(pass, meshes, &self.model_draws);
            }
        }
        if !skin.camera_draws.is_empty() {
            pass.set_pipeline(&self.two_sided[p]);
            skin.draw(pass, &skin.camera_draws);
        }
    }

    /// Draws the camera's meshes into the depth prepass (globals bound by the caller).
    pub(super) fn draw_prepass(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        meshes: &MeshArena,
        skin: &SkinPass,
    ) {
        if let Some(buffers) = self.buffers() {
            for (pipeline, draws) in self.prepass.iter().zip([&self.draws, &self.model_draws]) {
                if !draws.is_empty() {
                    pass.set_pipeline(pipeline);
                    buffers.draw(pass, meshes, draws);
                }
            }
        }
        if !skin.camera_draws.is_empty() {
            pass.set_pipeline(&self.prepass[2]);
            skin.draw(pass, &skin.camera_draws);
        }
    }

    /// The palette's per-character slots (G1.3e).
    pub(super) fn palette(&self) -> &GpuPalette {
        &self.palette
    }

    /// Replaces the palette model vertices name.
    pub(super) fn set_palette(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        palette: &creeer_art::Palette,
    ) {
        self.palette.set(device, queue, palette);
    }
}

pub(super) fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: (capacity * std::mem::size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
