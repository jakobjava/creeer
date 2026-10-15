//! End-to-end rendering checks on a real GPU (headless). Skipped, with a message, on machines
//! without a usable adapter (e.g. CI runners without a GPU).

use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_render::{
    Atmosphere, Camera, DebugLines, Frame, Image, Lighting, Renderer, Scene, Sky, wgpu,
};

const RADIUS: f64 = 500_000.0;
const TOWARDS_SUN: f64 = -std::f64::consts::FRAC_PI_2;
const AWAY_FROM_SUN: f64 = std::f64::consts::FRAC_PI_2;

fn renderer() -> Option<Renderer> {
    match pollster::block_on(Renderer::new_headless((320, 180))) {
        Ok(renderer) => Some(renderer),
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

fn sky() -> Sky {
    Sky {
        atmosphere: Atmosphere::earthlike(RADIUS),
        planet_center: DVec3::new(0.0, -RADIUS, 0.0),
        sun_irradiance: [3.6, 3.4, 3.1],
        star_brightness: 1.0,
    }
}

fn sun(elevation_degrees: f64) -> DVec3 {
    let (s, c) = det::sin_cos(elevation_degrees.to_radians());
    DVec3::new(c, s, 0.0)
}

/// Renders an empty landscape (yaw −π/2 faces the sun, +π/2 faces away); returns the image
/// and the exposure.
fn render_landscape(renderer: &mut Renderer, sun_elevation: f64, yaw: f64) -> (Image, f32) {
    let sky = sky();
    let mut camera = Camera::new(DVec3::new(0.0, 2.0, 0.0));
    camera.yaw = yaw;
    let lighting = sky.lighting(camera.position, sun(sun_elevation), &Lighting::default());
    let image = renderer.capture(Frame {
        camera: &camera,
        scene: &Scene::default(),
        lighting: &lighting,
        lines: &DebugLines::default(),
        sky: Some(&sky),
        heatmap: None,
        overlay: None,
        clear_color: wgpu::Color::BLACK,
    });
    (image, renderer.stats().exposure)
}

fn mean_brightness(image: &Image) -> f64 {
    let sum: u64 = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2]))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let mean = sum as f64 / (image.rgba.len() / 4 * 3) as f64;
    mean
}

#[test]
fn eye_adaptation_brightens_the_sunset_but_keeps_it_darker_than_day() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let (day, day_exposure) = render_landscape(&mut renderer, 40.0, TOWARDS_SUN);
    let (sunset, sunset_exposure) = render_landscape(&mut renderer, 2.0, TOWARDS_SUN);

    assert!(
        day_exposure > 0.25 && day_exposure < 4.0,
        "day exposure {day_exposure}"
    );
    assert!(
        sunset_exposure > 2.0 * day_exposure,
        "sunset {sunset_exposure} vs day {day_exposure}"
    );
    let (day_mean, sunset_mean) = (mean_brightness(&day), mean_brightness(&sunset));
    assert!(
        day_mean > 60.0 && day_mean < 220.0,
        "day image mean {day_mean}"
    );
    assert!(sunset_mean > 20.0, "sunset image too dark: {sunset_mean}");
    assert!(sunset_mean < day_mean, "sunset should stay darker than day");
}

#[test]
fn sky_is_blue_above_and_planet_shows_below_the_horizon() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    // Away from the sun: near the sun, aerosols make the sky white (Mie glow), correctly.
    let (image, _) = render_landscape(&mut renderer, 40.0, AWAY_FROM_SUN);
    let top = image.pixel(160, 5);
    let bottom = image.pixel(160, 175);
    assert!(top[2] > top[0], "sky is not blue: {top:?}");
    assert_ne!(top, bottom, "no horizon visible");
}

/// The overlay must show the palette colour of the value, not the material colour.
#[test]
fn heatmap_overlay_colours_parts_by_value() {
    use creeer_math::Frame64;
    use creeer_math::glam::Vec3;
    use creeer_render::{Heatmap, Instance, Material, MeshData, Palette};

    let Some(mut renderer) = renderer() else {
        return;
    };
    let cube = renderer.meshes.add(&MeshData::cuboid(Vec3::splat(0.5)));
    let heatmap = Heatmap {
        min: 0.0,
        max: 100.0,
        palette: Palette::Sequential,
    };
    let camera = Camera::new(DVec3::new(0.0, 0.0, 3.0));
    let lighting = Lighting::default();
    let mut centre_colour = |value: f32| {
        let mut scene = Scene::default();
        scene.add(
            cube,
            Instance {
                frame: Frame64::IDENTITY,
                scale: Vec3::splat(2.0),
                // A grey material: any colour on screen must come from the overlay.
                material: Material::dielectric([0.5, 0.5, 0.5], 0.8),
                heat: Some(value),
            },
        );
        let image = renderer.capture(Frame {
            camera: &camera,
            scene: &scene,
            lighting: &lighting,
            lines: &DebugLines::default(),
            sky: None,
            heatmap: Some(&heatmap),
            overlay: None,
            clear_color: wgpu::Color::BLACK,
        });
        image.pixel(160, 90)
    };
    // Viridis: dark purple at the bottom of the scale, yellow at the top.
    let low = centre_colour(0.0);
    let high = centre_colour(100.0);
    assert!(low[2] > low[1], "low end should be purple-blue: {low:?}");
    assert!(
        high[1] > high[2] && high[0] > high[2],
        "high end should be yellow: {high:?}"
    );
}
