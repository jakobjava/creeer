//! F3 terrain: cost of building chunks with the production pipeline (heights, geology,
//! meshing, occlusion), streaming the surroundings of a camera, and physics on the terrain.

use crate::spikes::{Better, DT, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_math::glam::DVec3;
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use creeer_sim::{TerrainGround, planet_gravity};
use creeer_terrain::chunk::ChunkKey;
use creeer_terrain::elevation::fibonacci_sphere;
use creeer_terrain::geology::deposits::tangent_frame;
use creeer_terrain::stream::{StreamSettings, Streamer};
use creeer_terrain::{Planet, Terrain};
use rayon::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Budget: chunks per second on 6 minimum-spec cores (the F0 meshing target, now for the full
/// production pipeline).
const CHUNKS_TARGET_PER_S: f64 = 200.0;

fn load() -> Arc<Terrain> {
    let path = creeer_terrain::data_path("planets/home.ron");
    Arc::new(Terrain::new(Planet::load(&path).expect("home.ron loads")))
}

/// A land spot with relief, and the surface point there.
fn spot(terrain: &Terrain) -> (DVec3, DVec3) {
    let up = fibonacci_sphere(5000)
        .find(|&d| (300.0..1500.0).contains(&terrain.surface_height(d)))
        .expect("land");
    let surface = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
    (up, surface)
}

/// `n × n` chunks of `level` along the surface around `surface`.
fn patch(surface: DVec3, up: DVec3, level: u8, n: i32) -> Vec<ChunkKey> {
    let (east, north) = tangent_frame(up);
    let size = ChunkKey::containing(surface, level).size();
    let mut keys = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let offset = east * f64::from(i - n / 2) + north * f64::from(j - n / 2);
            let key = ChunkKey::containing(surface + offset * size, level);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

pub fn run(report: &mut Report, threads: usize) {
    println!(
        "F3 terrain: chunks built by the production pipeline (heights, geology, mesh, occlusion)"
    );
    let terrain = load();
    let (up, surface) = spot(&terrain);
    for (level, name) in [(0u8, "terrain_chunk_l0_ms"), (6, "terrain_chunk_l6_ms")] {
        // Cold tiles, as streaming meets them.
        let keys = patch(surface, up, level, 4);
        let start = Instant::now();
        let triangles: usize = keys
            .iter()
            .filter_map(|&k| terrain.mesh_chunk(k))
            .map(|m| m.triangle_count())
            .sum();
        #[allow(clippy::cast_precision_loss)]
        let ms = start.elapsed().as_secs_f64() * 1e3 / keys.len() as f64;
        println!(
            "  level {level}: {ms:.2} ms per chunk, one thread ({} chunks, {} triangles each)",
            keys.len(),
            triangles / keys.len()
        );
        report.metric(name, ms, Better::Lower);
    }

    // All threads, as the streaming workers do: 256 fresh level-0 chunks.
    let fresh = load();
    let (east, _) = tangent_frame(up);
    let aside = (surface + east * 3000.0).normalize();
    let aside_surface = aside * (fresh.planet().datum.radius(aside) + fresh.surface_height(aside));
    let keys = patch(aside_surface, aside, 0, 16);
    let start = Instant::now();
    let built = keys.par_iter().filter_map(|&k| fresh.mesh_chunk(k)).count();
    #[allow(clippy::cast_precision_loss)]
    let per_s = keys.len() as f64 / start.elapsed().as_secs_f64();
    #[allow(clippy::cast_precision_loss)]
    let projected = per_s * 6.0 / threads as f64 * MIN_SPEC_CORE_FACTOR;
    println!(
        "  {threads} threads: {per_s:.0} chunks/s ({built} of {} with surface)",
        keys.len()
    );
    report.metric("terrain_chunks_per_s_threads", per_s, Better::Higher);
    report.budget(
        projected >= CHUNKS_TARGET_PER_S,
        format!(
            "terrain chunks projected to 6 min-spec cores: {projected:.0}/s >= {CHUNKS_TARGET_PER_S}"
        ),
    );

    // Streaming the surroundings of a camera standing on the ground, from nothing.
    let cold = load();
    let camera = surface + up * 2.0;
    let mut streamer = Streamer::new(Arc::clone(&cold), StreamSettings::default(), |t, k| {
        t.mesh_chunk(k).map(|m| m.triangle_count())
    });
    let start = Instant::now();
    let drawn = loop {
        let update = streamer.update(camera);
        let drawn = update.draw.len();
        if streamer.queued == 0
            && update.ready.is_empty()
            && start.elapsed() > Duration::from_millis(100)
        {
            break drawn;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    let settle = start.elapsed().as_secs_f64();
    println!(
        "  streaming from nothing, camera on the ground: {settle:.2} s for {} chunks ({drawn} drawn)",
        streamer.cached()
    );
    report.metric("terrain_stream_settle_s", settle, Better::Info);

    // Physics on the terrain: 50 crates dropped on it, in the planet's gravity.
    let mut world = World::new(DT);
    world.planet = Some(planet_gravity(cold.planet()));
    world.ground = Some(Arc::new(TerrainGround(Arc::clone(&cold))));
    let north = tangent_frame(up).1;
    for i in 0..50 {
        let (a, b) = (f64::from(i % 10) * 1.6 - 8.0, f64::from(i / 10) * 1.6 - 4.0);
        let p = surface + east * a + north * b;
        let at = p.normalize() * (cold.planet().datum.radius(p) + cold.surface_height(p) + 1.5);
        world.add(Body::rigid(
            Shape::Box {
                half: DVec3::splat(0.5),
            },
            200.0,
            at,
            DT,
        ));
    }
    // Falling, landing and settling: the busy part (resting crates fall asleep).
    for _ in 0..20 {
        world.step();
    }
    let ticks = time_ticks(|| world.step(), 60);
    println!(
        "  50 crates landing on the terrain: {:.3} / {:.3} ms per tick (p50 / p99), {} contacts",
        p(&ticks, 50.0),
        p(&ticks, 99.0),
        world.contacts.len()
    );
    report.metric(
        "world_planet_50_crates_p50_ms",
        p(&ticks, 50.0),
        Better::Lower,
    );
    println!();
}
