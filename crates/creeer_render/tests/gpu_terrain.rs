//! Terrain rendering on a real GPU (headless): lit by the sun as it reaches the ground, dark
//! at night, and caps drawn only for the faces the frame asks for. Skipped, with a message,
//! on machines without a usable adapter.

use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_render::{
    Atmosphere, Camera, DebugLines, Frame, Image, Lighting, Renderer, Scene, Sky, TerrainFrame,
    TerrainMeshData, TerrainVertex, wgpu,
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

/// A 200 m square of red ground at the top of the planet (`surface`), and as cap of face 0
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
        caps: [(6, 12), (12, 12), (12, 12), (12, 12), (12, 12), (12, 12)],
        chunk_size: 0.0,
    }
}

fn render(renderer: &mut Renderer, sun_elevation: f64, pitch: f64, caps: u8) -> Image {
    render_patch(renderer, &patch(), (sun_elevation, pitch, caps))
}

fn render_patch(
    renderer: &mut Renderer,
    mesh: &TerrainMeshData,
    (sun_elevation, pitch, caps): (f64, f64, u8),
) -> Image {
    let sky = Sky {
        atmosphere: Atmosphere::earthlike(RADIUS),
        planet_center: DVec3::ZERO,
        sun_irradiance: [3.6, 3.4, 3.1],
        star_brightness: 1.0,
    };
    let ground = DVec3::Y * RADIUS;
    renderer.insert_terrain(7, ground, mesh);
    let mut camera = Camera::new(ground + DVec3::Y * 20.0);
    camera.pitch = pitch;
    let (s, c) = det::sin_cos(sun_elevation.to_radians());
    let to_sun = DVec3::new(c, s, 0.0);
    let lighting = sky.lighting(camera.position, to_sun, &Lighting::default());
    renderer.capture(Frame {
        camera: &camera,
        scene: &Scene::default(),
        terrain: &[(7, caps)],
        terrain_frames: &[],
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
fn caps_are_drawn_only_for_the_faces_asked_for() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    // Looking at the horizon: the wall (a cap of face 0) fills the middle of the view.
    let without = render(&mut renderer, 50.0, 0.0, 0);
    let with = render(&mut renderer, 50.0, 0.0, 1);
    let (a, b) = (without.pixel(80, 30), with.pixel(80, 30));
    assert!(b[0] > b[2] + 20, "the wall is not drawn: {b:?}");
    assert!(
        a[2] >= a[0],
        "a cap was drawn without being asked for: {a:?}"
    );
    renderer.remove_terrain(7);
    assert_eq!(renderer.terrain_chunk_count(), 0);
}

/// The same patch drawn in a body frame that is turned and moved, seen by a camera turned and
/// moved with it, under a sun and sky moved with it, gives the same picture: what a moon's
/// terrain needs while the moon flies past.
#[test]
fn terrain_in_a_turned_and_moved_body_frame_looks_the_same() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let reference = render(&mut renderer, 40.0, -0.6, 0);
    renderer.remove_terrain(7);
    let turn = DQuat::from_axis_angle(DVec3::new(0.3, -0.5, 0.8).normalize(), 2.1);
    let shift = DVec3::new(3.1e7, -2.2e7, 1.4e7);
    let sky = Sky {
        atmosphere: Atmosphere::earthlike(RADIUS),
        planet_center: shift,
        sun_irradiance: [3.6, 3.4, 3.1],
        star_brightness: 1.0,
    };
    let ground = DVec3::Y * RADIUS;
    renderer.insert_terrain_in(7, 0, ground, &patch());
    let mut camera = Camera::new(turn * (ground + DVec3::Y * 20.0) + shift);
    camera.frame = turn;
    camera.pitch = -0.6;
    let (s, c) = det::sin_cos(40f64.to_radians());
    let to_sun = turn * DVec3::new(c, s, 0.0);
    let lighting = sky.lighting(camera.position, to_sun, &Lighting::default());
    let moved = renderer.capture(Frame {
        camera: &camera,
        scene: &Scene::default(),
        terrain: &[(7, 0)],
        terrain_frames: &[TerrainFrame {
            position: shift,
            rotation: turn,
        }],
        lighting: &lighting,
        lines: &DebugLines::default(),
        sky: Some(&sky),
        heatmap: None,
        overlay: None,
        clear_color: wgpu::Color::BLACK,
    });
    let mut worst = 0u8;
    for y in 0..90 {
        for x in 0..160 {
            let (a, b) = (reference.pixel(x, y), moved.pixel(x, y));
            for k in 0..3 {
                worst = worst.max(a[k].abs_diff(b[k]));
            }
        }
    }
    // The stars behind turn with the camera; the ground and sky must not change.
    assert!(worst <= 6, "largest channel difference {worst}");
}

/// Geomorphing: far enough from the camera (5 chunk sizes), a vertex takes the coarser
/// level's normal; with no chunk size there is no blending.
#[test]
fn far_terrain_blends_to_the_coarser_level() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    // The coarser level's ground turns away from the sun (which stands at +X).
    let away = Vec3::new(-0.8, 0.6, 0.0);
    let blended = |chunk_size: f32| {
        let mut mesh = patch();
        for v in &mut mesh.vertices {
            *v = v.with_coarser(away, 0.0);
        }
        mesh.chunk_size = chunk_size;
        mesh
    };
    let view = (35.0, -1.2, 0);
    let fine = render_patch(&mut renderer, &blended(0.0), view);
    // The patch is about 20 m away: with 1 m chunks it lies past 5 sizes, fully blended.
    let coarse = render_patch(&mut renderer, &blended(1.0), view);
    // With 100 m chunks it is nearer than 4.25 sizes: not blended at all.
    let near = render_patch(&mut renderer, &blended(100.0), view);
    let (f, c, n) = (fine.pixel(80, 70), coarse.pixel(80, 70), near.pixel(80, 70));
    // (The capture's automatic exposure brightens the darker image, so compare the change
    // itself rather than a brightness ratio.)
    let change = (0..3).map(|k| f[k].abs_diff(c[k])).max().unwrap_or(0);
    assert!(change >= 25, "blended {c:?} vs own {f:?}");
    assert_eq!(n, f, "nothing blends before 4.25 chunk sizes");
}
