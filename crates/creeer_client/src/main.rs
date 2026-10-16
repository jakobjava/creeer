//! Game client: window, input, rendering, UI.
//!
//! Tech sandbox: a free-fly camera over an orbiting test system (running on its own
//! simulation thread) and an optional factory of 10,000 instanced parts. The overlay shows
//! frame and tick timings and has switches to prove requirements live:
//! - render/simulation decoupling (add artificial tick time; the frame rate stays smooth),
//! - camera-relative precision (move the scene 500 km from the origin; nothing jitters),
//! - the F1 instancing budget (10,000 parts at 60 FPS).
//!
//! `creeer_client --screenshot <file.png> [--view <name>] [--far] [--no-factory] [--sun <deg>]
//! [--time <s>] [--overlay <off|temperature|stress|utilisation>] [--physics <bridge|crash|stacking>
//! [--physics-time <s>]]` renders
//! one frame headless (no window) and saves it, for reviewing rendering changes.
//! `creeer_client --bench <frames> [--view <name>] [--no-factory]` renders frames headless
//! and reports frame-time percentiles (CPU scene build + GPU, no vsync).

mod app;
mod demo;
mod fly_camera;
mod headless;
mod physics_sandbox;
mod physics_view;
mod sandbox;

use app::App;
use headless::{ScreenshotOptions, bench, screenshot};

use creeer_core::{RateMeter, SimThread, Simulation, TimingStats};
use creeer_math::glam::DVec3;
use creeer_render::{
    Camera, DebugLines, Frame, Lighting, OverlayFrame, Renderer, Scene, ShadowQuality, wgpu,
};
use creeer_ui::{PerfInfo, egui, heatmap_legend, perf_window};
use demo::{DemoFrame, DemoMeshes, OrbitView, Overlay};
use fly_camera::FlyCamera;
use sandbox::{OrbitSandbox, SandboxSnapshot};
use std::path::PathBuf;
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
/// Default sun elevation above the horizon (degrees): late morning.
const DEFAULT_SUN_ELEVATION: f64 = 40.0;
/// Behind the sky nothing is visible; the atmosphere pass covers every empty pixel.
const BACKGROUND: wgpu::Color = wgpu::Color::BLACK;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == "--bench") {
        let frames = args
            .get(i + 1)
            .and_then(|n| n.parse().ok())
            .ok_or("--bench needs a frame count")?;
        let view = args
            .iter()
            .position(|a| a == "--view")
            .and_then(|i| args.get(i + 1))
            .map_or("overview", String::as_str);
        let factory = !args.iter().any(|a| a == "--no-factory");
        return bench(frames, view, factory);
    }
    if let Some(options) = ScreenshotOptions::parse(&args)? {
        return screenshot(&options);
    }
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}
