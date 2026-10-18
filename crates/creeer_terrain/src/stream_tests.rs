use crate::chunk::ChunkKey;
use crate::elevation::fibonacci_sphere;
use crate::stream::{StreamSettings, Streamer, Update};
use crate::{Planet, Terrain};
use creeer_math::glam::DVec3;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn terrain() -> Arc<Terrain> {
    Arc::new(Terrain::new(
        Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("loads"),
    ))
}

fn streamer(terrain: &Arc<Terrain>) -> Streamer<usize> {
    let settings = StreamSettings {
        cache: 100_000,
        ..StreamSettings::default()
    };
    Streamer::new(Arc::clone(terrain), settings, |t, key| {
        t.mesh_chunk(key).map(|m| m.triangle_count())
    })
}

/// Updates until nothing is left to build; returns the last update.
fn settle(s: &mut Streamer<usize>, camera: DVec3) -> Update<usize> {
    let start = Instant::now();
    loop {
        let update = s.update(camera);
        if s.outstanding() == 0
            && update.ready.is_empty()
            && start.elapsed() > Duration::from_millis(50)
        {
            // One more update after the last result, to draw it.
            return s.update(camera);
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "streaming does not settle"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Surface points around `center` (up to `radius` m away along the surface).
fn surface_points(terrain: &Terrain, center: DVec3, radius: f64) -> Vec<DVec3> {
    let up = center.normalize();
    fibonacci_sphere(4000)
        .filter(|d| d.angle_between(up) * 500_000.0 < radius)
        .map(|d| d * (terrain.planet().datum.radius(d) + terrain.surface_height(d)))
        .collect()
}

fn covered(draw: &[ChunkKey], points: &[DVec3]) -> f64 {
    let inside = points
        .iter()
        .filter(|&&p| draw.iter().any(|k| k.distance_to(p) == 0.0))
        .count();
    inside as f64 / points.len() as f64
}

#[test]
fn chunk_ids_are_unique_and_containment_works() {
    let a = ChunkKey::containing(DVec3::new(-1000.0, 499_000.0, 12.0), 0);
    let b = ChunkKey { level: 1, ..a };
    assert_ne!(a.id(), b.id());
    assert_ne!(
        a.id(),
        ChunkKey {
            pos: a.pos + 1,
            ..a
        }
        .id()
    );
    assert!(a.parent().contains(&a) && a.parent().parent().contains(&a));
    assert!(!a.contains(&a.parent()));
    assert!(!a.parent().contains(&ChunkKey {
        pos: a.pos + 2,
        ..a
    }));
}

#[test]
fn streaming_settles_on_a_consistent_level_of_detail() {
    let terrain = terrain();
    let up = DVec3::new(0.2, 0.9, -0.4).normalize();
    let ground = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
    let camera = ground + up * 10.0;
    let mut s = streamer(&terrain);
    let update = settle(&mut s, camera);
    let draw: Vec<ChunkKey> = update.draw.iter().map(|d| d.0).collect();
    // Caps exactly where the neighbour of the same level is not drawn.
    for &(k, mask) in &update.draw {
        for face in 0..6 {
            let mut step = creeer_math::glam::IVec3::ZERO;
            step[face / 2] = if face % 2 == 0 { -1 } else { 1 };
            let neighbour = ChunkKey {
                pos: k.pos + step,
                ..k
            };
            let cap = mask & (1 << face) != 0;
            assert_eq!(cap, !draw.contains(&neighbour), "{k:?} face {face}");
        }
    }
    assert!(!draw.is_empty());
    // Full detail under the camera.
    assert!(
        draw.iter()
            .any(|k| k.level == 0 && k.distance_to(ground) == 0.0)
    );
    let detail = StreamSettings::default().detail;
    for k in &draw {
        // Settled: no drawn chunk still wants to be split, and its parent did.
        assert!(
            k.level == 0 || k.distance_to(camera) >= detail * k.size(),
            "{k:?} wants a split"
        );
        assert!(k.parent().distance_to(camera) < detail * k.parent().size());
        // No two drawn chunks overlap.
        for other in &draw {
            assert!(k == other || !k.contains(other), "{k:?} overlaps {other:?}");
        }
    }
    let points = surface_points(&terrain, ground, 50_000.0);
    assert!(covered(&draw, &points) > 0.99, "surface not covered");
}

#[test]
fn flying_away_never_opens_holes() {
    let terrain = terrain();
    let start_up = DVec3::new(0.2, 0.9, -0.4).normalize();
    let height = |up: DVec3| terrain.planet().datum.radius(up) + terrain.surface_height(up);
    let mut s = streamer(&terrain);
    settle(&mut s, start_up * (height(start_up) + 50.0));
    // 20 km away: every frame of the transition still shows the whole surface around both.
    let end_up = (start_up + DVec3::X * 0.04).normalize();
    let camera = end_up * (height(end_up) + 50.0);
    let points = surface_points(&terrain, (start_up + end_up) * 0.5, 60_000.0);
    let start = Instant::now();
    loop {
        let update = s.update(camera);
        let draw: Vec<ChunkKey> = update.draw.iter().map(|d| d.0).collect();
        assert!(covered(&draw, &points) > 0.99, "a hole opened");
        if s.outstanding() == 0
            && update.ready.is_empty()
            && start.elapsed() > Duration::from_millis(50)
        {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(120));
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The renderer blends a chunk of size s towards the next coarser level between 4.25 s and
/// 5 s from the camera (`levels.rs`). That hides the borders only if, wherever two levels
/// meet, the finer chunk is fully blended (the border at least 5 s_fine away) and the
/// coarser one not at all (at most 4.25 s_coarse away), and levels never differ by more
/// than one. The split rule (2.5 chunk sizes) guarantees it; this checks the streamer.
#[test]
fn level_borders_lie_where_the_blend_hides_them() {
    let terrain = terrain();
    for (height, label) in [(300.0, "low"), (20_000.0, "high")] {
        let up = DVec3::new(0.3, 0.8, -0.5).normalize();
        let ground = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
        let camera = ground + up * height;
        let mut s = streamer(&terrain);
        let update = settle(&mut s, camera);
        let draw: Vec<ChunkKey> = update.draw.iter().map(|&(k, _)| k).collect();
        let mut borders = 0;
        for (i, a) in draw.iter().enumerate() {
            for b in &draw[i + 1..] {
                if a.level == b.level {
                    continue;
                }
                let (lo, hi) = (
                    a.origin().max(b.origin()),
                    (a.origin() + DVec3::splat(a.size())).min(b.origin() + DVec3::splat(b.size())),
                );
                let extent = hi - lo;
                // Touching across a face: one axis of zero extent, two positive.
                let flat = extent.to_array().iter().filter(|e| e.abs() < 1e-6).count();
                if extent.min_element() < -1e-6 || flat != 1 {
                    continue;
                }
                let (fine, coarse) = if a.level < b.level { (a, b) } else { (b, a) };
                assert_eq!(
                    coarse.level,
                    fine.level + 1,
                    "{label}: levels {} and {} meet",
                    fine.level,
                    coarse.level
                );
                let nearest = (camera.clamp(lo, hi) - camera).length();
                let farthest = (0..8)
                    .map(|c| {
                        let pick = |bit: usize, axis: usize| {
                            if c >> bit & 1 == 1 {
                                hi[axis]
                            } else {
                                lo[axis]
                            }
                        };
                        DVec3::new(pick(0, 0), pick(1, 1), pick(2, 2))
                    })
                    .map(|corner| (corner - camera).length())
                    .fold(0.0, f64::max);
                assert!(
                    nearest >= 5.0 * fine.size(),
                    "{label}: border {nearest:.0} m from the camera, fine chunk {} m",
                    fine.size()
                );
                assert!(
                    farthest <= 4.25 * coarse.size(),
                    "{label}: border {farthest:.0} m from the camera, coarse chunk {} m",
                    coarse.size()
                );
                borders += 1;
            }
        }
        assert!(borders > 10, "{label}: only {borders} level borders");
    }
}
