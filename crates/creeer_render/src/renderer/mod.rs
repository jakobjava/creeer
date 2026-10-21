//! GPU device, render targets and the per-frame passes (ADR-0003):
//!
//! scene pass (HDR, 4× MSAA, reversed-Z) → tonemap → UI.
//!
//! The renderer works with a window surface or headless (for screenshots and tests).

use crate::atmosphere::{Atmosphere, MULTISCATTER_LUT, Sky, TRANSMITTANCE_LUT, atmosphere_luts};
use crate::camera::Camera;
use crate::capture::Image;
use crate::debug_lines::{DebugLines, LineVertex};
use crate::half::f16_bits;
use crate::heatmap::Heatmap;
use crate::mesh::{MeshArena, MeshVertex};
use crate::scene::{Frustum, InstanceRaw, Lighting, Scene};
use crate::shadow::{CASCADES, Cascade, LightBasis, ShadowQuality, compute_cascades};
use encode::Globals;
use exposure_pass::ExposurePass;
use line_pass::LinePass;
use mesh_pass::MeshPass;
use sea_pass::SeaPass;
use shadow_pass::ShadowPass;
use sky_pass::SkyPass;
use targets::{Targets, scene_depth_state, scene_multisample, shader};
use terrain_pass::TerrainPass;
use tonemap_pass::TonemapPass;

mod encode;
mod exposure_pass;
mod line_pass;
mod mesh_pass;
mod noise;
mod palette;
mod readback;
mod sea_pass;
mod setup;
mod shadow_pass;
mod sky_pass;
mod targets;
mod terrain_pass;
mod tonemap_pass;
use bytemuck::{Pod, Zeroable};
use creeer_math::glam::Mat4;
use std::borrow::Cow;
use std::fmt;

/// Reversed-Z needs a float depth buffer for its precision near 0.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Scene colour before tonemapping.
const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Multisample anti-aliasing (crisp edges on thin beams).
const MSAA_SAMPLES: u32 = 4;
/// Entries of the sky radiance table by sun elevation (`terrain.wgsl`).
const SKY_TABLE: usize = 16;

const GLOBALS_WGSL: &str = include_str!("../shaders/globals.wgsl");

/// Something went wrong creating the GPU context.
#[derive(Debug)]
pub struct RenderError(String);

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RenderError {}

fn err(context: &str, e: impl fmt::Display) -> RenderError {
    RenderError(format!("{context}: {e}"))
}

/// egui output for one frame.
#[derive(Debug)]
pub struct OverlayFrame<'a> {
    /// Tessellated UI shapes.
    pub primitives: &'a [egui::ClippedPrimitive],
    /// Font and image texture changes. The renderer consumes (drains) them.
    pub textures_delta: &'a mut egui::TexturesDelta,
    /// UI scale (physical pixels per egui point).
    pub pixels_per_point: f32,
}

/// Everything needed to draw one frame.
#[derive(Debug)]
pub struct Frame<'a> {
    /// Viewpoint.
    pub camera: &'a Camera,
    /// Mesh instances.
    pub scene: &'a Scene,
    /// Terrain chunks to draw: ids given to [`Renderer::insert_terrain`], each with the chunk
    /// faces whose caps to draw (bit f for face f of `TerrainMeshData::caps`).
    pub terrain: &'a [(u64, u8)],
    /// Body frames the terrain chunks are drawn in (by the frame index given at upload);
    /// empty: every chunk in simulation space.
    pub terrain_frames: &'a [crate::TerrainFrame],
    /// Sun and ambient light.
    pub lighting: &'a Lighting,
    /// World-space debug lines.
    pub lines: &'a DebugLines,
    /// Planet and atmosphere; without one, the background is `clear_color`.
    pub sky: Option<&'a Sky>,
    /// Engineering overlay: colour parts by their measured value instead of their material.
    pub heatmap: Option<&'a Heatmap>,
    /// UI drawn on top of the 3D scene.
    pub overlay: Option<OverlayFrame<'a>>,
    /// Background colour (linear), used where no sky is drawn.
    pub clear_color: wgpu::Color,
}

/// Counters from the last rendered frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderStats {
    /// Instances that passed frustum culling.
    pub instances_drawn: usize,
    /// Instances rejected by frustum culling.
    pub instances_culled: usize,
    /// Instances drawn into shadow cascades (summed over cascades).
    pub shadow_instances: usize,
    /// Indexed draw calls issued.
    pub draw_calls: usize,
    /// Terrain chunks that passed frustum culling.
    pub terrain_chunks_drawn: usize,
    /// Terrain triangles drawn for the camera.
    pub terrain_triangles: usize,
    /// Automatic exposure (only filled in by [`Renderer::capture`]).
    pub exposure: f32,
    /// Average scene luminance the exposure was derived from (only filled in by capture).
    pub average_luminance: f32,
}

/// Owns the GPU device, the output surface (if any) and all render pipelines.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    output_format: wgpu::TextureFormat,
    size: (u32, u32),
    targets: Targets,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    mesh_pass: MeshPass,
    terrain: TerrainPass,
    shadows: ShadowPass,
    line_pass: LinePass,
    tonemap: TonemapPass,
    exposure: ExposurePass,
    /// When the last windowed frame was drawn (for exposure adaptation).
    last_frame: Option<std::time::Instant>,
    sky_pipeline: SkyPass,
    sea: SeaPass,
    egui: egui_wgpu::Renderer,
    line_vertices: Vec<LineVertex>,
    stats: RenderStats,
    /// Sky radiance by sun elevation for the atmosphere and sun it was made for.
    sky_table: Option<(Atmosphere, [f64; 3], [[f32; 4]; SKY_TABLE])>,
    /// Meshes available to [`Scene`]s drawn by this renderer.
    pub meshes: MeshArena,
}

impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer")
            .field("adapter", &self.adapter_info.name)
            .field("backend", &self.adapter_info.backend)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// GPU name, driver and backend (Vulkan / DX12 / Metal / GL).
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// Output size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Counters from the last rendered frame.
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Call when the window is resized. Zero sizes (minimised windows) are ignored.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.size = (width, height);
        if let Some((surface, config)) = &mut self.surface {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
        self.targets = Targets::new(&self.device, self.size);
        self.tonemap.rebind(&self.device, &self.targets);
        self.exposure.rebind(&self.device, &self.targets);
    }

    /// The palette model meshes name their tokens in (G1.3b): the one their models were read
    /// against ([`crate::MeshData::from_model`]). Until set, model vertices draw neutral grey.
    pub fn set_palette(&mut self, palette: &creeer_art::Palette) {
        self.mesh_pass
            .set_palette(&self.device, &self.queue, palette);
    }

    /// Changes the sun shadow resolution (see [`ShadowQuality`] for the memory cost).
    pub fn set_shadow_quality(&mut self, quality: ShadowQuality) {
        self.shadows.resize(&self.device, quality.map_size());
    }

    /// Switches between vsync (no tearing, capped at the refresh rate) and uncapped frames.
    pub fn set_vsync(&mut self, vsync: bool) {
        if let Some((surface, config)) = &mut self.surface {
            let mode = present_mode(vsync);
            if config.present_mode != mode {
                config.present_mode = mode;
                surface.configure(&self.device, config);
            }
        }
    }

    /// Draws and presents one frame to the window. Frames that cannot be drawn right now
    /// (window hidden, surface being recreated) are skipped, but UI texture changes are always
    /// applied: dropping them would lose egui's font atlas.
    pub fn render(&mut self, mut frame: Frame<'_>) {
        self.apply_ui_textures(&mut frame);
        let Some((surface, config)) = &self.surface else {
            self.free_ui_textures(&mut frame);
            return;
        };
        let surface_texture = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Some(texture),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => None,
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => {
                surface.configure(&self.device, config);
                None
            }
        };
        if let Some(surface_texture) = surface_texture {
            let view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let commands = self.encode(&frame, &view, None, false);
            self.queue.submit(commands);
            self.queue.present(surface_texture);
        }
        self.free_ui_textures(&mut frame);
    }

    /// Draws one frame into an internal off-screen image and blocks until the GPU has
    /// finished it. Used for benchmarks: the wall time of this call is the full frame cost
    /// (CPU preparation + GPU execution) without presentation or vsync.
    ///
    /// # Panics
    /// If waiting for the GPU fails (device lost).
    pub fn render_offscreen(&mut self, mut frame: Frame<'_>) {
        self.apply_ui_textures(&mut frame);
        let view = self
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("offscreen"),
                size: wgpu::Extent3d {
                    width: self.size.0,
                    height: self.size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.output_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let commands = self.encode(&frame, &view, None, false);
        self.queue.submit(commands);
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("GPU poll failed");
        self.free_ui_textures(&mut frame);
    }

    fn apply_ui_textures(&mut self, frame: &mut Frame<'_>) {
        if let Some(overlay) = &mut frame.overlay {
            for (id, deltas) in overlay.textures_delta.set.drain() {
                for delta in &deltas {
                    self.egui
                        .update_texture(&self.device, &self.queue, id, delta);
                }
            }
        }
    }

    fn free_ui_textures(&mut self, frame: &mut Frame<'_>) {
        if let Some(overlay) = &mut frame.overlay {
            for id in overlay.textures_delta.free.drain() {
                self.egui.free_texture(&id);
            }
        }
    }
}

fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    }
}
