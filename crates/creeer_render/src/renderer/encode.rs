//! Per-frame command recording: globals, culling, shadow, scene, tonemap and UI passes.

use super::*;

/// Per-frame constants (must match globals.wgsl).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct Globals {
    view_proj: [[f32; 4]; 4],
    sun_direction: [f32; 4],
    sun_color: [f32; 4],
    sky_ambient: [f32; 4],
    ground_ambient: [f32; 4],
    up: [f32; 4],
    params: [f32; 4],
    camera_forward: [f32; 4],
    cascades: [[[f32; 4]; 4]; CASCADES],
    cascade_far: [f32; 4],
    cascade_texel: [f32; 4],
    inv_view_proj: [[f32; 4]; 4],
    atmo_rayleigh: [f32; 4],
    atmo_mie: [f32; 4],
    atmo_geometry: [f32; 4],
    sun_irradiance: [f32; 4],
    ground_albedo: [f32; 4],
    heatmap: [f32; 4],
    sky_irradiance: [[f32; 4]; SKY_TABLE],
}

impl Renderer {
    /// Records all passes of one frame into command buffers targeting `output`.
    pub(super) fn encode(
        &mut self,
        frame: &Frame<'_>,
        output: &wgpu::TextureView,
        after: Option<&dyn Fn(&mut wgpu::CommandEncoder)>,
        snap_exposure: bool,
    ) -> Vec<wgpu::CommandBuffer> {
        // Exposure adapts over real time; screenshots (no history) jump to the target.
        let now = std::time::Instant::now();
        let adaptation_dt = self
            .last_frame
            .map_or(0.0, |last| (now - last).as_secs_f32().min(0.25));
        let snap_exposure = snap_exposure || self.last_frame.is_none();
        self.last_frame = Some(now);

        // Upload meshes registered since the last frame.
        if let Some((vertices, indices)) = self.meshes.take_upload() {
            self.mesh_pass
                .upload_meshes(&self.device, &self.queue, vertices, indices);
        }

        #[allow(clippy::cast_precision_loss)] // window sizes are far below 2^24
        let aspect = self.size.0 as f32 / self.size.1 as f32;
        let view_proj = frame.camera.view_projection(aspect);
        let lighting = frame.lighting;
        let shadows_on = lighting.shadow_distance > 0.0;
        let basis = LightBasis::new(lighting.sun_direction);
        let cascades = compute_cascades(
            frame.camera,
            aspect,
            &basis,
            lighting.shadow_distance.max(1.0),
            self.shadows.size(),
        );
        let rgb = |c: [f32; 3]| [c[0], c[1], c[2], 0.0];
        let sky_irradiance = self.sky_table(frame.sky);
        let globals = Globals {
            sky_irradiance,
            view_proj: view_proj.to_cols_array_2d(),
            sun_direction: lighting.sun_direction.as_vec3().extend(0.0).to_array(),
            sun_color: rgb(lighting.sun_color),
            sky_ambient: rgb(lighting.sky_ambient),
            ground_ambient: rgb(lighting.ground_ambient),
            up: lighting.up.as_vec3().extend(0.0).to_array(),
            #[allow(clippy::cast_precision_loss)] // map sizes are small powers of two
            params: [
                lighting.exposure,
                self.shadows.size() as f32,
                adaptation_dt,
                f32::from(u8::from(snap_exposure)),
            ],
            // Shadow distance 0 disables sampling in the shader (every depth is "beyond").
            camera_forward: frame
                .camera
                .forward()
                .as_vec3()
                .extend(lighting.shadow_distance.max(0.0))
                .to_array(),
            cascades: cascades.map(|c: Cascade| c.view_proj.to_cols_array_2d()),
            cascade_far: cascades.map(|c| c.far),
            cascade_texel: cascades.map(|c| c.texel),
            inv_view_proj: view_proj.inverse().to_cols_array_2d(),
            heatmap: frame
                .heatmap
                .map_or([0.0; 4], |h| [1.0, h.min, h.max, h.palette.shader_id()]),
            ..sky_globals(frame.sky, frame.camera)
        };
        self.queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        self.shadows.write_cascades(&self.queue, &cascades);
        if let Some(sky) = frame.sky {
            self.sky_pipeline.prepare(&self.queue, &sky.atmosphere);
        }

        self.stats = self.mesh_pass.prepare(
            &self.device,
            &self.queue,
            &self.meshes,
            frame.scene,
            frame.camera,
            view_proj,
            shadows_on.then_some((&cascades, &basis)),
        );
        self.terrain.prepare(
            &self.device,
            &self.queue,
            frame.terrain,
            frame.terrain_frames,
            frame.camera.position,
            &Frustum::from_view_projection(view_proj),
            shadows_on.then_some((&cascades, &basis)),
        );
        self.stats.terrain_chunks_drawn = self.terrain.drawn();
        self.stats.terrain_triangles = self.terrain.triangles;
        frame
            .lines
            .write_vertices(frame.camera.position, &mut self.line_vertices);
        let line_count = self
            .line_pass
            .prepare(&self.device, &self.queue, &self.line_vertices);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.0, self.size.1],
            pixels_per_point: frame.overlay.as_ref().map_or(1.0, |o| o.pixels_per_point),
        };
        let mut commands = Vec::new();
        if let Some(overlay) = &frame.overlay {
            commands = self.egui.update_buffers(
                &self.device,
                &self.queue,
                &mut encoder,
                overlay.primitives,
                &screen,
            );
        }

        // Pass 0: sun shadow cascades (cleared even when disabled, so sampling stays valid).
        self.shadows.encode(
            &mut encoder,
            &self.globals_bind_group,
            self.mesh_pass.buffers().as_ref(),
            &self.meshes,
            &self.mesh_pass.shadow_draws,
            &self.terrain,
        );

        // Pass 1: 3D scene into the multisampled HDR target, resolved to a single sample.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.hdr_msaa,
                    depth_slice: None,
                    resolve_target: Some(&self.targets.hdr_resolved),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(frame.clear_color),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        // Reversed-Z: 0 is infinitely far away.
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            self.mesh_pass
                .draw(&mut pass, &self.meshes, &self.shadows.sample_bind_group);
            self.terrain.draw(
                &mut pass,
                &self.shadows.sample_bind_group,
                self.sky_pipeline.luts().1,
            );
            if frame.sky.is_some() {
                self.sky_pipeline.draw(&mut pass);
            }
            self.line_pass.draw(&mut pass, line_count);
        }

        // Measure the image and adapt the exposure (compute), then tonemap with it.
        self.exposure
            .encode(&mut encoder, &self.globals_bind_group, self.size);

        // Pass 2: tonemap HDR → output.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tonemap"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            self.tonemap.draw(&mut pass);
        }

        // Pass 3: UI on top.
        if let Some(overlay) = &frame.overlay {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: output,
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

        if let Some(after) = after {
            after(&mut encoder);
        }
        commands.push(encoder.finish());
        commands
    }
}

/// Atmosphere part of the globals (all zero when there is no sky).
fn sky_globals(sky: Option<&Sky>, camera: &Camera) -> Globals {
    let mut globals = Globals::zeroed();
    let Some(sky) = sky else {
        return globals;
    };
    #[allow(clippy::cast_possible_truncation)] // physical constants fit f32 comfortably
    let f = |v: f64| v as f32;
    let a = &sky.atmosphere;
    let (height, _) = sky.height_and_up(camera.position);
    globals.atmo_rayleigh = [
        f(a.rayleigh_scattering[0]),
        f(a.rayleigh_scattering[1]),
        f(a.rayleigh_scattering[2]),
        f(a.rayleigh_scale_height),
    ];
    globals.atmo_mie = [
        f(a.mie_scattering),
        f(a.mie_extinction),
        f(a.mie_scale_height),
        f(a.mie_g),
    ];
    // The camera height comes from f64: subtracting the radius in f32 would lose metres.
    globals.atmo_geometry = [f(a.planet_radius), f(a.height), f(height), 1.0];
    let [r, g, b] = sky.sun_irradiance.map(f);
    globals.sun_irradiance = [r, g, b, sky.star_brightness];
    let [r, g, b] = a.ground_albedo.map(f);
    globals.ground_albedo = [r, g, b, 0.0];
    globals
}

impl Renderer {
    /// The sky radiance table for terrain lighting (`terrain.wgsl`): the cosine-weighted
    /// radiance of the sky at the ground for sun cosines from −0.2 to 1, integrated once
    /// per atmosphere and sun (`atmosphere::local_light`).
    fn sky_table(&mut self, sky: Option<&Sky>) -> [[f32; 4]; SKY_TABLE] {
        let Some(sky) = sky else {
            return [[0.0; 4]; SKY_TABLE];
        };
        if let Some((atmosphere, sun, table)) = &self.sky_table
            && *atmosphere == sky.atmosphere
            && sun.map(f64::to_bits) == sky.sun_irradiance.map(f64::to_bits)
        {
            return *table;
        }
        let table = std::array::from_fn(|i| {
            #[allow(clippy::cast_precision_loss)] // 16 entries
            let mu_s = -0.2 + 1.2 * i as f64 / (SKY_TABLE - 1) as f64;
            let to_sun = creeer_math::glam::DVec3::new((1.0 - mu_s * mu_s).sqrt(), mu_s, 0.0);
            let light = crate::atmosphere::local_light(
                &sky.atmosphere,
                0.0,
                creeer_math::glam::DVec3::Y,
                to_sun,
                sky.sun_irradiance,
            );
            #[allow(clippy::cast_possible_truncation)] // light values are small
            let f = |v: f64| v as f32;
            [f(light.sky[0]), f(light.sky[1]), f(light.sky[2]), 0.0]
        });
        self.sky_table = Some((sky.atmosphere, sky.sun_irradiance, table));
        table
    }
}
