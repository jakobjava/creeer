//! Ambient occlusion on a real GPU (G1.3d, ADR-0026) against closed forms: a sphere resting
//! on a plane, and a floor meeting a wall. GTAO's own integral (power 1, no floor) is read
//! back and compared at the floor's pixels, found by casting each pixel's ray on the CPU.
//! Skipped, with a message, on machines without a usable adapter.

use creeer_math::glam::{DVec3, Vec3, Vec4};
use creeer_math::{Frame64, det};
use creeer_render::renderer::{AoQuality, AoSettings};
use creeer_render::{
    Camera, DebugLines, Frame, Instance, Lighting, Material, MeshData, Renderer, Scene, wgpu,
};

const SIZE: (u32, u32) = (640, 360);

fn renderer() -> Option<Renderer> {
    match pollster::block_on(Renderer::new_headless(SIZE)) {
        Ok(mut renderer) => {
            renderer.set_ao(AoSettings {
                quality: AoQuality::Reference,
                radius: 4.0,
                power: 1.0,
                denoise: false,
                floor: 0.0,
            });
            Some(renderer)
        }
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

fn grey() -> Material {
    Material::dielectric([0.5, 0.5, 0.5], 0.8)
}

/// The ground: a slab whose top is the plane y = 0.
fn ground(renderer: &mut Renderer, scene: &mut Scene) {
    let slab = renderer
        .meshes
        .add(&MeshData::cuboid(Vec3::new(30.0, 0.5, 30.0)));
    scene.add(
        slab,
        Instance {
            frame: Frame64::from_position(DVec3::new(0.0, -0.5, 0.0)),
            scale: Vec3::ONE,
            material: grey(),
            heat: None,
        },
    );
}

/// Draws `scene` from `camera` and reads the occlusion back.
fn occlusion(renderer: &mut Renderer, scene: &Scene, camera: &Camera) -> Vec<f32> {
    let lighting = Lighting {
        shadow_distance: 0.0,
        ..Lighting::default()
    };
    renderer.capture(Frame {
        camera,
        scene,
        terrain: &[],
        terrain_frames: &[],
        lighting: &lighting,
        lines: &DebugLines::default(),
        sky: None,
        heatmap: None,
        overlay: None,
        clear_color: wgpu::Color::BLACK,
    });
    let (width, height, values) = renderer.read_ao();
    assert_eq!((width, height), SIZE);
    values
}

/// The ray through pixel (x, y), as a unit direction from the camera.
fn ray(camera: &Camera, x: u32, y: u32) -> DVec3 {
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (SIZE.0 as f32, SIZE.1 as f32);
    #[allow(clippy::cast_precision_loss)]
    let ndc = Vec4::new(
        2.0 * (x as f32 + 0.5) / w - 1.0,
        1.0 - 2.0 * (y as f32 + 0.5) / h,
        1.0,
        1.0,
    );
    let near = camera.view_projection(w / h).inverse() * ndc;
    (near.truncate() / near.w).as_dvec3().normalize()
}

/// Where the ray from `camera` along `dir` meets y = 0, if ahead.
fn floor_hit(camera: &Camera, dir: DVec3) -> Option<DVec3> {
    let t = -camera.position.y / dir.y;
    (t > 0.0).then(|| camera.position + dir * t)
}

/// GTAO against the closed form for a sphere of radius R resting on a plane: from a point of
/// the plane at distance d from where it touches, the sphere hides the cosine-weighted share
/// R³ / (d² + R²)^{3/2} of the sky (its form factor, the whole sphere being above the
/// point's horizon).
#[test]
fn a_sphere_on_a_plane_occludes_as_its_closed_form() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    const R: f64 = 0.5;
    let mut scene = Scene::default();
    ground(&mut renderer, &mut scene);
    #[allow(clippy::cast_possible_truncation)]
    let ball = renderer.meshes.add(&MeshData::uv_sphere(R as f32, 96, 48));
    scene.add(
        ball,
        Instance {
            frame: Frame64::from_position(DVec3::new(0.0, R, 0.0)),
            scale: Vec3::ONE,
            material: grey(),
            heat: None,
        },
    );
    let mut camera = Camera::new(DVec3::new(0.0, 2.5, 3.0));
    camera.pitch = -det::atan(2.5 / 3.0);
    let values = occlusion(&mut renderer, &scene, &camera);
    // Rings of the floor in front of the sphere (towards the camera), by distance.
    let rings = [0.6, 0.8, 1.0, 1.25, 1.5];
    let mut sums = vec![(0.0, 0.0, 0usize); rings.len() - 1];
    for y in 0..SIZE.1 {
        for x in 0..SIZE.0 {
            let dir = ray(&camera, x, y);
            let Some(p) = floor_hit(&camera, dir) else {
                continue;
            };
            // The sphere in front of the floor along this ray: not a floor pixel.
            let to_centre = DVec3::new(0.0, R, 0.0) - camera.position;
            let along = to_centre.dot(dir);
            if (to_centre - dir * along).length() < R * 1.02 {
                continue;
            }
            let d = det::hypot(p.x, p.z);
            if p.z <= 0.3 * d {
                continue;
            }
            let Some(ring) = rings.windows(2).position(|r| d >= r[0] && d < r[1]) else {
                continue;
            };
            let closed = 1.0 - R * R * R / det::pow(d * d + R * R, 1.5);
            let at = (y * SIZE.0 + x) as usize;
            sums[ring].0 += f64::from(values[at]);
            sums[ring].1 += closed;
            sums[ring].2 += 1;
        }
    }
    for (ring, (gtao, closed, n)) in sums.iter().enumerate() {
        assert!(*n > 200, "ring {ring}: only {n} floor pixels");
        #[allow(clippy::cast_precision_loss)]
        let (gtao, closed) = (gtao / *n as f64, closed / *n as f64);
        println!(
            "d {:.2}–{:.2} m: GTAO {gtao:.3}, closed form {closed:.3} ({n} pixels)",
            rings[ring],
            rings[ring + 1]
        );
        assert!(
            (gtao - closed).abs() < 0.03,
            "ring {ring}: GTAO {gtao:.3} against {closed:.3}"
        );
    }
}

/// A block standing on the floor: half its width, its height, half its depth (m). Its front
/// face is the plane z = 0.
const BLOCK: (f64, f64, f64) = (0.3, 0.3, 0.15);

/// The share of the sky, cosine-weighted, that a point `p` of the floor in front of the block
/// sees past it: every ray up from `p` that passes the plane z = 0 within the block's front
/// face is hidden (from in front of the block only the front face can be met). Midpoint
/// quadrature over the hemisphere.
fn visibility_by_block(p: DVec3) -> f64 {
    let (n_theta, n_phi) = (300, 600);
    let (mut hidden, mut total) = (0.0, 0.0);
    for i in 0..n_theta {
        let theta = (f64::from(i) + 0.5) / f64::from(n_theta) * std::f64::consts::FRAC_PI_2;
        let (s, c) = det::sin_cos(theta);
        for j in 0..n_phi {
            let phi = (f64::from(j) + 0.5) / f64::from(n_phi) * std::f64::consts::TAU;
            let (sin_phi, cos_phi) = det::sin_cos(phi);
            let dir = DVec3::new(s * cos_phi, c, -s * sin_phi);
            let w = c * s;
            total += w;
            if dir.z < 0.0 {
                let t = -p.z / dir.z;
                let at = p + dir * t;
                if at.x.abs() <= BLOCK.0 && at.y <= BLOCK.1 {
                    hidden += w;
                }
            }
        }
    }
    1.0 - hidden / total
}

/// GTAO at a crease where a block meets the floor, against the exact visibility found by
/// quadrature. The block lies wholly within GTAO's radius, so no sample fades: an upright
/// wall would not (GTAO bounds what occludes by its radius, and fades its horizons out
/// towards it).
#[test]
fn a_floor_by_a_block_sees_the_sky_past_it() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = Scene::default();
    ground(&mut renderer, &mut scene);
    #[allow(clippy::cast_possible_truncation)]
    let half = Vec3::new(BLOCK.0 as f32, (BLOCK.1 / 2.0) as f32, BLOCK.2 as f32);
    let block = renderer.meshes.add(&MeshData::cuboid(half));
    scene.add(
        block,
        Instance {
            frame: Frame64::from_position(DVec3::new(0.0, BLOCK.1 / 2.0, -BLOCK.2)),
            scale: Vec3::ONE,
            material: grey(),
            heat: None,
        },
    );
    let mut camera = Camera::new(DVec3::new(0.0, 1.0, 1.6));
    camera.pitch = -det::atan(0.85 / 1.6);
    // The block lies within 0.5 m of every point tested: inside the 1 m radius's full weight
    // (to 0.56 m), with the steps close enough to find its edges.
    renderer.set_ao(AoSettings {
        radius: 1.0,
        ..renderer.ao_settings()
    });
    let values = occlusion(&mut renderer, &scene, &camera);
    let bands = [0.01, 0.03, 0.06, 0.12, 0.25];
    let mut worst = 0.0f64;
    for band in bands.windows(2) {
        let (mut sum, mut exact, mut n) = (0.0, 0.0, 0usize);
        for y in 0..SIZE.1 {
            for x in 0..SIZE.0 {
                let dir = ray(&camera, x, y);
                let Some(p) = floor_hit(&camera, dir) else {
                    continue;
                };
                // Floor pixels in front of the middle of the block (none hidden by it: the
                // camera looks at its front face from in front).
                if p.z >= band[0] && p.z < band[1] && p.x.abs() < 0.5 * BLOCK.0 {
                    sum += f64::from(values[(y * SIZE.0 + x) as usize]);
                    exact += visibility_by_block(p);
                    n += 1;
                }
            }
        }
        assert!(n > 30, "band {band:?}: only {n} pixels");
        #[allow(clippy::cast_precision_loss)]
        let (gtao, exact) = (sum / n as f64, exact / n as f64);
        println!("z {band:?} m: GTAO {gtao:.3}, exact {exact:.3} ({n} pixels)");
        worst = worst.max((gtao - exact).abs());
    }
    assert!(worst < 0.03, "GTAO off the exact visibility by {worst:.3}");
}
