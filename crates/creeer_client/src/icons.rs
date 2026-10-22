//! Item icons (G1.3h, `docs/art-style.md` §9): rendered by our own renderer from each item's
//! model, never drawn: one three-quarter view (azimuth 35°, elevation 30°), a key light from
//! the upper left and fixed ambient, on a transparent background over a soft drop shadow, at
//! 64 and 128 px. Cached by the packs' content hash, so a changed model gets a new icon.
//!
//! `creeer_client --icon <model> [--out <dir>]` renders one model's icons.

use crate::icon_image::Premultiplied;
use creeer_art::model::{Class, Entry};
use creeer_math::glam::{DVec3, Vec3};
use creeer_math::{Frame64, det};
use creeer_render::{
    Camera, DebugLines, Frame, Image, Instance, Lighting, Material, MeshId, Renderer, Scene,
};
use std::path::{Path, PathBuf};

/// The icons' sizes (px).
pub const SIZES: [u32; 2] = [64, 128];
/// Rendered this many times larger and reduced, for smooth edges at icon size.
const SUPERSAMPLE: u32 = 4;
/// The view (degrees): azimuth from the model's front (+Z) to its right, elevation above.
const AZIMUTH: f64 = 35.0;
const ELEVATION: f64 = 30.0;
/// The key light (degrees): from the viewer's upper left, between the view and the left.
const LIGHT_AZIMUTH: f64 = AZIMUTH - 50.0;
const LIGHT_ELEVATION: f64 = 50.0;
/// The camera this many bounding radii away: nearly orthographic, sizes across the model
/// within ±2.5 % (a true orthographic view is in docs/future-improvements.md).
const DISTANCE: f64 = 40.0;
/// Room around the model's bounding sphere, for the drop shadow.
const MARGIN: f64 = 1.12;
/// The renderer's exposure for a clear noon scene (exposure.wgsl, REFERENCE_EXPOSURE): icons
/// are exposed as the world is at noon, never adapted to their background.
const EXPOSURE: f32 = 1.5;
/// The white background (linear, before exposure): mid-tone, where tonemapping is linear.
const WHITE: f64 = 0.4;
/// The drop shadow, in parts of the icon's size: its offset right and down, its blur, and
/// its darkness.
const SHADOW_OFFSET: (f64, f64) = (0.025, 0.04);
const SHADOW_BLUR: f64 = 0.03;
const SHADOW_OPACITY: f64 = 0.45;

/// Renders icons.
pub struct Icons {
    renderer: Renderer,
}

impl Icons {
    /// A renderer for icons, with the packs' palette.
    ///
    /// # Errors
    /// Without a GPU.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let side = SIZES[1] * SUPERSAMPLE;
        let mut renderer = pollster::block_on(Renderer::new_headless((side, side)))?;
        crate::models::upload_palette(&mut renderer);
        renderer.fix_exposure(Some(EXPOSURE));
        Ok(Self { renderer })
    }

    /// The icon of the packs' model `name`, `size` px square.
    ///
    /// # Errors
    /// If the packs have no such model.
    pub fn icon(&mut self, name: &str, size: u32) -> Result<Image, String> {
        let (mesh, entry) = crate::models::load(&mut self.renderer.meshes, name)
            .ok_or_else(|| format!("the packs have no model `{name}`"))?;
        Ok(self.render(mesh, &entry, size))
    }

    fn render(&mut self, mesh: MeshId, entry: &Entry, size: u32) -> Image {
        let side = size * SUPERSAMPLE;
        self.renderer.resize(side, side);
        let (low, high) = (DVec3::from(entry.bounds.0), DVec3::from(entry.bounds.1));
        let (centre, radius) = ((low + high) * 0.5, (high - low).length() * 0.5);
        let towards = |azimuth: f64, elevation: f64| {
            let (a, e) = (azimuth.to_radians(), elevation.to_radians());
            DVec3::new(
                det::sin(a) * det::cos(e),
                det::sin(e),
                det::cos(a) * det::cos(e),
            )
        };
        let mut camera = Camera::new(centre + towards(AZIMUTH, ELEVATION) * DISTANCE * radius);
        (camera.yaw, camera.pitch) = (AZIMUTH.to_radians(), -ELEVATION.to_radians());
        #[allow(clippy::cast_possible_truncation)] // an angle
        let fov = (2.0 * det::asin(MARGIN / DISTANCE)) as f32;
        camera.fov_y = fov;
        #[allow(clippy::cast_possible_truncation)] // metres
        let near = ((DISTANCE - 2.0) * radius) as f32;
        camera.near = near;
        let lighting = Lighting {
            sun_direction: towards(LIGHT_AZIMUTH, LIGHT_ELEVATION),
            #[allow(clippy::cast_possible_truncation)] // metres
            shadow_distance: ((DISTANCE + 2.0) * radius) as f32,
            ..Lighting::default()
        };
        let mut scene = Scene::default();
        scene.add(
            mesh,
            Instance {
                frame: Frame64::from_position(DVec3::ZERO),
                scale: Vec3::ONE,
                material: Material::dielectric([1.0; 3], 0.8),
                heat: None,
            },
        );
        let mut on = |grey: f64| {
            self.renderer.capture(Frame {
                camera: &camera,
                scene: &scene,
                terrain: &[],
                terrain_frames: &[],
                lighting: &lighting,
                lines: &DebugLines::default(),
                sky: None,
                heatmap: None,
                overlay: None,
                clear_color: creeer_render::wgpu::Color {
                    r: grey,
                    g: grey,
                    b: grey,
                    a: 1.0,
                },
            })
        };
        let (black, white) = (on(0.0), on(WHITE));
        // The white as the image shows it, from a corner the model never reaches.
        let shown = |image: &Image| creeer_art::colour::decode(f64::from(image.rgba[0]) / 255.0);
        let matted = Premultiplied::matted(&black, &white, shown(&white) - shown(&black));
        let s = f64::from(size);
        let offset = (SHADOW_OFFSET.0 * s, SHADOW_OFFSET.1 * s);
        (matted.reduced(SUPERSAMPLE))
            .shadowed(offset, SHADOW_BLUR * s, SHADOW_OPACITY)
            .image()
    }
}

/// The icons of every item and tool in the packs, `size` px square.
///
/// # Errors
/// Without a GPU.
pub fn items(size: u32) -> Result<Vec<(String, Image)>, Box<dyn std::error::Error>> {
    let mut icons = Icons::new()?;
    let mut names = crate::models::names(Class::Item);
    names.extend(crate::models::names(Class::Tool));
    names
        .into_iter()
        .map(|name| Ok((name.clone(), icons.icon(&name, size)?)))
        .collect()
}

/// Where the icons of the current packs are kept: by the packs' content hash, so any change to
/// them makes new icons (the build's target directory while in development).
pub fn cache() -> PathBuf {
    let hash = crate::content::data().hash();
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/icons/{hash:016x}"))
}

/// The icon of model `name` at `size`, from the cache or rendered into it.
///
/// # Errors
/// If it cannot be rendered or written.
pub fn cached(
    icons: &mut Option<Icons>,
    name: &str,
    size: u32,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = cache().join(format!("{name}-{size}.png"));
    if !path.exists() {
        let image = match icons {
            Some(icons) => icons.icon(name, size)?,
            None => icons.insert(Icons::new()?).icon(name, size)?,
        };
        std::fs::create_dir_all(cache())?;
        image.save_png(&path)?;
    }
    Ok(path)
}

/// Renders the icons `args` asks for (after `--icon`).
///
/// # Errors
/// If the model is not in the packs, or without a GPU.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let value_of = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    let name = value_of("--icon").ok_or("--icon needs a model's name")?;
    let mut icons = None;
    for size in SIZES {
        let path = match value_of("--out") {
            Some(dir) => {
                let icons = match &mut icons {
                    Some(icons) => icons,
                    None => icons.insert(Icons::new()?),
                };
                std::fs::create_dir_all(dir)?;
                let path = Path::new(dir).join(format!("{name}-{size}.png"));
                icons.icon(name, size)?.save_png(&path)?;
                path
            }
            None => cached(&mut icons, name, size)?,
        };
        println!("{}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon_image::overlap;

    #[test]
    fn every_item_has_an_icon_and_no_two_silhouettes_alike() {
        let icons = match items(SIZES[0]) {
            Ok(icons) => icons,
            Err(error) => {
                eprintln!("skipping GPU test: {error}");
                return;
            }
        };
        assert!(!icons.is_empty(), "the packs have items");
        for (name, icon) in &icons {
            assert_eq!((icon.width, icon.height), (SIZES[0], SIZES[0]));
            // The model fills the icon well, on a transparent background.
            let solid = icon.rgba.chunks(4).filter(|p| p[3] > 127).count();
            #[allow(clippy::cast_precision_loss)] // pixel counts
            let share = solid as f64 / f64::from(SIZES[0] * SIZES[0]);
            assert!((0.25..0.8).contains(&share), "{name} covers {share}");
            assert_eq!(icon.pixel(0, 0)[3], 0, "{name}'s corner");
        }
        // Recognisable as silhouettes (`art-style.md` §7): any two overlap by IoU < 0.8.
        for (i, (a, first)) in icons.iter().enumerate() {
            for (b, second) in &icons[i + 1..] {
                let iou = overlap(first, second);
                assert!(iou < 0.8, "{a} and {b} overlap by {iou}");
            }
        }
    }
}
