//! Headless modes: screenshots and the render benchmark (no window needed).

use super::*;
use crate::physics_sandbox::{PhysicsSandbox, PhysicsScene};
use crate::physics_view;
use creeer_render::Image;
use creeer_render::renderer::{AoQuality, AoSettings};

/// Command-line options of the headless screenshot mode.
#[derive(Debug)]
pub(crate) struct ScreenshotOptions {
    path: PathBuf,
    view: String,
    far: bool,
    factory: bool,
    size: (u32, u32),
    sun_elevation: f64,
    time: f64,
    shadows: ShadowQuality,
    overlay: Overlay,
    /// A physics scene to add, and how many seconds to simulate it first.
    physics: Option<(PhysicsScene, f64)>,
    /// Ambient occlusion's quality (`--ao off|low|medium|high`), and where to save its map
    /// (`--ao-map <file.png>`, G1.3d).
    ao: AoQuality,
    ao_map: Option<PathBuf>,
}

impl ScreenshotOptions {
    pub(crate) fn parse(args: &[String]) -> Result<Option<Self>, String> {
        let Some(position) = args.iter().position(|a| a == "--screenshot") else {
            return Ok(None);
        };
        let path = args
            .get(position + 1)
            .ok_or("--screenshot needs a file name")?;
        let value_of = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
        };
        Ok(Some(Self {
            path: PathBuf::from(path),
            ao: ao_quality(args)?,
            ao_map: value_of("--ao-map").map(PathBuf::from),
            view: value_of("--view")
                .cloned()
                .unwrap_or_else(|| "overview".into()),
            far: args.iter().any(|a| a == "--far"),
            factory: !args.iter().any(|a| a == "--no-factory"),
            size: (1600, 900),
            overlay: match value_of("--overlay").map(String::as_str) {
                None | Some("off") => Overlay::Off,
                Some("temperature") => Overlay::Temperature,
                Some("stress") => Overlay::AxialStress,
                Some("utilisation") => Overlay::Utilisation,
                Some("heat") => Overlay::PartTemperature,
                Some(other) => {
                    return Err(format!(
                        "--overlay expects off/temperature/stress/utilisation/heat, got `{other}`"
                    ));
                }
            },
            shadows: match value_of("--shadows").map(String::as_str) {
                None | Some("high") => ShadowQuality::High,
                Some("medium") => ShadowQuality::Medium,
                Some("low") => ShadowQuality::Low,
                Some(other) => {
                    return Err(format!("--shadows expects low/medium/high, got `{other}`"));
                }
            },
            time: match value_of("--time") {
                Some(seconds) => seconds
                    .parse()
                    .map_err(|_| format!("--time expects seconds, got `{seconds}`"))?,
                None => 1.0,
            },
            physics: match value_of("--physics").map(String::as_str) {
                None => None,
                Some(name) => {
                    let scene = PhysicsScene::ALL
                        .iter()
                        .find(|(_, n)| n.starts_with(name))
                        .ok_or(format!(
                            "--physics expects bridge/crash/stacking/rock/cooling/radiator/frame/flywheel/air/harbour, got `{name}`"
                        ))?
                        .0;
                    let seconds = match value_of("--physics-time") {
                        Some(s) => s
                            .parse()
                            .map_err(|_| format!("--physics-time expects seconds, got `{s}`"))?,
                        None => 3.0,
                    };
                    Some((scene, seconds))
                }
            },
            sun_elevation: match value_of("--sun") {
                Some(degrees) => degrees
                    .parse()
                    .map_err(|_| format!("--sun expects degrees, got `{degrees}`"))?,
                None => DEFAULT_SUN_ELEVATION,
            },
        }))
    }
}

/// Ambient occlusion's quality from `--ao off|low|medium|high` (high when not given).
pub(crate) fn ao_quality(args: &[String]) -> Result<AoQuality, String> {
    let value = args
        .iter()
        .position(|a| a == "--ao")
        .and_then(|i| args.get(i + 1));
    Ok(match value.map(String::as_str) {
        None | Some("high") => AoQuality::High,
        Some("medium") => AoQuality::Medium,
        Some("low") => AoQuality::Low,
        Some("off") => AoQuality::Off,
        Some(other) => {
            return Err(format!(
                "--ao expects off, low, medium or high, got `{other}`"
            ));
        }
    })
}

/// Renders `frames` animated frames headless at 1080p and prints frame-time percentiles.
pub(crate) fn bench(
    frames: usize,
    view: &str,
    factory: bool,
    players: Option<(&str, usize)>,
    ao: AoQuality,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = pollster::block_on(Renderer::new_headless((1920, 1080)))?;
    renderer.set_ao(AoSettings {
        quality: ao,
        ..renderer.ao_settings()
    });
    let meshes = DemoMeshes::register_for(&mut renderer);
    let players = match players {
        Some((path, count)) => {
            let (mesh, triangles) = crate::players_bench::load(path, &mut renderer.meshes)?;
            println!("  {count} stand-in players of {triangles} triangles ({path})");
            Some((mesh, count))
        }
        None => None,
    };
    let camera =
        demo::camera_preset(view, DVec3::ZERO).ok_or_else(|| format!("unknown view `{view}`"))?;
    let mut sim = OrbitSandbox::demo();
    let sky = demo::sky(DVec3::ZERO);
    let lighting = sky.lighting(
        camera.position,
        demo::sun_direction(DEFAULT_SUN_ELEVATION),
        &Lighting::default(),
    );
    let (mut scene, mut lines) = (Scene::default(), DebugLines::default());
    let mut total = TimingStats::new(frames.max(1));
    let mut build = TimingStats::new(frames.max(1));
    let mut passes = crate::gpu_passes::GpuPasses::default();
    let dt = 1.0 / f64::from(TICK_RATE_HZ);
    for frame in 0..frames + 30 {
        let start = Instant::now();
        sim.step(dt);
        let snapshot = sim.snapshot();
        #[allow(clippy::cast_precision_loss)]
        let time = frame as f64 * dt;
        let demo = DemoFrame {
            origin: DVec3::ZERO,
            time,
            orbits: OrbitView {
                previous: &snapshot,
                current: &snapshot,
                alpha: 1.0,
            },
            factory,
            overlay: Overlay::Off,
        };
        demo::build(&mut scene, &mut lines, &meshes, &demo);
        if let Some((mesh, count)) = players {
            crate::players_bench::add(&mut scene, &camera, mesh, count, 0.0);
        }
        let built = start.elapsed();
        renderer.render_offscreen(Frame {
            camera: &camera,
            scene: &scene,
            terrain: &[],
            terrain_frames: &[],
            lighting: &lighting,
            lines: &lines,
            sky: Some(&sky),
            heatmap: None,
            overlay: None,
            clear_color: BACKGROUND,
        });
        // The first 30 frames warm up caches, shader compilation and buffer growth.
        if frame >= 30 {
            total.push(start.elapsed());
            build.push(built);
            passes.push(renderer.gpu_times());
        }
    }
    let stats = renderer.stats();
    let info = renderer.adapter_info();
    let p = |s: &TimingStats, q: f64| s.percentile_ms(q).unwrap_or(f64::NAN);
    println!(
        "bench `{view}` 1920x1080, {} instances ({} drawn, {} culled), {} on {} ({:?})",
        scene.instance_count(),
        stats.instances_drawn,
        stats.instances_culled,
        frames,
        info.name,
        info.backend
    );
    println!(
        "  full frame (CPU + GPU): p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms  ->  {:.0} FPS at p50",
        p(&total, 50.0),
        p(&total, 99.0),
        total.max_ms().unwrap_or(f64::NAN),
        1000.0 / p(&total, 50.0)
    );
    println!(
        "  scene build (CPU):      p50 {:.2} ms, p99 {:.2} ms",
        p(&build, 50.0),
        p(&build, 99.0)
    );
    println!("{}", passes.report());
    Ok(())
}

/// Renders the sandbox once without a window and saves it as a PNG.
pub(crate) fn screenshot(options: &ScreenshotOptions) -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = pollster::block_on(Renderer::new_headless(options.size))?;
    renderer.set_shadow_quality(options.shadows);
    renderer.set_ao(AoSettings {
        quality: options.ao,
        ..renderer.ao_settings()
    });
    let meshes = DemoMeshes::register_for(&mut renderer);
    let origin = if options.far { FAR_ORIGIN } else { DVec3::ZERO };
    let camera = demo::camera_preset(&options.view, origin)
        .ok_or_else(|| format!("unknown view `{}`", options.view))?;

    // Advance the orbit system a few seconds so the bodies are spread out.
    let mut sim = OrbitSandbox::demo();
    for _ in 0..300 {
        sim.step(1.0 / f64::from(TICK_RATE_HZ));
    }
    let snapshot = sim.snapshot();
    let (mut scene, mut lines) = (Scene::default(), DebugLines::default());
    let orbits = OrbitView {
        previous: &snapshot,
        current: &snapshot,
        alpha: 1.0,
    };
    let demo = DemoFrame {
        origin,
        time: options.time,
        orbits,
        factory: options.factory,
        overlay: options.overlay,
    };
    demo::build(&mut scene, &mut lines, &meshes, &demo);
    if let Some((physics, seconds)) = options.physics {
        let dt = 1.0 / f64::from(TICK_RATE_HZ);
        let mut sandbox = PhysicsSandbox::new(physics, dt);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for _ in 0..(seconds / dt).round() as usize {
            sandbox.step(dt);
        }
        let snap = sandbox.snapshot();
        println!(
            "physics: t = {:.2} s, {:?} (bodies, asleep, contacts), last step {:.2} ms",
            snap.time, snap.counts, snap.step_ms
        );
        if let Some(heat) = &snap.heat {
            println!("physics: {heat}");
            let most = snap.beams.iter().map(|b| b.3).fold(0.0f32, f32::max);
            println!("physics: most used member {:.0} %", most * 100.0);
        }
        if let Some(drive) = &snap.drive {
            println!("physics: {drive}");
        }
        let at = origin + physics_view::PHYSICS_OFFSET;
        physics_view::draw(
            &mut scene,
            &meshes,
            at,
            (&snap, &snap, 1.0),
            options.overlay,
        );
    }
    let sky = demo::sky(origin);
    let lighting = sky.lighting(
        camera.position,
        demo::sun_direction(options.sun_elevation),
        &Lighting::default(),
    );
    let heatmap = options.overlay.heatmap();
    let image = renderer.capture(Frame {
        camera: &camera,
        scene: &scene,
        terrain: &[],
        terrain_frames: &[],
        lighting: &lighting,
        lines: &lines,
        sky: Some(&sky),
        heatmap: heatmap.as_ref(),
        overlay: None,
        clear_color: BACKGROUND,
    });
    image.save_png(&options.path)?;
    if let Some(path) = &options.ao_map {
        let (width, height, values) = renderer.read_ao();
        let rgba = values
            .iter()
            .flat_map(|v| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=1
                let g = (v * 255.0).round() as u8;
                [g, g, g, 255]
            })
            .collect();
        Image {
            width,
            height,
            rgba,
        }
        .save_png(path)?;
    }
    let stats = renderer.stats();
    let info = renderer.adapter_info();
    println!(
        "saved {} ({}x{}, view `{}`) on {} ({:?}): {} instances drawn, {} culled, {} draw calls, exposure {:.2} (average luminance {:.3})",
        options.path.display(),
        image.width,
        image.height,
        options.view,
        info.name,
        info.backend,
        stats.instances_drawn,
        stats.instances_culled,
        stats.draw_calls,
        stats.exposure,
        stats.average_luminance
    );
    Ok(())
}
