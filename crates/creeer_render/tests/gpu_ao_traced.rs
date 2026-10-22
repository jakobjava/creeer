//! Ambient occlusion against a ray-traced reference (G1.3d, ADR-0026): the style board's
//! crate on the ground, its occlusion traced on the CPU as XeGTAO's own references are (the
//! cosine-weighted share of rays that leave the radius without meeting anything), and
//! compared pixel by pixel with what the shipped preset draws. The error is recorded in
//! ADR-0026 and gated here against regressions. Skipped, with a message, without a GPU.

use creeer_art::Palette;
use creeer_art::model::{Entry, Model};
use creeer_data::Data;
use creeer_math::glam::{DVec3, Vec3, Vec4};
use creeer_math::{Frame64, det};
use creeer_render::renderer::{AoQuality, AoSettings};
use creeer_render::{
    Camera, DebugLines, Frame, Instance, Lighting, Material, MeshData, Renderer, Scene, wgpu,
};

const SIZE: (u32, u32) = (640, 360);
/// The radius occlusion counts to (m): GTAO's setting and the traced rays' length.
const RADIUS: f32 = 1.0;
/// Rays per traced pixel: 16 × 16 strata of the cosine-weighted hemisphere.
const STRATA: u32 = 16;

/// A triangle mesh with a bounding volume hierarchy (median splits), for the rays.
struct Tracer {
    triangles: Vec<[DVec3; 3]>,
    nodes: Vec<Node>,
}

struct Node {
    min: DVec3,
    max: DVec3,
    /// Leaf: its triangles' range; inner: its children (left, right).
    first: usize,
    count: usize,
    children: Option<(usize, usize)>,
}

impl Tracer {
    fn new(triangles: Vec<[DVec3; 3]>) -> Self {
        let mut tracer = Self {
            triangles,
            nodes: Vec::new(),
        };
        let n = tracer.triangles.len();
        tracer.build(0, n);
        tracer
    }

    fn build(&mut self, first: usize, count: usize) -> usize {
        let tris = &mut self.triangles[first..first + count];
        let (mut min, mut max) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
        for t in tris.iter() {
            for p in t {
                min = min.min(*p);
                max = max.max(*p);
            }
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            first,
            count,
            children: None,
        });
        if count > 4 {
            let axis = (max - min).max_position();
            tris.sort_by(|a, b| {
                let ca = a[0][axis] + a[1][axis] + a[2][axis];
                let cb = b[0][axis] + b[1][axis] + b[2][axis];
                ca.total_cmp(&cb)
            });
            let half = count / 2;
            let left = self.build(first, half);
            let right = self.build(first + half, count - half);
            self.nodes[index].children = Some((left, right));
        }
        index
    }

    /// The nearest hit along `origin + t·dir`, 0 < t < `far`: its distance and the face's
    /// unit normal.
    fn hit(&self, origin: DVec3, dir: DVec3, far: f64) -> Option<(f64, DVec3)> {
        let inverse = dir.recip();
        let mut best: Option<(f64, DVec3)> = None;
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let node = &self.nodes[i];
            let a = (node.min - origin) * inverse;
            let b = (node.max - origin) * inverse;
            let enter = a.min(b).max_element().max(0.0);
            let leave = a.max(b).min_element().min(best.map_or(far, |h| h.0));
            if enter > leave {
                continue;
            }
            if let Some((l, r)) = node.children {
                stack.push(l);
                stack.push(r);
                continue;
            }
            for t in &self.triangles[node.first..node.first + node.count] {
                if let Some(d) = intersect(origin, dir, t)
                    && d < best.map_or(far, |h| h.0)
                {
                    best = Some((d, (t[1] - t[0]).cross(t[2] - t[0]).normalize()));
                }
            }
        }
        best
    }
}

/// Möller–Trumbore, both faces.
fn intersect(origin: DVec3, dir: DVec3, [a, b, c]: &[DVec3; 3]) -> Option<f64> {
    let (e1, e2) = (*b - *a, *c - *a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let s = origin - *a;
    let u = s.dot(p) / det;
    let q = s.cross(e1);
    let v = dir.dot(q) / det;
    let t = e2.dot(q) / det;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 1e-6).then_some(t)
}

/// The crate model as the packs have it.
fn crate_model() -> Model {
    let data = Data::base();
    let palette = Palette::from_data(&data).expect("the base palette loads");
    let entry = Entry::all(&data)
        .expect("the models' manifest loads")
        .into_iter()
        .find(|e| e.name == "crate")
        .expect("the packs have a crate");
    Model::read(data.binary(&entry.file).expect("its file"), &palette).expect("it reads")
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

/// The traced visibility at `p` facing `n`: the cosine-weighted share of rays that travel
/// `RADIUS` meeting neither the crate nor the ground.
fn traced(tracer: &Tracer, p: DVec3, n: DVec3) -> f64 {
    let t = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = n.cross(t).normalize();
    let v = n.cross(u);
    let origin = p + n * 1e-4;
    let mut open = 0u32;
    for i in 0..STRATA {
        for j in 0..STRATA {
            // Malley's method: uniform on the disc, lifted to the hemisphere.
            let r = ((f64::from(i) + 0.5) / f64::from(STRATA)).sqrt();
            let phi = (f64::from(j) + 0.5) / f64::from(STRATA) * std::f64::consts::TAU;
            let (sin_phi, cos_phi) = det::sin_cos(phi);
            let (x, y) = (r * cos_phi, r * sin_phi);
            let dir = (u * x + v * y + n * (1.0 - r * r).max(0.0).sqrt()).normalize();
            let ground = (dir.y < 0.0)
                .then(|| -origin.y / dir.y)
                .filter(|t| *t < f64::from(RADIUS));
            if ground.is_none() && tracer.hit(origin, dir, f64::from(RADIUS)).is_none() {
                open += 1;
            }
        }
    }
    f64::from(open) / f64::from(STRATA * STRATA)
}

#[test]
fn gtao_against_the_traced_crate() {
    let mut renderer = match pollster::block_on(Renderer::new_headless(SIZE)) {
        Ok(renderer) => renderer,
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            return;
        }
    };
    let model = crate_model();
    let triangles: Vec<[DVec3; 3]> = model
        .indices
        .chunks(3)
        .map(|t| {
            let at = |i: u32| Vec3::from(model.vertices[i as usize].position).as_dvec3();
            [at(t[0]), at(t[1]), at(t[2])]
        })
        .collect();
    let tracer = Tracer::new(triangles);
    let mut scene = Scene::default();
    let slab = renderer
        .meshes
        .add(&MeshData::cuboid(Vec3::new(10.0, 0.5, 10.0)));
    let grey = Material::dielectric([0.5; 3], 0.9);
    let place = |position| Instance {
        frame: Frame64::from_position(position),
        scale: Vec3::ONE,
        material: grey,
        heat: None,
    };
    scene.add(slab, place(DVec3::new(0.0, -0.5, 0.0)));
    let mesh = renderer.meshes.add(&MeshData::from_model(&model));
    scene.add(mesh, place(DVec3::ZERO));
    // The style board's camera.
    let mut camera = Camera::new(DVec3::new(0.0, 1.55, 3.1));
    camera.pitch = -0.3;
    // The traced reference at every third pixel of the crate and the ground about it, and
    // whether it is the crate's.
    let mut samples = Vec::new();
    for y in (0..SIZE.1).step_by(3) {
        for x in (0..SIZE.0).step_by(3) {
            let dir = ray(&camera, x, y);
            let ground = (dir.y < 0.0).then(|| -camera.position.y / dir.y);
            let (t, normal, on_crate) = match (tracer.hit(camera.position, dir, 100.0), ground) {
                (Some((t, n)), g) if g.is_none_or(|g| t < g) => (t, n, true),
                (_, Some(g)) => (g, DVec3::Y, false),
                _ => continue,
            };
            let p = camera.position + dir * t;
            if p.x.abs() > 1.6 || p.z.abs() > 1.6 {
                continue;
            }
            let facing = if normal.dot(dir) > 0.0 {
                -normal
            } else {
                normal
            };
            samples.push((
                (y * SIZE.0 + x) as usize,
                traced(&tracer, p, facing),
                on_crate,
            ));
        }
    }
    let crate_n = samples.iter().filter(|s| s.2).count();
    assert!(samples.len() > 1000 && crate_n > 300, "too few pixels");
    let presets = [
        ("high", AoQuality::High, 2.2),
        ("medium", AoQuality::Medium, 2.2),
        ("low", AoQuality::Low, 2.2),
        ("high, power 1", AoQuality::High, 1.0),
        ("reference, power 1", AoQuality::Reference, 1.0),
    ];
    for (name, quality, power) in presets {
        renderer.set_ao(AoSettings {
            quality,
            power,
            radius: RADIUS,
            ..AoSettings::default()
        });
        let values = occlusion(&mut renderer, &scene, &camera);
        let error = |crate_only: bool| {
            let chosen: Vec<_> = samples.iter().filter(|s| !crate_only || s.2).collect();
            #[allow(clippy::cast_precision_loss)]
            let n = chosen.len() as f64;
            let at = |s: &&(usize, f64, bool)| upsampled(&values, quality, s.0) - s.1;
            (
                chosen.iter().map(|s| at(s).abs()).sum::<f64>() / n,
                chosen.iter().map(at).sum::<f64>() / n,
            )
        };
        let ((all, signed), (on_crate, _)) = (error(false), error(true));
        println!(
            "{name}: mean error {all:.3} ({signed:+.3}) over {} pixels, {on_crate:.3} on the crate",
            samples.len()
        );
        if quality == AoQuality::High && power > 2.0 {
            // Measured 2026-10-22 (ADR-0026, Validation): 0.033; a regression gate above it.
            assert!(all < 0.05, "high: mean error {all:.3}");
        }
    }
}

/// The occlusion at full-resolution pixel `at`: at half resolution (low), its texel's.
fn upsampled(values: &[f32], quality: AoQuality, at: usize) -> f64 {
    if quality != AoQuality::Low {
        return f64::from(values[at]);
    }
    let (x, y) = (at % SIZE.0 as usize, at / SIZE.0 as usize);
    let half = SIZE.0 as usize / 2;
    f64::from(values[(y / 2) * half + x / 2])
}

/// Draws `scene` without sun shadows and reads the occlusion back.
fn occlusion(renderer: &mut Renderer, scene: &Scene, camera: &Camera) -> Vec<f32> {
    renderer.capture(Frame {
        camera,
        scene,
        terrain: &[],
        terrain_frames: &[],
        lighting: &Lighting {
            shadow_distance: 0.0,
            ..Lighting::default()
        },
        lines: &DebugLines::default(),
        sky: None,
        heatmap: None,
        overlay: None,
        clear_color: wgpu::Color::BLACK,
    });
    renderer.read_ao().2
}
