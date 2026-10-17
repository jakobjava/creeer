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
//! [--time <s>] [--overlay <off|temperature|stress|utilisation>] [--physics <bridge|crash|stacking|rock>
//! [--physics-time <s>]]` renders
//! one frame headless (no window) and saves it, for reviewing rendering changes.
//! `creeer_client --bench <frames> [--view <name>] [--no-factory]` renders frames headless
//! and reports frame-time percentiles (CPU scene build + GPU, no vsync).
//!
//! The F3 planet sandbox (button in the Sandbox window) streams the reference planet around the
//! camera, with physics, day and night, digging and filling. Headless:
//! `creeer_client --planet <orbit|high|low|surface|tunnel|night> <file.png> [--sun <deg>]`, and
//! `creeer_client --planet-bench [seconds]` flies from orbit to the ground (frame times).
//!
//! The F4 solar-system sandbox (button in the Sandbox window): a generated solar system with
//! n-body orbits, ships in orbit, time warp, pushing bodies, and the map (M). Headless:
//! `creeer_client --system <home|orbit|moon|map|system|lagrange|transfer> <file.png>` and
//! `creeer_client --system-bench` (frame times in orbit, on the map, at warp).
//!
//! The planet sandbox is a client of a server (F5): of one it hosts itself, or with
//! `creeer_client --connect <address:port> [--name <player>]` of a dedicated server
//! (`creeer_server`), joined at once, walking.

mod app;
mod circuit_bench;
mod demo;
mod drive_bench;
mod fly_camera;
mod headless;
mod hulls;
mod physics_drive;
mod physics_heat;
mod physics_rocks;
mod physics_sandbox;
mod physics_view;
mod planet_net;
mod planet_shot;
mod planet_ui;
mod planet_view;
mod sandbox;
mod system_bench;
mod system_cmd;
mod system_frame;
mod system_map;
mod system_shot;
mod system_sim;
mod system_ui;
mod system_view;

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

pub(crate) const TICK_RATE_HZ: u32 = 60;
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
    if args.iter().any(|a| a == "--planet-bench") {
        return planet_shot::descent_bench(&args);
    }
    if args.iter().any(|a| a == "--planet") {
        return planet_shot::run(&args);
    }
    if args.iter().any(|a| a == "--system-bench") {
        return system_bench::run();
    }
    if args.iter().any(|a| a == "--system") {
        return system_shot::run(&args);
    }
    if let Some(options) = ScreenshotOptions::parse(&args)? {
        return screenshot(&options);
    }
    let connect = match args.iter().position(|a| a == "--connect") {
        Some(i) => {
            let server = args
                .get(i + 1)
                .and_then(|a| a.parse().ok())
                .ok_or("--connect needs an address, like 192.168.1.20:27015")?;
            let name = args
                .iter()
                .position(|a| a == "--name")
                .and_then(|i| args.get(i + 1))
                .map_or_else(|| "player".to_owned(), Clone::clone);
            Some((server, name))
        }
        None => None,
    };
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    app.connect = connect;
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}
