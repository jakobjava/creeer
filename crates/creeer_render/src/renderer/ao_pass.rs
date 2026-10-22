//! Ambient occlusion (G1.3d, ADR-0026): GTAO after XeGTAO, in three compute passes between
//! the depth prepass and the scene pass. The scene's meshes and terrain read the result and
//! shade their ambient light with it, never the sun's.

use super::*;
use ao_textures::AoTextures;

#[path = "ao_textures.rs"]
mod ao_textures;

/// How much ambient occlusion costs and how good it is (XeGTAO's presets: slices × steps
/// per side).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AoQuality {
    /// None: ambient light unoccluded.
    Off,
    /// One slice of two steps, at half resolution.
    Low,
    /// Two slices of two steps.
    Medium,
    /// Three slices of three steps.
    #[default]
    High,
    /// Sixteen slices of sixteen steps: what the presets converge to, for validation only.
    Reference,
}

impl AoQuality {
    /// Slices, steps per side, MIP the grid is computed at (0 full, 1 half resolution).
    fn shape(self) -> Option<(f32, f32, u32)> {
        match self {
            Self::Off => None,
            Self::Low => Some((1.0, 2.0, 1)),
            Self::Medium => Some((2.0, 2.0, 0)),
            Self::High => Some((3.0, 3.0, 0)),
            Self::Reference => Some((16.0, 16.0, 0)),
        }
    }
}

/// Ambient occlusion's settings (XeGTAO's defaults where not said).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AoSettings {
    pub quality: AoQuality,
    /// The distance occluders count to (m): the length of the rays the ray-traced
    /// occlusion GTAO is fitted to.
    pub radius: f32,
    /// Power on the final value: XeGTAO's 2.2 fits ray-traced references; 1 gives the
    /// method's own integral (for tests against closed forms).
    pub power: f32,
    /// The 4×4 spatial filter (off: the raw, noisy value, for tests).
    pub denoise: bool,
    /// The darkest the ambient light may become (`art-style.md` §5: creases darken by at
    /// most half). Tests set 0 to read the raw value.
    pub floor: f32,
}

impl Default for AoSettings {
    fn default() -> Self {
        Self {
            quality: AoQuality::default(),
            radius: 1.0,
            power: 2.2,
            denoise: true,
            floor: 0.5,
        }
    }
}

/// XeGTAO's tuned constants (`XeGTAO.h`, found by searching for the least error against
/// ray-traced references).
const RADIUS_MULTIPLIER: f32 = 1.457;
const FALLOFF_RANGE: f32 = 0.615;
const SAMPLE_DISTRIBUTION_POWER: f32 = 2.0;
const DEPTH_MIP_SAMPLING_OFFSET: f32 = 3.30;

/// The constants the compute passes share (`ao_common.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct AoConstants {
    size: [f32; 2],
    pixel: [f32; 2],
    ndc_to_view_mul: [f32; 2],
    ndc_to_view_add: [f32; 2],
    mul_x_pixel: [f32; 2],
    near: f32,
    radius: f32,
    falloff: f32,
    power: f32,
    distribution: f32,
    mip_offset: f32,
    slices: f32,
    steps: f32,
    base_mip: f32,
    denoise_on: f32,
    pad: [f32; 2],
}

/// What the scene's shaders read (bind group 3): x, y one over the screen's size, z 1 when
/// occlusion is on, w the floor.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct AoView([f32; 4]);

pub(super) struct AoPass {
    settings: AoSettings,
    constants: wgpu::Buffer,
    view: wgpu::Buffer,
    prefilter: wgpu::ComputePipeline,
    last_mip: wgpu::ComputePipeline,
    main: wgpu::ComputePipeline,
    denoise: wgpu::ComputePipeline,
    layouts: ao_textures::Layouts,
    textures: AoTextures,
    /// Read by the scene's mesh and terrain shaders.
    pub(super) sample_layout: wgpu::BindGroupLayout,
    pub(super) sample_bind_group: wgpu::BindGroup,
}

impl AoPass {
    pub(super) fn new(device: &wgpu::Device, depth: &wgpu::TextureView, size: (u32, u32)) -> Self {
        let common = include_str!("../shaders/ao_common.wgsl");
        let module = |label, body: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(format!("{common}\n{body}"))),
            })
        };
        let layouts = ao_textures::Layouts::new(device);
        let pipeline = |label, module: &wgpu::ShaderModule, entry, layout| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let depth_module = module("ao depth", include_str!("../shaders/ao_depth.wgsl"));
        let main_module = module("gtao", include_str!("../shaders/gtao.wgsl"));
        let denoise_module = module("ao denoise", include_str!("../shaders/ao_denoise.wgsl"));
        let buffer = |label, size: usize| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let constants = buffer("ao constants", std::mem::size_of::<AoConstants>());
        let view = buffer("ao view", std::mem::size_of::<AoView>());
        let settings = AoSettings::default();
        let textures = AoTextures::new(
            device,
            &layouts,
            &constants,
            depth,
            size,
            base_mip(settings),
        );
        let sample_layout = ao_textures::sample_layout(device);
        let sample_bind_group = textures.sample_bind_group(device, &sample_layout, &view);
        Self {
            prefilter: pipeline("ao depth", &depth_module, "prefilter", &layouts.prefilter),
            last_mip: pipeline("ao last mip", &depth_module, "last_mip", &layouts.last_mip),
            main: pipeline("gtao", &main_module, "main", &layouts.main),
            denoise: pipeline("ao denoise", &denoise_module, "tile", &layouts.denoise),
            settings,
            constants,
            view,
            layouts,
            textures,
            sample_layout,
            sample_bind_group,
        }
    }

    pub(super) fn settings(&self) -> AoSettings {
        self.settings
    }

    /// Whether occlusion is computed (and the depth prepass drawn for it).
    pub(super) fn on(&self) -> bool {
        self.settings.quality.shape().is_some()
    }

    /// Changes the settings; the textures follow the resolution they need.
    pub(super) fn set(
        &mut self,
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        settings: AoSettings,
    ) {
        let size = self.textures.full;
        self.settings = settings;
        self.resize(device, depth, size);
    }

    /// Makes the textures for a new screen size (and depth target).
    pub(super) fn resize(
        &mut self,
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        size: (u32, u32),
    ) {
        self.textures = AoTextures::new(
            device,
            &self.layouts,
            &self.constants,
            depth,
            size,
            base_mip(self.settings),
        );
        self.sample_bind_group =
            self.textures
                .sample_bind_group(device, &self.sample_layout, &self.view);
    }

    /// Writes this frame's constants for the camera's projection.
    pub(super) fn prepare(&self, queue: &wgpu::Queue, camera: &Camera, aspect: f32) {
        let (w, h) = self.textures.full;
        #[allow(clippy::cast_precision_loss)] // screen sizes
        let inverse_full = [1.0 / w as f32, 1.0 / h as f32];
        let on = self.settings.quality.shape().is_some();
        let view = AoView([
            inverse_full[0],
            inverse_full[1],
            f32::from(u8::from(on)),
            self.settings.floor,
        ]);
        queue.write_buffer(&self.view, 0, bytemuck::bytes_of(&view));
        let Some((slices, steps, base)) = self.settings.quality.shape() else {
            return;
        };
        let (gw, gh) = self.textures.grid;
        #[allow(clippy::cast_precision_loss)]
        let (size, pixel) = ([gw as f32, gh as f32], [1.0 / gw as f32, 1.0 / gh as f32]);
        let tan_y = (0.5 * camera.fov_y).tan();
        let tan_x = tan_y * aspect;
        let mul = [2.0 * tan_x, -2.0 * tan_y];
        #[allow(clippy::cast_precision_loss)] // 0 or 1
        let constants = AoConstants {
            size,
            pixel,
            ndc_to_view_mul: mul,
            ndc_to_view_add: [-tan_x, tan_y],
            mul_x_pixel: [mul[0] * pixel[0], mul[1] * pixel[1]],
            near: camera.near,
            radius: self.settings.radius * RADIUS_MULTIPLIER,
            falloff: FALLOFF_RANGE,
            power: self.settings.power,
            distribution: SAMPLE_DISTRIBUTION_POWER,
            // The reference reads full-resolution depth only: no MIP heuristics.
            mip_offset: if self.settings.quality == AoQuality::Reference {
                100.0
            } else {
                DEPTH_MIP_SAMPLING_OFFSET
            },
            slices,
            steps,
            base_mip: base as f32,
            denoise_on: f32::from(u8::from(self.settings.denoise)),
            pad: [0.0; 2],
        };
        queue.write_buffer(&self.constants, 0, bytemuck::bytes_of(&constants));
    }

    /// Records the three passes (nothing when off).
    pub(super) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        timer: Option<&super::timing::GpuTimer>,
    ) {
        if self.settings.quality.shape().is_none() {
            return;
        }
        let t = &self.textures;
        let groups = |(w, h): (u32, u32), tile: u32| (w.div_ceil(tile), h.div_ceil(tile));
        // A compute pass each, so the profiler times each stage.
        let mut pass = begin(encoder, timer, "ao depth");
        pass.set_pipeline(&self.prefilter);
        pass.set_bind_group(0, &t.prefilter, &[]);
        let (x, y) = groups(t.full, 16);
        pass.dispatch_workgroups(x, y, 1);
        // MIP 4 has a texel for every 16 pixels; one thread each.
        pass.set_pipeline(&self.last_mip);
        pass.set_bind_group(0, &t.last_mip, &[]);
        let (x, y) = groups(t.full, 16 * 8);
        pass.dispatch_workgroups(x, y, 1);
        drop(pass);
        let mut pass = begin(encoder, timer, "ao main");
        pass.set_pipeline(&self.main);
        pass.set_bind_group(0, &t.main, &[]);
        let (x, y) = groups(t.grid, 8);
        pass.dispatch_workgroups(x, y, 1);
        drop(pass);
        let mut pass = begin(encoder, timer, "ao denoise");
        pass.set_pipeline(&self.denoise);
        pass.set_bind_group(0, &t.denoise, &[]);
        pass.dispatch_workgroups(x, y, 1);
    }

    /// The texture of the last occlusion computed (for tests and screenshots).
    pub(super) fn result(&self) -> (&wgpu::Texture, (u32, u32)) {
        (&self.textures.result, self.textures.grid)
    }
}

fn base_mip(settings: AoSettings) -> u32 {
    settings.quality.shape().map_or(0, |(_, _, base)| base)
}

/// A compute pass of its own for a stage of the occlusion, timed under `name`.
fn begin<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    timer: Option<&super::timing::GpuTimer>,
    name: &'static str,
) -> wgpu::ComputePass<'e> {
    encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some(name),
        timestamp_writes: timer.and_then(|t| t.compute(name)),
    })
}
