//! Geomorphing data (`levels.rs`): neighbours agree, and a fine chunk moved fully to the
//! coarser level lies on the coarser chunk's mesh with its shading.

use crate::chunk::ChunkKey;
use crate::elevation::fibonacci_sphere;
use crate::surface_nets::TerrainMesh;
use crate::{Planet, Terrain};
use creeer_math::glam::{DVec3, Vec3};
use std::collections::HashMap;
use std::sync::OnceLock;

fn terrain() -> &'static Terrain {
    static TERRAIN: OnceLock<Terrain> = OnceLock::new();
    TERRAIN.get_or_init(|| {
        Terrain::new(
            Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("home.ron"),
        )
    })
}

/// A point of the surface on hilly land (between 400 and 1200 m up) lying in the middle
/// third of its chunk at `level` along every axis.
fn hills(t: &Terrain, level: u8) -> DVec3 {
    fibonacci_sphere(20_000)
        .filter(|&d| (400.0..1200.0).contains(&t.surface_height(d)))
        .map(|up| up * (t.planet().datum.radius(up) + t.surface_height(up)))
        .find(|&p| {
            let key = ChunkKey::containing(p, level);
            let local = (p - key.origin()) / key.size();
            local.min_element() > 0.33 && local.max_element() < 0.67
        })
        .expect("hills in the middle of a chunk")
}

fn world(key: ChunkKey, m: &TerrainMesh, v: usize) -> DVec3 {
    key.origin() + m.positions[v].as_dvec3()
}

#[test]
fn neighbours_of_one_level_refine_shared_vertices_identically() {
    let t = terrain();
    let a = ChunkKey::containing(hills(t, 2), 2);
    let mut checked = 0;
    for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
        let b = ChunkKey::containing(a.center() + axis * a.size(), 2);
        let (Some((_, wa)), Some((_, wb))) = (t.mesh_world(a), t.mesh_world(b)) else {
            continue;
        };
        // The vertices on the common face are made by both chunks, at the same points.
        let bits = |p: DVec3| p.to_array().map(f64::to_bits);
        let from_b: HashMap<[u64; 3], usize> = (0..wb.surface_vertices)
            .map(|j| (bits(wb.v.world[j]), j))
            .collect();
        for i in 0..wa.surface_vertices {
            if let Some(&j) = from_b.get(&bits(wa.v.world[i])) {
                assert_eq!(wa.v.normals[i], wb.v.normals[j]);
                assert_eq!(wa.v.coarse_normals[i], wb.v.coarse_normals[j]);
                assert_eq!(wa.v.morph[i].to_bits(), wb.v.morph[j].to_bits());
                checked += 1;
            }
        }
    }
    assert!(checked > 20, "only {checked} shared vertices found");
}

/// The closest point of a mesh's surface to `q`, with the normal interpolated there.
fn closest_on(key: ChunkKey, m: &TerrainMesh, q: DVec3) -> (f64, Vec3) {
    let mut best = (f64::INFINITY, Vec3::ZERO);
    for tri in m.indices[..m.surface_indices].as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| world(key, m, i as usize));
        let (point, w) = closest_on_triangle(q, a, b, c);
        let d = (q - point).length();
        if d < best.0 {
            #[allow(clippy::cast_possible_truncation)] // barycentric weights in 0..1
            let w = w.map(|x| x as f32);
            let n = m.normals[tri[0] as usize] * w[0]
                + m.normals[tri[1] as usize] * w[1]
                + m.normals[tri[2] as usize] * w[2];
            best = (d, n.normalize());
        }
    }
    best
}

/// Closest point of triangle abc to p and its barycentric weights (Ericson, *Real-Time
/// Collision Detection*, 5.1.5).
fn closest_on_triangle(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> (DVec3, [f64; 3]) {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return (a, [1.0, 0.0, 0.0]);
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return (b, [0.0, 1.0, 0.0]);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return (a + ab * v, [1.0 - v, v, 0.0]);
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return (c, [0.0, 0.0, 1.0]);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return (a + ac * w, [1.0 - w, 0.0, w]);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (b + (c - b) * w, [0.0, 1.0 - w, w]);
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    (a + ab * v + ac * w, [1.0 - v - w, v, w])
}

/// Moved fully to the coarser level, a fine chunk takes the coarser chunk's shape and
/// shading: what makes the border between them invisible.
#[test]
fn a_fully_morphed_chunk_lies_on_the_coarser_mesh() {
    let t = terrain();
    for level in [1u8, 3] {
        let fine = ChunkKey::containing(hills(t, level + 1), level);
        let coarse = fine.parent();
        let (mf, mc) = (
            t.mesh_chunk(fine).expect("fine"),
            t.mesh_chunk(coarse).expect("coarse"),
        );
        let cell = coarse.cell_size();
        let (mut own, mut morphed) = ((0.0f64, 0.0f64, 0.0f64), (0.0f64, 0.0f64, 0.0f64));
        let mut count = 0.0;
        for v in 0..mf.surface_vertices {
            let q = world(fine, &mf, v);
            // Away from the coarse chunk's own border, where its mesh stops.
            let local = q - coarse.origin();
            if local.min_element() < 2.0 * cell || local.max_element() > coarse.size() - 2.0 * cell
            {
                continue;
            }
            let n = mf.normals[v];
            let target = q + n.as_dvec3() * f64::from(mf.morph[v]);
            let (d0, n0) = closest_on(coarse, &mc, q);
            let (d1, n1) = closest_on(coarse, &mc, target);
            let angle = |a: Vec3, b: Vec3| f64::from(a.angle_between(b)).to_degrees();
            own = (own.0 + d0, own.1.max(d0), own.2 + angle(n, n0));
            morphed = (
                morphed.0 + d1,
                morphed.1.max(d1),
                morphed.2 + angle(mf.coarse_normals[v], n1),
            );
            count += 1.0;
        }
        assert!(count > 100.0, "level {level}: {count} vertices");
        println!(
            "level {level} → {}: own level {:.3} m mean / {:.3} m max off the coarse mesh, \
             normals {:.2}° apart; morphed {:.3} / {:.3} m, {:.2}° (coarse cell {cell} m)",
            level + 1,
            own.0 / count,
            own.1,
            own.2 / count,
            morphed.0 / count,
            morphed.1,
            morphed.2 / count
        );
        // What remains is the coarse mesh's own chord error between its vertices.
        assert!(
            morphed.0 / count < 0.03 * cell,
            "level {level}: mean distance"
        );
        assert!(morphed.1 < 0.2 * cell, "level {level}: largest distance");
        assert!(morphed.2 / count < 1.0, "level {level}: mean normal angle");
        assert!(
            morphed.2 < 0.5 * own.2,
            "level {level}: the shading must get closer"
        );
    }
}

/// Triangles of the surface whose winding disagrees with their vertex normals.
/// Triangles of the surface whose winding disagrees with their vertex normals.
fn flipped(key: ChunkKey, m: &TerrainMesh, morphed: bool) -> usize {
    let pos = |v: usize| {
        let p = world(key, m, v);
        if morphed {
            p + m.normals[v].as_dvec3() * f64::from(m.morph[v])
        } else {
            p
        }
    };
    m.indices[..m.surface_indices]
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| {
            let [a, b, c] = t.map(|i| i as usize);
            let geometric = (pos(b) - pos(a)).cross(pos(c) - pos(a));
            let n = (m.normals[a] + m.normals[b] + m.normals[c]).as_dvec3();
            geometric.dot(n) <= 0.0
        })
        .count()
}

/// Refined vertices stay in their cells and morphing moves them along smooth normals: no
/// triangle turns over, on its own level or fully moved to the coarser one.
#[test]
fn no_triangle_turns_over() {
    let t = terrain();
    let mut total = [0usize; 3];
    for up in fibonacci_sphere(400) {
        if !(300.0..2500.0).contains(&t.surface_height(up)) {
            continue;
        }
        let p = up * (t.planet().datum.radius(up) + t.surface_height(up));
        for level in [1u8, 3, 5] {
            let key = ChunkKey::containing(p, level);
            let Some(m) = t.mesh_chunk(key) else { continue };
            total[0] += m.surface_indices / 3;
            total[1] += flipped(key, &m, false);
            total[2] += flipped(key, &m, true);
        }
    }
    println!(
        "{} triangles, {} turned over as refined, {} fully morphed",
        total[0], total[1], total[2]
    );
    assert!(total[0] > 100_000, "only {} triangles", total[0]);
    assert_eq!(total[1..], [0, 0]);
}

/// The exact gradient agrees with fine finite differences of the same field: on generated
/// ground, and around an edit, where it is the brush's or (near the crease) a cell's
/// central differences.
#[test]
fn level_gradients_are_exact() {
    use crate::edits::{Brush, Edit};
    use crate::levels::LevelField;
    let t = Terrain::new(
        Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("home.ron"),
    );
    let p = hills(&t, 3);
    let radius = 6.0;
    t.edit(Edit::Dig(Brush::Sphere { center: p, radius }));
    let up = p.normalize();
    for cell in [1.0, 8.0] {
        let mut field = LevelField::new(&t, p - 200.0, p + 200.0, cell);
        let fine = |field: &mut LevelField, q: DVec3| {
            let h = 1e-3;
            DVec3::new(
                field.density(q + DVec3::X * h) - field.density(q - DVec3::X * h),
                field.density(q + DVec3::Y * h) - field.density(q - DVec3::Y * h),
                field.density(q + DVec3::Z * h) - field.density(q - DVec3::Z * h),
            ) / (2.0 * h)
        };
        let side = up.any_orthonormal_vector();
        let (mut exact, mut crease) = (0, 0);
        for k in 0..60 {
            let q = p + side * (f64::from(k) * 2.5) + up * 0.37;
            let (d, g) = field.value_gradient(q);
            assert_eq!(d, field.density(q), "value at {k}");
            if g == field.numeric_gradient(q) {
                crease += 1;
            } else {
                let f = fine(&mut field, q);
                assert!(
                    (g - f).length() < 1e-5 * f.length(),
                    "cell {cell}, {k}: {g} vs {f}"
                );
                exact += 1;
            }
        }
        // Inside the dug ball the brush's gradient, far out the terrain's; near its rim the
        // crease, with central differences.
        assert!(
            exact > 40 && crease > 0,
            "cell {cell}: {exact} exact, {crease} near the crease"
        );
    }
}

/// Pairs of triangles sharing an edge and folded back onto each other (faces more than 120°
/// apart): flaps, judged on the geometry alone.
fn folds(m: &TerrainMesh) -> usize {
    let tris = &m.indices[..m.surface_indices];
    let face = |t: &[u32; 3]| {
        let [a, b, c] = t.map(|i| m.positions[i as usize].as_dvec3());
        (b - a).cross(c - a).normalize_or_zero()
    };
    let mut by_edge: HashMap<(u32, u32), usize> = HashMap::new();
    let mut folded = 0;
    for (n, t) in tris.as_chunks::<3>().0.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if let Some(&other) = by_edge.get(&(b, a)) {
                let o = &tris.as_chunks::<3>().0[other];
                folded += usize::from(face(t).dot(face(o)) < -0.5);
            }
            by_edge.insert((a, b), n);
        }
    }
    folded
}

/// A tunnel breaking through the ground: its rim is a crease, and its roof thinner than a
/// cell near the mouth. Vertices there stay where Surface Nets puts them: refining folds no
/// more triangles than plain Surface Nets does at such edges (moved along normals across
/// the edge, they made flaps over the opening, measured in F5).
#[test]
fn tunnel_rims_keep_their_shape() {
    use crate::edits::{Brush, Edit};
    let t = Terrain::new(
        Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("home.ron"),
    );
    let ground = hills(&t, 2);
    let up = ground.normalize();
    let east = up.any_orthonormal_vector();
    let a = ground - up * 3.0 - east * 2.0;
    let b = a + east * 30.0 - up * 6.0;
    t.edit(Edit::Dig(Brush::Capsule { a, b, radius: 3.0 }));
    t.edit(Edit::Dig(Brush::Sphere {
        center: a - east * 2.0,
        radius: 4.5,
    }));
    let (mut triangles, mut refined, mut plain) = (0, 0, 0);
    for level in [0u8, 1] {
        for k in [a - east * 3.0, a, a + east * 15.0, b] {
            let key = ChunkKey::containing(k, level);
            let Some(m) = t.mesh_chunk(key) else { continue };
            triangles += m.surface_indices / 3;
            refined += folds(&m);
            plain += folds(&crate::surface_nets::mesh(&t.sample_chunk(key), |_, _| 0));
        }
    }
    println!("{triangles} triangles: {refined} folds as refined, {plain} by Surface Nets");
    assert!(triangles > 2000, "only {triangles} triangles");
    assert!(
        refined <= plain,
        "{refined} folds, {plain} without refining"
    );
}
