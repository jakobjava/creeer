use crate::chunk::{ChunkKey, Occupancy};
use crate::edits::{Brush, Edit, Edits, EditsError};
use crate::{Planet, Terrain};
use creeer_math::glam::DVec3;
use std::collections::{HashMap, HashSet};

fn home() -> Terrain {
    Terrain::new(Planet::load(&crate::data_path("planets/home.ron")).expect("loads"))
}

/// A point `depth` metres under the surface somewhere on the planet, and the local up.
fn underground(terrain: &Terrain, depth: f64) -> (DVec3, DVec3) {
    let up = DVec3::new(0.31, 0.82, -0.48).normalize();
    let surface = terrain.planet().datum.radius(up) + terrain.surface_height(up);
    (up * (surface - depth), up)
}

#[test]
fn brush_distances_bounds_and_volumes() {
    let s = Brush::Sphere {
        center: DVec3::new(1.0, 2.0, 3.0),
        radius: 2.0,
    };
    assert!((s.distance(DVec3::new(1.0, 2.0, 3.0)) + 2.0).abs() < 1e-12);
    assert!((s.distance(DVec3::new(4.0, 2.0, 3.0)) - 1.0).abs() < 1e-12);
    let c = Brush::Capsule {
        a: DVec3::ZERO,
        b: DVec3::new(10.0, 0.0, 0.0),
        radius: 1.5,
    };
    assert!((c.distance(DVec3::new(5.0, 3.0, 0.0)) - 1.5).abs() < 1e-12);
    assert!((c.distance(DVec3::new(-2.0, 0.0, 0.0)) - 0.5).abs() < 1e-12);
    let (lo, hi) = c.bounds();
    assert_eq!((lo, hi), (DVec3::splat(-1.5), DVec3::new(11.5, 1.5, 1.5)));
    let r: f64 = 1.5;
    let expected = std::f64::consts::PI * r * r * (10.0 + 4.0 / 3.0 * r);
    assert!((c.volume() - expected).abs() < 1e-9);
}

#[test]
fn digging_makes_air_and_filling_makes_rock() {
    let terrain = home();
    let (p, up) = underground(&terrain, 30.0);
    assert!(terrain.density(p) < 0.0, "starts in rock");
    let rock = terrain.rock_at(p).expect("rock");
    terrain.edit(Edit::Dig(Brush::Sphere {
        center: p,
        radius: 3.0,
    }));
    assert!(terrain.density(p) > 0.0, "dug out");
    assert!(terrain.rock_at(p).is_none());
    // 2.5 m into the wall: rock again, and its density is the distance to the wall.
    let wall = p + up.cross(DVec3::X).normalize() * 5.5;
    assert!(
        (terrain.density(wall) + 2.5).abs() < 0.01,
        "{}",
        terrain.density(wall)
    );
    // The rock of a dug wall is the generator's.
    assert_eq!(terrain.rock_at(wall), terrain.rock_at(wall + up * 0.1));
    // Later edits override earlier ones: fill the hole with granite.
    let granite = terrain
        .planet()
        .geology
        .rocks
        .rock("granite")
        .expect("granite");
    terrain.edit(Edit::Fill(
        Brush::Sphere {
            center: p,
            radius: 1.0,
        },
        granite,
    ));
    assert_eq!(terrain.rock_at(p), Some(granite));
    assert!(terrain.density(p + up * 2.0) > 0.0, "the rest is still dug");
    assert_ne!(rock, u16::MAX);
}

#[test]
fn a_tunnel_inside_the_rock_meshes_closed_across_chunks() {
    let terrain = home();
    let (p, up) = underground(&terrain, 200.0);
    let key = ChunkKey::containing(p, 0);
    assert_eq!(terrain.occupancy(key), Occupancy::Rock);
    // A tunnel from the middle of this chunk into its +x neighbour.
    let a = key.center();
    let b = a + DVec3::X * 30.0;
    terrain.edit(Edit::Dig(Brush::Capsule { a, b, radius: 2.5 }));
    assert_eq!(terrain.occupancy(key), Occupancy::Mixed);
    let neighbour = ChunkKey {
        pos: key.pos + creeer_math::glam::IVec3::X,
        ..key
    };
    let mut welded: HashMap<[i64; 3], u32> = HashMap::new();
    let mut indices = Vec::new();
    let mut vertices = 0;
    for k in [key, neighbour] {
        let m = terrain.mesh_chunk(k).expect("the tunnel has walls");
        for (p, n) in m.positions.iter().zip(&m.normals).take(m.surface_vertices) {
            let world = k.origin() + p.as_dvec3();
            // Walls lie on the brush and face into the tunnel (towards the air).
            let axis = (a
                + (b - a) * ((world - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0))
                - world;
            assert!((world - a).length() > 0.0);
            assert!(n.as_dvec3().dot(axis) > 0.0, "wall normal faces the rock");
            let off = terrain.density(world);
            assert!(
                off.abs() < 0.15,
                "vertex {:.3} m off the surface, {:.2} m from the axis",
                off,
                axis.length()
            );
        }
        for &i in &m.indices[..m.surface_indices] {
            let world = k.origin() + m.positions[i as usize].as_dvec3();
            #[allow(clippy::cast_possible_truncation)]
            let grid = (world * 1000.0).round().as_i64vec3().to_array();
            let next = u32::try_from(welded.len()).expect("small");
            indices.push(*welded.entry(grid).or_insert(next));
        }
        vertices += m.positions.len();
    }
    // Welded together, the two halves form one closed tube: no open edges.
    let directed: HashSet<(u32, u32)> = indices
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
        .collect();
    let open = directed
        .iter()
        .filter(|&&(x, y)| !directed.contains(&(y, x)))
        .count();
    assert_eq!(open, 0, "tunnel has holes at the chunk boundary");
    assert!(vertices > 100);
    let _ = up;
}

#[test]
fn coarse_chunks_leave_out_brushes_they_cannot_resolve() {
    let terrain = home();
    let (p, _) = underground(&terrain, 600.0);
    terrain.edit(Edit::Dig(Brush::Sphere {
        center: p,
        radius: 2.0,
    }));
    // Level 2 (4 m cells) resolves a 2 m brush; level 3 (8 m cells) does not.
    assert_eq!(terrain.chunk_edits(ChunkKey::containing(p, 2)).len(), 1);
    assert!(terrain.chunk_edits(ChunkKey::containing(p, 3)).is_empty());
    terrain.edit(Edit::Dig(Brush::Sphere {
        center: p,
        radius: 40.0,
    }));
    assert_eq!(terrain.chunk_edits(ChunkKey::containing(p, 6)).len(), 1);
}

#[test]
fn filled_material_shows_on_the_mesh() {
    let terrain = home();
    let (p, up) = underground(&terrain, -6.0); // 6 m above the ground
    let marble = terrain
        .planet()
        .geology
        .rocks
        .rock("marble")
        .expect("marble");
    terrain.edit(Edit::Fill(
        Brush::Sphere {
            center: p,
            radius: 4.0,
        },
        marble,
    ));
    let m = terrain
        .mesh_chunk(ChunkKey::containing(p + up * 3.9, 0))
        .expect("the ball has a surface");
    let key = ChunkKey::containing(p + up * 3.9, 0);
    let on_ball = m
        .positions
        .iter()
        .zip(&m.materials)
        .filter(|(q, _)| ((key.origin() + q.as_dvec3()) - p).length() > 3.0)
        .filter(|(q, _)| ((key.origin() + q.as_dvec3()) - p).length() < 4.5);
    let (mut marble_count, mut total) = (0, 0);
    for (_, &material) in on_ball {
        total += 1;
        marble_count += usize::from(material == marble);
    }
    assert!(
        total > 20 && marble_count * 10 >= total * 9,
        "{marble_count}/{total}"
    );
}

#[test]
fn edit_logs_round_trip_bit_exactly() {
    let mut edits = Edits::default();
    edits.push(Edit::Dig(Brush::Sphere {
        center: DVec3::new(1.0 / 3.0, -2.5e5, 7.0),
        radius: 1.25,
    }));
    edits.push(Edit::Fill(
        Brush::Capsule {
            a: DVec3::new(0.1, 0.2, 0.3),
            b: DVec3::new(-4.0, 5.0e5, 6.0),
            radius: 2.0,
        },
        7,
    ));
    let bytes = edits.to_bytes();
    let back = Edits::from_bytes(&bytes).expect("reads back");
    assert_eq!(back.log(), edits.log());
    // Corrupt files are refused, not misread.
    let mut wrong = bytes.clone();
    wrong[0] = b'X';
    assert!(Edits::from_bytes(&wrong).is_err());
    let mut version = bytes.clone();
    version[8] = 99;
    assert!(matches!(Edits::from_bytes(&version), Err(EditsError(m)) if m.contains("version")));
    assert!(Edits::from_bytes(&bytes[..bytes.len() - 3]).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(Edits::from_bytes(&trailing).is_err());
}

#[test]
fn a_dug_tunnel_is_dark_and_a_dug_pit_is_lit() {
    let terrain = home();
    let (p, up) = underground(&terrain, 12.0);
    let (east, _) = crate::geology::deposits::tangent_frame(up);
    // A tunnel 12 m down, and 60 m away a bowl dug into the ground, 8 m deep and 16 m wide,
    // whose floor sees about half the sky (a narrow shaft would rightly be dark).
    let a = p - east * 14.0;
    terrain.edit(Edit::Dig(Brush::Capsule {
        a,
        b: a + east * 28.0,
        radius: 2.5,
    }));
    let bowl = Brush::Sphere {
        center: p + up * 12.0 + east * 60.0,
        radius: 8.0,
    };
    terrain.edit(Edit::Dig(bowl));
    // Mean openness of the vertices on a brush's wall (not the natural ground nearby).
    let mean_openness = |at: DVec3, brush: Brush| {
        let key = ChunkKey::containing(at, 0);
        let m = terrain.mesh_chunk(key).expect("has walls");
        let on_wall: Vec<f32> = (0..m.surface_vertices)
            .filter(|&v| {
                brush
                    .distance(key.origin() + m.positions[v].as_dvec3())
                    .abs()
                    < 0.5
            })
            .map(|v| m.openness[v])
            .collect();
        assert!(on_wall.len() > 20, "{} wall vertices", on_wall.len());
        on_wall.iter().sum::<f32>() / on_wall.len() as f32
    };
    let tunnel = mean_openness(
        p,
        Brush::Capsule {
            a,
            b: a + east * 28.0,
            radius: 2.5,
        },
    );
    let floor = mean_openness(p + up * 6.0 + east * 60.0, bowl);
    assert!(tunnel < 0.15, "tunnel openness {tunnel}");
    assert!(
        floor > 0.35 && floor > 3.0 * tunnel,
        "bowl openness {floor}"
    );
}

#[test]
fn a_thin_roof_still_hides_the_sky() {
    // A tunnel 4 m down with 1.5 m of rock over it: sky rays from its floor cross the roof
    // between samples 4 and 8 m up (found in F5 at a tunnel's mouth); they march instead.
    let terrain = home();
    let (p, up) = underground(&terrain, 4.0);
    let (east, _) = crate::geology::deposits::tangent_frame(up);
    let tunnel = Brush::Capsule {
        a: p - east * 14.0,
        b: p + east * 14.0,
        radius: 2.5,
    };
    terrain.edit(Edit::Dig(tunnel));
    let key = ChunkKey::containing(p, 0);
    let m = terrain.mesh_chunk(key).expect("has walls");
    // The lower half of the wall: the floor, with the roof straight above.
    let floor: Vec<f32> = (0..m.surface_vertices)
        .filter(|&v| {
            let q = key.origin() + m.positions[v].as_dvec3();
            tunnel.distance(q).abs() < 0.5 && (q - p).dot(up) < -1.0
        })
        .map(|v| m.openness[v])
        .collect();
    assert!(floor.len() > 20, "{} floor vertices", floor.len());
    let mean = floor.iter().sum::<f32>() / floor.len() as f32;
    assert!(mean < 0.15, "floor openness {mean}");
}
