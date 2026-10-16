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
    // Closed: no open edges, so no skirts, and V − E + F = 2 for a sphere.
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
        // The rim: within one cell of the block's outer faces (x, z in [-1, 63]).
        let rim = m.x < 0.0 || m.x > 62.0 || m.z < 0.0 || m.z > 62.0;
        assert!(rim, "gap inside the block at {m}");
    }
}

#[test]
fn skirts_hang_into_the_rock() {
    let ground = |p: DVec3| p.y - 15.5;
    let k = key(1, 0, 0, 0);
    let m = mesh(&ChunkSamples::from_fn(k, ground), |_, _| 0);
    assert!(
        m.indices.len() > m.surface_indices,
        "an open chunk has skirts"
    );
    let skirt_vertices: HashSet<u32> = m.indices[m.surface_indices..].iter().copied().collect();
    let surface_vertices: HashSet<u32> = surface(&m).iter().copied().collect();
    for v in skirt_vertices.difference(&surface_vertices) {
        let p = m.positions[*v as usize].as_dvec3() + k.origin();
        // 3 cells of 2 m below the ground.
        assert!(
            (ground(p) + 6.0).abs() < 1e-3,
            "skirt vertex at {}",
            ground(p)
        );
    }
    // Skirt triangles stand vertical on flat ground, two-sided: per border edge two face
    // outwards from the chunk and two (the reversed copy) inwards.
    for (k, t) in m.indices[m.surface_indices..]
        .as_chunks::<3>()
        .0
        .iter()
        .enumerate()
    {
        let [a, b, c] = t.map(|i| m.positions[i as usize]);
        let n = (b - a).cross(c - a).normalize();
        assert!(n.y.abs() < 1e-4);
        let outward = ((a + b + c) / 3.0 - Vec3::splat(32.0)) * Vec3::new(1.0, 0.0, 1.0);
        let facing_out = k % 4 < 2;
        assert_eq!(n.dot(outward) > 0.0, facing_out, "skirt triangle {k}");
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

#[test]
fn skirts_are_grouped_by_the_face_they_hang_from() {
    // Flat ground through a chunk: skirts on the four side faces, none on top or bottom.
    let k = key(0, 0, 0, 0);
    let m = mesh(&ChunkSamples::from_fn(k, |p| p.y - 15.5), |_, _| 0);
    let faces = &m.skirt_faces;
    assert_eq!(faces[0].start, m.surface_indices);
    assert_eq!(faces[5].end, m.indices.len());
    for f in [0, 1, 4, 5] {
        assert!(!faces[f].is_empty(), "face {f} has no skirt");
        for t in m.indices[faces[f].clone()].as_chunks::<3>().0 {
            let c = t.iter().map(|&i| m.positions[i as usize]).sum::<Vec3>() / 3.0;
            let axis = f / 2;
            let beyond = if f % 2 == 0 {
                c[axis] < 0.0
            } else {
                c[axis] > 31.0
            };
            assert!(beyond, "skirt of face {f} at {c}");
        }
    }
    assert!(faces[2].is_empty() && faces[3].is_empty());
}
