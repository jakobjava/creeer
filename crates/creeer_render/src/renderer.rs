//! Window surface, GPU device and the per-frame render passes.

use crate::camera::Camera;
use crate::debug_lines::{DebugLines, LineVertex};
use creeer_math::glam::Mat4;
use std::fmt;

/// Reversed-Z needs a float depth buffer: 32-bit float has the precision near 0 that
/// reversed-Z relies on.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

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
    /// World-space debug lines.
    pub lines: &'a DebugLines,
    /// UI drawn on top of the 3D scene.
    pub overlay: Option<OverlayFrame<'a>>,
    /// Background colour.
    pub clear_color: wgpu::Color,
}

/// Owns the GPU device, the window surface and all render pipelines.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    adapter_info: wgpu::AdapterInfo,
    depth_view: wgpu::TextureView,
    lines: LinePass,
    line_vertices: Vec<LineVertex>,
    egui: egui_wgpu::Renderer,
}

impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer")
            .field("adapter", &self.adapter_info.name)
            .field("backend", &self.adapter_info.backend)
            .field("size", &(self.config.width, self.config.height))
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Creates the GPU context for a window.
    ///
    /// `display` is the platform display connection (winit's `OwnedDisplayHandle`); it is
    /// required for OpenGL/Wayland and harmless elsewhere. The backend can be forced with the
    /// `WGPU_BACKEND` environment variable (`vulkan`, `dx12`, `metal`, `gl`).
    ///
    /// # Errors
    /// If no suitable GPU adapter or surface format is available.
    pub async fn new(
        display: Box<dyn wgpu::wgt::WgpuHasDisplayHandle>,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        (width, height): (u32, u32),
        vsync: bool,
    ) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(display),
        );
        let surface = instance
            .create_surface(target)
            .map_err(|e| err("create surface", e))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                // Only matters for browsers exposing wgpu to untrusted content.
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| err("request adapter", e))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("creeer device"),
                // Portable WebGPU defaults: anything beyond must be justified in an ADR,
                // since it might not exist on Metal or older GPUs.
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await
            .map_err(|e| err("request device", e))?;

        let caps = surface.get_capabilities(&adapter);
        // egui expects a non-sRGB target and does its own gamma handling.
        let format = egui_wgpu::preferred_framebuffer_format(&caps.formats)
            .map_err(|e| err("choose surface format", e))?;
        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or_else(|| RenderError("surface is not supported by the adapter".into()))?;
        config.format = format;
        config.present_mode = present_mode(vsync);
        surface.configure(&device, &config);

        let depth_view = create_depth_view(&device, &config);
        let lines = LinePass::new(&device, format);
        let egui = egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());

        Ok(Self {
            surface,
            device,
            queue,
            config,
            adapter_info: adapter.get_info(),
            depth_view,
            lines,
            line_vertices: Vec::new(),
            egui,
        })
    }

    /// GPU name, driver and backend (Vulkan / DX12 / Metal / GL).
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// Current surface size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Call when the window is resized. Zero sizes (minimised windows) are ignored.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, &self.config);
    }

    /// Switches between vsync (no tearing, capped at the refresh rate) and uncapped frames.
    pub fn set_vsync(&mut self, vsync: bool) {
        let mode = present_mode(vsync);
        if self.config.present_mode != mode {
            self.config.present_mode = mode;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Draws and presents one frame. Frames that cannot be drawn right now (window hidden,
    /// surface being recreated) are silently skipped, but UI texture changes are always
    /// applied: dropping them would lose egui's font atlas.
    pub fn render(&mut self, frame: Frame<'_>) {
        let Frame {
            camera,
            lines,
            mut overlay,
            clear_color,
        } = frame;
        if let Some(overlay) = &mut overlay {
            for (id, deltas) in overlay.textures_delta.set.drain() {
                for delta in &deltas {
                    self.egui
                        .update_texture(&self.device, &self.queue, id, delta);
                }
            }
        }
        self.draw(camera, lines, overlay.as_ref(), clear_color);
        if let Some(overlay) = &mut overlay {
            for id in overlay.textures_delta.free.drain() {
                self.egui.free_texture(&id);
            }
        }
    }

    fn draw(
        &mut self,
        camera: &Camera,
        lines: &DebugLines,
        overlay: Option<&OverlayFrame<'_>>,
        clear_color: wgpu::Color,
    ) {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
        };
        let target = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        // Prepare 3D data.
        #[allow(clippy::cast_precision_loss)] // window sizes are far below 2^24
        let aspect = self.config.width as f32 / self.config.height as f32;
        lines.write_vertices(camera.position, &mut self.line_vertices);
        let line_count = self.lines.prepare(
            &self.device,
            &self.queue,
            &self.line_vertices,
            camera.view_projection(aspect),
        );

        // Prepare UI data.
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: overlay.map_or(1.0, |o| o.pixels_per_point),
        };
        let mut ui_commands = Vec::new();
        if let Some(overlay) = overlay {
            ui_commands = self.egui.update_buffers(
                &self.device,
                &self.queue,
                &mut encoder,
                overlay.primitives,
                &screen,
            );
        }

        // Pass 1: 3D scene.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        // Reversed-Z: 0 is infinitely far away.
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            self.lines.draw(&mut pass, line_count);
        }

        // Pass 2: UI on top.
        if let Some(overlay) = overlay {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.egui.render(&mut pass, overlay.primitives, &screen);
        }

        self.queue
            .submit(ui_commands.into_iter().chain([encoder.finish()]));
        self.queue.present(surface_texture);
    }
}

fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    }
}

fn create_depth_view(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Pipeline and buffers for debug lines.
struct LinePass {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    /// Capacity of `vertices`, in vertices.
    capacity: usize,
}

impl LinePass {
    fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("lines.wgsl"));
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("line globals"),
            size: std::mem::size_of::<[f32; 16]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("line globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("line globals"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lines"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lines"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                // Reversed-Z: nearer fragments have *greater* depth.
                depth_compare: Some(wgpu::CompareFunction::Greater),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let capacity = 1024;
        Self {
            pipeline,
            globals,
            bind_group,
            vertices: create_vertex_buffer(device, capacity),
            capacity,
        }
    }

    /// Uploads vertices and the view-projection matrix; returns the vertex count to draw.
    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[LineVertex],
        view_proj: Mat4,
    ) -> u32 {
        queue.write_buffer(
            &self.globals,
            0,
            bytemuck::cast_slice(&view_proj.to_cols_array()),
        );
        if vertices.len() > self.capacity {
            self.capacity = vertices.len().next_power_of_two();
            self.vertices = create_vertex_buffer(device, self.capacity);
        }
        queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(vertices));
        u32::try_from(vertices.len()).expect("more than 4 billion line vertices")
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, count: u32) {
        if count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..count, 0..1);
    }
}

fn create_vertex_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("line vertices"),
        size: (capacity * std::mem::size_of::<LineVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
