//! The shimmer check (G1.3c, ADR-0026; `docs/art-style.md` §6: detail finer than a pixel
//! fades, it never shimmers). Crates of three ages near and far, a field of parts and a long
//! ground, rendered while the camera turns a twentieth of a pixel a frame.
//!
//! Turning that slowly, a well-filtered image changes almost linearly from frame to frame, so
//! each pixel's second difference in time, `I(t+1) − 2·I(t) + I(t−1)`, stays near zero. Detail
//! popping in and out between pixels (noise or grain lines finer than a pixel) makes it jump.
//!
//! Edges step too, as 4× multisampling crosses its samples: that is the edges' antialiasing,
//! which no surface can mend. So the same motion is rendered a second time with every surface
//! one colour (the engineering overlay): what changes sharply there, in space or in time, is
//! geometry. The surfaces are judged away from it, and held under [`GATE`]; the edges'
//! flicker is reported beside them.
//!
//! `creeer_client --shimmer [--frames <n>] [--map <file.png>]` prints both and fails above the
//! gate; the map shows the flicker in red, and the edges left out in blue.

use crate::demo::{self, DemoMeshes};
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_render::{
    Camera, DebugLines, Frame, Heatmap, Image, Instance, Lighting, Material, MeshId, Renderer,
    Scene,
};

/// A pixel flickers when its second differences' root mean square exceeds this (8-bit levels
/// of luminance).
pub const FLICKER: f32 = 2.0;
/// The largest share of the surfaces that may flicker.
pub const GATE: f32 = 0.002;
/// How far the camera turns each frame (pixels).
const STEP_PIXELS: f32 = 0.05;
/// In the one-colour render, a step this large between neighbours (8-bit levels) is an edge…
const EDGE: f32 = 4.0;
/// …and the surfaces are judged this many pixels away from it.
const WIDEN: usize = 2;

/// What the check measured over a set of pixels.
#[derive(Clone, Copy, Debug)]
pub struct Shimmer {
    /// The share of the pixels that flicker.
    pub share: f32,
    /// Percentiles of their flicker: the 99th and 99.9th (8-bit levels).
    pub p99: f32,
    pub p999: f32,
}

/// The luminance of each pixel of `image` (8-bit levels, as displayed).
fn luminance(image: &Image) -> Vec<f32> {
    image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|[r, g, b, _]| {
            0.2126 * f32::from(*r) + 0.7152 * f32::from(*g) + 0.0722 * f32::from(*b)
        })
        .collect()
}

/// Each pixel's flicker in `frames` (luminance, all of one size, at least three): the root
/// mean square of its second differences in time (8-bit levels).
pub fn flicker(frames: &[Vec<f32>]) -> Vec<f32> {
    assert!(frames.len() >= 3, "a second difference needs three frames");
    let steps = frames.len() - 2;
    (0..frames[0].len())
        .map(|i| {
            let sum: f32 = frames
                .windows(3)
                .map(|w| {
                    let d2 = w[2][i] - 2.0 * w[1][i] + w[0][i];
                    d2 * d2
                })
                .sum();
            #[allow(clippy::cast_precision_loss)] // a few dozen frames
            let mean = sum / steps as f32;
            mean.sqrt()
        })
        .collect()
}

/// Measures the flicker of `frames` (as [`flicker`] takes them) over the pixels `keep` marks,
/// or all.
pub fn measure(frames: &[Vec<f32>], keep: Option<&[bool]>) -> Shimmer {
    let mut flicker: Vec<f32> = flicker(frames)
        .into_iter()
        .enumerate()
        .filter(|&(i, _)| keep.is_none_or(|keep| keep[i]))
        .map(|(_, f)| f)
        .collect();
    let pixels = flicker.len().max(1);
    flicker.sort_by(f32::total_cmp);
    let at = |q: f64| {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )] // an index within the image
        let k = ((pixels - 1) as f64 * q).round() as usize;
        flicker.get(k).copied().unwrap_or(0.0)
    };
    let flickering = flicker.iter().filter(|&&f| f > FLICKER).count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = flickering as f32 / pixels as f32;
    Shimmer {
        share,
        p99: at(0.99),
        p999: at(0.999),
    }
}

/// The pixels near geometry in `flat`, frames of `width` pixels with every surface one colour:
/// where the first frame steps by [`EDGE`] between neighbours or a pixel flickers, widened by
/// [`WIDEN`] pixels.
pub fn edges(flat: &[Vec<f32>], width: usize) -> Vec<bool> {
    let first = &flat[0];
    let height = first.len() / width;
    let flickers = flicker(flat);
    let mut edge: Vec<bool> = flickers.iter().map(|&f| f > FLICKER).collect();
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            for j in [
                (x + 1 < width).then_some(i + 1),
                (y + 1 < height).then_some(i + width),
            ]
            .into_iter()
            .flatten()
            {
                if (first[i] - first[j]).abs() > EDGE {
                    edge[i] = true;
                    edge[j] = true;
                }
            }
        }
    }
    // Widened: rows, then columns.
    let widen = |edge: &[bool], step: usize, len: usize| -> Vec<bool> {
        (0..edge.len())
            .map(|i| {
                let along = (i / step) % len;
                (along.saturating_sub(WIDEN)..=(along + WIDEN).min(len - 1))
                    .any(|a| edge[i - along * step + a * step])
            })
            .collect()
    };
    let rows = widen(&edge, 1, width);
    widen(&rows, width, height)
}

/// What the check found: the edges' flicker (in the one-colour render), and the surfaces'
/// away from them, which cover `surface_share` of the image.
#[derive(Clone, Copy, Debug)]
pub struct Check {
    pub edges: Shimmer,
    pub surfaces: Shimmer,
    pub surface_share: f32,
}

/// The scene: the crate new, a year and four years weathered close by and in rows far off,
/// pipes and balls among them, on a long slab of ground; every part showing `heat` if given.
fn scene(meshes: &DemoMeshes, crate_model: MeshId, heat: Option<f32>) -> Scene {
    let mut scene = Scene::default();
    let mut add = |mesh, at: DVec3, turn: f64, scale: Vec3, material| {
        let frame = Frame64::new(at, DQuat::from_rotation_y(turn));
        scene.add(
            mesh,
            Instance {
                frame,
                scale,
                material,
                heat,
            },
        );
    };
    let ground = Material::dielectric([0.42, 0.31, 0.21], 0.92);
    add(
        meshes.cube,
        DVec3::new(0.0, -0.1, -40.0),
        0.0,
        Vec3::new(60.0, 0.2, 90.0),
        ground,
    );
    for (row, distance) in [3.0, 9.0, 18.0, 35.0].into_iter().enumerate() {
        for (k, age) in [0.0, 1.0, 4.0].into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)] // a few crates
            let (x, turn) = ((k as f64 - 1.0) * (1.6 + 0.3 * row as f64), 0.35 * k as f64);
            let wood = Material::dielectric([1.0; 3], 0.8).aged(age, 0.0);
            add(
                crate_model,
                DVec3::new(x, 0.0, -distance),
                turn,
                Vec3::ONE,
                wood,
            );
        }
    }
    let steel = Material::metal([0.56, 0.57, 0.58], 0.35);
    let paint = Material::dielectric([0.85, 0.55, 0.1], 0.5);
    for k in 0..6 {
        let z = -6.0 - 5.0 * f64::from(k);
        let side = if k % 2 == 0 { -4.5 } else { 4.5 };
        // A pipe lying across the view and a ball beside it.
        add(
            meshes.cylinder,
            DVec3::new(side, 0.15, z),
            1.2,
            Vec3::new(0.3, 3.0, 0.3),
            steel,
        );
        add(
            meshes.sphere,
            DVec3::new(side * 0.6, 0.4, z - 2.0),
            0.0,
            Vec3::splat(0.8),
            paint,
        );
    }
    scene
}

/// Renders `frames` frames of `scene` (with `heatmap` if given), the camera turning a
/// twentieth of a pixel each, after one to settle on; returns their luminance.
fn render(
    renderer: &mut Renderer,
    scene: &Scene,
    heatmap: Option<&Heatmap>,
    frames: usize,
) -> Vec<Vec<f32>> {
    let mut camera = Camera::new(DVec3::new(0.0, 1.6, 0.5));
    camera.pitch = -0.12;
    #[allow(clippy::cast_precision_loss)] // window sizes
    let focal = renderer.size().1 as f32 / (2.0 * (0.5 * camera.fov_y).tan());
    let sky = demo::sky(DVec3::ZERO);
    let sun = demo::sun_direction(50.0);
    (0..=frames)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)] // a few dozen frames
            let turn = f64::from(STEP_PIXELS / focal) * k as f64;
            camera.yaw = turn;
            let lighting = sky.lighting(camera.position, sun, &Lighting::default());
            luminance(&renderer.capture(Frame {
                camera: &camera,
                scene,
                terrain: &[],
                terrain_frames: &[],
                lighting: &lighting,
                lines: &DebugLines::default(),
                sky: Some(&sky),
                heatmap,
                overlay: None,
                clear_color: creeer_render::wgpu::Color::BLACK,
            }))
        })
        .skip(1)
        .collect()
}

/// What the check found, the shaded frames (luminance) and the pixels near edges.
type Found = (Check, Vec<Vec<f32>>, Vec<bool>);

/// Runs the check at `size` pixels over `frames` frames.
pub fn check(size: (u32, u32), frames: usize) -> Result<Found, Box<dyn std::error::Error>> {
    let mut renderer = pollster::block_on(Renderer::new_headless(size))?;
    let meshes = DemoMeshes::register_for(&mut renderer);
    let crate_model = meshes
        .models
        .crate_box
        .ok_or("the packs have no crate model")?;
    let shaded = render(
        &mut renderer,
        &scene(&meshes, crate_model, None),
        None,
        frames,
    );
    let one_colour = Heatmap {
        min: 0.0,
        max: 1.0,
        palette: creeer_render::Palette::Sequential,
    };
    let flat = render(
        &mut renderer,
        &scene(&meshes, crate_model, Some(0.5)),
        Some(&one_colour),
        frames,
    );
    let near = edges(&flat, size.0 as usize);
    let keep: Vec<bool> = near.iter().map(|&e| !e).collect();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let surface_share = keep.iter().filter(|&&k| k).count() as f32 / keep.len() as f32;
    let found = Check {
        edges: measure(&flat, None),
        surfaces: measure(&shaded, Some(&keep)),
        surface_share,
    };
    Ok((found, shaded, near))
}

/// Runs the check described by `args` (after `--shimmer`).
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let value_of = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    let frames = match value_of("--frames") {
        Some(n) => n.parse().map_err(|_| "--frames expects a number")?,
        None => 32,
    };
    let size = (1280, 720);
    let (found, shaded, near) = check(size, frames)?;
    if let Some(path) = value_of("--map") {
        // The first frame dimmed; flicker in red (full at 6 levels), the edges left out blue.
        let mut rgba = Vec::with_capacity(shaded[0].len() * 4);
        for ((&level, &f), &edge) in shaded[0].iter().zip(&flicker(&shaded)).zip(&near) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0–255
            let (grey, red) = ((level / 3.0) as u8, (f * 42.5).min(255.0) as u8);
            let blue = if edge { grey.saturating_add(60) } else { grey };
            rgba.extend_from_slice(&[grey.max(red), grey, blue, 255]);
        }
        let map = Image {
            width: size.0,
            height: size.1,
            rgba,
        };
        map.save_png(std::path::Path::new(path))?;
    }
    let (s, e) = (found.surfaces, found.edges);
    println!(
        "surfaces ({:.0} % of the image): {:.3} % flicker (gate {:.1} %), p99 {:.2}, p99.9 {:.2} levels",
        100.0 * found.surface_share,
        100.0 * s.share,
        100.0 * GATE,
        s.p99,
        s.p999
    );
    println!(
        "edges (every surface one colour): {:.2} % of the image flickers, p99 {:.2}, p99.9 {:.2} levels",
        100.0 * e.share,
        e.p99,
        e.p999
    );
    if s.share > GATE {
        return Err("the surfaces shimmer more than the gate allows".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "shimmer_tests.rs"]
mod tests;
