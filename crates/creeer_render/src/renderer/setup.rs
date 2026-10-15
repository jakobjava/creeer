//! GPU device creation and renderer construction (windowed or headless).

use super::*;

async fn request_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), RenderError> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface,
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
    Ok((adapter, device, queue))
}

impl Renderer {
    /// Creates a renderer drawing into a window.
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
        let (adapter, device, queue) = request_device(&instance, Some(&surface)).await?;
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
        let size = (config.width, config.height);
        Ok(Self::build(
            device,
            queue,
            adapter.get_info(),
            Some((surface, config)),
            format,
            size,
        ))
    }

    /// Creates a renderer without a window, drawing only into images ([`Renderer::capture`]).
    ///
    /// # Errors
    /// If no GPU adapter is available.
    pub async fn new_headless((width, height): (u32, u32)) -> Result<Self, RenderError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let (adapter, device, queue) = request_device(&instance, None).await?;
        Ok(Self::build(
            device,
            queue,
            adapter.get_info(),
            None,
            wgpu::TextureFormat::Rgba8Unorm,
            (width.max(1), height.max(1)),
        ))
    }

    pub(super) fn build(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_info: wgpu::AdapterInfo,
        surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
        output_format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Self {
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                // Compute: the exposure pass reads the frame time and flags.
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT.union(wgpu::ShaderStages::COMPUTE),
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let targets = Targets::new(&device, size);
        let shadows = ShadowPass::new(
            &device,
            &globals_layout,
            GLOBALS_WGSL,
            ShadowQuality::default().map_size(),
        );
        let mesh_pass = MeshPass::new(&device, &globals_layout, &shadows.sample_layout);
        let line_pass = LinePass::new(&device, &globals_layout);
        let sky_pipeline = SkyPass::new(&device, &globals_layout);
        let exposure = ExposurePass::new(&device, &globals_layout, &targets);
        let tonemap = TonemapPass::new(
            &device,
            &globals_layout,
            output_format,
            &targets,
            &exposure.exposure,
        );
        let egui = egui_wgpu::Renderer::new(
            &device,
            output_format,
            egui_wgpu::RendererOptions::default(),
        );
        Self {
            device,
            queue,
            adapter_info,
            surface,
            output_format,
            size,
            targets,
            globals_buffer,
            globals_bind_group,
            mesh_pass,
            shadows,
            line_pass,
            tonemap,
            exposure,
            last_frame: None,
            sky_pipeline,
            egui,
            line_vertices: Vec::new(),
            stats: RenderStats::default(),
            meshes: MeshArena::default(),
        }
    }
}
