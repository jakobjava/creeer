//! The interactive tech sandbox: window, input, simulation thread and UI.

use super::*;

mod planet;
mod sandbox_ui;
pub(crate) mod system;

#[derive(Default)]
pub(crate) struct App {
    running: Option<Running>,
    pub(crate) error: Option<String>,
    /// A dedicated server to join at once (`--connect`), and the player's name.
    pub(crate) connect: Option<(std::net::SocketAddr, String)>,
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
    physics: physics_view::PhysicsView,
    /// The circuit bench (F6.3) and the drivetrain bench (F6.4).
    circuit: crate::circuit_bench::CircuitBench,
    drive: crate::drive_bench::DriveBench,
    /// The chemistry lab (F6.5).
    lab: crate::lab::Lab,
    /// The pipe bench (F6.6).
    water: crate::water_bench::WaterBench,
    /// The control panel (F6.7).
    controls: crate::control_panel::ControlPanel,
    /// The planet sandbox, when it is open (instead of the tech demo).
    planet: Option<crate::planet_view::PlanetView>,
    /// The solar-system sandbox, when it is open.
    system: Option<crate::system_view::SystemView>,
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
            Ok(mut running) => {
                if let Some((server, name)) = self.connect.take()
                    && let Err(e) = running.enter_connected(server, &name)
                {
                    self.error = Some(e);
                    event_loop.exit();
                }
                self.running = Some(running);
            }
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
                    let pressed = event.state == ElementState::Pressed;
                    if !(pressed && !event.repeat && (app.system_key(code) || app.planet_key(code)))
                    {
                        app.fly.key(code, pressed);
                    }
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
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if !response.consumed && !app.egui_ctx.egui_wants_pointer_input() => {
                if let Some(planet) = &mut app.planet {
                    planet.use_tool(&app.camera);
                }
                if let Some(system) = &app.system
                    && !system.map
                {
                    system.dig(&app.camera, system.snapshot().current.anchor);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if !response.consumed => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    MouseScrollDelta::PixelDelta(p) => p.y / 50.0,
                };
                if !app.system_scroll(notches) {
                    app.fly.scroll(notches);
                }
            }
            WindowEvent::RedrawRequested => app.frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let (Some(app), DeviceEvent::MouseMotion { delta }) = (&mut self.running, event)
            && app.looking
            && !app.system_mouse(delta.0, delta.1)
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
            physics: physics_view::PhysicsView::new(TICK_RATE_HZ),
            circuit: crate::circuit_bench::CircuitBench::new(),
            drive: crate::drive_bench::DriveBench::new(),
            lab: crate::lab::Lab::new(),
            water: crate::water_bench::WaterBench::new(),
            controls: crate::control_panel::ControlPanel::new(),
            planet: None,
            system: None,
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
        self.circuit.advance(dt.as_secs_f64());
        self.drive.advance(dt.as_secs_f64());
        self.lab.advance(dt.as_secs_f64());
        self.water.advance(dt.as_secs_f64());
        self.controls.advance(dt.as_secs_f64());
        self.frame_times.push(dt);
        if self.planet.is_some() {
            self.planet_frame(dt);
            return;
        }
        if self.system.is_some() {
            self.system_frame(now, dt);
            return;
        }
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
        self.physics.draw(
            &mut self.scene,
            &self.meshes,
            origin,
            now,
            self.settings.overlay,
        );
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
            self.physics.window(ui.ctx(), &mut self.settings.overlay);
            self.circuit.window(ui.ctx());
            self.drive.window(ui.ctx());
            self.lab.window(ui.ctx());
            self.water.window(ui.ctx());
            self.controls.window(ui.ctx());
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
            terrain: &[],
            terrain_frames: &[],
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
}
