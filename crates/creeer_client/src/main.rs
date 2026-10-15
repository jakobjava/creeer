//! Game client: window, input, rendering, UI.
//!
//! F0 tech sandbox: a free-fly camera over a reference grid with an orbiting test system that
//! runs on its own simulation thread. The overlay shows frame and tick timings and has
//! switches to prove two F0 requirements live:
//! - render/simulation decoupling (add artificial tick time; the frame rate stays smooth),
//! - camera-relative precision (move the scene 500 km from the origin; nothing jitters).

mod fly_camera;
mod sandbox;

use creeer_core::{RateMeter, SimThread, TimingStats};
use creeer_math::glam::DVec3;
use creeer_render::{Camera, Color, DebugLines, Frame, OverlayFrame, Renderer, wgpu};
use creeer_ui::{PerfInfo, egui, perf_window};
use fly_camera::FlyCamera;
use sandbox::{OrbitSandbox, SandboxSnapshot};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{CursorGrabMode, Window, WindowId};

const TICK_RATE_HZ: u32 = 60;
/// Where the scene moves to for the precision test: the surface of a 500 km planet.
const FAR_ORIGIN: DVec3 = DVec3::new(0.0, 500_000.0, 0.0);

const GRID_MINOR: Color = [0.22, 0.24, 0.28, 1.0];
const GRID_MAJOR: Color = [0.36, 0.39, 0.45, 1.0];
const AXIS_X: Color = [0.85, 0.30, 0.30, 1.0];
const AXIS_Y: Color = [0.35, 0.80, 0.35, 1.0];
const AXIS_Z: Color = [0.35, 0.50, 0.95, 1.0];
const SUN: Color = [1.0, 0.85, 0.3, 1.0];
const BODY: Color = [0.95, 0.95, 0.95, 1.0];
const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.035,
    g: 0.04,
    b: 0.05,
    a: 1.0,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}

#[derive(Default)]
struct App {
    running: Option<Running>,
    error: Option<String>,
}

/// User-adjustable sandbox settings.
#[derive(Debug)]
struct Settings {
    vsync: bool,
    tick_delay_ms: f32,
    far_origin: bool,
}

struct Running {
    window: Arc<Window>,
    renderer: Renderer,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    sim: SimThread<OrbitSandbox>,
    camera: Camera,
    fly: FlyCamera,
    looking: bool,
    lines: DebugLines,
    frame_times: TimingStats,
    sim_speed: RateMeter,
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
                        .with_title("Creëer — F0 tech sandbox")
                        .with_inner_size(LogicalSize::new(1600.0, 900.0)),
                )
                .map_err(|e| format!("create window: {e}"))?,
        );
        let size = window.inner_size();
        let settings = Settings {
            vsync: true,
            tick_delay_ms: 0.0,
            far_origin: false,
        };
        let renderer = pollster::block_on(Renderer::new(
            Box::new(event_loop.owned_display_handle()),
            Arc::clone(&window),
            (size.width, size.height),
            settings.vsync,
        ))
        .map_err(|e| e.to_string())?;
        let info = renderer.adapter_info();
        let adapter_label = format!("{} ({:?})", info.name, info.backend);

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

        let mut camera = Camera::new(DVec3::new(0.0, 18.0, 45.0));
        camera.pitch = -0.35;

        Ok(Self {
            window,
            renderer,
            egui_ctx,
            egui_state,
            sim: SimThread::spawn(OrbitSandbox::demo(), TICK_RATE_HZ),
            camera,
            fly: FlyCamera::default(),
            looking: false,
            lines: DebugLines::default(),
            frame_times: TimingStats::new(300),
            sim_speed: RateMeter::new(Duration::from_secs(1)),
            last_frame: Instant::now(),
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
        let alpha = published.alpha(now);
        self.build_scene(&published.previous, &published.current, alpha);

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
        });
        self.egui_state
            .handle_platform_output(&self.window, output.platform_output);
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);

        self.renderer.render(Frame {
            camera: &self.camera,
            lines: &self.lines,
            overlay: Some(OverlayFrame {
                primitives: &primitives,
                textures_delta: &mut output.textures_delta,
                pixels_per_point: output.pixels_per_point,
            }),
            clear_color: CLEAR,
        });
    }

    fn build_scene(&mut self, previous: &SandboxSnapshot, current: &SandboxSnapshot, alpha: f64) {
        let origin = self.scene_origin();
        let lines = &mut self.lines;
        lines.clear();
        lines.grid_xz(origin, 50.0, 1.0, GRID_MINOR);
        lines.grid_xz(origin, 500.0, 10.0, GRID_MAJOR);
        lines.line(origin, origin + DVec3::X * 5.0, AXIS_X);
        lines.line(origin, origin + DVec3::Y * 5.0, AXIS_Y);
        lines.line(origin, origin + DVec3::Z * 5.0, AXIS_Z);

        let center = origin + DVec3::Y * 10.0;
        lines.cross(center, 0.8, SUN);
        for (a, b) in previous.positions.iter().zip(&current.positions) {
            lines.cross(center + a.lerp(*b, alpha), 0.25, BODY);
        }
    }

    fn sandbox_window(&mut self, ctx: &egui::Context, snapshot: &SandboxSnapshot) {
        egui::Window::new("Sandbox")
            .default_pos([12.0, 330.0])
            .resizable(false)
            .show(ctx, |ui| {
                if ui.checkbox(&mut self.settings.vsync, "VSync").changed() {
                    self.renderer.set_vsync(self.settings.vsync);
                }
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
