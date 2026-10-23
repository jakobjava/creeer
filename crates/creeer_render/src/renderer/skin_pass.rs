//! The skinning pass (G1.3e, ADR-0026 decision 6): the frame's characters, each at the level of
//! detail it is drawn at, skinned in one compute dispatch into a vertex buffer that the shadow,
//! prepass and scene passes draw as a plain mesh (as Unreal's GPU skin cache does). Their rest
//! vertices and their outfits' index lists stay on the GPU; a frame uploads only bones, jobs
//! and one instance each.

use super::grown::Grown;
use super::skin_layout::{RestRaw, SKINNED_BYTES, STATIC_BYTES, StaticRaw, split};
use super::*;
use crate::scene::Instance;
use crate::skin::{DualQuat, OutfitId, SkinnedId, SkinnedMeshData};
use creeer_art::model_skin::{LEVELS, Part};
use creeer_math::Frame64;
use creeer_math::glam::{DVec3, Vec3};
use std::ops::Range;

/// Characters drawn with colours of their own at once; more share the last one's.
pub(crate) const SLOTS: u32 = 64;
/// A token that takes no per-player slot (`skin.wgsl`).
pub(super) const NO_SLOT: u32 = 255;
/// How far a posed character may reach beyond its rest sphere about its first bone (a ragdoll
/// lying, its limbs flung).
const REACH: f32 = 1.25;

/// A character's work (`skin.wgsl`'s `Job`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct JobRaw {
    rest_first: u32,
    count: u32,
    out_first: u32,
    bone_first: u32,
    slot_base: u32,
    pad: [u32; 3],
}

/// One skinned draw: an outfit's indices at a level, from its character's first skinned vertex,
/// with its instance.
#[derive(Clone, Debug)]
pub(super) struct SkinDraw {
    indices: Range<u32>,
    /// Its character's first skinned vertex, and its level's first rest vertex.
    out_first: u32,
    rest_first: u32,
    instance: u32,
}

struct ModelEntry {
    data: SkinnedMeshData,
    /// Its first vertex in the rest arena.
    rest_first: u32,
}

struct OutfitEntry {
    model: usize,
    /// Each level's index list in the index arena.
    levels: [Range<u32>; LEVELS as usize],
}

pub(super) struct SkinPass {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    models: Vec<ModelEntry>,
    outfits: Vec<OutfitEntry>,
    rest: Vec<RestRaw>,
    statics: Vec<StaticRaw>,
    indices: Vec<u32>,
    /// The arenas changed since they were uploaded.
    dirty: bool,
    rest_buffer: Grown,
    static_buffer: Grown,
    index_buffer: Grown,
    output: Grown,
    bones: Grown,
    jobs: Grown,
    slots: Grown,
    instances: Grown,
    bind_group: Option<wgpu::BindGroup>,
    /// This frame's work: characters, and the most vertices one of them has.
    job_count: u32,
    widest: u32,
    pub(super) camera_draws: Vec<SkinDraw>,
    pub(super) shadow_draws: [Vec<SkinDraw>; CASCADES],
    /// Each drawn character's palette slot and colours.
    pub(super) colours: Vec<(u32, [[f32; 3]; 3])>,
}

impl SkinPass {
    pub(super) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skin"),
            entries: &[
                storage(0, true),
                storage(1, true),
                storage(2, true),
                storage(3, true),
                storage(4, false),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("skin"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("../shaders/skin.wgsl"))),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skin"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("skin"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let s = wgpu::BufferUsages::STORAGE;
        let mut pass = Self {
            pipeline,
            layout,
            models: Vec::new(),
            outfits: Vec::new(),
            rest: Vec::new(),
            statics: Vec::new(),
            indices: Vec::new(),
            dirty: false,
            rest_buffer: Grown::new(device, "skin rest", s),
            static_buffer: Grown::new(device, "skin statics", wgpu::BufferUsages::VERTEX),
            index_buffer: Grown::new(device, "skin indices", wgpu::BufferUsages::INDEX),
            output: Grown::new(
                device,
                "skinned",
                s | wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_SRC,
            ),
            bones: Grown::new(device, "skin bones", s),
            jobs: Grown::new(device, "skin jobs", s),
            slots: Grown::new(device, "skin slots", s),
            instances: Grown::new(device, "skin instances", wgpu::BufferUsages::VERTEX),
            bind_group: None,
            job_count: 0,
            widest: 0,
            camera_draws: Vec::new(),
            shadow_draws: std::array::from_fn(|_| Vec::new()),
            colours: Vec::new(),
        };
        // No token takes a slot until a palette says which do.
        pass.set_slots(device, queue, &[]);
        pass
    }

    /// Registers a skinned model.
    pub(super) fn add(&mut self, data: &SkinnedMeshData) -> SkinnedId {
        let rest_first = u32::try_from(self.rest.len()).expect("fewer than 2³² vertices");
        let (rest, statics): (Vec<RestRaw>, Vec<StaticRaw>) =
            data.model.vertices.iter().map(split).unzip();
        self.rest.extend(rest);
        self.statics.extend(statics);
        self.models.push(ModelEntry {
            data: data.clone(),
            rest_first,
        });
        self.dirty = true;
        SkinnedId(u32::try_from(self.models.len() - 1).expect("few models"))
    }

    /// Registers an outfit of `model`: the parts `shown` (`wearable::visible`), merged a level.
    pub(super) fn add_outfit(&mut self, model: SkinnedId, shown: &[&Part]) -> OutfitId {
        let entry = &self.models[model.0 as usize];
        let lists = entry.data.outfit(shown);
        let levels = lists.map(|list| {
            let first = u32::try_from(self.indices.len()).expect("fewer than 2³² indices");
            self.indices.extend(list);
            first..u32::try_from(self.indices.len()).expect("fits")
        });
        self.outfits.push(OutfitEntry {
            model: model.0 as usize,
            levels,
        });
        self.dirty = true;
        OutfitId(u32::try_from(self.outfits.len() - 1).expect("few outfits"))
    }

    /// Which per-player slot each palette token takes (`skin.wgsl`'s `slots`).
    pub(super) fn set_slots(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, table: &[u32]) {
        if self.slots.reserve(device, 4 * table.len() as u64) {
            self.bind_group = None;
        }
        // The shader reads the buffer's whole length: the rest takes no slot.
        let mut padded = table.to_vec();
        let words = usize::try_from(self.slots.buffer.size() / 4).expect("fits");
        padded.resize(words, NO_SLOT);
        queue.write_buffer(&self.slots.buffer, 0, bytemuck::cast_slice(&padded));
    }

    /// Culls the scene's characters, chooses their levels, and writes their bones, jobs and
    /// instances; `slot_base` is the palette's first per-character entry.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        characters: &[crate::scene::Character],
        camera: DVec3,
        frustum: &Frustum,
        shadows: Option<(&[Cascade; CASCADES], &LightBasis)>,
        focal: f32,
        slot_base: u32,
    ) {
        if self.dirty {
            self.upload(device, queue);
        }
        self.camera_draws.clear();
        for list in &mut self.shadow_draws {
            list.clear();
        }
        self.colours.clear();
        let (mut bones, mut jobs, mut instances) = (Vec::new(), Vec::new(), Vec::new());
        let mut out_first = 0u32;
        self.widest = 0;
        for character in characters {
            let Some(outfit) = self.outfits.get(character.outfit.0 as usize) else {
                continue;
            };
            let entry = &self.models[outfit.model];
            let data = &entry.data;
            let Some(pelvis) = character.bones.first() else {
                continue;
            };
            if character.bones.len() != data.bones {
                continue;
            }
            let centre = (pelvis.transform_point(data.centre.as_dvec3()) - camera).as_vec3();
            let radius = data.radius * REACH;
            let seen = frustum.intersects_sphere(centre, radius);
            let casts: Vec<usize> = shadows.map_or_else(Vec::new, |(cascades, basis)| {
                (0..CASCADES)
                    .filter(|&c| cascades[c].may_cast_into(basis, centre, radius))
                    .collect()
            });
            if !seen && casts.is_empty() {
                continue;
            }
            let level = data.level((centre.length() - radius).max(0.0), focal);
            let range = &data.levels[level];
            let count = range.end - range.start;
            let instance = u32::try_from(instances.len()).expect("few characters");
            let slot = slot_base + 3 * instance.min(SLOTS - 1);
            jobs.push(JobRaw {
                rest_first: entry.rest_first + range.start,
                count,
                out_first,
                bone_first: u32::try_from(bones.len() / 2).expect("few bones"),
                slot_base: slot,
                pad: [0; 3],
            });
            bones.extend(character.bones.iter().flat_map(|bone| {
                let dq = DualQuat::from_rigid(
                    bone.rotation.as_quat(),
                    (bone.position - camera).as_vec3(),
                );
                [dq.real.to_array(), dq.dual.to_array()]
            }));
            instances.push(InstanceRaw::new(
                &Instance {
                    frame: Frame64::from_position(camera),
                    scale: Vec3::ONE,
                    material: character.material,
                    heat: None,
                },
                camera,
            ));
            self.colours.push((slot, character.colours));
            let draw = SkinDraw {
                indices: outfit.levels[level].clone(),
                out_first,
                rest_first: entry.rest_first + range.start,
                instance,
            };
            for &c in &casts {
                self.shadow_draws[c].push(draw.clone());
            }
            if seen {
                self.camera_draws.push(draw);
            }
            out_first += count;
            self.widest = self.widest.max(count);
        }
        self.job_count = u32::try_from(jobs.len()).expect("few characters");
        if jobs.is_empty() {
            return;
        }
        let mut grew = self
            .output
            .reserve(device, u64::from(out_first) * SKINNED_BYTES);
        grew |= self
            .bones
            .write(device, queue, bytemuck::cast_slice(&bones));
        grew |= self.jobs.write(device, queue, bytemuck::cast_slice(&jobs));
        self.instances
            .write(device, queue, bytemuck::cast_slice(&instances));
        if grew {
            self.bind_group = None;
        }
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self
            .rest_buffer
            .write(device, queue, bytemuck::cast_slice(&self.rest))
        {
            self.bind_group = None;
        }
        (self.static_buffer).write(device, queue, bytemuck::cast_slice(&self.statics));
        // Index buffers are written in whole words of four bytes.
        let mut indices = self.indices.clone();
        if indices.len() % 2 == 1 {
            indices.push(0);
        }
        self.index_buffer
            .write(device, queue, bytemuck::cast_slice(&indices));
        self.dirty = false;
    }

    /// Records the dispatch skinning this frame's characters.
    pub(super) fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        timer: Option<&super::timing::GpuTimer>,
    ) {
        if self.job_count == 0 {
            return;
        }
        let bind_group = self.bind_group.get_or_insert_with(|| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("skin"),
                layout: &self.layout,
                entries: &[
                    entry(0, &self.rest_buffer.buffer),
                    entry(1, &self.bones.buffer),
                    entry(2, &self.jobs.buffer),
                    entry(3, &self.slots.buffer),
                    entry(4, &self.output.buffer),
                ],
            })
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("skin"),
            timestamp_writes: timer.and_then(|t| t.compute("skin")),
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &*bind_group, &[]);
        pass.dispatch_workgroups(self.widest.div_ceil(64), self.job_count, 1);
    }

    /// Issues `draws` (the pipeline and bind groups set by the caller).
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[SkinDraw]) {
        if draws.is_empty() {
            return;
        }
        pass.set_vertex_buffer(1, self.instances.buffer.slice(..));
        pass.set_index_buffer(
            self.index_buffer.buffer.slice(..),
            wgpu::IndexFormat::Uint32,
        );
        for draw in draws {
            // Each stream from its own first vertex: the skinned ones from the character's, the
            // static ones from its level's at rest (one base vertex could not serve both).
            let skinned = u64::from(draw.out_first) * SKINNED_BYTES;
            let statics = u64::from(draw.rest_first) * STATIC_BYTES;
            pass.set_vertex_buffer(0, self.output.buffer.slice(skinned..));
            pass.set_vertex_buffer(2, self.static_buffer.buffer.slice(statics..));
            pass.draw_indexed(draw.indices.clone(), 0, draw.instance..draw.instance + 1);
        }
    }

    /// The skinned vertices (for tests).
    pub(super) fn output(&self) -> &wgpu::Buffer {
        &self.output.buffer
    }
}

fn entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}
