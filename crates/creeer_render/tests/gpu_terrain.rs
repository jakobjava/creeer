//! Terrain rendering on a real GPU (headless): lit by the sun as it reaches the ground, dark
//! at night, and skirts drawn only for the faces the frame asks for. Skipped, with a message,
//! on machines without a usable adapter.

use creeer_math::det;
use creeer_math::glam::{DVec3, Vec3};
use creeer_render::{
    Atmosphere, Camera, DebugLines, Frame, Image, Lighting, Renderer, Scene, Sky, TerrainMeshData,
    TerrainVertex, wgpu,
};

const RADIUS: f64 = 500_000.0;

fn renderer() -> Option<Renderer> {
    match pollster::block_on(Renderer::new_headless((160, 90))) {
        Ok(renderer) => Some(renderer),
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

/// A 200 m square of red ground at the top of the planet (`surface`), and as skirt of face 0
/// a red wall standing 30 m in front of the camera.
fn patch() -> TerrainMeshData {
    let red = [200, 30, 30];
    let v = |x: f32, y: f32, z: f32| TerrainVertex::new(Vec3::new(x, y, z), Vec3::Y, 1.0, red, 0.9);
    let wall = |x: f32, y: f32| TerrainVertex::new(Vec3::new(x, y, -30.0), Vec3::Z, 1.0, red, 0.9);
    TerrainMeshData {
        vertices: vec![
            v(-100.0, 0.0, -100.0),
            v(100.0, 0.0, -100.0),
            v(100.0, 0.0, 100.0),
            v(-100.0, 0.0, 100.0),
            wall(-100.0, 0.0),
            wall(100.0, 0.0),
            wall(100.0, 200.0),
            wall(-100.0, 200.0),
        ],
        // Counter-clockwise seen from above; the wall faces the camera (+Z).
        indices: vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7],
        surface_indices: 6,
        skirts: [(6, 12), (12, 12), (12, 12), (12, 12), (12, 12), (12, 12)],
    }
}

fn render(renderer: &mut Renderer, sun_elevation: f64, pitch: f64, skirts: u8) -> Image {
    let sky = Sky {
        atmosphere: Atmosphere::earthlike(RADIUS),
        planet_center: DVec3::ZERO,
        sun_irradiance: [3.6, 3.4, 3.1],
        star_brightness: 1.0,
    };
    let ground = DVec3::Y * RADIUS;
    renderer.insert_terrain(7, ground, &patch());
    let mut camera = Camera::new(ground + DVec3::Y * 20.0);
    camera.pitch = pitch;
    let (s, c) = det::sin_cos(sun_elevation.to_radians());
    let to_sun = DVec3::new(c, s, 0.0);
    let lighting = sky.lighting(camera.position, to_sun, &Lighting::default());
    renderer.capture(Frame {
        camera: &camera,
        scene: &Scene::default(),
        terrain: &[(7, skirts)],
        lighting: &lighting,
        lines: &DebugLines::default(),
        sky: Some(&sky),
        heatmap: None,
        overlay: None,
        clear_color: wgpu::Color::BLACK,
    })
}

fn luminance(p: [u8; 4]) -> f64 {
    0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])
}

#[test]
fn terrain_is_lit_by_day_and_dark_at_night() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    assert_eq!(renderer.terrain_chunk_count(), 0);
    let down = -1.2;
    let day = render(&mut renderer, 50.0, down, 0);
    let ground = day.pixel(80, 70);
    assert!(
        ground[0] > ground[1] + 20 && ground[0] > ground[2] + 20,
        "not red: {ground:?}"
    );
    assert_eq!(renderer.terrain_chunk_count(), 1);
    // Sun 20° below the horizon: the planet itself shadows the ground. Even with the
    // automatic exposure brightening the night, it stays far darker (measured: ~8×).
    let night = render(&mut renderer, -20.0, down, 0);
    let (lit, unlit) = (luminance(ground), luminance(night.pixel(80, 70)));
    assert!(unlit < lit * 0.3, "night ground {unlit} vs day {lit}");
}

#[test]
fn skirts_are_drawn_only_for_the_faces_asked_for() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    // Looking at the horizon: the wall (a skirt of face 0) fills the middle of the view.
    let without = render(&mut renderer, 50.0, 0.0, 0);
    let with = render(&mut renderer, 50.0, 0.0, 1);
    let (a, b) = (without.pixel(80, 30), with.pixel(80, 30));
    assert!(b[0] > b[2] + 20, "the wall is not drawn: {b:?}");
    assert!(
        a[2] >= a[0],
        "a skirt was drawn without being asked for: {a:?}"
    );
    renderer.remove_terrain(7);
    assert_eq!(renderer.terrain_chunk_count(), 0);
}
