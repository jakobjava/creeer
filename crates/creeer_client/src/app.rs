//! The interactive tech sandbox: window, input, simulation thread and UI.

use super::*;

#[derive(Default)]
pub(crate) struct App {
    running: Option<Running>,
    pub(crate) error: Option<String>,
}

/// User-adjustable sandbox settings.
#[derive(Debug)]
struct Settings {
    vsync: bool,
    tick_delay_ms: f32,
    far_origin: bool,
    factory: bool,
    /// Sun elevation above the horizon (degrees).
    sun_elevation: f64,
    /// Sun shadow resolution.
    shadows: ShadowQuality,
    /// Engineering overlay shown on the factory.
    overlay: Overlay,
}

struct Running {
    window: Arc<Window>,
    renderer: Renderer,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    sim: SimThread<OrbitSandbox>,
    meshes: DemoMeshes,
    scene: Scene,
    lines: DebugLines,
    lighting: Lighting,
    /// (camera height, sun elevation) the lighting was last computed for.
    lighting_key: Option<(f64, f64)>,
    camera: Camera,
    fly: FlyCamera,
    looking: bool,
    frame_times: TimingStats,
    scene_build_times: TimingStats,
    sim_speed: RateMeter,
    started: Instant,
    last_frame: Instant,
    settings: Settings,
    adapter_label: String,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        match Running::new(event_loop) {
            Ok(running) => self.running = Some(running),
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(app) = &mut self.running else {
            return;
        };
        let response = app.egui_state.on_window_event(&app.window, &event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => app.renderer.resize(size.width, size.height),
            WindowEvent::Focused(false) => {
                app.fly.release_all();
                app.set_looking(false);
            }
            WindowEvent::KeyboardInput { event, .. } if !response.consumed => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    app.fly.key(code, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => {
                let pressed = state == ElementState::Pressed;
                if !pressed || !app.egui_ctx.egui_wants_pointer_input() {
                    app.set_looking(pressed);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if !response.consumed => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    MouseScrollDelta::PixelDelta(p) => p.y / 50.0,
                };
                app.fly.scroll(notches);
            }
            WindowEvent::RedrawRequested => app.frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let (Some(app), DeviceEvent::MouseMotion { delta }) = (&mut self.running, event)
            && app.looking
        {
            app.fly.mouse_motion(delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(app) = &self.running {
            app.window.request_redraw();
        }
    }
}

impl Running {
    fn new(event_loop: &ActiveEventLoop) -> Result<Self, String> {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Creëer — tech sandbox")
                        .with_inner_size(LogicalSize::new(1600.0, 900.0)),
                )
                .map_err(|e| format!("create window: {e}"))?,
        );
        let size = window.inner_size();
        let settings = Settings {
            vsync: true,
            tick_delay_ms: 0.0,
            far_origin: false,
            factory: true,
            sun_elevation: DEFAULT_SUN_ELEVATION,
            shadows: ShadowQuality::default(),
            overlay: Overlay::Off,
        };
        let mut renderer = pollster::block_on(Renderer::new(
            Box::new(event_loop.owned_display_handle()),
            Arc::clone(&window),
            (size.width, size.height),
            settings.vsync,
        ))
        .map_err(|e| e.to_string())?;
        let info = renderer.adapter_info();
        let adapter_label = format!("{} ({:?})", info.name, info.backend);
        let meshes = DemoMeshes::register(&mut renderer.meshes);

        let egui_ctx = egui::Context::default();
        #[allow(clippy::cast_possible_truncation)] // DPI scale factors are small (1.0–4.0)
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );

        let camera = demo::camera_preset("overview", DVec3::ZERO).expect("preset exists");
        let now = Instant::now();
        Ok(Self {
            window,
            renderer,
            egui_ctx,
            egui_state,
            sim: SimThread::spawn(OrbitSandbox::demo(), TICK_RATE_HZ),
            meshes,
            scene: Scene::default(),
            lines: DebugLines::default(),
            lighting: Lighting::default(),
            lighting_key: None,
            camera,
            fly: FlyCamera::default(),
            looking: false,
            frame_times: TimingStats::new(300),
            scene_build_times: TimingStats::new(300),
            sim_speed: RateMeter::new(Duration::from_secs(1)),
            started: now,
            last_frame: now,
            settings,
            adapter_label,
        })
    }

    fn scene_origin(&self) -> DVec3 {
        if self.settings.far_origin {
            FAR_ORIGIN
        } else {
            DVec3::ZERO
        }
    }

    /// Recomputes object lighting from the atmosphere when the camera height or the sun has
    /// changed noticeably (the hemisphere integration is too costly to repeat every frame).
    fn update_lighting(&mut self, sky: &creeer_render::Sky) {
        let (height, _) = sky.height_and_up(self.camera.position);
        let sun = self.settings.sun_elevation;
        let stale = self.lighting_key.is_none_or(|(h, s)| {
            (height - h).abs() > (0.02 * h.abs()).max(1.0) || (sun - s).abs() > 0.05
        });
        if stale {
            self.lighting = sky.lighting(
                self.camera.position,
                demo::sun_direction(sun),
                &Lighting::default(),
            );
            self.lighting_key = Some((height, sun));
        }
    }

    /// Mouse look: hide and capture the cursor while the right button is held.
    fn set_looking(&mut self, looking: bool) {
        if self.looking == looking {
            return;
        }
        self.looking = looking;
        if looking {
            // Not every platform supports both grab modes; fall back gracefully.
            let _ = self
                .window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Confined));
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        }
        self.window.set_cursor_visible(!looking);
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = now - self.last_frame;
        self.last_frame = now;
        self.frame_times.push(dt);
        self.fly.update(&mut self.camera, dt.as_secs_f64());

        let published = self.sim.latest();
        self.sim_speed.record(now, published.current.time);
        let origin = self.scene_origin();
        let sky = demo::sky(origin);
        self.update_lighting(&sky);
        let build_start = Instant::now();
        let demo = DemoFrame {
            origin,
            time: (now - self.started).as_secs_f64(),
            orbits: OrbitView {
                previous: &published.previous,
                current: &published.current,
                alpha: published.alpha(now),
            },
            factory: self.settings.factory,
            overlay: self.settings.overlay,
        };
        demo::build(&mut self.scene, &mut self.lines, &self.meshes, &demo);
        self.scene_build_times.push(build_start.elapsed());

        // UI
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let ctx = self.egui_ctx.clone();
        let tick_times = self.sim.tick_times();
        let mut output = ctx.run_ui(raw_input, |ui| {
            perf_window(
                ui.ctx(),
                &PerfInfo {
                    frame_times: &self.frame_times,
                    tick_times: &tick_times,
                    tick_rate_hz: TICK_RATE_HZ,
                    tick: published.tick,
                    adapter: &self.adapter_label,
                },
            );
            self.sandbox_window(ui.ctx(), &published.current);
            if let Some(heatmap) = self.settings.overlay.heatmap() {
                let (title, unit) = self.settings.overlay.label();
                heatmap_legend(ui.ctx(), &heatmap, title, unit);
            }
        });
        self.egui_state
            .handle_platform_output(&self.window, output.platform_output);
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);

        let heatmap = self.settings.overlay.heatmap();
        self.renderer.render(Frame {
            camera: &self.camera,
            scene: &self.scene,
            lighting: &self.lighting,
            sky: Some(&sky),
            heatmap: heatmap.as_ref(),
            lines: &self.lines,
            overlay: Some(OverlayFrame {
                primitives: &primitives,
                textures_delta: &mut output.textures_delta,
                pixels_per_point: output.pixels_per_point,
            }),
            clear_color: BACKGROUND,
        });
    }

    fn sandbox_window(&mut self, ctx: &egui::Context, snapshot: &SandboxSnapshot) {
        egui::Window::new("Sandbox")
            .default_pos([12.0, 330.0])
            .resizable(false)
            .show(ctx, |ui| {
                if ui.checkbox(&mut self.settings.vsync, "VSync").changed() {
                    self.renderer.set_vsync(self.settings.vsync);
                }
                ui.checkbox(&mut self.settings.factory, "Factory (10,000 parts)");
                ui.horizontal(|ui| {
                    ui.label("Overlay:");
                    for (overlay, name) in [
                        (Overlay::Off, "off"),
                        (Overlay::Temperature, "temperature"),
                        (Overlay::AxialStress, "axial stress"),
                    ] {
                        ui.radio_value(&mut self.settings.overlay, overlay, name);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Shadows:");
                    let before = self.settings.shadows;
                    for (quality, name) in [
                        (ShadowQuality::Low, "low"),
                        (ShadowQuality::Medium, "medium"),
                        (ShadowQuality::High, "high"),
                    ] {
                        ui.radio_value(&mut self.settings.shadows, quality, name);
                    }
                    if self.settings.shadows != before {
                        self.renderer.set_shadow_quality(self.settings.shadows);
                    }
                });
                ui.add(
                    egui::Slider::new(&mut self.settings.sun_elevation, -12.0..=90.0)
                        .text("sun elevation (°)"),
                );
                let stats = self.renderer.stats();
                let build_ms = self.scene_build_times.percentile_ms(50.0).unwrap_or(0.0);
                ui.label(format!(
                    "drawn {} · culled {} · {} draw calls · scene build {build_ms:.2} ms",
                    stats.instances_drawn, stats.instances_culled, stats.draw_calls
                ));
                ui.separator();
                let delay = ui.add(
                    egui::Slider::new(&mut self.settings.tick_delay_ms, 0.0..=40.0)
                        .text("extra tick time (ms)"),
                );
                if delay.changed() {
                    self.sim.set_extra_tick_delay(Duration::from_secs_f32(
                        self.settings.tick_delay_ms / 1000.0,
                    ));
                }
                let far = ui.checkbox(
                    &mut self.settings.far_origin,
                    "Move scene 500 km from the origin",
                );
                if far.changed() {
                    // Shift the camera with the scene so the view stays identical.
                    let shift = if self.settings.far_origin {
                        FAR_ORIGIN
                    } else {
                        -FAR_ORIGIN
                    };
                    self.camera.position += shift;
                }
                ui.separator();
                ui.label(format!("simulated time: {:.1} s", snapshot.time));
                let speed = self
                    .sim_speed
                    .rate()
                    .map_or_else(|| "-".to_owned(), |r| format!("{r:.2}"));
                ui.label(format!("simulation speed: {speed}x real time"));
                ui.label(format!(
                    "orbital energy drift: {:+.2e} (leapfrog, should stay tiny)",
                    snapshot.energy_drift
                ));
                let p = self.camera.position;
                ui.label(format!("camera: ({:.3}, {:.3}, {:.3}) m", p.x, p.y, p.z));
                ui.label(format!("fly speed: {:.1} m/s", self.fly.speed));
                ui.separator();
                ui.label("Right mouse: look · WASD: move · Space/Ctrl: up/down");
                ui.label("Shift: ×10 speed · Mouse wheel: change speed");
            });
    }
}
