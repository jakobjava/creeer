use crate::chunk::{ChunkKey, ChunkSamples, Occupancy};
use crate::surface_nets::{TerrainMesh, mesh};
use crate::{Planet, Terrain};
use creeer_math::glam::{DVec3, IVec3, Vec3};
use std::collections::{HashMap, HashSet};

fn key(level: u8, x: i32, y: i32, z: i32) -> ChunkKey {
    ChunkKey {
        level,
        pos: IVec3::new(x, y, z),
    }
}

/// Directed edges used by exactly one triangle in the given index range.
fn open_edges(indices: &[u32]) -> Vec<(u32, u32)> {
    let directed: HashSet<(u32, u32)> = indices
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
        .collect();
    directed
        .iter()
        .filter(|&&(a, b)| !directed.contains(&(b, a)))
        .copied()
        .collect()
}

fn surface(m: &TerrainMesh) -> &[u32] {
    &m.indices[..m.surface_indices]
}

#[test]
fn keys_nest_and_locate_points() {
    let k = key(3, -2, 5, 7);
    assert_eq!(k.size(), 256.0);
    for child in k.children() {
        assert_eq!(child.parent(), k);
        assert_eq!(child.size() * 2.0, k.size());
    }
    let p = k.origin() + DVec3::new(1.0, 255.0, 100.0);
    assert_eq!(ChunkKey::containing(p, 3), k);
    assert_eq!(ChunkKey::containing(p, 0).parent().parent().parent(), k);
    // A chunk straddling the planet centre: nearest distance 0.
    assert_eq!(key(0, -1, -1, -1).radial_range().0, 0.0);
    let (near, far) = key(0, 10, 0, 0).radial_range();
    assert!((near - 320.0).abs() < 1e-9 && far > 352.0);
}

#[test]
fn a_small_sphere_meshes_closed_and_on_its_surface() {
    let r = 9.3;
    let c = DVec3::splat(16.0);
    let samples = ChunkSamples::from_fn(key(0, 0, 0, 0), |p| (p - c).length() - r);
    let m = mesh(&samples, |_, _| 1);
    assert!(!m.is_empty());
    // Closed: no open edges, so no caps, and V − E + F = 2 for a sphere.
    assert!(open_edges(&m.indices).is_empty(), "sphere mesh has holes");
    assert_eq!(m.surface_indices, m.indices.len());
    let faces = m.indices.len() / 3;
    let edges = faces * 3 / 2;
    let euler = m.positions.len() as i64 - edges as i64 + faces as i64;
    assert_eq!(euler, 2);
    for (p, n) in m.positions.iter().zip(&m.normals) {
        let radial = (p.as_dvec3() - c).normalize();
        let distance = (p.as_dvec3() - c).length();
        assert!(
            (distance - r).abs() < 0.1,
            "vertex {distance} m from centre"
        );
        assert!(n.as_dvec3().dot(radial) > 0.97, "normal {n} not radial");
    }
    // Triangles face outwards.
    for t in m.indices.as_chunks::<3>().0 {
        let [a, b, cc] = t.map(|i| m.positions[i as usize]);
        let face = (b - a).cross(cc - a);
        assert!(face.dot((a + b + cc) / 3.0 - c.as_vec3()) > 0.0);
    }
    assert!(m.materials.iter().all(|&mat| mat == 1));
}

#[test]
fn neighbouring_chunks_join_without_gaps() {
    // A tilted, wavy ground through a 2×1×2 block of chunks: welded together, the surface
    // may only be open along the block's outer rim.
    // Stays within y 13..28, inside the single row of chunks.
    let ground = |p: DVec3| p.y - 16.0 - 0.15 * p.x + 3.0 * creeer_math::det::sin(p.z * 0.2);
    let mut welded: HashMap<[i64; 3], u32> = HashMap::new();
    let mut indices = Vec::new();
    for (x, z) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let k = key(0, x, 0, z);
        let m = mesh(&ChunkSamples::from_fn(k, ground), |_, _| 0);
        for &i in surface(&m) {
            let p = m.positions[i as usize].as_dvec3() + k.origin();
            #[allow(clippy::cast_possible_truncation)]
            let grid = (p * 1000.0).round().as_i64vec3().to_array();
            let next = u32::try_from(welded.len()).expect("small");
            indices.push(*welded.entry(grid).or_insert(next));
        }
    }
    let positions: HashMap<u32, [i64; 3]> = welded.iter().map(|(&p, &i)| (i, p)).collect();
    for (a, b) in open_edges(&indices) {
        let mid = |i: u32| DVec3::from_array(positions[&i].map(|v| v as f64 / 1000.0));
        let m = (mid(a) + mid(b)) * 0.5;
        // The rim: the block's outer faces (x, z at 0 and 64).
        let rim = m.x < 1e-3 || m.x > 64.0 - 1e-3 || m.z < 1e-3 || m.z > 64.0 - 1e-3;
        assert!(rim, "gap inside the block at {m}");
    }
}

fn home() -> Terrain {
    Terrain::new(Planet::load(&crate::data_path("planets/home.ron")).expect("loads"))
}

#[test]
fn chunks_on_the_real_planet() {
    let terrain = home();
    let dir = DVec3::new(0.2, 0.9, -0.4).normalize();
    let surface = dir * (terrain.planet().datum.radius(dir) + terrain.surface_height(dir));
    let at = ChunkKey::containing(surface, 0);
    assert_eq!(terrain.occupancy(at), Occupancy::Mixed);
    let samples = terrain.sample_chunk(at);
    assert!(samples.has_surface());
    let m = mesh(&samples, |_, _| 0);
    assert!(m.triangle_count() > 500, "{} triangles", m.triangle_count());
    // Far above and deep below need no sampling.
    assert_eq!(
        terrain.occupancy(ChunkKey::containing(surface + dir * 9000.0, 0)),
        Occupancy::Air
    );
    assert_eq!(
        terrain.occupancy(ChunkKey::containing(surface - dir * 9000.0, 0)),
        Occupancy::Rock
    );
    // A coarse chunk around the same place also has surface.
    let coarse = ChunkKey::containing(surface, 6);
    assert_eq!(terrain.occupancy(coarse), Occupancy::Mixed);
    assert!(terrain.sample_chunk(coarse).has_surface());
}

#[test]
#[ignore = "measurement: run in release with --nocapture"]
fn print_chunk_costs() {
    use std::time::Instant;
    let terrain = home();
    let dir = DVec3::new(0.2, 0.9, -0.4).normalize();
    let surface = dir * (terrain.planet().datum.radius(dir) + terrain.surface_height(dir));
    for level in [0u8, 3, 6, 9] {
        let center = ChunkKey::containing(surface, level);
        // A 4×4 patch of chunks along the surface, so tile generation is shared as it is in
        // practice.
        let (east, north) = crate::geology::deposits::tangent_frame(dir);
        let keys: Vec<ChunkKey> = (0..16)
            .map(|i| {
                let (a, b) = (f64::from(i % 4) - 1.5, f64::from(i / 4) - 1.5);
                ChunkKey::containing(surface + (east * a + north * b) * center.size(), level)
            })
            .collect();
        let start = Instant::now();
        let mut triangles = 0;
        let mut meshed = 0;
        for &k in &keys {
            if let Some(m) = terrain.mesh_chunk(k) {
                triangles += m.triangle_count();
                meshed += 1;
            }
        }
        let total = start.elapsed().as_secs_f64() * 1e3;
        // Again with warm tiles: the meshing alone.
        let warm_start = Instant::now();
        for &k in &keys {
            let _ = terrain.mesh_chunk(k);
        }
        println!(
            "level {level} warm: {:.2} ms per chunk",
            warm_start.elapsed().as_secs_f64() * 1e3 / 16.0
        );
        let start = Instant::now();
        for &k in &keys {
            let _ = terrain.sample_chunk(k);
        }
        let sampling = start.elapsed().as_secs_f64() * 1e3;
        println!(
            "level {level}: {:.2} ms per chunk ({:.2} ms resampling with warm tiles), {meshed} meshed, {} triangles/chunk",
            total / 16.0,
            sampling / 16.0,
            triangles / meshed.max(1)
        );
    }
}

#[test]
fn the_surface_rock_does_not_depend_on_the_level_of_detail() {
    // A coarse chunk's vertices lie metres off the surface: if they took the rock at their
    // own depth, distant land would turn from soil to the bedrock below it.
    let terrain = home();
    // The share of each rock among the chunk's vertices within `radius` of `near`.
    let shares = |key: ChunkKey, near: DVec3, radius: f64| {
        let m = terrain.mesh_chunk(key).expect("surface");
        let mut counts = HashMap::<u16, f64>::new();
        let mut total = 0.0;
        for (p, &rock) in m
            .positions
            .iter()
            .zip(&m.materials)
            .take(m.surface_vertices)
        {
            if (key.origin() + p.as_dvec3() - near).length() < radius {
                *counts.entry(rock).or_default() += 1.0;
                total += 1.0;
            }
        }
        counts.values_mut().for_each(|c| *c /= total);
        counts
    };
    let mut agree = 0;
    let spots: Vec<DVec3> = crate::elevation::fibonacci_sphere(400)
        .filter(|&d| terrain.surface_height(d) > 50.0)
        .take(12)
        .collect();
    for up in &spots {
        let p = *up * (terrain.planet().datum.radius(*up) + terrain.surface_height(*up));
        let fine = shares(ChunkKey::containing(p, 0), p, 40.0);
        let coarse = shares(ChunkKey::containing(p, 9), p, 3000.0);
        let (&rock, _) = fine
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .expect("vertices");
        // The coarse chunk shows the fine surface's rock nearly everywhere around the spot.
        agree += usize::from(coarse.get(&rock).copied().unwrap_or(0.0) > 0.85);
    }
    // Slopes and provinces differ a little between levels (soil thins on steep ground), so allow one miss.
    assert!(
        agree + 1 >= spots.len(),
        "{agree} of {} spots agree",
        spots.len()
    );
}

/// Directed edges of the whole mesh, welded by exact position, without a reversed partner:
/// none for a closed solid.
fn unmatched(m: &TerrainMesh) -> usize {
    let key = |i: u32| m.positions[i as usize].to_array().map(f32::to_bits);
    let mut count: HashMap<([u32; 3], [u32; 3]), i64> = HashMap::new();
    for t in m.indices.as_chunks::<3>().0 {
        for k in 0..3 {
            let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
            if a != b {
                *count.entry((a, b)).or_default() += 1;
            }
        }
    }
    count
        .iter()
        .filter(|&(&(a, b), &n)| count.get(&(b, a)).copied().unwrap_or(0) != n)
        .count()
}

/// Area of the caps of `face`, checking that each lies on the face and faces out of the box.
fn cap_area(m: &TerrainMesh, face: usize, size: f32) -> f64 {
    let (axis, plane) = (face / 2, if face.is_multiple_of(2) { 0.0 } else { size });
    let mut outward = Vec3::ZERO;
    outward[axis] = if face.is_multiple_of(2) { -1.0 } else { 1.0 };
    let mut area = 0.0;
    for t in m.indices[m.cap_faces[face].clone()].as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| m.positions[i as usize]);
        for p in [a, b, c] {
            assert!(
                (p[axis] - plane).abs() < 1e-4,
                "cap of face {face} off its plane: {p}"
            );
        }
        let n = (b - a).cross(c - a);
        assert!(
            n.dot(outward) >= 0.0,
            "cap of face {face} faces into the box"
        );
        area += 0.5 * f64::from(n.length());
    }
    area
}

/// Signed volume enclosed by a closed mesh wound outwards.
fn volume(m: &TerrainMesh) -> f64 {
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| m.positions[i as usize].as_dvec3());
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

#[test]
fn caps_cover_the_rock_each_face_cuts() {
    // Flat ground 15.5 m up through a chunk 64 m wide: each side face cuts rock 15.5 m high,
    // the bottom face is all rock, the top one all air.
    let k = key(1, 0, 0, 0);
    let m = mesh(&ChunkSamples::from_fn(k, |p| p.y - 15.5), |_, _| 0);
    for p in &m.positions {
        assert!(
            p.cmpge(Vec3::ZERO).all() && p.cmple(Vec3::splat(64.0)).all(),
            "{p} outside the box"
        );
    }
    for (face, expected) in [992.0, 992.0, 4096.0, 0.0, 992.0, 992.0]
        .into_iter()
        .enumerate()
    {
        let area = cap_area(&m, face, 64.0);
        assert!(
            (area - expected).abs() < 1e-3,
            "face {face}: cap of {area} m²"
        );
    }
    assert_eq!(unmatched(&m), 0, "surface and caps are not closed");
    assert!(
        (volume(&m) - 64.0 * 64.0 * 15.5).abs() < 0.5,
        "volume {}",
        volume(&m)
    );
    // Cells all in rock are covered by strips: two triangles per row, and one per corner of
    // the rim (where the side faces' caps meet it), not two per cell.
    assert_eq!(
        m.cap_faces[2].len() / 3,
        2 * 30 + 2 * 33,
        "triangles on the bottom"
    );
}

#[test]
fn a_sphere_cut_by_a_face_is_closed_by_its_cap() {
    // A sphere reaching 5.3 m out of the chunk through the lower and through the upper x face
    // (the upper one needs the samples beyond the box).
    let r = 9.3;
    let h = r - 4.0;
    let inside = 4.0 / 3.0 * std::f64::consts::PI * r * r * r
        - std::f64::consts::PI * h * h * (3.0 * r - h) / 3.0;
    let disc = std::f64::consts::PI * (r * r - 16.0);
    for (x, face) in [(4.0, 0), (28.0, 1)] {
        let c = DVec3::new(x, 16.0, 16.0);
        let m = mesh(
            &ChunkSamples::from_fn(key(0, 0, 0, 0), |p| (p - c).length() - r),
            |_, _| 0,
        );
        assert_eq!(unmatched(&m), 0, "face {face}: not closed");
        let (v, area) = (volume(&m), cap_area(&m, face, 32.0));
        println!("face {face}: volume {v:.2} of {inside:.2} m³, cap {area:.2} of {disc:.2} m²");
        // A Surface Nets sphere of 9 cells is slightly faceted: within 2 %.
        assert!((v / inside - 1.0).abs() < 0.02, "face {face}: volume {v}");
        assert!((area / disc - 1.0).abs() < 0.02, "face {face}: cap {area}");
        for other in (0..6).filter(|&f| f != face) {
            assert!(
                m.cap_faces[other].is_empty(),
                "face {face}: a cap on face {other}"
            );
        }
    }
}

#[test]
fn ground_exactly_on_a_chunk_face_stays_closed() {
    // Level ground at y = 32 m: the vertices fall exactly on the face between two chunks.
    // Each is moved just into its own cell, so both chunks clip it alike and stay closed.
    let ground = |p: DVec3| p.y - 32.0;
    let below = mesh(&ChunkSamples::from_fn(key(0, 0, 0, 0), ground), |_, _| 0);
    let above = mesh(&ChunkSamples::from_fn(key(0, 0, 1, 0), ground), |_, _| 0);
    assert_eq!(unmatched(&below), 0, "the chunk below is not closed");
    assert_eq!(unmatched(&above), 0, "the chunk above is not closed");
    // The ground belongs to the chunk below: all of its area, none above.
    let area = |m: &TerrainMesh| -> f64 {
        m.indices[..m.surface_indices]
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| m.positions[i as usize]);
                0.5 * f64::from((b - a).cross(c - a).length())
            })
            .sum()
    };
    assert!(
        (area(&below) - 32.0 * 32.0).abs() < 1e-2,
        "area below {}",
        area(&below)
    );
    assert!(area(&above) < 1e-2, "area above {}", area(&above));
}
